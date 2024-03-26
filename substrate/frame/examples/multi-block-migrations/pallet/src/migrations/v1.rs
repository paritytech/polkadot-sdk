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

//! # Multi-Block Migration v1
//!
//! This module showcases a simple migration that iterates over the values in the
//! [`old::StoredValue`](`crate::migrations::v1::old::StoredValue`) storage map, transforms them,
//! and inserts them into the [`StoredValue`](`crate::pallet::StoredValue`) storage map.

use super::{MigrationIdentifier, PALLET_MIGRATIONS_ID};
use crate::pallet::{Config, StoredValue};
use frame_support::{
	migrations::{SteppedMigration, SteppedMigrationError},
	pallet_prelude::PhantomData,
	weights::WeightMeter,
	Hashable,
};

/// Module containing the OLD storage items.
///
/// Before running this migration, the storage alias defined here represents the
/// `on_chain` storage.
// This module is public only for the purposes of linking it in the documentation. It is not
// intended to be used by any other code.
pub mod old {
	use super::Config;
	use crate::pallet::Pallet;
	use frame_support::{storage_alias, Blake2_128Concat};

	#[storage_alias]
	/// The storage item that is being migrated from.
	pub type StoredValue<T: Config> = StorageMap<Pallet<T>, Blake2_128Concat, u32, u32>;
}

pub struct LazyMigrationV1<T: Config>(PhantomData<T>);
impl<T: Config> SteppedMigration for LazyMigrationV1<T> {
	type Cursor = u32;
	type Identifier = MigrationIdentifier;

	/// The identifier of this migration. Which should be globally unique.
	fn id() -> Self::Identifier {
		MigrationIdentifier {
			pallet_identifier: (*PALLET_MIGRATIONS_ID).twox_128(),
			version_from: 0,
			version_to: 1,
		}
	}

	/// The actual logic of the migration.
	///
	/// This function is called repeatedly until it returns `Ok(None)`, indicating that the
	/// migration is complete. Ideally, the migration should be designed in such a way that each
	/// step consumes as much weight as possible. However, this is simplified to perform one
	/// stored value mutation per block.
	fn step(
		cursor: Option<Self::Cursor>,
		_meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let mut iter = if let Some(last_key) = cursor {
			// If a cursor is provided, start iterating from the stored value
			// corresponding to the last key processed in the previous step.
			old::StoredValue::<T>::iter_from(old::StoredValue::<T>::hashed_key_for(last_key))
		} else {
			// If no cursor is provided, start iterating from the beginning.
			old::StoredValue::<T>::iter()
		};

		if let Some((hash, value)) = iter.next() {
			// If there's a next item in the iterator, perform the migration.
			StoredValue::<T>::insert(hash, value as u64);
			Ok(Some(hash)) // Return the processed key as the new cursor.
		} else {
			Ok(None) // Signal that the migration is complete (no more items to process).
		}
	}
}
