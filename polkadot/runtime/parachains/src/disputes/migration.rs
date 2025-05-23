// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Storage migration(s) related to disputes pallet

use frame_support::traits::StorageVersion;

pub mod v2 {
	use super::*;
	use crate::disputes::slashing::{UnappliedSlashes, Config, Pallet};
	use frame_support::{
		pallet_prelude::*, traits::UncheckedOnRuntimeUpgrade, weights::Weight, migrations::VersionedMigration,
	};
	use polkadot_primitives::vstaging::PendingSlashes as PendingSlashesV2;

	pub struct VersionUncheckedMigrateV1ToV2<T>(core::marker::PhantomData<T>);
	impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateV1ToV2<T> {
		fn on_runtime_upgrade() -> Weight {
			log::info!(target: crate::disputes::LOG_TARGET, "Migrating disputes storage from v1 to v2");

			let mut read_count = 0;
			let mut write_count = 0;
	
			for (session_index, candidate_hash, v1_slash) in v1::UnappliedSlashes::<T>::drain() {
				read_count += 1;
	
				let v2_slash: PendingSlashesV2 = v1_slash.into();
				UnappliedSlashes::<T>::insert(session_index, candidate_hash, v2_slash);
	
				write_count += 1;
			}

			log::info!(target: crate::disputes::LOG_TARGET, "Migration to v2 applied successfully");
			T::DbWeight::get().reads_writes(read_count+1, write_count+1)
		}
	}

	pub type MigrateV1ToV2<T> = VersionedMigration<
		1,
		2,
		VersionUncheckedMigrateV1ToV2<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

pub mod v1 {
	use super::*;
	use crate::disputes::{Config, Pallet};
	use alloc::vec::Vec;
	use frame_support::{
		pallet_prelude::*, storage_alias, traits::OnRuntimeUpgrade, weights::Weight,
	};
	use polkadot_primitives::{SessionIndex, CandidateHash, slashing::PendingSlashes as PendingSlashesV1};

	#[storage_alias]
	type SpamSlots<T: Config> = StorageMap<Pallet<T>, Twox64Concat, SessionIndex, Vec<u32>>;

	#[storage_alias]
	pub type UnappliedSlashes<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Twox64Concat,
		SessionIndex,
		Blake2_128Concat,
		CandidateHash,
		PendingSlashesV1,
	>;

	pub struct MigrateToV1<T>(core::marker::PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for MigrateToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight: Weight = Weight::zero();

			if StorageVersion::get::<Pallet<T>>() < 1 {
				log::info!(target: crate::disputes::LOG_TARGET, "Migrating disputes storage to v1");
				weight += migrate_to_v1::<T>();
				StorageVersion::new(1).put::<Pallet<T>>();
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
			} else {
				log::info!(
					target: crate::disputes::LOG_TARGET,
					"Disputes storage up to date - no need for migration"
				);
			}

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			log::trace!(
				target: crate::disputes::LOG_TARGET,
				"SpamSlots before migration: {}",
				SpamSlots::<T>::iter().count()
			);
			ensure!(
				StorageVersion::get::<Pallet<T>>() == 0,
				"Storage version should be less than `1` before the migration",
			);
			Ok(Vec::new())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			log::trace!(target: crate::disputes::LOG_TARGET, "Running post_upgrade()");
			ensure!(
				StorageVersion::get::<Pallet<T>>() >= 1,
				"Storage version should be `1` after the migration"
			);
			ensure!(
				SpamSlots::<T>::iter().count() == 0,
				"SpamSlots should be empty after the migration"
			);
			Ok(())
		}
	}

	/// Migrates the pallet storage to the most recent version, checking and setting the
	/// `StorageVersion`.
	pub fn migrate_to_v1<T: Config>() -> Weight {
		let mut weight: Weight = Weight::zero();

		// SpamSlots should not contain too many keys so removing everything at once should be safe
		let res = SpamSlots::<T>::clear(u32::MAX, None);
		// `loops` is the number of iterations => used to calculate read weights
		// `backend` is the number of keys removed from the backend => used to calculate write
		// weights
		weight = weight
			.saturating_add(T::DbWeight::get().reads_writes(res.loops as u64, res.backend as u64));

		weight
	}
}