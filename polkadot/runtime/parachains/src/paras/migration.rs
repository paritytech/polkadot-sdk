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

//! Storage migrations for the paras pallet.
//!
//! # v1: Remove Parathread lifecycle
//!
//! The `Parathread` (on-demand parachain) lifecycle variants are removed from `ParaLifecycle`.
//! Every registered para is now a parachain. This migration converts existing storage:
//!
//! - `Parathread`            → `Parachain` (also inserted into the `Parachains` list)
//! - `UpgradingParathread`   → `Parachain` (upgrade is completed immediately)
//! - `OffboardingParathread` → `OffboardingParachain`
//! - `DowngradingParachain`  → `Parachain` (downgrade is cancelled; para stays a parachain)

pub use v1::MigrateToV1;

pub mod v0 {
	use crate::paras::{Config, Pallet};
	use codec::{Decode, Encode};
	use frame_support::{storage_alias, Twox64Concat};
	use polkadot_primitives::Id as ParaId;
	use scale_info::TypeInfo;

	/// The old `ParaLifecycle` enum, including the parathread variants that are being removed.
	#[derive(PartialEq, Eq, Clone, Encode, Decode, Debug, TypeInfo)]
	pub enum ParaLifecycle {
		Onboarding,
		Parathread,
		Parachain,
		UpgradingParathread,
		DowngradingParachain,
		OffboardingParathread,
		OffboardingParachain,
	}

	/// Storage alias so we can read the old encoded layout during migration.
	#[storage_alias]
	pub type ParaLifecycles<T: Config> = StorageMap<Pallet<T>, Twox64Concat, ParaId, ParaLifecycle>;
}

mod v1 {
	use super::v0;
	use crate::paras::{
		Config, Pallet, ParaLifecycle, ParaLifecycles as V1ParaLifecycles, ParachainsCache,
	};
	use alloc::vec::Vec;
	use frame_support::{traits::UncheckedOnRuntimeUpgrade, weights::Weight};
	use polkadot_primitives::Id as ParaId;

	#[cfg(feature = "try-runtime")]
	use codec::{Decode, Encode};
	#[cfg(feature = "try-runtime")]
	use frame_support::ensure;

	pub struct VersionUncheckedMigrateToV1<T>(core::marker::PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateToV1<T> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let parathread_count = v0::ParaLifecycles::<T>::iter()
				.filter(|(_, lc)| {
					matches!(
						lc,
						v0::ParaLifecycle::Parathread |
							v0::ParaLifecycle::UpgradingParathread |
							v0::ParaLifecycle::OffboardingParathread |
							v0::ParaLifecycle::DowngradingParachain
					)
				})
				.count() as u32;
			log::info!(
				target: crate::paras::LOG_TARGET,
				"paras MigrateToV1 pre_upgrade: {} parathread lifecycle entries to migrate",
				parathread_count,
			);
			Ok(parathread_count.encode())
		}

		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();
			let mut parachains = ParachainsCache::<T>::new();

			// Collect all entries from the old storage.
			let all_entries: Vec<(ParaId, v0::ParaLifecycle)> =
				v0::ParaLifecycles::<T>::drain().collect();

			// One read per entry in the drain.
			weight = weight.saturating_add(T::DbWeight::get().reads(all_entries.len() as u64));

			for (para, old_lifecycle) in all_entries {
				let new_lifecycle = match old_lifecycle {
					// Stable states: Onboarding and Parachain are unchanged.
					v0::ParaLifecycle::Onboarding => Some(ParaLifecycle::Onboarding),
					v0::ParaLifecycle::Parachain => Some(ParaLifecycle::Parachain),

					// Parathread → Parachain: register in the parachains list.
					v0::ParaLifecycle::Parathread => {
						parachains.add(para);
						Some(ParaLifecycle::Parachain)
					},

					// UpgradingParathread → Parachain: the upgrade is enacted immediately.
					v0::ParaLifecycle::UpgradingParathread => {
						parachains.add(para);
						Some(ParaLifecycle::Parachain)
					},

					// DowngradingParachain → Parachain: cancel the downgrade.
					v0::ParaLifecycle::DowngradingParachain => Some(ParaLifecycle::Parachain),

					// OffboardingParathread → OffboardingParachain: keep offboarding.
					v0::ParaLifecycle::OffboardingParathread => {
						Some(ParaLifecycle::OffboardingParachain)
					},

					// OffboardingParachain: unchanged.
					v0::ParaLifecycle::OffboardingParachain => {
						Some(ParaLifecycle::OffboardingParachain)
					},
				};

				if let Some(lc) = new_lifecycle {
					V1ParaLifecycles::<T>::insert(&para, lc);
					weight = weight.saturating_add(T::DbWeight::get().writes(1));
				}
			}

