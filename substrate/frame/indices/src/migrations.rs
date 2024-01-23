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

//! A module that is responsible for migration of storage for Indices pallet

use super::*;
use frame_support::{
	traits::{OnRuntimeUpgrade, ReservableCurrency},
	Blake2_128Concat,
	storage_alias,
};
use log;

pub mod v0 {
	use super::*;

	pub type BalanceOf<T, OldCurrency> = <OldCurrency as frame_support::traits::Currency<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

	// The old `Accounts`, removed in <https://github.com/paritytech/polkadot-sdk/pull/1789>, used in v0
	#[storage_alias]
	pub type Accounts<T: Config, OldCurrency> =
		StorageMap<
			Pallet<T>,
			Blake2_128Concat,
			<T as Config>::AccountIndex,
			(<T as frame_system::Config>::AccountId, BalanceOf<T, OldCurrency>, bool)
		>;
}


/// Version 1 Migration
/// This migration ensures that:
/// - Deposits are properly removed from `Accounts`
/// - Hold reasons are stored in pallet Balances
pub mod v1 {
	use super::*;
	use frame_support::pallet_prelude::*;
	#[cfg(feature = "try-runtime")]
	use sp_std::prelude::*;

	pub struct MigrateToV1<T, OldCurrency> {
		_marker: sp_std::marker::PhantomData<(T, OldCurrency)>
	}

	impl<T, OldCurrency> OnRuntimeUpgrade for MigrateToV1<T, OldCurrency>
	where
		T: Config,
		OldCurrency: 'static + ReservableCurrency<<T as frame_system::Config>::AccountId>,
		BalanceOf<T>: From<OldCurrency::Balance>,
	{
		fn on_runtime_upgrade() -> Weight {
			let current = Pallet::<T>::current_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			if current == 1 && onchain == 0 {
				// update the version nonetheless.
				current.put::<Pallet<T>>();

				// TODO: Replace unbound storage iteration by lazy migration or multiblock migration
				v0::Accounts::<T, OldCurrency>::iter().for_each(|(account_index, (account_id, deposit, perm))| {
					let remaining = OldCurrency::unreserve(&account_id, deposit);

					if remaining > Zero::zero() {
						log::warn!(
							target: LOG_TARGET,
							"Account {:?} has some non-unreservable deposit {:?} from a total of {:?} that will remain in reserved.",
							account_id,
							remaining,
							deposit,
						);
					}

					let unreserved = deposit.saturating_sub(remaining);
					let amount = BalanceOf::<T>::from(unreserved);

					// TODO: is there a way of calculating exactly the same deposit with a Footprint?
					let ticket = T::Consideration::new_from_exact(
						&account_id,
						amount
					).map_err(|err| {
						log::error!(
							target: LOG_TARGET,
							"Failed creating a new Consideration for the account {:?}, reason: {:?}.",
							account_id,
							err
						);
						err
					}).ok();

					Accounts::<T>::set(account_index, Some((account_id, ticket, perm)));
				});

				// TODO: Fix weight when lazy migration or multi block migration is in place
				T::DbWeight::get().reads_writes(2, 3)
			} else {
				log::info!(
					target: LOG_TARGET,
					"Migration did not execute. This probably should be removed"
				);
				T::DbWeight::get().reads(2)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
			ensure!(
				Pallet::<T>::on_chain_storage_version() == 0,
				"The onchain storage version must be zero for the migration to execute."
			);
			// Count the number of `Accounts` and calculate the total reserved balance
			let accounts_info = v0::Accounts::<T>::iter().fold((0, 0), |(count, total_reserved), account| {
				let (account_id, deposit, _) = account;
				// Try to unreserve the deposit
				//
				// TODO: does the state persists between `pre_upgrade` and `post_upgrade`?
				// If that's the case I should reserve back `unreserved_deposit` as `can_unreserve()` method
				// does not exists
				let remaining = OldCurrency::unreserve(&account_id, deposit);
				let unreserved_deposit = deposit.saturating_sub(remaining);

				(count + 1, total_reserved + unreserved_deposit)
			});
			let (accounts_count, total_reserved) = accounts_info;

			Ok((accounts_count as u32, total_reserved as u32).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
			ensure!(
				Pallet::<T>::on_chain_storage_version() == 1,
				"The onchain version must be updated after the migration."
			);

			let (pre_accounts_count, pre_total_reserved): (u32, u32) = Decode::decode(
				&mut state,
			);

			// Count the number of `Accounts` and calculate the total held balance
			let accounts_info = Accounts::<T>::iter().fold((0, 0), |(count, total_held), account| {
				let (account_id, _) = account;
				let held = T::Currency::balance_on_hold(&HoldReason::ClaimedIndex.into(), account_id);
				(count + 1, total_held + held)
			});

			let (post_accounts_count, post_total_held) = accounts_info;

			// Number of accounts should remain the same
			ensure!(
				pre_accounts_count == post_accounts_count,
				"The number of migrated accounts should remain"
			);

			// Total reserved/held amount should remain the same
			ensure!(
				pre_total_reserved == post_total_held,
				"Total real reserved/held amount should remain"
			);

			Ok(())
		}
	}
}
