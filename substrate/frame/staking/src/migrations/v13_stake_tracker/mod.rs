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
use crate::{log, weights, Config, Nominations, Nominators, Pallet, StakerStatus, Validators};
use core::marker::PhantomData;
use frame_election_provider_support::{SortedListProvider, VoteWeight};
use frame_support::{
	ensure,
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	traits::Defensive,
	BoundedVec,
};
use sp_staking::StakingInterface;
use sp_std::prelude::*;

#[cfg(test)]
mod tests;

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

		// do as much progress as possible per step.
		while meter.try_consume(required).is_ok() {
			// fetch the next nominator to migrate.
			let mut iter = if let Some(ref last_nom) = cursor {
				Nominators::<T>::iter_from(Nominators::<T>::hashed_key_for(last_nom))
			} else {
				Nominators::<T>::iter()
			};

			if let Some((nominator, _)) = iter.next() {
				let nominator_vote = Pallet::<T>::weight_of(&nominator);

				// clean the nominations before migrating. This will ensure that the voter is not
				// nominating duplicate and/or dangling targets.
				let nominations = Self::clean_nominations(&nominator)?;

				// iter over up to `MaxNominationsOf<T>` targets of `nominator` and insert or
				// update the target's approval's score.
				for target in nominations.into_iter() {
					if <T as Config>::TargetList::contains(&target) {
						Self::update_target(&target, nominator_vote)?;
					} else {
						Self::insert_target(&target, nominator_vote)?;
					}
				}

				// progress cursor.
				cursor = Some(nominator)
			} else {
				// done, return earlier.

				// TODO: do this as the migration is performed -- not a large step at the end.
				// but before, add active validators without any nominations.
				for (validator, _) in Validators::<T>::iter() {
					if !<T as Config>::TargetList::contains(&validator) &&
						<Pallet<T> as StakingInterface>::status(&validator) ==
							Ok(StakerStatus::Validator)
					{
						let self_stake = Pallet::<T>::weight_of(&validator);
						<T as Config>::TargetList::on_insert(validator, self_stake.into())
							.expect("node does not exist, checked above; qed.");
					}
				}
				return Ok(None)
			}
		}

		Ok(cursor)
	}
}

impl<T: Config<CurrencyBalance = u128>, W: weights::WeightInfo> MigrationV13<T, W> {
	/// Inserts a new target in the list.
	///
	/// Note: the caller must ensure that the target node does not exist in the list yet.
	/// Oterhwise, use [`Self::update_target`].
	fn insert_target(
		who: &T::AccountId,
		nomination_stake: VoteWeight,
	) -> Result<(), SteppedMigrationError> {
		let init_stake = match <Pallet<T> as StakingInterface>::status(&who) {
			Ok(StakerStatus::Validator) => {
				let self_stake = Pallet::<T>::weight_of(&who);
				if let Some(total_stake) = self_stake.checked_add(nomination_stake) {
					total_stake
				} else {
					log!(error, "target stake overflow. exit.");
					return Err(SteppedMigrationError::Failed)
				}
			},
			_ => nomination_stake,
		};

		match <T as Config>::TargetList::on_insert(who.clone(), init_stake.into()) {
			Err(e) => {
				log!(error, "inserting {:?} in TL: {:?}", who, e);
				Err(SteppedMigrationError::Failed)
			},
			Ok(_) => Ok(()),
		}
	}

	/// Updates the target score in the target list.
	///
	/// Note: the caller must ensure that the target node already exists in the list. Otherwise,
	/// use [`Self::insert_target`].
	fn update_target(
		who: &T::AccountId,
		nomination_stake: VoteWeight,
	) -> Result<(), SteppedMigrationError> {
		let current_stake =
			<T as Config>::TargetList::get_score(&who).expect("node is in the list");

		if let Some(total_stake) = current_stake.checked_add(nomination_stake.into()) {
			let _ = <T as Config>::TargetList::on_update(&who, total_stake)
				.map_err(|e| log!(error, "updating TL score of {:?}: {:?}", who, e))
				.defensive();
			Ok(())
		} else {
			log!(error, "target stake overflow. exit.");
			Err(SteppedMigrationError::Failed)
		}
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

		let raw_nominations =
			Nominators::<T>::get(who).expect("who is nominator as per the check above; qed.");
		let mut raw_targets = raw_nominations.targets.into_inner();

		let count_before = raw_targets.len();

		// remove duplicate nominations.
		let dedup_noms: Vec<T::AccountId> =
			raw_targets.drain(..).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();

		// remove all non-validator nominations.
		let targets = dedup_noms
			.into_iter()
			.filter(|n| Pallet::<T>::status(n) == Ok(StakerStatus::Validator))
			.collect::<Vec<_>>();

		if targets.len() == 0 {
			// if no nominations are left, chill the nominator.
			let _ = <Pallet<T> as StakingInterface>::chill(&who)
				.map_err(|e| {
					log!(error, "error when chilling {:?}", who);
					e
				})
				.defensive();
		} else if count_before > targets.len() {
			// force update the nominations.
			let bounded_targets = targets
				.clone()
				.into_iter()
				.collect::<Vec<_>>()
				.try_into()
				.expect(
				"new bound should be within the existent set of targets, thus it should fit; qed.",
			);

			let nominations = Nominations {
				targets: bounded_targets,
				submitted_in: raw_nominations.submitted_in,
				suppressed: raw_nominations.suppressed,
			};

			<Pallet<T>>::do_add_nominator(who, nominations);
		}

		Ok(targets)
	}
}
