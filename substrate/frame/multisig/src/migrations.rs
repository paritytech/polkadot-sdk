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
				return T::DbWeight::get().reads(1)
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

// pub mod v2 {
//     use super::*;
//     use frame::prelude::*;
//     use sp_runtime::VersionedCall;

//     #[frame::storage_alias]
//     pub type Calls<T: Config> = StorageDoubleMap<
//         Pallet<T>,
//         Twox64Concat,
//         <T as frame_system::Config>::AccountId,
//         Blake2_128Concat,
//         [u8; 32],
//         <T as Config>::RuntimeCall,
//     >;

//     /// Migrate the multisig pallet to use VersionedCall
//     pub struct MigrateToVersionedCall<T>(core::marker::PhantomData<T>);

//     impl<T: Config> OnRuntimeUpgrade for MigrateToVersionedCall<T> {
//         #[cfg(feature = "try-runtime")]
//         fn pre_upgrade() -> Result<Vec<u8>, frame::try_runtime::TryRuntimeError> {
//             let call_count = Calls::<T>::iter().count();
//             log!(info, "Migrating {} stored calls to VersionedCall", call_count);

//             // Check that all calls can be converted
//             for (account, call_hash, call) in Calls::<T>::iter() {
//                 let encoded = call.encode();
//                 // Just check encoding/decoding
//                 if let Err(e) = <T as Config>::RuntimeCall::decode(&mut &encoded[..]) {
//                     log!(error, "Failed to decode call for account {:?}: {:?}", account, e);
//                     return Err("Cannot decode stored call".into());
//                 }
//             }

//             Ok((call_count as u32).encode())
//         }

//         fn on_runtime_upgrade() -> Weight {
//             let current_version =
// frame_system::Pallet::<T>::runtime_version().transaction_version;             let mut migrated =
// 0u32;             let mut failed = 0u32;

//             // Migrate all stored calls to VersionedCall
//             Calls::<T>::translate::<<T as Config>::RuntimeCall, _>(
//                 |account, call_hash, call| {
//                     let versioned_call = VersionedCall::new(call, current_version);
//                     migrated += 1;
//                     Some(versioned_call)
//                 }
//             );

//             // If there were any entries, we need to update storage version
//             if migrated > 0 {
//                 // Update storage version to 2 (if we're creating a new version)
//                 // Note: Currently the pallet is at version 1
//                 StorageVersion::new(2).put::<Pallet<T>>();
//             }

//             log!(info, "Migrated {} calls to VersionedCall ({} failed)", migrated, failed);

//             T::DbWeight::get().reads_writes(migrated as u64 + 1, migrated as u64 + 1)
//         }

//         #[cfg(feature = "try-runtime")]
//         fn post_upgrade(state: Vec<u8>) -> Result<(), frame::try_runtime::TryRuntimeError> {
//             let old_count: u32 = Decode::decode(&mut &state[..]).unwrap_or(0);
//             let new_count = crate::Calls::<T>::iter().count() as u32;

//             ensure!(
//                 old_count == new_count,
//                 "Call count mismatch after migration: before={}, after={}",
//                 old_count,
//                 new_count
//             );

//             // Verify all calls are now VersionedCall
//             for (account, call_hash, versioned_call) in crate::Calls::<T>::iter() {
//                 let current_version =
// frame_system::Pallet::<T>::runtime_version().transaction_version;                 if let Err(e) =
// versioned_call.validate_version(current_version) {                     log!(error, "Versioned
// call validation failed: {:?}", e);                     // This is OK - it means the call was
// stored with a different version                     // and will fail validation when executed
//                 }
//             }

//             Ok(())
//         }
//     }
// }
