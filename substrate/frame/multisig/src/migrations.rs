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

// Migrations for Multisig Pallet

use crate::*;
use frame::prelude::*;

pub mod v1 {
	use super::*;

	type OpaqueCall<T> = frame::traits::WrapperKeepOpaque<<T as Config>::RuntimeCall>;

	#[frame::storage_alias]
	type Calls<T: Config> = StorageMap<
		Pallet<T>,
		Identity,
		[u8; 32],
		(OpaqueCall<T>, <T as frame_system::Config>::AccountId, BalanceOf<T>),
	>;

	pub struct MigrateToV1<T>(core::marker::PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for MigrateToV1<T> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, frame::try_runtime::TryRuntimeError> {
			log!(info, "Number of calls to refund and delete: {}", Calls::<T>::iter().count());

			Ok(Vec::new())
		}

		fn on_runtime_upgrade() -> Weight {
			use frame::traits::ReservableCurrency as _;
			let current = Pallet::<T>::in_code_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			if onchain > 0 {
				log!(info, "MigrateToV1 should be removed");
				return T::DbWeight::get().reads(1);
			}

			let mut call_count = 0u64;
			Calls::<T>::drain().for_each(|(_call_hash, (_data, caller, deposit))| {
				T::Currency::unreserve(&caller, deposit);
				call_count.saturating_inc();
			});

			current.put::<Pallet<T>>();

			T::DbWeight::get().reads_writes(
				// Reads: Get Calls + Get Version
				call_count.saturating_add(1),
				// Writes: Drain Calls + Unreserves + Set version
				call_count.saturating_mul(2).saturating_add(1),
			)
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), frame::try_runtime::TryRuntimeError> {
			ensure!(
				Calls::<T>::iter().count() == 0,
				"there are some dangling calls that need to be destroyed and refunded"
			);
			Ok(())
		}
	}
}

pub mod v2 {
	use super::*;
	use sp_runtime::VersionedCall;

	#[frame::storage_alias]
	type OldCalls<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Twox64Concat,
		<T as frame_system::Config>::AccountId,
		Blake2_128Concat,
		[u8; 32],
		<T as Config>::RuntimeCall,
	>;

	pub struct MigrateToVersionedCall<T>(core::marker::PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for MigrateToVersionedCall<T> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, frame::try_runtime::TryRuntimeError> {
			use codec::Encode;

			let version = Pallet::<T>::on_chain_storage_version();
			ensure!(version == 1, "Can only migrate from version 1");

			let call_count = OldCalls::<T>::iter().count() as u32;
			log::info!(
				target: crate::LOG_TARGET,
				"Preparing to migrate {} stored calls to VersionedCall format",
				call_count
			);

			Ok(call_count.encode())
		}

		fn on_runtime_upgrade() -> Weight {
			let version = Pallet::<T>::on_chain_storage_version();
			let current_version = Pallet::<T>::in_code_storage_version();

			if version != 1 {
				log::warn!(
					target: crate::LOG_TARGET,
					"Skipping migration: expected version 1, found {:?}",
					version
				);
				return T::DbWeight::get().reads(1);
			}

			let mut migrated = 0u32;
			let mut weight = T::DbWeight::get().reads_writes(1, 0);

			// Get current transaction version
			let current_tx_version =
				<frame_system::Pallet<T>>::runtime_version().transaction_version;

			// Migrate all stored calls to VersionedCall format
			for (account, call_hash, old_call) in OldCalls::<T>::drain() {
				// Wrap the old call in VersionedCall with current version
				let versioned_call = VersionedCall::new(old_call, current_tx_version);

				// Store the versioned call
				crate::Calls::<T>::insert(&account, call_hash, versioned_call);

				migrated += 1;
				weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
			}

			// Update storage version
			current_version.put::<Pallet<T>>();
			weight.saturating_accrue(T::DbWeight::get().writes(1));

			log::info!(
				target: crate::LOG_TARGET,
				"Migrated {} calls to VersionedCall format",
				migrated
			);

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), frame::try_runtime::TryRuntimeError> {
			use codec::Decode;

			let expected_migrated: u32 =
				Decode::decode(&mut &state[..]).expect("pre_upgrade provides valid state; qed");

			// Verify storage version was updated
			ensure!(
				Pallet::<T>::on_chain_storage_version() == 2,
				"Storage version not updated correctly"
			);

			// Verify all calls were migrated
			let current_call_count = crate::Calls::<T>::iter().count() as u32;
			ensure!(current_call_count == expected_migrated, "Call count mismatch after migration");

			// Verify no old calls remain
			ensure!(OldCalls::<T>::iter().count() == 0, "Old calls still exist after migration");

			log::info!(
				target: crate::LOG_TARGET,
				"Successfully migrated {} calls",
				current_call_count
			);

			Ok(())
		}
	}
}
