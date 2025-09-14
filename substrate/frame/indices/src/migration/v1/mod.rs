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

//! # Multi-Block Migration v1

extern crate alloc;

use super::PALLET_MIGRATIONS_ID;
use crate::pallet::Config;
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::PhantomData,
	weights::WeightMeter,
};
// use frame_support::traits::{ReservableCurrency, fungible::MutateHold};

#[cfg(feature = "try-runtime")]
use alloc::collections::btree_map::BTreeMap;

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;

#[cfg(test)]
mod tests;

/// Module containing the OLD (v0) storage items.
///
/// Before running this migration, the storage alias defined here represents the
/// `on_chain` storage.
// This module is public only for the purposes of linking it in the documentation. It is not
// intended to be used by any other code.
pub mod v0 {
    use super::Config;
    use crate::pallet::Pallet;
    use frame_support::{storage_alias, Blake2_128Concat};

    /// The old balance type that used Currency trait instead of Inspect trait.
    type OldBalanceOf<T> = 
        <<T as Config>::Currency as frame_support::traits::Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[storage_alias]
    /// The storage item that is being migrated from.
    /// This represents the Accounts storage as it was in v0, using the old Currency trait.
    pub type Accounts<T: Config> = StorageMap<
        Pallet<T>,
        Blake2_128Concat,
        <T as Config>::AccountIndex,
        (<T as frame_system::Config>::AccountId, OldBalanceOf<T>, bool)
    >;
}

/// Migrates the items of the [`crate::Indices::Currency`] from `reserved` to `holds`.
///
/// The `step` function will be called once per block. It is very important that this function
/// *never* panics and never uses more weight than it got in its meter. The migrations should also
/// try to make maximal progress per step, so that the total time it takes to migrate stays low.
pub struct LazyMigrationV1<T: Config>(PhantomData<T>);

impl<T: Config> Default for LazyMigrationV1<T> {
	fn default() -> Self {
		Self(PhantomData)
	}
}

impl<T: Config> SteppedMigration for LazyMigrationV1<T> {
    type Cursor = u32;
	// Without the explicit length here the construction of the ID would not be infallible.
	type Identifier = MigrationId<18>;

	/// The identifier of this migration. Which should be globally unique.
	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 0, version_to: 1 }
	}

	/// The actual logic of the migration.
	///
	/// This function is called repeatedly until it returns `Ok(None)`, indicating that the
	/// migration is complete. The migration converts from the old reserved system to the new
	/// hold system by unreserving deposits and re-claiming indices with holds.
	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, frame_support::sp_runtime::TryRuntimeError> {
		// Collect information about the current state before migration
		let mut accounts_count = 0u32;
		let mut total_deposits = BTreeMap::new();
		
		// Iterate through all accounts to collect pre-migration state
		for (index, (account, deposit, frozen)) in v0::Accounts::<T>::iter() {
			accounts_count += 1;
			*total_deposits.entry(account).or_insert(0u64) += deposit;
		}
		
		// Serialize the pre-migration state
		let pre_state = (accounts_count, total_deposits);
		Ok(pre_state.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(prev: Vec<u8>) -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
		// Decode the pre-migration state
		let (prev_accounts_count, prev_total_deposits): (u32, BTreeMap<T::AccountId, u64>) = 
			codec::Decode::decode(&mut &prev[..])
				.map_err(|_| "Failed to decode pre-migration state")?;
		
		// Verify post-migration state
		let mut post_accounts_count = 0u32;
		let mut post_total_deposits = BTreeMap::new();
		
		// Iterate through all accounts to collect post-migration state
		for (index, (account, deposit, frozen)) in crate::pallet::Accounts::<T>::iter() {
			post_accounts_count += 1;
			*post_total_deposits.entry(account).or_insert(0u64) += deposit;
		}
		
		// Verify that the number of accounts hasn't changed
		if prev_accounts_count != post_accounts_count {
			return Err("Account count mismatch after migration".into());
		}
		
		// Verify that total deposits per account haven't changed
		for (account, prev_deposit) in prev_total_deposits {
			let post_deposit = post_total_deposits.get(&account).copied().unwrap_or(0);
			if prev_deposit != post_deposit {
				return Err(format!("Deposit mismatch for account {:?}: {} vs {}", 
					account, prev_deposit, post_deposit).into());
			}
		}
		
		Ok(())
	}
}