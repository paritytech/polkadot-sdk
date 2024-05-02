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
use crate::{log, weights, Config, Nominators, Pallet};
use core::marker::PhantomData;
use frame_election_provider_support::SortedListProvider;
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	traits::Defensive,
};
use sp_runtime::traits::CheckedAdd;
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
impl<T: Config, W: weights::WeightInfo> SteppedMigration for MigrationV13<T, W> {
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

		// Do as much progress as possible per step.
		while meter.try_consume(required).is_ok() {
			// 1. get next validator in the Validators map.
			let mut iter = if let Some(ref last_nom) = cursor {
				Nominators::<T>::iter_from(Nominators::<T>::hashed_key_for(last_nom))
			} else {
				// first step, start from beginning of the validator's map.
				Nominators::<T>::iter()
			};

			if let Some((nominator, _)) = iter.next() {
				let nominator_stake =
					Pallet::<T>::stake(&nominator).defensive_unwrap_or_default().total;
				let nominations = Nominators::<T>::get(&nominator)
					.map(|n| n.targets.into_inner())
					.unwrap_or_default();

				log!(
					info,
					"multi-block migrations: processing nominator {:?} with {} nominations. remaining {} nominators to migrate.",
					nominator,
                    nominations.len(),
					iter.count()
				);

				// iter over up to `MaxNominationsOf<T>` targets of `nominator`.
				for target in nominations.into_iter() {
					if let Ok(current_stake) = T::TargetList::get_score(&target) {
						// target is not in the target list. update with nominator's stake.

						if let Some(total_stake) = current_stake.checked_add(&nominator_stake) {
							let _ = T::TargetList::on_update(&target, total_stake).defensive();
						} else {
							log!(error, "target stake overflow. exit.");
							return Err(SteppedMigrationError::Failed)
						}
					} else {
						// target is not in the target list, insert new node and consider self
						// stake.
						let self_stake = Pallet::<T>::stake(&target).defensive_unwrap_or_default();
						if let Some(total_stake) = self_stake.total.checked_add(&nominator_stake) {
							let _ = T::TargetList::on_insert(target, total_stake).defensive();
						} else {
							log!(error, "target stake overflow. exit.");
							return Err(SteppedMigrationError::Failed)
						}
					}
				}

				let _ = T::VoterList::on_update(
					&nominator,
					Pallet::<T>::stake(&nominator).defensive_unwrap_or_default().active,
				);

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
