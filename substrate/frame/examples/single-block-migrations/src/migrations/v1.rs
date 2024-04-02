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

use frame_support::{
	storage_alias,
	traits::{Get, UncheckedOnRuntimeUpgrade},
};

#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

/// Collection of storage item formats from the previous storage version.
///
/// Required so we can read values in the v0 storage format during the migration.
mod v0 {
	use super::*;

	/// V0 type for [`crate::Value`].
	#[storage_alias]
	pub type Value<T: crate::Config> = StorageValue<crate::Pallet<T>, u32>;
}

/// Implements [`UncheckedOnRuntimeUpgrade`], migrating the state of this pallet from V0 to V1.
///
/// In V0 of the template [`crate::Value`] is just a `u32`. In V1, it has been upgraded to
/// contain the struct [`crate::CurrentAndPreviousValue`].
///
/// In this migration, update the on-chain storage for the pallet to reflect the new storage
/// layout.
pub struct InnerMigrateV0ToV1<T: crate::Config>(sp_std::marker::PhantomData<T>);

impl<T: crate::Config> UncheckedOnRuntimeUpgrade for InnerMigrateV0ToV1<T> {
	/// Return the existing [`crate::Value`] so we can check that it was correctly set in
	/// `InnerMigrateV0ToV1::post_upgrade`.
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		use codec::Encode;

		// Access the old value using the `storage_alias` type
		let old_value = v0::Value::<T>::get();
		// Return it as an encoded `Vec<u8>`
		Ok(old_value.encode())
	}

	/// Migrate the storage from V0 to V1.
	///
	/// - If the value doesn't exist, there is nothing to do.
	/// - If the value exists, it is read and then written back to storage inside a
	/// [`crate::CurrentAndPreviousValue`].
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// Read the old value from storage
		if let Some(old_value) = v0::Value::<T>::take() {
			// Write the new value to storage
			let new = crate::CurrentAndPreviousValue { current: old_value, previous: None };
			crate::Value::<T>::put(new);
			// One read for the old value, one write for the new value
			T::DbWeight::get().reads_writes(1, 1)
		} else {
			// One read for trying to access the old value
			T::DbWeight::get().reads(1)
		}
	}

	/// Verifies the storage was migrated correctly.
	///
	/// - If there was no old value, the new value should not be set.
	/// - If there was an old value, the new value should be a [`crate::CurrentAndPreviousValue`].
	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use codec::Decode;
		use frame_support::ensure;

		let maybe_old_value = Option::<u32>::decode(&mut &state[..]).map_err(|_| {
			sp_runtime::TryRuntimeError::Other("Failed to decode old value from storage")
		})?;

		match maybe_old_value {
			Some(old_value) => {
				let expected_new_value =
					crate::CurrentAndPreviousValue { current: old_value, previous: None };
				let actual_new_value = crate::Value::<T>::get();

				ensure!(actual_new_value.is_some(), "New value not set");
				ensure!(
					actual_new_value == Some(expected_new_value),
					"New value not set correctly"
				);
			},
			None => {
				ensure!(crate::Value::<T>::get().is_none(), "New value unexpectedly set");
			},
		};
		Ok(())
	}
}

/// [`UncheckedOnRuntimeUpgrade`] implementation [`InnerMigrateV0ToV1`] wrapped in a
/// [`VersionedMigration`](frame_support::migrations::VersionedMigration), which ensures that:
/// - The migration only runs once when the on-chain storage version is 0
/// - The on-chain storage version is updated to `1` after the migration executes
/// - Reads/Writes from checking/settings the on-chain storage version are accounted for
pub type MigrateV0ToV1<T> = frame_support::migrations::VersionedMigration<
	0, // The migration will only execute when the on-chain storage version is 0
	1, // The on-chain storage version will be set to 1 after the migration is complete
	InnerMigrateV0ToV1<T>,
	crate::pallet::Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

/// Tests for our migration.
///
/// When writing migration tests, it is important to check:
/// 1. `on_runtime_upgrade` returns the expected weight
/// 2. `post_upgrade` succeeds when given the bytes returned by `pre_upgrade`
/// 3. The storage is in the expected state after the migration
#[cfg(any(all(feature = "try-runtime", test), doc))]
mod test {
	use self::InnerMigrateV0ToV1;
	use super::*;
	use crate::mock::{new_test_ext, MockRuntime};
	use frame_support::assert_ok;

	#[test]
	fn handles_no_existing_value() {
		new_test_ext().execute_with(|| {
			// By default, no value should be set. Verify this assumption.
			assert!(crate::Value::<MockRuntime>::get().is_none());
			assert!(v0::Value::<MockRuntime>::get().is_none());

			// Get the pre_upgrade bytes
			let bytes = match InnerMigrateV0ToV1::<MockRuntime>::pre_upgrade() {
				Ok(bytes) => bytes,
				Err(e) => panic!("pre_upgrade failed: {:?}", e),
			};

			// Execute the migration
			let weight = InnerMigrateV0ToV1::<MockRuntime>::on_runtime_upgrade();

			// Verify post_upgrade succeeds
			assert_ok!(InnerMigrateV0ToV1::<MockRuntime>::post_upgrade(bytes));

			// The weight should be just 1 read for trying to access the old value.
			assert_eq!(weight, <MockRuntime as frame_system::Config>::DbWeight::get().reads(1));

			// After the migration, no value should have been set.
			assert!(crate::Value::<MockRuntime>::get().is_none());
		})
	}

	#[test]
	fn handles_existing_value() {
		new_test_ext().execute_with(|| {
			// Set up an initial value
			let initial_value = 42;
			v0::Value::<MockRuntime>::put(initial_value);

			// Get the pre_upgrade bytes
			let bytes = match InnerMigrateV0ToV1::<MockRuntime>::pre_upgrade() {
				Ok(bytes) => bytes,
				Err(e) => panic!("pre_upgrade failed: {:?}", e),
			};

			// Execute the migration
			let weight = InnerMigrateV0ToV1::<MockRuntime>::on_runtime_upgrade();

			// Verify post_upgrade succeeds
			assert_ok!(InnerMigrateV0ToV1::<MockRuntime>::post_upgrade(bytes));

			// The weight used should be 1 read for the old value, and 1 write for the new
			// value.
			assert_eq!(
				weight,
				<MockRuntime as frame_system::Config>::DbWeight::get().reads_writes(1, 1)
			);

			// After the migration, the new value should be set as the `current` value.
			let expected_new_value =
				crate::CurrentAndPreviousValue { current: initial_value, previous: None };
			assert_eq!(crate::Value::<MockRuntime>::get(), Some(expected_new_value));
		})
	}
}
