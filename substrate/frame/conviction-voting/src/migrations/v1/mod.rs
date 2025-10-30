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
// limitations under the License.

//! The v0 -> v1 multi-block migration.

extern crate alloc;

use super::CONVICTION_VOTING_ID;
use crate::pallet::Config;
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::PhantomData,
	weights::WeightMeter,
};

// #[cfg(feature = "try-runtime")]
// use alloc::collections::btree_map::BTreeMap;

// #[cfg(feature = "try-runtime")]
// use alloc::vec::Vec;

mod benchmarking;
mod tests;
pub mod weights;

/// V0 types.
pub mod v0 {
	use super::Config;
	use crate::pallet::Pallet;
	use frame_support::{storage_alias, Blake2_128Concat};

	#[storage_alias]
	// pub type MyMap<T: Config> = StorageMap<Pallet<T>, Blake2_128Concat, u32, u32>;
}

/// Migrates storage items from v0 to v1.
pub struct SteppedMigrationV1<T: Config, W: weights::WeightInfo>(PhantomData<(T, W)>);
impl<T: Config, W: weights::WeightInfo> SteppedMigration for SteppedMigrationV1<T, W> {
	type Cursor = u32;
	type Identifier = MigrationId<24>;

	/// The identifier of this migration. Which should be globally unique.
	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *CONVICTION_VOTING_ID, version_from: 0, version_to: 1 }
	}

    #[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, frame_support::sp_runtime::TryRuntimeError> {
		use codec::Encode;

		// Return the state of the storage before the migration.
		// Ok(v0::MyMap::<T>::iter().collect::<BTreeMap<_, _>>().encode())
	}

	/// The logic for each step in the migratoin.
	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let required = W::step();
		
        // No weight for even a single step.
		if meter.remaining().any_lt(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		// We loop here to do as much progress as possible per step.
		loop {
			if meter.try_consume(required).is_err() {
				break;
			}

			let mut iter = if let Some(last_key) = cursor {
				// Iterate over value.
				// v0::MyMap::<T>::iter_from(v0::MyMap::<T>::hashed_key_for(last_key))
			} else {
				// If no cursor is provided, start iterating from the beginning.
				// v0::MyMap::<T>::iter()
			};

			// If there's a next item in the iterator, perform the migration.
			if let Some((last_key, value)) = iter.next() {
				// Migrate the inner value: u32 -> u64.
				// let value = value as u64;
				// We can just insert here since the old and the new map share the same key-space.
				// Otherwise it would have to invert the concat hash function and re-hash it.
				// MyMap::<T>::insert(last_key, value);
				cursor = Some(last_key) // Return the processed key as the new cursor.
			} else {
                // Migration is complete.
				cursor = None;
				break
			}
		}
		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(prev: Vec<u8>) -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
		use codec::Decode;

		// Check the state of the storage after the migration.
		// let prev_map = BTreeMap::<u32, u32>::decode(&mut &prev[..])
		// 	.expect("Failed to decode the previous storage state");

		// Check the len of prev and post are the same.
		// assert_eq!(
		// 	MyMap::<T>::iter().count(),
		// 	prev_map.len(),
		// 	"Migration failed: the number of items in the storage after the migration is not the same as before"
		// );

		// for (key, value) in prev_map {
		// 	let new_value =
		// 		MyMap::<T>::get(key).expect("Failed to get the value after the migration");
		// 	assert_eq!(
		// 		value as u64, new_value,
		// 		"Migration failed: the value after the migration is not the same as before"
		// 	);
		// }

		Ok(())
	}
}
