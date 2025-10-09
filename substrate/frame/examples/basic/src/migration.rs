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

use super::{BalanceOf, LOG_TARGET, *};
use alloc::vec::Vec;
use frame_support::{
	pallet_prelude::ValueQuery,
	storage_alias,
	traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
	weights::Weight,
};

#[cfg(feature = "try-runtime")]
use frame_support::ensure;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

mod v0 {
	use super::*;

	#[storage_alias]
	pub type Dummy<T: Config> = StorageValue<Pallet<T>, BalanceOf<T>>;
}

pub mod v1 {
	use frame_support::traits::StorageVersion;

	use super::*;

	pub struct MigrateToV1<T>(core::marker::PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for MigrateToV1<T> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			log::info!(
				target: LOG_TARGET,
				"Current balance: {}",
				v0::Dummy::<T>::get()
			);

			Ok(Vec::new())
		}

		fn on_runtime_upgrade() -> Weight {
			if Pallet::<T>::on_chain_storage_version() > 0 {
				log::info!(target: LOG_TARGET, "pallet_example_basic::MigrateToV1 should be removed");
				return T::DbWeight::get().reads(1)
			}

			v0::Dummy::<T>::kill();
			StorageVersion::new(1).put::<Pallet<T>>();

			// + 1 for reading/writing the new storage version
			T::DbWeight::get().reads_writes(1, 1)
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
			let onchain = Pallet::<T>::on_chain_storage_version();
			ensure!(onchain == 1, "pallet_offences::MigrateToV1 needs to be run");
			ensure!(
				v0::Dummy::<T>::get() == 0,
				"there are some dangling reports that need to be destroyed and refunded"
			);
			Ok(())
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::tests::{new_test_ext, Test as T};

	#[test]
	fn migration_to_v1_works() {
		let mut ext = new_test_ext();

		ext.execute_with(|| {
			<v0::Dummy<T>>::put(100);
		});

		ext.commit_all().unwrap();

		ext.execute_with(|| {
			assert_eq!(
				v1::MigrateToV1::<T>::on_runtime_upgrade(),
				<T as frame_system::Config>::DbWeight::get().reads_writes(2, 2),
			);

			assert!(<v0::Dummy<T>>::get() == None);
		})
	}
}
