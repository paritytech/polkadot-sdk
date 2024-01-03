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

//! Storage migrations for the Identity pallet.

use super::*;
use frame_support::{migrations::VersionedMigration, pallet_prelude::*, traits::OnRuntimeUpgrade};

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

pub mod versioned {
	use super::*;

	pub type V0ToV1<T> = VersionedMigration<
		0,
		1,
		v1::VersionUncheckedMigrateV0ToV1<T>,
		crate::pallet::Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

pub mod v1 {
	use super::*;

	/// The log target.
	const TARGET: &'static str = "runtime::identity::migration::v1";

	/// The old identity type, useful in pre-upgrade.
	mod v0 {
		use super::*;
		use frame_support::storage_alias;

		#[storage_alias]
		pub type IdentityOf<T: Config> = StorageMap<
			Pallet<T>,
			Twox64Concat,
			<T as frame_system::Config>::AccountId,
			Registration<
				BalanceOf<T>,
				<T as pallet::Config>::MaxRegistrars,
				<T as pallet::Config>::IdentityInformation,
			>,
			OptionQuery,
		>;
	}

	/// Migration to add usernames to Identity info.
	pub struct VersionUncheckedMigrateV0ToV1<T>(PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for VersionUncheckedMigrateV0ToV1<T> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let identities = v0::IdentityOf::<T>::iter().count();
			log::info!(
				target: TARGET,
				"pre-upgrade state contains '{}' identities.",
				identities
			);
			Ok((identities as u32).encode())
		}

		fn on_runtime_upgrade() -> Weight {
			log::info!(
				target: TARGET,
				"running storage migration from version 0 to version 1."
			);

			let mut weight = T::DbWeight::get().reads(1);
			let mut translated: u64 = 0;

			IdentityOf::<T>::translate::<
				Registration<BalanceOf<T>, T::MaxRegistrars, T::IdentityInformation>,
				_,
			>(|_, registration| {
				translated.saturating_inc();
				Some((registration, None::<Username<T>>))
			});
			log::info!("translated {} identities", translated);

			weight.saturating_accrue(T::DbWeight::get().reads_writes(translated, translated));
			weight.saturating_accrue(T::DbWeight::get().writes(1));
			weight
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			let identities_to_migrate: u32 = Decode::decode(&mut &state[..])
				.expect("failed to decode the state from pre-upgrade.");
			let identities = IdentityOf::<T>::iter().count() as u32;
			log::info!("post-upgrade expects '{}' identities to have been migrated.", identities);
			ensure!(identities_to_migrate == identities, "must migrate all identities.");
			log::info!(target: TARGET, "migrated all identities.");
			Ok(())
		}
	}
}
