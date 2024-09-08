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

use super::*;
use frame_support::traits::OnRuntimeUpgrade;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

pub mod unversioned {
	use super::*;
	#[cfg(feature = "try-runtime")]
	use alloc::vec::Vec;
	use sp_runtime::traits::AccountIdConversion;

	/// Migrates `ProxyDelegator` accounts with better entropy than the old logic which didn't take
	/// into account all the bytes of the agent account ID.
	pub struct ProxyDelegatorMigration<T, MaxAgents>(PhantomData<(T, MaxAgents)>);

	impl<T: Config, MaxAgents: Get<u32>> OnRuntimeUpgrade for ProxyDelegatorMigration<T, MaxAgents> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();
			let old_proxy_delegator = |agent: T::AccountId| {
				T::PalletId::get()
					.into_sub_account_truncating((AccountType::ProxyDelegator, agent.clone()))
			};

			Agents::<T>::iter_keys().take(MaxAgents::get() as usize).for_each(|agent| {
				weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 0));
				let old_proxy = old_proxy_delegator(agent.clone());

				// if delegation does not exist, it does not need to be migrated.
				if let Some(delegation) = Delegation::<T>::get(&old_proxy) {
					weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 0));

					let new_proxy =
						Pallet::<T>::generate_proxy_delegator(Agent::from(agent.clone()));

					// accrue read writes for `do_migrate_delegation`
					weight.saturating_accrue(T::DbWeight::get().reads_writes(8, 8));
					let _ = Pallet::<T>::do_migrate_delegation(
						Delegator::from(old_proxy.clone()),
						new_proxy.clone(),
						delegation.amount,
					)
					.map_err(|e| {
						log!(
							error,
							"Failed to migrate old proxy delegator {:?} to new proxy {:?} for agent {:?} with error: {:?}",
								old_proxy,
								new_proxy,
								agent,
								e,
						);
					});
				};
			});

			log!(info, "Finished migrating old proxy delegator accounts to new ones");
			weight
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_data: Vec<u8>) -> Result<(), TryRuntimeError> {
			let mut unmigrated_count = 0;
			let old_proxy_delegator = |agent: T::AccountId| {
				T::PalletId::get()
					.into_sub_account_truncating((AccountType::ProxyDelegator, agent.clone()))
			};

			Agents::<T>::iter_keys().take(MaxAgents::get() as usize).for_each(|agent| {
				let old_proxy: T::AccountId = old_proxy_delegator(agent.clone());
				let held_balance = Pallet::<T>::held_balance_of(Delegator::from(old_proxy.clone()));
				let delegation = Delegation::<T>::get(&old_proxy);
				if delegation.is_some() || !held_balance.is_zero() {
					log!(
						error,
						"Old proxy delegator {:?} for agent {:?} is not migrated.",
						old_proxy,
						agent,
					);
					unmigrated_count += 1;
				}
			});

			if unmigrated_count > 0 {
				Err(TryRuntimeError::Other("Some old proxy delegator accounts are not migrated."))
			} else {
				Ok(())
			}
		}
	}
}
