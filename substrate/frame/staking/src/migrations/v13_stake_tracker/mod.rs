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
use codec::{Decode, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use frame_election_provider_support::{SortedListProvider, VoteWeight};
use frame_support::{
	ensure,
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	traits::{Defensive, DefensiveSaturating},
};
use scale_info::TypeInfo;
use sp_staking::StakingInterface;
use sp_std::prelude::*;

#[cfg(test)]
mod tests;

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, PartialEq, Debug, Clone)]
pub enum Processing {
	Nominators,
	Validators,
	Done,
}
impl Default for Processing {
	fn default() -> Self {
		Processing::Nominators
	}
}

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
impl<T: Config, W: weights::WeightInfo> SteppedMigration for MigrationV13<T, W> {
	// nominator cursor and validator cursor.
	type Cursor = (Option<T::AccountId>, Processing);
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
			let new_cursor = match cursor {
				None => {
					// start processing first nominator.
					if let Some((nominator, nominations)) = Nominators::<T>::iter().next() {
						Self::process_nominator(&nominator, nominations)?;
						Some((Some(nominator), Processing::Nominators))
					} else {
						Some((None, Processing::Validators))
					}
				},
				Some((maybe_nominator, Processing::Nominators)) => {
					let mut iter = if let Some(last_nominator) = maybe_nominator {
						Nominators::<T>::iter_from(Nominators::<T>::hashed_key_for(last_nominator))
					} else {
						Nominators::<T>::iter()
					};

					if let Some((nominator, nominations)) = iter.next() {
						Self::process_nominator(&nominator, nominations)?;
						Some((Some(nominator), Processing::Nominators))
					} else {
						// no more nominators to process, go to next phase.
						Some((None, Processing::Validators))
					}
				},
				Some((maybe_validator, Processing::Validators)) => {
					// process validator.
					let mut iter = if let Some(last_validator) = maybe_validator {
						Validators::<T>::iter_from(Validators::<T>::hashed_key_for(last_validator))
					} else {
						Validators::<T>::iter()
					};

					// nominators have been all processed, start processing validators.
					if let Some((validator, _)) = iter.next() {
						Self::process_validator(&validator);
						Some((Some(validator), Processing::Validators))
					} else {
						Some((None, Processing::Done))
					}
				},
				Some((_, Processing::Done)) => Some((None, Processing::Done)),
			};

			// progress or terminate.
			if new_cursor.clone().unwrap_or_default().1 == Processing::Done {
				return Ok(None)
			} else {
				cursor = new_cursor;
			}
		}

		Ok(cursor)
	}
}

impl<T: Config, W: weights::WeightInfo> MigrationV13<T, W> {
	fn process_nominator(
		nominator: &T::AccountId,
		nominations: Nominations<T>,
	) -> Result<(), SteppedMigrationError> {
		let nominator_vote = Pallet::<T>::weight_of(nominator);
		// clean the nominations before migrating. This will ensure that the voter is not
		// nominating duplicate and/or dangling targets.
		let nominations = Self::clean_nominations(nominator, nominations)?;

		// iter over up to `MaxNominationsOf<T>` targets of `nominator` and insert or
		// update the target's approval's score.
		for target in nominations.into_iter() {
			if <T as Config>::TargetList::contains(&target) {
				Self::update_target(&target, nominator_vote)?;
			} else {
				Self::insert_target(&target, nominator_vote)?;
			}
		}
		Ok(())
	}

	fn process_validator(validator: &T::AccountId) {
		if !<T as Config>::TargetList::contains(validator) &&
			<Pallet<T> as StakingInterface>::status(validator) == Ok(StakerStatus::Validator)
		{
			let self_stake = Pallet::<T>::weight_of(validator);
			<T as Config>::TargetList::on_insert(validator.clone(), self_stake.into())
				.expect("node does not exist, checked above; qed.");
		}
	}

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
				let total_stake = self_stake.defensive_saturating_add(nomination_stake);
				total_stake
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

		let total_stake = current_stake.defensive_saturating_add(nomination_stake.into());
		let _ = <T as Config>::TargetList::on_update(&who, total_stake.into()).map_err(|e| {
			log!(error, "updating TL score of {:?}: {:?}", who, e);
			SteppedMigrationError::Failed
		})?;

		Ok(())
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
		raw_nominations: Nominations<T>,
	) -> Result<Vec<T::AccountId>, SteppedMigrationError> {
		use sp_std::collections::btree_set::BTreeSet;

		ensure!(
			Pallet::<T>::status(who).map(|x| x.is_nominator()).unwrap_or(false),
			SteppedMigrationError::Failed
		);

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
