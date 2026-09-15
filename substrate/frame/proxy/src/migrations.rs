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

//! Storage migrations for the proxy pallet.

use super::*;
use frame::traits::UncheckedOnRuntimeUpgrade;

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;

/// Version-unchecked migration logic, kept private so that only [`MigrateV0ToV1`] can reach it.
mod version_unchecked {
	use super::*;

	/// Sorts every [`Proxies`] entry and drops exact duplicates.
	///
	/// [`Pallet::add_proxy_delegate`] and [`Pallet::remove_proxy_delegate`] find a delegate with
	/// `binary_search`, which needs a sorted list. Some live Asset Hub entries are unsorted, so
	/// their owners cannot remove the affected proxy or recover its deposit, and `add_proxy` adds a
	/// duplicate instead of failing with [`Error::Duplicate`].
	///
	/// Deposits are left alone: a shorter list prices below its stored deposit, which
	/// [`Pallet::poke_deposit`] corrects. The migration cannot, a pure proxy's deposit being
	/// reserved on its spawner.
	pub struct MigrateV0ToV1<T>(core::marker::PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for MigrateV0ToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut reads = 0u64;
			let mut writes = 0u64;
			let mut repaired = 0u64;

			// Collected up front, so the map is not written while it is iterated.
			let keys = Proxies::<T>::iter_keys()
				.inspect(|_| reads += 1)
				.collect::<alloc::vec::Vec<_>>();

			for delegator in keys {
				reads += 1;
				// `try_get`, not `get`: under `ValueQuery` an undecodable value would read as an
				// empty list and be written back as one.
				let Ok((proxies, deposit)) = Proxies::<T>::try_get(&delegator) else {
					log::error!(
						target: LOG_TARGET,
						"Proxies entry for {:?} could not be read, leaving it untouched",
						delegator,
					);
					continue;
				};

				let mut sorted = proxies.clone().into_inner();
				sorted.sort();
				sorted.dedup();
				if sorted[..] == proxies[..] {
					continue;
				}

				// `sorted` is no longer than `proxies`, which was bounded by `MaxProxies`.
				let Ok(sorted) = BoundedVec::try_from(sorted) else {
					log::error!(
						target: LOG_TARGET,
						"Sorted proxies for {:?} exceed `MaxProxies`, which is impossible, \
						leaving the entry untouched",
						delegator,
					);
					continue;
				};

				log::info!(
					target: LOG_TARGET,
					"Repairing Proxies entry for {:?}: {} delegates become {}",
					delegator,
					proxies.len(),
					sorted.len(),
				);

				// Deposit carried over unchanged, see the type docs.
				Proxies::<T>::insert(&delegator, (sorted, deposit));
				writes += 1;
				repaired += 1;
			}

			log::info!(
				target: LOG_TARGET,
				"Proxies migration v0 -> v1 done, {} entries repaired",
				repaired,
			);

			T::DbWeight::get().reads_writes(reads, writes)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			// The state the migration must produce.
			let expected = Proxies::<T>::iter()
				.map(|(delegator, (proxies, deposit))| {
					let mut sorted = proxies.into_inner();
					sorted.sort();
					sorted.dedup();
					(delegator, sorted, deposit)
				})
				.collect::<Vec<_>>();

			log::info!(
				target: LOG_TARGET,
				"Proxies migration v0 -> v1 will walk {} entries",
				expected.len(),
			);

			Ok(expected.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			type ExpectedState<T> = Vec<(
				<T as frame_system::Config>::AccountId,
				alloc::vec::Vec<
					ProxyDefinition<
						<T as frame_system::Config>::AccountId,
						<T as Config>::ProxyType,
						BlockNumberFor<T>,
					>,
				>,
				BalanceOf<T>,
			)>;

			let expected = ExpectedState::<T>::decode(&mut &state[..])
				.map_err(|_| "Proxies migration v0 -> v1 cannot decode its pre-upgrade state")?;

			ensure!(
				Proxies::<T>::iter().count() == expected.len(),
				"Proxies migration v0 -> v1 changed the number of entries"
			);

			for (delegator, proxies, deposit) in expected {
				let (migrated, migrated_deposit) = Proxies::<T>::get(&delegator);
				ensure!(
					migrated[..] == proxies[..],
					"Proxies entry is not the sorted, deduplicated form of its pre-upgrade value"
				);
				ensure!(
					migrated_deposit == deposit,
					"Proxies migration v0 -> v1 changed a deposit"
				);
			}

			Ok(())
		}
	}
}

/// Sorts and deduplicates every [`Proxies`] entry, moving the pallet from storage version 0 to 1.
///
/// Walks the whole map in one block: the heaviest chain measured against a live snapshot, the
/// Polkadot Asset Hub, costs 262.2 KiB of the 10 MiB proof limit and 3.07% of ref time.
pub type MigrateV0ToV1<T> = frame::deps::frame_support::migrations::VersionedMigration<
	0,
	1,
	version_unchecked::MigrateV0ToV1<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;
