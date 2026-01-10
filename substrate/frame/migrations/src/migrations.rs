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

//! Generic multi block migrations not specific to any pallet.

use crate::{weights::WeightInfo, Config};
use alloc::vec::Vec;
use codec::Encode;
use core::marker::PhantomData;
use frame_support::{
	migrations::{SteppedMigration, SteppedMigrationError, StoreInCodeStorageVersion},
	traits::{GetStorageVersion, PalletInfoAccess},
	weights::WeightMeter,
};
use sp_core::{twox_128, Get};
use sp_io::{storage::clear_prefix, KillStorageResult};
use sp_runtime::{traits::Saturating, SaturatedConversion};

/// Remove all of a pallet's state and re-initializes it to the current in-code storage version.
///
/// It uses the multi block migration frame. Hence it is safe to use even on
/// pallets that contain a lot of storage.
///
/// # Parameters
///
/// - T: The runtime. Used to access the weight definition.
/// - P: The pallet to resetted as defined in construct runtime
///
/// # Note
///
/// If your pallet does rely of some state in genesis you need to take care of that
/// separately. This migration only sets the storage version after wiping.
pub struct ResetPallet<T, P>(PhantomData<(T, P)>);

impl<T, P> ResetPallet<T, P>
where
	P: PalletInfoAccess,
{
	#[cfg(feature = "try-runtime")]
	fn num_keys() -> u64 {
		let prefix = P::name_hash().to_vec();
		crate::storage::KeyPrefixIterator::new(prefix.clone(), prefix, |_| Ok(())).count() as _
	}
}

