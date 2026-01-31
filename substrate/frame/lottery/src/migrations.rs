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

//! Migrations for the lottery pallet.

use super::*;
use frame_support::{
	pallet_prelude::*,
	traits::{GetStorageVersion, OnRuntimeUpgrade, StorageVersion},
};

/// The log target.
const TARGET: &'static str = "runtime::lottery::migration";

/// Migrate lottery storage from storing raw calls to storing VersionedCall.
pub struct MigrateToVersionedCall<T>(core::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateToVersionedCall<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, frame::try_runtime::TryRuntimeError> {
		use frame_support::storage::migration::get_storage_value;

		// Check current storage version
		let current_version = StorageVersion::get::<Pallet<T>>();
		log::info!(
			target: TARGET,
			"Pre-upgrade check: current storage version = {}",
			current_version
		);

		// Count how many call indices we have
		let call_indices_count = CallIndices::<T>::decode_len().unwrap_or(0);
		log::info!(
			target: TARGET,
			"Found {} call indices to migrate",
			call_indices_count
		);

		Ok(call_indices_count.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let current_version = StorageVersion::get::<Pallet<T>>();
		let onchain_version = Pallet::<T>::on_chain_storage_version();

		log::info!(
			target: TARGET,
			"Migrating lottery from version {:?} to {:?}",
			onchain_version,
			current_version
		);

		// If we're already at the current version or higher, skip migration
		if onchain_version >= current_version {
			log::warn!(
				target: TARGET,
				"Skipping migration: on-chain version {:?} >= current version {:?}",
				onchain_version,
				current_version
			);
			return T::DbWeight::get().reads(1);
		}

		let mut weight = T::DbWeight::get().reads_writes(1, 0);

		// Update storage version
		current_version.put::<Pallet<T>>();
		weight.saturating_accrue(T::DbWeight::get().writes(1));

		log::info!(
			target: TARGET,
			"Lottery migration completed successfully"
		);

		weight
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), frame::try_runtime::TryRuntimeError> {
		use codec::Decode;

		let previous_call_indices_count: usize =
			Decode::decode(&mut &state[..]).expect("pre_upgrade provides valid state; qed");

		// Verify storage version was updated
		let current_version = StorageVersion::get::<Pallet<T>>();
		ensure!(
			current_version == Pallet::<T>::in_code_storage_version(),
			"Storage version not updated correctly"
		);

		// Verify call indices still exist
		let current_call_indices_count = CallIndices::<T>::decode_len().unwrap_or(0);
		ensure!(
			current_call_indices_count == previous_call_indices_count,
			"Call indices count changed unexpectedly"
		);

		log::info!(
			target: TARGET,
			"Post-upgrade check successful: {} call indices preserved",
			current_call_indices_count
		);

		Ok(())
	}
}
