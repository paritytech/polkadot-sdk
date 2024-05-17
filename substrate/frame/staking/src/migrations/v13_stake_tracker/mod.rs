// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and

//! # Multi-Block Migration v13
//!
//! Implements the multi-block migrations to support the `pallet-stake-tracker` and a strictly
//! sorted list of targets with a bags-list.

use super::PALLET_MIGRATIONS_ID;
use crate::{log, weights, Config, Nominators, Pallet, StakerStatus};
use core::marker::PhantomData;
use frame_election_provider_support::SortedListProvider;
use frame_support::{
	ensure,
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	traits::Defensive,
};
use sp_staking::StakingInterface;
use sp_std::prelude::*;

#[cfg(test)]
mod tests;

#[cfg(test)]
const TRY_STATE_INTERVAL: usize = 10_000_000;

/// V13 Multi-block migration to introduce the stake-tracker pallet.
///
/// A step of the migration consists of processing one nominator in the [`Nominators`] list. All
/// nominatior's target nominations are processed per step (bound by upper bound of the max
/// nominations).
///
/// The goals of the migration are:
/// - Insert all the nominated targets into the [`SortedListProvider`] target list.
/// - Ensure the target score (total stake) is the sum of the self stake and all its nominations
/// stake.
/// - Ensure the new targets in the list are sorted per total stake (as per the underlying
///   [`SortedListProvider`]).
pub struct MigrationV13<T: Config, W: weights::WeightInfo>(PhantomData<(T, W)>);

// TODO: check bonds again.
impl<T: Config<CurrencyBalance = u128>, W: weights::WeightInfo> SteppedMigration
	for MigrationV13<T, W>
{
	type Cursor = T::AccountId;
	type Identifier = MigrationId<18>;

	/// Identifier of this migration which should be globally unique.
	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 12, version_to: 13 }
	}

	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut frame_support::weights::WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let required = W::v13_mmb_step();

		// If there's no enough weight left in the block for a migration step, return an error.
		if meter.remaining().any_lt(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		#[cfg(test)]
		let mut counter = 0;

		// do as much progress as possible per step.
		while meter.try_consume(required).is_ok() {
			// fetch the next nominator to migrate.
			let mut iter = if let Some(ref last_nom) = cursor {
				Nominators::<T>::iter_from(Nominators::<T>::hashed_key_for(last_nom))
			} else {
				Nominators::<T>::iter()
			};

			if let Some((nominator, _)) = iter.next() {
				// try chill nominator. If chilled, skip migration of this nominator.
				if Self::try_chill_nominator(&nominator) {
					log!(info, "nominator {:?} chilled, skip it.", nominator);
					continue;
				}
				// clean the nominations before migrating. This will ensure that the voter is not
				// nominating duplicate and/or dangling targets.
				let nominations = Self::clean_nominations(&nominator)?;

				let nominator_stake = Pallet::<T>::weight_of(&nominator);

				log!(
					info,
					"mmb: processing nominator {:?} with {} nominations. remaining {} nominators to migrate.",
					nominator,
                    nominations.len(),
					iter.count()
				);

				// iter over up to `MaxNominationsOf<T>` targets of `nominator`.
				for target in nominations.into_iter() {
					if let Ok(current_stake) = <T as Config>::TargetList::get_score(&target) {
						// target is in the target list. update with nominator's stake.
						if let Some(total_stake) = current_stake.checked_add(nominator_stake.into())
						{
							let _ = <T as Config>::TargetList::on_update(&target, total_stake)
								.defensive();
						} else {
							log!(error, "target stake overflow. exit.");
							return Err(SteppedMigrationError::Failed)
						}
					} else {
						// target is not in the target list, insert new node and consider self
						// stake.
						let self_stake = Pallet::<T>::weight_of(&target);
						if let Some(total_stake) = self_stake.checked_add(nominator_stake) {
							let _ =
								<T as Config>::TargetList::on_insert(target, total_stake.into())
									.defensive();
						} else {
							log!(error, "target stake overflow. exit.");
							return Err(SteppedMigrationError::Failed)
						}
					}
				}

				// enable partial checks for testing purposes only.
				#[cfg(test)]
				{
					counter += 1;
					if counter % TRY_STATE_INTERVAL == 0 {
						Pallet::<T>::do_try_state_approvals(Some(nominator.clone())).unwrap();
					}
				}

				// progress cursor.
				cursor = Some(nominator)
			} else {
				// done, return earlier.
				return Ok(None)
			}
		}

		Ok(cursor)
	}
}

impl<T: Config, W: weights::WeightInfo> MigrationV13<T, W> {
	/// Chills a nominator if their active stake is below the minimum bond.
	pub(crate) fn try_chill_nominator(who: &T::AccountId) -> bool {
		if Pallet::<T>::active_stake(&who).unwrap_or_default() <
			Pallet::<T>::minimum_nominator_bond()
		{
			let _ = <Pallet<T> as StakingInterface>::chill(&who).defensive();
			return true;
		}
		false
	}

	/// Cleans up the nominations of `who`.
	///
	/// After calling this method, the following invariants are respected:
	/// - `stash` has no duplicate nominations;
	/// - `stash` has no dangling nominations (i.e. nomination of non-active validator stashes).
	///
	/// If the clean set of nominations is empty, `who` is chilled.
	///
	/// When successful, the final nominations of the stash are returned.
	pub(crate) fn clean_nominations(
		who: &T::AccountId,
	) -> Result<Vec<T::AccountId>, SteppedMigrationError> {
		use sp_std::collections::btree_set::BTreeSet;

		ensure!(
			Pallet::<T>::status(who).map(|x| x.is_nominator()).unwrap_or(false),
			SteppedMigrationError::Failed
		);

		let mut raw_nominations = Nominators::<T>::get(who)
			.map(|n| n.targets.into_inner())
			.expect("who is nominator as per the check above; qed.");

		let count_before = raw_nominations.len();

		// remove duplicate nominations.
		let dedup_noms: Vec<T::AccountId> = raw_nominations
			.drain(..)
			.collect::<BTreeSet<_>>()
			.into_iter()
			.collect::<Vec<_>>();

		// remove all non-validator nominations.
		let nominations = dedup_noms
			.into_iter()
			.filter(|n| Pallet::<T>::status(n) == Ok(StakerStatus::Validator))
			.collect::<Vec<_>>();

		// update `who`'s nominations in staking or chill voter, if necessary.
		if nominations.len() == 0 {
			let _ = <Pallet<T> as StakingInterface>::chill(&who).defensive();
		} else if count_before > nominations.len() {
			<Pallet<T> as StakingInterface>::nominate(who, nominations.clone()).map_err(|e| {
				log!(error, "failed to migrate nominations {:?}.", e);
				SteppedMigrationError::Failed
			})?;
		}

		Ok(nominations)
	}
}
