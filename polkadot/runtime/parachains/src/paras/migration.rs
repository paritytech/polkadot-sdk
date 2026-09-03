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
	use sp_core::Get;

	#[cfg(feature = "try-runtime")]
	use codec::{Decode, Encode};
	#[cfg(feature = "try-runtime")]
	use frame_support::ensure;

	/// The lifecycle a para ends up with. Shared with `pre_upgrade` so the expectation
	/// `post_upgrade` checks cannot drift from what the migration writes.
	fn migrated_lifecycle(old: &v0::ParaLifecycle) -> ParaLifecycle {
		match old {
			v0::ParaLifecycle::Onboarding => ParaLifecycle::Onboarding,
			v0::ParaLifecycle::Parachain => ParaLifecycle::Parachain,
			v0::ParaLifecycle::Parathread => ParaLifecycle::Parachain,
			// Upgrade enacted immediately, downgrade cancelled.
			v0::ParaLifecycle::UpgradingParathread => ParaLifecycle::Parachain,
			v0::ParaLifecycle::DowngradingParachain => ParaLifecycle::Parachain,
			v0::ParaLifecycle::OffboardingParathread => ParaLifecycle::OffboardingParachain,
			v0::ParaLifecycle::OffboardingParachain => ParaLifecycle::OffboardingParachain,
		}
	}

	/// Whether the migration has to add the para to the `Parachains` list. Only the former
	/// parathreads; everything else was already in it or is offboarding.
	fn joins_parachains_list(old: &v0::ParaLifecycle) -> bool {
		matches!(old, v0::ParaLifecycle::Parathread | v0::ParaLifecycle::UpgradingParathread)
	}

	pub struct VersionUncheckedMigrateToV1<T>(core::marker::PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateToV1<T> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			// The lifecycle every para is expected to hold afterwards, checked one by one in
			// `post_upgrade`.
			let expected: Vec<(ParaId, ParaLifecycle)> = v0::ParaLifecycles::<T>::iter()
				.map(|(para, old_lifecycle)| (para, migrated_lifecycle(&old_lifecycle)))
				.collect();

			log::info!(
				target: crate::paras::LOG_TARGET,
				"paras MigrateToV1 pre_upgrade: {} lifecycle entries to migrate",
				expected.len(),
			);

			Ok(expected.encode())
		}

		fn on_runtime_upgrade() -> Weight {
			let mut parachains = ParachainsCache::<T>::new();
			let mut parachains_touched = false;

			let all_entries: Vec<(ParaId, v0::ParaLifecycle)> =
				v0::ParaLifecycles::<T>::drain().collect();

			// The drain reads every entry and deletes it.
			let mut reads = all_entries.len() as u64;
			let mut writes = all_entries.len() as u64;

			for (para, old_lifecycle) in all_entries {
				if joins_parachains_list(&old_lifecycle) {
					parachains.add(para);
					parachains_touched = true;
				}

				V1ParaLifecycles::<T>::insert(&para, migrated_lifecycle(&old_lifecycle));
				writes = writes.saturating_add(1);
			}

			// `ParachainsCache` reads `Parachains` when first touched and writes it back on drop;
			// untouched, it does neither.
			if parachains_touched {
				reads = reads.saturating_add(1);
				writes = writes.saturating_add(1);
			}
			drop(parachains);

			T::DbWeight::get().reads_writes(reads, writes)
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let expected = <Vec<(ParaId, ParaLifecycle)>>::decode(&mut &state[..])
				.expect("Was properly encoded");

			// `iter_keys` does not decode values, so entries left in the old encoding still count.
			ensure!(
				V1ParaLifecycles::<T>::iter_keys().count() == expected.len(),
				"paras MigrateToV1: the number of lifecycle entries changed"
			);

			// A para that was missed either fails to decode, giving `None`, or decodes to another
			// variant, since the variant indices changed.
			for (para, expected_lifecycle) in expected.iter() {
				ensure!(
					V1ParaLifecycles::<T>::get(para).as_ref() == Some(expected_lifecycle),
					"paras MigrateToV1: para lifecycle was not migrated as expected"
				);
			}

			log::info!(
				target: crate::paras::LOG_TARGET,
				"paras MigrateToV1 post_upgrade: verified {} lifecycle entries",
				expected.len(),
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

	#[cfg(feature = "try-runtime")]
	#[test]
	fn migrate_to_v1_post_upgrade_verifies_every_para() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			insert_v0_lifecycles();

			let state =
				<VersionUncheckedMigrateToV1<Test> as UncheckedOnRuntimeUpgrade>::pre_upgrade()
					.unwrap();
			<VersionUncheckedMigrateToV1<Test> as UncheckedOnRuntimeUpgrade>::on_runtime_upgrade();

			assert!(
				<VersionUncheckedMigrateToV1<Test> as UncheckedOnRuntimeUpgrade>::post_upgrade(
					state
				)
				.is_ok()
			);
		});
	}

	#[cfg(feature = "try-runtime")]
	#[test]
	fn migrate_to_v1_post_upgrade_catches_an_unmigrated_para() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			insert_v0_lifecycles();

			let state =
				<VersionUncheckedMigrateToV1<Test> as UncheckedOnRuntimeUpgrade>::pre_upgrade()
					.unwrap();
			<VersionUncheckedMigrateToV1<Test> as UncheckedOnRuntimeUpgrade>::on_runtime_upgrade();

			// One entry back in the old encoding, as if skipped. The count is unchanged, so only
			// the per-para check catches it.
			v0::ParaLifecycles::<Test>::insert(
				ParaId::from(3u32),
				v0::ParaLifecycle::OffboardingParathread,
			);

			assert!(
				<VersionUncheckedMigrateToV1<Test> as UncheckedOnRuntimeUpgrade>::post_upgrade(
					state
				)
				.is_err()
			);
		});
	}

	#[cfg(feature = "try-runtime")]
	#[test]
	fn migrate_to_v1_post_upgrade_catches_a_dropped_para() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			insert_v0_lifecycles();

			let state =
				<VersionUncheckedMigrateToV1<Test> as UncheckedOnRuntimeUpgrade>::pre_upgrade()
					.unwrap();
			<VersionUncheckedMigrateToV1<Test> as UncheckedOnRuntimeUpgrade>::on_runtime_upgrade();

			ParaLifecycles::<Test>::remove(ParaId::from(1u32));

			assert!(
				<VersionUncheckedMigrateToV1<Test> as UncheckedOnRuntimeUpgrade>::post_upgrade(
					state
				)
				.is_err()
			);
		});
	}

	/// One para in every old lifecycle variant.
	#[cfg(feature = "try-runtime")]
	fn insert_v0_lifecycles() {
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
		v0::ParaLifecycles::<Test>::insert(
			ParaId::from(7u32),
			v0::ParaLifecycle::OffboardingParachain,
		);
	}
}
