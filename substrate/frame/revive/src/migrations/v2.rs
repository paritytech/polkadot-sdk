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

//! # Multi-Block Migration v2
//!
//! This migrate the old `CodeInfoOf` storage to the new `CodeInfoOf` which add the new `code_type`
//! field.

extern crate alloc;

use super::PALLET_MIGRATIONS_ID;
use crate::{weights::WeightInfo, Config, H256};
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
mod old {
	use super::Config;
	use crate::{pallet::Pallet, AccountIdOf, BalanceOf, H256};
	use codec::{Decode, Encode};
	use frame_support::{storage_alias, Identity};

	#[derive(Clone, Encode, Decode)]
	pub struct CodeInfo<T: Config> {
		pub owner: AccountIdOf<T>,
		#[codec(compact)]
		pub deposit: BalanceOf<T>,
		#[codec(compact)]
		pub refcount: u64,
		pub code_len: u32,
		pub behaviour_version: u32,
	}

	#[storage_alias]
	/// The storage item that is being migrated from.
	pub type CodeInfoOf<T: Config> = StorageMap<Pallet<T>, Identity, H256, CodeInfo<T>>;
}

mod new {
	use super::Config;
	use crate::{pallet::Pallet, AccountIdOf, BalanceOf, H256};
	use codec::{Decode, Encode};
	use frame_support::{storage_alias, Identity, RuntimeDebugNoBound};

	#[derive(RuntimeDebugNoBound, Clone, Encode, Decode, PartialEq, Eq)]
	pub enum BytecodeInfo<T: Config> {
		Pvm {
			owner: AccountIdOf<T>,
			#[codec(compact)]
			refcount: u64,
		},
		Evm,
	}

	#[derive(RuntimeDebugNoBound, PartialEq, Eq, Encode, Decode)]
	pub struct CodeInfo<T: Config> {
		#[codec(compact)]
		pub deposit: BalanceOf<T>,
		pub code_len: u32,
		pub bytecode_info: BytecodeInfo<T>,
		pub behaviour_version: u32,
	}

	#[storage_alias]
	/// The storage item that is being migrated to.
	pub type CodeInfoOf<T: Config> = StorageMap<Pallet<T>, Identity, H256, CodeInfo<T>>;
}

/// Migrates the items of the [`old::CodeInfoOf`] map into [`crate::CodeInfoOf`] by adding the
/// `code_type` field.
pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> SteppedMigration for Migration<T> {
	type Cursor = H256;
	type Identifier = MigrationId<17>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 1, version_to: 2 }
	}

	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let required = <T as Config>::WeightInfo::v2_migration_step();
		if meter.remaining().any_lt(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		loop {
			if meter.try_consume(required).is_err() {
				break;
			}

			let iter = if let Some(last_key) = cursor {
				old::CodeInfoOf::<T>::iter_from(old::CodeInfoOf::<T>::hashed_key_for(last_key))
			} else {
				old::CodeInfoOf::<T>::iter()
			};

			if let Some((last_key, value)) = iter.drain().next() {
				new::CodeInfoOf::<T>::insert(
					last_key,
					new::CodeInfo {
						deposit: value.deposit,
						code_len: value.code_len,
						bytecode_info: new::BytecodeInfo::Pvm {
							owner: value.owner,
							refcount: value.refcount,
						},
						behaviour_version: value.behaviour_version,
					},
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
		Ok(old::CodeInfoOf::<T>::iter().collect::<BTreeMap<_, _>>().encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(prev: Vec<u8>) -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
		use codec::Decode;

		// Check the state of the storage after the migration.
		let prev_map = BTreeMap::<H256, old::CodeInfo<T>>::decode(&mut &prev[..])
			.expect("Failed to decode the previous storage state");

		// Check the len of prev and post are the same.
		assert_eq!(
			crate::CodeInfoOf::<T>::iter().count(),
			prev_map.len(),
			"Migration failed: the number of items in the storage after the migration is not the same as before"
		);

		for (key, value) in prev_map {
			let new_value = new::CodeInfoOf::<T>::get(key)
				.expect("Failed to get the value after the migration");

			let expected = new::CodeInfo {
				deposit: value.deposit,
				code_len: value.code_len,
				bytecode_info: new::BytecodeInfo::Pvm {
					owner: value.owner,
					refcount: value.refcount,
				},
				behaviour_version: value.behaviour_version,
			};

			assert_eq!(new_value, expected, "Migration failed: CodeInfo mismatch for key {key:?}");
		}

		Ok(())
	}
}

#[cfg(any(feature = "runtime-benchmarks", test))]
impl<T: Config> Migration<T> {
	/// Insert an old CodeInfo for benchmarking purposes.
	pub fn insert_old_code_info(code_hash: H256, code_info: old::CodeInfo<T>) {
		old::CodeInfoOf::<T>::insert(code_hash, code_info);
	}

	/// Create an old CodeInfo struct for benchmarking.
	pub fn create_old_code_info(
		owner: crate::AccountIdOf<T>,
		deposit: crate::BalanceOf<T>,
		refcount: u64,
		code_len: u32,
		behaviour_version: u32,
	) -> old::CodeInfo<T> {
		old::CodeInfo { owner, deposit, refcount, code_len, behaviour_version }
	}

	/// Assert that the migrated CodeInfo matches the expected values from the old CodeInfo.
	pub fn assert_migrated_code_info_matches(code_hash: H256, old_code_info: &old::CodeInfo<T>) {
		let migrated =
			new::CodeInfoOf::<T>::get(code_hash).expect("Failed to get migrated CodeInfo");

		let expected = new::CodeInfo {
			deposit: old_code_info.deposit,
			code_len: old_code_info.code_len,
			bytecode_info: new::BytecodeInfo::Pvm {
				owner: old_code_info.owner.clone(),
				refcount: old_code_info.refcount,
			},
			behaviour_version: old_code_info.behaviour_version,
		};

		if migrated != expected {
			panic!("Migration failed: deposit mismatch for key {code_hash:?}",);
		}
	}
}

#[test]
fn migrate_to_v2() {
	use crate::{
		tests::{ExtBuilder, Test},
		AccountIdOf,
	};
	use alloc::collections::BTreeMap;

	ExtBuilder::default().build().execute_with(|| {
		// Store the original values to verify against later
		let mut original_values = BTreeMap::new();

		for i in 0..10u8 {
			let code_hash = H256::from([i; 32]);
			let old_info = Migration::<Test>::create_old_code_info(
				AccountIdOf::<Test>::from([i; 32]),
				(1000u32 + i as u32).into(),
				1 + i as u64,
				100 + i as u32,
				i as u32,
			);

			Migration::<Test>::insert_old_code_info(code_hash, old_info.clone());
			original_values.insert(code_hash, old_info);
		}

		let mut cursor = None;
		let mut weight_meter = WeightMeter::new();
		while let Some(new_cursor) = Migration::<Test>::step(cursor, &mut weight_meter).unwrap() {
			cursor = Some(new_cursor);
		}

		assert_eq!(crate::CodeInfoOf::<Test>::iter().count(), 10);

		// Verify all values match between old and new with code_type set to PVM
		for (code_hash, old_value) in original_values {
			Migration::<Test>::assert_migrated_code_info_matches(code_hash, &old_value);
		}
	})
}
