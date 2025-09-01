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
//! - migrate the old `CodeInfoOf` storage to the new `CodeInfoOf` which add the new `code_type`
//! field.
//! - Unhold the deposit on the owner and transfer it to the pallet account.

extern crate alloc;
use super::PALLET_MIGRATIONS_ID;
use crate::{vm::BytecodeType, weights::WeightInfo, Config, Pallet, H256, LOG_TARGET};
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::PhantomData,
	traits::{
		fungible::{Inspect, Mutate, MutateHold},
		tokens::{Fortitude, Precision, Restriction},
	},
	weights::WeightMeter,
};

#[cfg(feature = "try-runtime")]
use alloc::{collections::btree_map::BTreeMap, vec::Vec};

#[cfg(feature = "try-runtime")]
use frame_support::{sp_runtime::TryRuntimeError, traits::fungible::InspectHold};

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
	use super::{BytecodeType, Config};
	use crate::{pallet::Pallet, AccountIdOf, BalanceOf, H256};
	use codec::{Decode, Encode};
	use frame_support::{storage_alias, DebugNoBound, Identity};

	#[derive(PartialEq, Eq, DebugNoBound, Encode, Decode)]
	pub struct CodeInfo<T: Config> {
		pub owner: AccountIdOf<T>,
		#[codec(compact)]
		pub deposit: BalanceOf<T>,
		#[codec(compact)]
		pub refcount: u64,
		pub code_len: u32,
		pub code_type: BytecodeType,
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

		if !frame_system::Pallet::<T>::account_exists(&Pallet::<T>::account_id()) {
			let _ =
				T::Currency::mint_into(&Pallet::<T>::account_id(), T::Currency::minimum_balance());
		}

		loop {
			if meter.try_consume(required).is_err() {
				break;
			}

			let mut iter = if let Some(last_key) = cursor {
				old::CodeInfoOf::<T>::iter_from(old::CodeInfoOf::<T>::hashed_key_for(last_key))
			} else {
				old::CodeInfoOf::<T>::iter()
			};

			if let Some((last_key, value)) = iter.next() {
				if let Err(err) = T::Currency::transfer_on_hold(
					&crate::HoldReason::CodeUploadDepositReserve.into(),
					&value.owner,
					&Pallet::<T>::account_id(),
					value.deposit,
					Precision::Exact,
					Restriction::OnHold,
					Fortitude::Polite,
				) {
					log::error!(
						target: LOG_TARGET,
						"Failed to unhold the deposit for code hash {last_key:?} and owner {:?}: {err:?}",
						value.owner,
					);
				}

				new::CodeInfoOf::<T>::insert(
					last_key,
					new::CodeInfo {
						owner: value.owner,
						deposit: value.deposit,
						refcount: value.refcount,
						code_len: value.code_len,
						code_type: BytecodeType::Pvm,
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
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		use codec::Encode;

		// Return the state of the storage before the migration.
		Ok(old::CodeInfoOf::<T>::iter().collect::<BTreeMap<_, _>>().encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(prev: Vec<u8>) -> Result<(), TryRuntimeError> {
		use codec::Decode;
		use sp_runtime::{traits::Zero, Saturating};

		// Check the state of the storage after the migration.
		let prev_map = BTreeMap::<H256, old::CodeInfo<T>>::decode(&mut &prev[..])
			.expect("Failed to decode the previous storage state");

		// Check the len of prev and post are the same.
		assert_eq!(
			crate::CodeInfoOf::<T>::iter().count(),
			prev_map.len(),
			"Migration failed: the number of items in the storage after the migration is not the same as before"
		);

		let deposit_sum: crate::BalanceOf<T> = Zero::zero();

		for (code_hash, old_code_info) in prev_map {
			deposit_sum.saturating_add(old_code_info.deposit);
			Self::assert_migrated_code_info(code_hash, &old_code_info);
		}

		assert_eq!(
			<T as Config>::Currency::balance_on_hold(
				&crate::HoldReason::CodeUploadDepositReserve.into(),
				&Pallet::<T>::account_id(),
			),
			deposit_sum,
		);

		Ok(())
	}
}

#[cfg(any(feature = "runtime-benchmarks", feature = "try-runtime", test))]
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
		use frame_support::traits::fungible::Mutate;
		T::Currency::mint_into(&owner, Pallet::<T>::min_balance() + deposit)
			.expect("Failed to mint into owner account");
		T::Currency::hold(&crate::HoldReason::CodeUploadDepositReserve.into(), &owner, deposit)
			.expect("Failed to hold the deposit on the owner account");

		old::CodeInfo { owner, deposit, refcount, code_len, behaviour_version }
	}

	/// Assert that the migrated CodeInfo matches the expected values from the old CodeInfo.
	pub fn assert_migrated_code_info(code_hash: H256, old_code_info: &old::CodeInfo<T>) {
		use frame_support::traits::fungible::InspectHold;
		use sp_runtime::traits::Zero;
		let migrated =
			new::CodeInfoOf::<T>::get(code_hash).expect("Failed to get migrated CodeInfo");

		assert!(<T as Config>::Currency::balance_on_hold(
			&crate::HoldReason::CodeUploadDepositReserve.into(),
			&old_code_info.owner
		)
		.is_zero());

		assert_eq!(
			migrated,
			new::CodeInfo {
				owner: old_code_info.owner.clone(),
				deposit: old_code_info.deposit,
				refcount: old_code_info.refcount,
				code_len: old_code_info.code_len,
				behaviour_version: old_code_info.behaviour_version,
				code_type: BytecodeType::Pvm,
			},
			"Migration failed: deposit mismatch for key {code_hash:?}",
		);
	}
}

#[test]
fn migrate_to_v2() {
	use crate::{
		tests::{ExtBuilder, Test},
		AccountIdOf,
	};
	use alloc::collections::BTreeMap;

	ExtBuilder::default().genesis_config(None).build().execute_with(|| {
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
			Migration::<Test>::assert_migrated_code_info(code_hash, &old_value);
		}
	})
}
