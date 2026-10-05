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
use frame::deps::frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	weights::WeightMeter,
};

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;

/// Identifies this pallet to the multi-block migration runner.
pub const PALLET_MIGRATIONS_ID: &[u8; 16] = b"pallet-proxy-mbm";

/// Sorts every [`Proxies`] entry and drops exact duplicates, moving the pallet from storage
/// version 0 to 1.
///
/// [`Pallet::add_proxy_delegate`] and [`Pallet::remove_proxy_delegate`] find a delegate with
/// `binary_search`, which needs a sorted list. Some live Asset Hub entries are unsorted, so their
/// owners cannot remove the affected proxy or recover its deposit, and `add_proxy` adds a
/// duplicate instead of failing with [`Error::Duplicate`].
///
/// Deposits are left alone: a shorter list prices below its stored deposit, which
/// [`Pallet::poke_deposit`] corrects. The migration cannot, a pure proxy's deposit being reserved
/// on its spawner.
///
/// This is a multi-block migration, so it needs to be listed in
/// `pallet_migrations::Config::Migrations` rather than in the runtime's single-block migration
/// tuple. One step repairs one entry, which bounds what a single block adds to the proof: the
/// whole map is large enough on the Asset Hubs (4,455 entries on Polkadot, 262.2 KiB of proof)
/// that walking it in one block is worth avoiding.
pub struct MigrateV0ToV1<T>(core::marker::PhantomData<T>);

impl<T: Config> MigrateV0ToV1<T> {
	/// Sorts and deduplicates the first [`Proxies`] entry after `last_key`, or the first entry of
	/// the map when `last_key` is `None`.
	///
	/// Returns the key it visited, or `None` once the map is exhausted. An entry that is already
	/// in order, or that cannot be repaired, is still reported as visited, so progress never
	/// depends on the repair itself succeeding.
	pub(crate) fn repair_next(last_key: Option<&T::AccountId>) -> Option<T::AccountId> {
		let mut keys = match last_key {
			Some(last_key) => Proxies::<T>::iter_keys_from_key(last_key),
			None => Proxies::<T>::iter_keys(),
		};
		let delegator = keys.next()?;

		// `try_get`, not `get`: under `ValueQuery` an undecodable value would read as an empty
		// list and be written back as one.
		let Ok((proxies, deposit)) = Proxies::<T>::try_get(&delegator) else {
			log::error!(
				target: LOG_TARGET,
				"Proxies entry for {:?} could not be read, leaving it untouched",
				delegator,
			);
			return Some(delegator);
		};

		let mut sorted = proxies.clone().into_inner();
		sorted.sort();
		sorted.dedup();
		if sorted[..] == proxies[..] {
			return Some(delegator);
		}

		// `sorted` is no longer than `proxies`, which was bounded by `MaxProxies`.
		let Ok(sorted) = BoundedVec::try_from(sorted) else {
			log::error!(
				target: LOG_TARGET,
				"Sorted proxies for {:?} exceed `MaxProxies`, which is impossible, \
				leaving the entry untouched",
				delegator,
			);
			return Some(delegator);
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
		Some(delegator)
	}
}

impl<T: Config> SteppedMigration for MigrateV0ToV1<T> {
	/// The last [`Proxies`] key this migration repaired, `None` while it is still before the first
	/// one. The `Option` the trait wraps this in means something else: `None` there is the end of
	/// the migration, which is why this cursor cannot be a bare `AccountId`.
	type Cursor = Option<T::AccountId>;
	type Identifier = MigrationId<16>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 0, version_to: 1 }
	}

	fn step(
		cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		if Pallet::<T>::on_chain_storage_version() != Self::id().version_from as u16 {
			return Ok(None);
		}

		let required = T::WeightInfo::migrate_v0_to_v1_step();
		if meter.remaining().any_lt(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		let mut last_key = cursor.flatten();
		while meter.try_consume(required).is_ok() {
			match Self::repair_next(last_key.as_ref()) {
				Some(delegator) => last_key = Some(delegator),
				None => {
					log::info!(target: LOG_TARGET, "Proxies migration v0 -> v1 done");
					StorageVersion::new(Self::id().version_to as u16).put::<Pallet<T>>();
					return Ok(None);
				},
			}
		}

		// `Some`, even when `last_key` is `None`: that only asks for the walk to restart at the
		// first key, where returning `None` would claim the migration had finished.
		Ok(Some(last_key))
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
			ensure!(migrated_deposit == deposit, "Proxies migration v0 -> v1 changed a deposit");
		}

		Ok(())
	}
}
