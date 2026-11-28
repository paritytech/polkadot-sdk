// This file is part of Substrate.

// Copyright (C) Amforc AG.
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

//! Storage migrations of the PSM pallet.
//!
//! Add [`v1::MigrateV0ToV1`] to the runtime's migrations to move from storage version 0 to 1.

/// Storage version 0 to 1: the admin roles in [`crate::PsmAdmin`] become [`crate::AdminOrigin`].
///
/// Each stored role becomes [`crate::AdminOrigin::Origin`] of the same origin, so every PSM keeps
/// its admins. A role follows the internal asset's owner only after the full admin sets it to
/// [`crate::AdminOrigin::AssetOwner`].
pub mod v1 {
	use crate::{AdminOrigin, Config, Pallet, PsmAdmin, PsmAdminInfo};
	use frame_support::{
		migrations::VersionedMigration, pallet_prelude::*, traits::UncheckedOnRuntimeUpgrade,
	};
	#[cfg(feature = "try-runtime")]
	use {alloc::vec::Vec, sp_runtime::TryRuntimeError};

	/// The storage layout of storage version 0.
	pub mod v0 {
		use crate::{Config, Pallet};
		use frame_support::pallet_prelude::*;

		/// [`crate::PsmAdminInfo`] at storage version 0, with plain origins as admins.
		#[derive(Encode, Decode)]
		pub struct PsmAdminInfo<T: Config> {
			/// Origin with `Full` management privileges.
			pub full_admin: T::PalletsOrigin,
			/// Origin with `Emergency` management privileges.
			pub emergency_admin: T::PalletsOrigin,
			/// Optional creation deposit and its depositor.
			pub deposit: Option<(T::AccountId, T::Consideration)>,
		}

		/// [`crate::PsmAdmin`] at storage version 0.
		#[frame_support::storage_alias]
		pub type PsmAdmin<T: Config> = StorageMap<
			Pallet<T>,
			Blake2_128Concat,
			<T as Config>::AssetId,
			PsmAdminInfo<T>,
			OptionQuery,
		>;
	}

	/// The storage version 1 form of a storage version 0 [`PsmAdmin`] entry: each role becomes
	/// [`AdminOrigin::Origin`] of the same origin, and the deposit is kept.
	fn migrate_entry<T: Config>(old: v0::PsmAdminInfo<T>) -> PsmAdminInfo<T> {
		PsmAdminInfo {
			full_admin: AdminOrigin::Origin(old.full_admin),
			emergency_admin: AdminOrigin::Origin(old.emergency_admin),
			deposit: old.deposit,
		}
	}

	/// Rewrites every [`PsmAdmin`] entry with `migrate_entry`. Use it through
	/// [`MigrateV0ToV1`], which runs it once and bumps the storage version.
	pub struct InnerMigrateV0ToV1<T>(core::marker::PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for InnerMigrateV0ToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut migrated: u64 = 0;
			PsmAdmin::<T>::translate::<v0::PsmAdminInfo<T>, _>(|_, old| {
				migrated = migrated.saturating_add(1);
				Some(migrate_entry::<T>(old))
			});
			log::info!(target: "runtime::psm", "migrated {migrated} PsmAdmin entries to v1");
			// Per entry: read it and write it back. One more read ends the iteration.
			T::DbWeight::get().reads_writes(migrated.saturating_add(1), migrated)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let before: Vec<(T::AssetId, v0::PsmAdminInfo<T>)> =
				v0::PsmAdmin::<T>::iter().collect();
			Ok(before.encode())
		}

		/// Checks that every role is [`AdminOrigin::Origin`] of its value before the upgrade, and
		/// that the deposits and the entry count are unchanged.
		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			let before = Vec::<(T::AssetId, v0::PsmAdminInfo<T>)>::decode(&mut &state[..])
				.map_err(|_| "pre_upgrade state does not decode")?;
			ensure!(
				PsmAdmin::<T>::iter_keys().count() == before.len(),
				"PsmAdmin entry count changed during the migration"
			);
			for (internal_asset, old) in before {
				let after = PsmAdmin::<T>::get(&internal_asset)
					.ok_or("PsmAdmin entry missing or undecodable after the migration")?;
				ensure!(
					after.full_admin == AdminOrigin::Origin(old.full_admin),
					"full_admin is not Origin of its value before the upgrade"
				);
				ensure!(
					after.emergency_admin == AdminOrigin::Origin(old.emergency_admin),
					"emergency_admin is not Origin of its value before the upgrade"
				);
				ensure!(after.deposit == old.deposit, "PsmAdmin deposit changed");
			}
			Ok(())
		}
	}

	/// Migrates [`PsmAdmin`] from storage version 0 to 1 and sets the storage version to 1.
	/// Does nothing at any other storage version.
	pub type MigrateV0ToV1<T> = VersionedMigration<
		0,
		1,
		InnerMigrateV0ToV1<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}