impl<T, P, V> SteppedMigration for ResetPallet<T, P>
where
	T: Config,
	P: PalletInfoAccess + GetStorageVersion<InCodeStorageVersion = V>,
	V: StoreInCodeStorageVersion<P>,
{
	type Cursor = bool;
	type Identifier = [u8; 16];

	fn id() -> Self::Identifier {
		("RemovePallet::", P::name()).using_encoded(twox_128)
	}

	fn step(
		cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		// we write the storage version in a separate block
		if cursor.unwrap_or(false) {
			let required = T::DbWeight::get().writes(1);
			meter
				.try_consume(required)
				.map_err(|_| SteppedMigrationError::InsufficientWeight { required })?;
			V::store_in_code_storage_version();
			return Ok(None);
		}

		let base_weight = T::WeightInfo::reset_pallet_migration(0);
		let weight_per_key = T::WeightInfo::reset_pallet_migration(1).saturating_sub(base_weight);
		let key_budget = meter
			.remaining()
			.saturating_sub(base_weight)
			.checked_div_per_component(&weight_per_key)
			.unwrap_or_default()
			.saturated_into();

		if key_budget == 0 {
			return Err(SteppedMigrationError::InsufficientWeight {
				required: T::WeightInfo::reset_pallet_migration(1),
			});
		}

		let (keys_removed, is_done) = match clear_prefix(&P::name_hash(), Some(key_budget)) {
			KillStorageResult::AllRemoved(value) => (value, true),
			KillStorageResult::SomeRemaining(value) => (value, false),
		};

		meter.consume(T::WeightInfo::reset_pallet_migration(keys_removed));

		Ok(Some(is_done))
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
		let num_keys: u64 = Self::num_keys();
		log::info!("ResetPallet<{}>: Trying to remove {num_keys} keys.", P::name());
		Ok(num_keys.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use codec::Decode;
		let keys_before = u64::decode(&mut state.as_ref()).expect("We encoded as u64 above; qed");
		let keys_now = Self::num_keys();
		log::info!("ResetPallet<{}>: Keys remaining after migration: {keys_now}", P::name());

		if keys_before <= keys_now {
			log::error!("ResetPallet<{}>: Did not remove any keys.", P::name());
			Err("ResetPallet failed")?;
		}

		if keys_now != 1 {
			log::error!("ResetPallet<{}>: Should have a single key after reset", P::name());
			Err("ResetPallet failed")?;
		}

		Ok(())
	}
}

/// Clear storage items for a specific pallet storage or all pallet storage.
///
/// This migration removes all storage entries for a given pallet and optionally a specific
/// storage name. It uses the multi-block migration framework, making it safe to use even when
/// clearing large amounts of storage data.
///
/// This is the recommended replacement for the deprecated
/// [`frame_support::migrations::RemoveStorage`].
///
/// # Parameters
///
/// - `T`: The runtime configuration. Used to access weight definitions and other runtime types.
/// - `Pallet`: A type implementing `Get<&'static str>` that provides the pallet name.
/// - `Storage`: A type implementing `Get<Option<&'static str>>` that provides the storage name.
///   When `None`, all storage items for the pallet will be removed.
///
/// # Example
///
/// Clearing a specific storage item from a pallet:
///
/// ```rust,ignore
/// use frame_support::parameter_types;
///
/// // Define the pallet and storage names to clear
/// parameter_types! {
///     pub const PalletName: &'static str = "MyPallet";
///     pub const StorageName: Option<&'static str> = Some("MyStorage");
/// }
///
/// // Configure the migration
/// pub type MultiBlockMigrations =
///     pallet_migrations::ClearStorage<Runtime, PalletName, StorageName>;
///
/// impl pallet_migrations::Config for Runtime {
///     type Migrations = MultiBlockMigrations;
///     // ... other configuration items
/// }
/// ```
///
/// Clearing all storage items from a pallet:
///
/// ```rust,ignore
/// use frame_support::parameter_types;
///
/// parameter_types! {
///     pub const PalletName: &'static str = "MyPallet";
///     pub const NoStorage: Option<&'static str> = None;
/// }
///
/// // This will remove ALL storage items from MyPallet
/// pub type MultiBlockMigrations =
///     pallet_migrations::ClearStorage<Runtime, PalletName, NoStorage>;
/// ```
///
/// # Notes
///
/// - The migration processes keys one at a time based on available weight, preventing block
///   overload and maintaining tight control over proof sizes.
/// - **Accurate proof size tracking**: Before deleting each key, the migration reads the value size
///   and calculates the actual proof size (key length + value length). This ensures we don't exceed
///   weight limits even with large values.
/// - When `Storage` returns `None`, all storage for the pallet is cleared (similar to
///   [`ResetPallet`] but without updating the storage version).
pub struct ClearStorage<T, Pallet, Storage>(PhantomData<(T, Pallet, Storage)>);

impl<T, Pallet, Storage> ClearStorage<T, Pallet, Storage>
where
	Pallet: Get<&'static str>,
	Storage: Get<Option<&'static str>>,
{
	fn storage_prefix() -> Vec<u8> {
		match Storage::get() {
			Some(storage) =>
				frame_support::storage::storage_prefix(Pallet::get().as_bytes(), storage.as_bytes())
					.to_vec(),
			None => twox_128(Pallet::get().as_bytes()).to_vec(),
		}
	}

	/// Get the next key that starts with the storage prefix.
	///
	/// Returns `None` if there are no keys with the prefix.
	fn next_key() -> Option<Vec<u8>> {
		let prefix = Self::storage_prefix();
		// Get the next key after the prefix if it starts with the prefix
		sp_io::storage::next_key(&prefix).filter(|key| key.starts_with(&prefix))
	}

	#[cfg(feature = "try-runtime")]
	fn num_keys() -> u64 {
		let storage_prefix = Self::storage_prefix();
		frame_support::storage::KeyPrefixIterator::new(
			storage_prefix.clone(),
			storage_prefix,
			|_| Ok(()),
		)
		.count() as _
	}
}

impl<T, Pallet, Storage> SteppedMigration for ClearStorage<T, Pallet, Storage>
where
	T: Config,
	Pallet: Get<&'static str>,
	Storage: Get<Option<&'static str>>,
{
	type Cursor = ();
	type Identifier = [u8; 16];

	fn id() -> Self::Identifier {
		("ClearStorage", Pallet::get(), Storage::get()).using_encoded(twox_128)
	}

	fn step(
		_cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		use frame_support::weights::Weight;

		// Get the benchmark weight for estimation
		let estimated_weight_per_key = T::WeightInfo::reset_pallet_migration(1);

		// Ensure there is enough weight to delete at least one key (using the benchmark weight)
		if !meter.can_consume(estimated_weight_per_key) {
			return Err(SteppedMigrationError::InsufficientWeight {
				required: estimated_weight_per_key,
			});
		}

		let mut keys_removed: u32 = 0;
		// Track the weight required if the key couldn't be processed due to insufficient weight
		let mut insufficient_weight: Option<Weight> = None;

		// Delete keys one by one while there is weight budget
		while meter.can_consume(estimated_weight_per_key) {
			match Self::next_key() {
				Some(key) => {
					// Get the value size without loading the entire value
					let value_len = sp_io::storage::read(&key, &mut [], 0).unwrap_or(0) as u64;

					// Calculate actual proof size: key length + value length
					let actual_proof_size = (key.len() as u64).saturating_add(value_len);
					let actual_weight =
						Weight::from_parts(estimated_weight_per_key.ref_time(), actual_proof_size);

					// Check if there is enough weight budget for this key actual weight
					if !meter.can_consume(actual_weight) {
						log::debug!(
							"ClearStorage<{}, {:?}>: Key with proof_size {} exceeds remaining \
							 weight, pausing after {} keys.",
							Pallet::get(),
							Storage::get(),
							actual_proof_size,
							keys_removed
						);
						insufficient_weight = Some(actual_weight);
						break;
					}

					// Delete the key
					sp_io::storage::clear(&key);

					// Consume weight with actual proof size
					meter.consume(actual_weight);

					keys_removed.saturating_inc();
				},
				None => {
					// No more keys with this prefix - migration complete
					log::info!(
						"ClearStorage<{}, {:?}>: Migration complete, removed {} keys in this step.",
						Pallet::get(),
						Storage::get(),
						keys_removed
					);
					return Ok(None);
				},
			}
		}

		// Couldn't process any key, return an error with the required weight
		if keys_removed == 0 {
			if let Some(required) = insufficient_weight {
				return Err(SteppedMigrationError::InsufficientWeight { required });
			}
		}

		log::debug!(
			"ClearStorage<{}, {:?}>: Removed {} keys in this step, continuing...",
			Pallet::get(),
			Storage::get(),
			keys_removed
		);

		// Migration still in progress
		Ok(Some(()))
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
		let num_keys: u64 = Self::num_keys();
		log::info!(
			"ClearStorage<{}, {:?}>: Trying to remove {num_keys} keys.",
			Pallet::get(),
			Storage::get()
		);
		Ok(num_keys.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use codec::Decode;
		let keys_before = u64::decode(&mut state.as_ref()).expect("We encoded as u64 above; qed");
		let keys_now = Self::num_keys();
		log::info!(
			"ClearStorage<{}, {:?}>: Keys remaining after migration: {keys_now}",
			Pallet::get(),
			Storage::get()
		);

		if keys_before <= keys_now {
			log::error!(
				"ClearStorage<{}, {:?}>: Did not remove any keys.",
				Pallet::get(),
				Storage::get()
			);
			Err("ClearStorage failed")?;
		}

		Ok(())
	}
}