			// Persist any changes to the Parachains list.
			drop(parachains);
			weight = weight.saturating_add(T::DbWeight::get().writes(1));

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let parathread_count_before =
				u32::decode(&mut &state[..]).expect("Was properly encoded") as usize;

			// After migration there must be no parathread lifecycle entries.
			let remaining_parathread_entries = V1ParaLifecycles::<T>::iter()
				.filter(|(_, lc)| {
					!matches!(
						lc,
						ParaLifecycle::Onboarding |
							ParaLifecycle::Parachain |
							ParaLifecycle::OffboardingParachain
					)
				})
				.count();

			ensure!(
				remaining_parathread_entries == 0,
				"All parathread lifecycle entries must be removed after migration"
			);

			log::info!(
				target: crate::paras::LOG_TARGET,
				"paras MigrateToV1 post_upgrade: migrated {} parathread entries, 0 remaining",
				parathread_count_before,
			);

			Ok(())
		}
	}

	/// Migrate `ParaLifecycles` storage to remove all parathread-related variants.
	/// - `Parathread` and `UpgradingParathread` become `Parachain`.
	/// - `OffboardingParathread` becomes `OffboardingParachain`.
	/// - `DowngradingParachain` becomes `Parachain` (downgrade cancelled).
	pub type MigrateToV1<T> = frame_support::migrations::VersionedMigration<
		0,
		1,
		VersionUncheckedMigrateToV1<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

#[cfg(test)]
mod tests {
	use super::{v0, v1::VersionUncheckedMigrateToV1};
	use crate::{
		mock::{new_test_ext, MockGenesisConfig, Test},
		paras::{ParaLifecycle, ParaLifecycles, Parachains},
	};
	use frame_support::traits::UncheckedOnRuntimeUpgrade;
	use polkadot_primitives::Id as ParaId;

	#[test]
	fn migrate_to_v1_parathread_becomes_parachain() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Insert parathread lifecycle entries using the old storage type.
			v0::ParaLifecycles::<Test>::insert(ParaId::from(1u32), v0::ParaLifecycle::Parathread);
			v0::ParaLifecycles::<Test>::insert(
				ParaId::from(2u32),
				v0::ParaLifecycle::UpgradingParathread,
			);
			v0::ParaLifecycles::<Test>::insert(
				ParaId::from(3u32),
				v0::ParaLifecycle::OffboardingParathread,
			);
			v0::ParaLifecycles::<Test>::insert(
				ParaId::from(4u32),
				v0::ParaLifecycle::DowngradingParachain,
			);
			v0::ParaLifecycles::<Test>::insert(ParaId::from(5u32), v0::ParaLifecycle::Parachain);
			v0::ParaLifecycles::<Test>::insert(ParaId::from(6u32), v0::ParaLifecycle::Onboarding);

			<VersionUncheckedMigrateToV1<Test> as UncheckedOnRuntimeUpgrade>::on_runtime_upgrade();

			// Para 1: Parathread → Parachain
			assert_eq!(
				ParaLifecycles::<Test>::get(ParaId::from(1u32)),
				Some(ParaLifecycle::Parachain)
			);
			assert!(Parachains::<Test>::get().contains(&ParaId::from(1u32)));

			// Para 2: UpgradingParathread → Parachain
			assert_eq!(
				ParaLifecycles::<Test>::get(ParaId::from(2u32)),
				Some(ParaLifecycle::Parachain)
			);
			assert!(Parachains::<Test>::get().contains(&ParaId::from(2u32)));

			// Para 3: OffboardingParathread → OffboardingParachain
			assert_eq!(
				ParaLifecycles::<Test>::get(ParaId::from(3u32)),
				Some(ParaLifecycle::OffboardingParachain)
			);

			// Para 4: DowngradingParachain → Parachain (downgrade cancelled)
			assert_eq!(
				ParaLifecycles::<Test>::get(ParaId::from(4u32)),
				Some(ParaLifecycle::Parachain)
			);

			// Para 5: Parachain unchanged
			assert_eq!(
				ParaLifecycles::<Test>::get(ParaId::from(5u32)),
				Some(ParaLifecycle::Parachain)
			);

			// Para 6: Onboarding unchanged
			assert_eq!(
				ParaLifecycles::<Test>::get(ParaId::from(6u32)),
				Some(ParaLifecycle::Onboarding)
			);
		});
	}
}
