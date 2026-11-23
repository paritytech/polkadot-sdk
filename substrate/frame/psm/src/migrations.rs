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
pub mod v1 {
	use crate::{AdminOrigin, Config, Pallet, PsmAdmin, PsmAdminInfo};
	use frame_support::{
		migrations::VersionedMigration,
		pallet_prelude::*,
		traits::{fungibles::roles::Inspect as FungiblesRolesInspect, UncheckedOnRuntimeUpgrade},
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

	/// The signed origin of the owner of `internal_asset`, if it has an owner.
	fn owner_origin<T: Config>(internal_asset: &T::AssetId) -> Option<T::PalletsOrigin> {
		T::Fungibles::owner(internal_asset.clone())
			.map(|who| frame_system::RawOrigin::Signed(who).into())
	}

	/// The storage version 1 form of a storage version 0 admin role.
	///
	/// A role equal to the signed origin of the internal asset's current owner becomes
	/// [`AdminOrigin::AssetOwner`], so it is held by whoever owns the asset. Every other role
	/// becomes [`AdminOrigin::Origin`] of the same origin.
	fn migrate_role<T: Config>(
		role: T::PalletsOrigin,
		owner: Option<&T::PalletsOrigin>,
	) -> AdminOrigin<T::PalletsOrigin> {
		if owner == Some(&role) {
			AdminOrigin::AssetOwner
		} else {
			AdminOrigin::Origin(role)
		}
	}

	/// The storage version 1 form of a storage version 0 [`PsmAdmin`] entry.
	fn migrate_entry<T: Config>(
		internal_asset: &T::AssetId,
		old: v0::PsmAdminInfo<T>,
	) -> PsmAdminInfo<T> {
		let owner = owner_origin::<T>(internal_asset);
		PsmAdminInfo {
			full_admin: migrate_role::<T>(old.full_admin, owner.as_ref()),
			emergency_admin: migrate_role::<T>(old.emergency_admin, owner.as_ref()),
			deposit: old.deposit,
		}
	}

	/// Rewrites every [`PsmAdmin`] entry with `migrate_role`. Use it through
	/// [`MigrateV0ToV1`], which runs it once and bumps the storage version.
	pub struct InnerMigrateV0ToV1<T>(core::marker::PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for InnerMigrateV0ToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut migrated: u64 = 0;
			PsmAdmin::<T>::translate::<v0::PsmAdminInfo<T>, _>(|internal_asset, old| {
				migrated = migrated.saturating_add(1);
				Some(migrate_entry::<T>(&internal_asset, old))
			});
			log::info!(target: "runtime::psm", "migrated {migrated} PsmAdmin entries to v1");
			// Per entry: read it, read the asset owner, write it back. One more read ends the
			// iteration.
			T::DbWeight::get().reads_writes(migrated.saturating_mul(2).saturating_add(1), migrated)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let before: Vec<PreUpgradeEntry<T>> = v0::PsmAdmin::<T>::iter()
				.map(|(internal_asset, old)| PreUpgradeEntry {
					owner: owner_origin::<T>(&internal_asset),
					internal_asset,
					full_admin: old.full_admin,
					emergency_admin: old.emergency_admin,
					deposit: old.deposit,
				})
				.collect();
			Ok(before.encode())
		}

		/// Checks each entry against the migration policy, independently of how the migration
		/// computes it: a role is [`AdminOrigin::AssetOwner`] exactly when it was the signed
		/// origin of the owner recorded before the upgrade, and otherwise it is
		/// [`AdminOrigin::Origin`] of the same origin.
		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
			let before = Vec::<PreUpgradeEntry<T>>::decode(&mut &state[..])
				.map_err(|_| "pre_upgrade state does not decode")?;
			ensure!(
				PsmAdmin::<T>::iter_keys().count() == before.len(),
				"PsmAdmin entry count changed during the migration"
			);
			for entry in before {
				let after = PsmAdmin::<T>::get(&entry.internal_asset)
					.ok_or("PsmAdmin entry missing or undecodable after the migration")?;
				check_role::<T>(&entry.full_admin, &after.full_admin, entry.owner.as_ref())?;
				check_role::<T>(
					&entry.emergency_admin,
					&after.emergency_admin,
					entry.owner.as_ref(),
				)?;
				ensure!(after.deposit == entry.deposit, "PsmAdmin deposit changed");
			}
			Ok(())
		}
	}

	/// A [`PsmAdmin`] entry and the internal asset's owner, recorded before the upgrade.
	#[cfg(feature = "try-runtime")]
	#[derive(Encode, Decode)]
	struct PreUpgradeEntry<T: Config> {
		internal_asset: T::AssetId,
		owner: Option<T::PalletsOrigin>,
		full_admin: T::PalletsOrigin,
		emergency_admin: T::PalletsOrigin,
		deposit: Option<(T::AccountId, T::Consideration)>,
	}

	/// Checks one migrated role against its storage version 0 value `old`.
	#[cfg(feature = "try-runtime")]
	fn check_role<T: Config>(
		old: &T::PalletsOrigin,
		new: &AdminOrigin<T::PalletsOrigin>,
		owner: Option<&T::PalletsOrigin>,
	) -> Result<(), TryRuntimeError> {
		let was_owner = owner == Some(old);
		match new {
			AdminOrigin::AssetOwner => {
				ensure!(was_owner, "AssetOwner role was not the owner's signed origin before")
			},
			AdminOrigin::Origin(origin) => {
				ensure!(origin == old, "Origin role differs from its value before the upgrade");
				ensure!(!was_owner, "the owner's signed origin stayed a fixed Origin role");
			},
		}
		Ok(())
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
