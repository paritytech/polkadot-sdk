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

use core::marker::PhantomData;

use frame_support::{
	migrations::{MigrationId, SteppedMigration},
	traits::Defensive,
};

use super::PALLET_MIGRATIONS_ID;
use crate::{log, BalanceOf, Config, Nominators, Pallet, Validators};
use frame_election_provider_support::SortedListProvider;
use sp_runtime::Saturating;
use sp_staking::StakingInterface;

mod benchmarks;
#[cfg(test)]
mod tests;
pub mod weights;

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
	) -> Result<Option<Self::Cursor>, frame_support::migrations::SteppedMigrationError> {
		let required = W::step();

		// If there's no enough weight left in the block for a migration step, return an error.
		if meter.remaining().any_lt(required) {
			return Err(frame_support::migrations::SteppedMigrationError::InsufficientWeight {
				required,
			});
		}

		// Do as much progress as possible per step.
		while meter.try_consume(required).is_ok() {
			// 1. get next validator in the Validators map.
			let mut iter = if let Some(ref last_val) = cursor {
				Validators::<T>::iter_from(Validators::<T>::hashed_key_for(last_val))
			} else {
				// first step, start from beginning of the validator's map.
				Validators::<T>::iter()
			};

			if let Some((target, _)) = iter.next() {
				log!(
                    info,
                    "multi-block migrations: processing target {:?}. remaining {} targets to migrate.",
                    target,
                    iter.count());

				// 2. calculate target's stake which consits of self-stake + all of its nominator's
				//    stake.
				let self_stake = Pallet::<T>::stake(&target).defensive_unwrap_or_default().total;

				let total_stake = Nominators::<T>::iter()
					.filter(|(_v, noms)| noms.targets.contains(&target))
					.map(|(v, _)| Pallet::<T>::stake(&v).defensive_unwrap_or_default())
					.fold(self_stake, |sum: BalanceOf<T>, stake| stake.total.saturating_add(sum));

				// 3. insert (validator, score = total_stake) to the target bags list.
				let _ = T::TargetList::on_insert(target.clone(), total_stake).defensive();

				// 4. progress cursor.
				cursor = Some(target)
			} else {
                // done, return earlier.
				return Ok(None)
			}
		}

		Ok(cursor)
	}
}
