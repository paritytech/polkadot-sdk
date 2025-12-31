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
//! This migrate the old `ContractInfoOf` storage to the new `AccountInfoOf`.

extern crate alloc;

use super::PALLET_MIGRATIONS_ID;
use crate::{weights::WeightInfo, AccountInfo, AccountInfoOf, Config, H160};
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::PhantomData,
	weights::WeightMeter,
};

#[cfg(feature = "try-runtime")]
use alloc::collections::btree_map::BTreeMap;

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;

/// Module containing the old storage items.
pub mod old {
	use super::Config;
	use crate::{pallet::Pallet, ContractInfo, H160};
	use frame_support::{storage_alias, Identity};

	#[storage_alias]
	/// The storage item that is being migrated from.
	pub type ContractInfoOf<T: Config> = StorageMap<Pallet<T>, Identity, H160, ContractInfo<T>>;
}

/// Migrates the items of the [`old::ContractInfoOf`] map into [`crate::AccountInfoOf`].
pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> SteppedMigration for Migration<T> {
	type Cursor = H160;
	type Identifier = MigrationId<17>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 0, version_to: 1 }
	}

	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let required = <T as Config>::WeightInfo::v1_migration_step();
		if meter.remaining().any_lt(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		loop {
			if meter.try_consume(required).is_err() {
				break;
			}

			let iter = if let Some(last_key) = cursor {
				old::ContractInfoOf::<T>::iter_from(old::ContractInfoOf::<T>::hashed_key_for(
					last_key,
				))
			} else {
				old::ContractInfoOf::<T>::iter()
			};

			if let Some((last_key, value)) = iter.drain().next() {
				AccountInfoOf::<T>::insert(
					last_key,
					AccountInfo { account_type: value.into(), ..Default::default() },
				);
				cursor = Some(last_key)
			} else {
				cursor = None;
				break
			}
		}
		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, frame_support::sp_runtime::TryRuntimeError> {
		use codec::Encode;

		// Return the state of the storage before the migration.
		Ok(old::ContractInfoOf::<T>::iter().collect::<BTreeMap<_, _>>().encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(prev: Vec<u8>) -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
		use codec::Decode;

		// Check the state of the storage after the migration.
		let prev_map = BTreeMap::<H160, crate::ContractInfo<T>>::decode(&mut &prev[..])
			.expect("Failed to decode the previous storage state");

		// Check the len of prev and post are the same.
		assert_eq!(
			AccountInfoOf::<T>::iter().count(),
			prev_map.len(),
			"Migration failed: the number of items in the storage after the migration is not the same as before"
		);

		for (key, value) in prev_map {
			let new_value = AccountInfo::<T>::load_contract(&key);
			assert_eq!(
				Some(value),
				new_value,
				"Migration failed: the value after the migration is not the same as before"
			);
		}

		Ok(())
	}
}

#[test]
fn migrate_to_v1() {
	use crate::{
		tests::{ExtBuilder, Test},
		ContractInfo,
	};
	ExtBuilder::default().build().execute_with(|| {
		for i in 0..10u8 {
			let addr = H160::from([i; 20]);
			old::ContractInfoOf::<Test>::insert(
				addr,
				ContractInfo::new(&addr, 1u32.into(), Default::default()).unwrap(),
			);
		}

		let mut cursor = None;
		let mut weight_meter = WeightMeter::new();
		while let Some(new_cursor) = Migration::<Test>::step(cursor, &mut weight_meter).unwrap() {
			cursor = Some(new_cursor);
		}

		assert_eq!(old::ContractInfoOf::<Test>::iter().count(), 0);
		assert_eq!(AccountInfoOf::<Test>::iter().count(), 10);
	})
}
