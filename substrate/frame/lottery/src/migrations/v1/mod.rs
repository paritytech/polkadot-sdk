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

//! # Single-Block Migration v1
//!
//! This migrate the old `Lottery` storage that used the currency trait to the new `Lottery` that uses the fungible trait.

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
	use crate::{pallet::Pallet, LotteryConfig, H160};
	use frame_support::{storage_alias, Identity};

	#[storage_alias]
	/// The storage item that is being migrated from.
	pub type Lottery<T: Config> =
	StorageValue<Pallet<T>, LotteryConfig<BlockNumberFor<T>, BalanceOf<T>>>;
}

/// Migrates the items of the [`old::Lottery`] map into [`crate::Lottery`].
pub struct InnerMigrateV0ToV1<T: Config>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for InnerMigrateV0ToV1<T> {
	/// Return the existing [`old::Lottery`] so we can check that it was correctly set in
	/// `InnerMigrateV0ToV1::post_upgrade`.
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		use codec::Encode;

		// Access the old value using the `storage_alias` type
		let old_value = old::Lottery::<T>::get();
		// Return it as an encoded `Vec<u8>`
		Ok(old_value.encode())
	}

	/// Migrate the storage from V0 to V1.
	///
	/// - If the value doesn't exist, there is nothing to do.
	/// - If the value exists, it is read and then written back to storage inside a
	///   [`crate::Lottery`].
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// Read the old value from storage
		if let Some(old_value) = old::Lottery::<T>::take() {
			// Write the new value to storage
			let new = crate::Lottery { price: old_value.price, start: old_value.start, length: old_value.length, delay: old_value.delay, repeat: old_value.repeat };
			crate::Lottery::<T>::put(new);
			// One read + write for taking the old value, and one write for setting the new value
			T::DbWeight::get().reads_writes(1, 2)
		} else {
			// No writes since there was no old value, just one read for checking
			T::DbWeight::get().reads(1)
		}
	}

	/// Verifies the storage was migrated correctly.
	///
	/// - If there was no old value, the new value should not be set.
	/// - If there was an old value, the new value should be a [`crate::Lottery`].
	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use codec::Decode;
		use frame_support::ensure;

		let old_value = state.decode::<LotteryConfig<BlockNumberFor<T>, BalanceOf<T>>>()?;
		let new_value = crate::Lottery::<T>::get();
		ensure!(old_value == new_value, "LotteryConfig mismatch");
		Ok(())
	}
}

/// Tests for our migration.
///
/// When writing migration tests, it is important to check:
/// 1. `on_runtime_upgrade` returns the expected weight
/// 2. `post_upgrade` succeeds when given the bytes returned by `pre_upgrade`
/// 3. The storage is in the expected state after the migration
#[cfg(any(all(feature = "try-runtime", test), doc))]
mod test {
	use super::*;
	use crate::mock::{new_test_ext, MockRuntime};
	use frame_support::assert_ok;

	#[test]
	fn handles_no_existing_value() {
		new_test_ext().execute_with(|| {
			assert!(old::Lottery::<MockRuntime>::get().is_none());

			let bytes = match InnerMigrateV0ToV1::<MockRuntime>::pre_upgrade() {
				Ok(bytes) => bytes,
				Err(e) => panic!("pre_upgrade failed: {e:?}"),
			};

			let weight = InnerMigrateV0ToV1::<MockRuntime>::on_runtime_upgrade();

			assert_ok!(InnerMigrateV0ToV1::<MockRuntime>::post_upgrade(bytes));
			
			assert_eq!(weight, <MockRuntime as frame_system::Config>::DbWeight::get().reads(1));
			assert!(old::Lottery::<MockRuntime>::get().is_none());
		})
	}

	#[test]
	fn handles_existing_value() {
		new_test_ext().execute_with(|| {
			let initial_value = LotteryConfig { price: 100, start: 1, length: 100, delay: 100, repeat: true };
			old::Lottery::<MockRuntime>::put(initial_value);

			let bytes = match InnerMigrateV0ToV1::<MockRuntime>::pre_upgrade() {
				Ok(bytes) => bytes,
				Err(e) => panic!("pre_upgrade failed: {e:?}"),
			};
		})

		let weight = InnerMigrateV0ToV1::<MockRuntime>::on_runtime_upgrade();

		assert_ok!(InnerMigrateV0ToV1::<MockRuntime>::post_upgrade(bytes));

		assert_eq!(weight, <MockRuntime as frame_system::Config>::DbWeight::get().reads_writes(1, 2));
		assert!(old::Lottery::<MockRuntime>::get().is_none());
	}
}