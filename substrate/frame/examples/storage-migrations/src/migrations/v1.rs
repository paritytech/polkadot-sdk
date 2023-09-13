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
	traits::{Get, OnRuntimeUpgrade},
};

/// Collection of storage item formats from the previous storage version.
///
/// Required so we can read values in the old storage format during the migration.
pub(crate) mod old {
	use super::*;

	/// V0 type for [`crate::Value`].
	#[storage_alias]
	pub type Value<T: crate::Config> = StorageValue<crate::Pallet<T>, u32>;
}

/// Implements [`OnRuntimeUpgrade`], migrating the state of this pallet from V0 to V1.
///
/// In V0 of the template [`crate::Value`] is just a `u32`.
/// In V1, it has been upgraded to contain the struct [`crate::CurrentAndPreviousValue`].
///
/// In this migration, update the on-chain storage for the pallet to reflect the new storage layout.

pub struct VersionUncheckedV0ToV1<T: crate::Config>(sp_std::marker::PhantomData<T>);

impl<T: crate::Config> OnRuntimeUpgrade for VersionUncheckedV0ToV1<T> {
	/// Return the existing [`crate::Value`] so we can check that it was correctly set in
	/// [`VersionUncheckedV0ToV1::post_upgrade`].
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		use codec::Encode;

		// Access the old value using the `storage_alias` type
		let old_value = old::Value::<T>::get();
		// Return it as an encoded `Vec<u8>`
		Ok(old_value.encode())
	}

	/// Migrate the storage from V0 to V1.
	///
	/// If the value doesn't exist, there is nothing to do.
	///
	/// If the value exists, it is read and then written back to storage inside a
	/// [`crate::SomethingEntry`] with the `maybe_account_id` field set to `None`.
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// Read the old value from storage
		if let Some(old_value) = old::Value::<T>::get() {
			// Write the new value to storage
			let new = crate::CurrentAndPreviousValue { previous: None, current: old_value };
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
	/// If there was no old value, the new value should not be set.
	///
	/// If there was an old value, the new value should be a
	/// [`crate::SomethingEntry`] with the `maybe_account_id` field set to `None`
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
