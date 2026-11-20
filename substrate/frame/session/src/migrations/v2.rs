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

use crate::{Config, KeyRegistration, KeyStatus as NewKeyStatus, Pallet};
use frame_support::{
	migrations::VersionedMigration,
	pallet_prelude::{Get, OptionQuery, Twox64Concat, Weight},
	traits::UncheckedOnRuntimeUpgrade,
};
use sp_runtime::traits::Convert;

#[cfg(feature = "try-runtime")]
use crate::Vec;
#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::ensure;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

/// `ExternallySetKeys` and `KeyDepositor`, the two storage items being merged into `KeyStatus`.
#[frame_support::storage_alias]
type ExternallySetKeys<T: Config> =
	StorageMap<Pallet<T>, Twox64Concat, <T as frame_system::Config>::AccountId, (), OptionQuery>;

#[frame_support::storage_alias]
type KeyDepositor<T: Config> = StorageMap<
	Pallet<T>,
	Twox64Concat,
	<T as Config>::ValidatorId,
	<T as frame_system::Config>::AccountId,
	OptionQuery,
>;

pub struct VersionUncheckedMigrateV1ToV2<T>(core::marker::PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateV1ToV2<T> {
	fn on_runtime_upgrade() -> Weight {
		let mut reads = 0u64;
		let mut writes = 0u64;

		// `ExternallySetKeys` goes first: it is account-keyed, so a stale entry left over
		// from an account that no longer manages `who` locally (the old `do_set_keys` and
		// `do_purge_keys` only ever cleared the calling account's own entry) must not
		// survive. `KeyDepositor`, which follows, is already validator-keyed and records an
		// actual held deposit and consumer reference, so it always wins for the same `who`.
		for (account, ()) in ExternallySetKeys::<T>::drain() {
			// One read for the entry, one write for `drain`'s removal.
			reads += 1;
			writes += 1;
			// `ValidatorIdOf::convert` costs at most one storage read.
			reads += 1;
			// An account with no associated validator ID any more has nothing left to merge:
			// there is no `who` for it to record external status against.
			if let Some(who) = T::ValidatorIdOf::convert(account) {
				writes += 1;
				NewKeyStatus::<T>::insert(&who, KeyRegistration::External);
			}
		}

		for (who, account) in KeyDepositor::<T>::drain() {
			reads += 1;
			writes += 1;
			writes += 1;
			NewKeyStatus::<T>::insert(&who, KeyRegistration::Local(account));
		}

		T::DbWeight::get().reads_writes(reads, writes)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		// The validators with a recorded local depositor: an actual held deposit and
		// consumer reference that must still be `Local` after the migration, never
		// overwritten by a stale `External` entry for the same validator.
		let locally_registered: Vec<T::ValidatorId> = KeyDepositor::<T>::iter_keys().collect();
		Ok(locally_registered.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		ensure!(KeyDepositor::<T>::iter().next().is_none(), "KeyDepositor should be drained");
		ensure!(
			ExternallySetKeys::<T>::iter().next().is_none(),
			"ExternallySetKeys should be drained"
		);

		let locally_registered = Vec::<T::ValidatorId>::decode(&mut &state[..])
			.map_err(|_| TryRuntimeError::Other("failed to decode pre-upgrade state"))?;
		for who in locally_registered {
			ensure!(
				matches!(NewKeyStatus::<T>::get(&who), Some(KeyRegistration::Local(_))),
				"a validator with a local deposit lost its `Local` registration during the migration",
			);
		}
		Ok(())
	}
}

pub type MigrateV1ToV2<T> = VersionedMigration<
	1,
	2,
	VersionUncheckedMigrateV1ToV2<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;
