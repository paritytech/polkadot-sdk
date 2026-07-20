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
//! [`MigrateToV1`] slims `UpcomingParasGenesis` from `ParaGenesisArgs` to [`UpcomingParaGenesis`],
//! dropping the stored validation code.

use super::*;
use frame_support::{traits::UncheckedOnRuntimeUpgrade, weights::Weight};

#[cfg(feature = "try-runtime")]
use alloc::collections::btree_map::BTreeMap;
#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;

/// The old `UpcomingParasGenesis` layout, storing the full [`ParaGenesisArgs`].
mod v0 {
	use super::*;
	use frame_support::{storage_alias, Twox64Concat};

	#[storage_alias]
	pub type UpcomingParasGenesis<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, ParaId, ParaGenesisArgs>;
}

mod v1 {
	use super::*;

	pub struct VersionUncheckedMigrateToV1<T>(core::marker::PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateToV1<T> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			// Snapshot each entry's expected code hash: `Some` for legacy entries that still carry
			// a non-empty code, `None` for the eager-code entries.
			let snapshot: BTreeMap<ParaId, Option<ValidationCodeHash>> =
				v0::UpcomingParasGenesis::<T>::iter()
					.map(|(id, args)| {
						let code = (!args.validation_code.0.is_empty())
							.then(|| args.validation_code.hash());
						(id, code)
					})
					.collect();
			Ok(snapshot.encode())
		}

		fn on_runtime_upgrade() -> Weight {
			let mut weight = Weight::zero();
			UpcomingParasGenesis::<T>::translate::<ParaGenesisArgs, _>(|id, args| {
				weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
				// Legacy entries stored a non-empty code; link it as enactment used to.
				if !args.validation_code.0.is_empty() {
					let code_hash = args.validation_code.hash();
					weight = weight.saturating_add(Pallet::<T>::increase_code_ref(
						&code_hash,
						&args.validation_code,
					));
					CurrentCodeHash::<T>::insert(id, code_hash);
					weight = weight.saturating_add(T::DbWeight::get().writes(1));
				}
				Some(UpcomingParaGenesis {
					genesis_head: args.genesis_head,
					para_kind: args.para_kind,
				})
			});
			weight
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let snapshot = BTreeMap::<ParaId, Option<ValidationCodeHash>>::decode(&mut &state[..])
				.expect("pre_upgrade snapshot decodes; qed");
			ensure!(
				UpcomingParasGenesis::<T>::iter().count() == snapshot.len(),
				"UpcomingParasGenesis entry count changed"
			);
			for (id, expected_code) in snapshot {
				ensure!(
					UpcomingParasGenesis::<T>::contains_key(id),
					"para missing from UpcomingParasGenesis after migration"
				);
				if let Some(hash) = expected_code {
					ensure!(
						CurrentCodeHash::<T>::get(id) == Some(hash),
						"legacy entry code hash not linked after migration"
					);
				}
			}
			Ok(())
		}
	}
}

/// Migrate the paras pallet storage from v0 to v1.
pub type MigrateToV1<T> = frame_support::migrations::VersionedMigration<
	0,
	1,
	v1::VersionUncheckedMigrateToV1<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

#[cfg(test)]
mod tests {
	use super::{v1::VersionUncheckedMigrateToV1, *};
	use crate::mock::{new_test_ext, MockGenesisConfig, Test};
	use frame_support::traits::{
		GetStorageVersion, OnRuntimeUpgrade, StorageVersion, UncheckedOnRuntimeUpgrade,
	};

	#[test]
	fn migrate_sentinel_entry_keeps_code_hash() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			let id = ParaId::from(1);
			let code_hash = ValidationCode(vec![1, 2, 3]).hash();
			// A normal post-`schedule_para_initialize` entry: empty code, hash already linked.
			v0::UpcomingParasGenesis::<Test>::insert(
				id,
				ParaGenesisArgs {
					genesis_head: HeadData(vec![4, 5, 6]),
					validation_code: ValidationCode(Vec::new()),
					para_kind: ParaKind::Parachain,
				},
			);
			CurrentCodeHash::<Test>::insert(id, code_hash);

			<VersionUncheckedMigrateToV1<Test> as UncheckedOnRuntimeUpgrade>::on_runtime_upgrade();

			assert_eq!(
				UpcomingParasGenesis::<Test>::get(id),
				Some(UpcomingParaGenesis {
					genesis_head: HeadData(vec![4, 5, 6]),
					para_kind: ParaKind::Parachain,
				})
			);
			// Untouched: no code was carried, so nothing to link.
			assert_eq!(CurrentCodeHash::<Test>::get(id), Some(code_hash));
			assert!(!CodeByHash::<Test>::contains_key(code_hash));
		});
	}

	#[test]
	fn migrate_legacy_entry_links_code() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			let id = ParaId::from(2);
			let code = ValidationCode(vec![7, 8, 9]);
			let code_hash = code.hash();
			// A legacy entry that still carries a non-empty code and has no linked hash yet.
			v0::UpcomingParasGenesis::<Test>::insert(
				id,
				ParaGenesisArgs {
					genesis_head: HeadData(vec![1]),
					validation_code: code.clone(),
					para_kind: ParaKind::Parathread,
				},
			);

			<VersionUncheckedMigrateToV1<Test> as UncheckedOnRuntimeUpgrade>::on_runtime_upgrade();

			assert_eq!(
				UpcomingParasGenesis::<Test>::get(id),
				Some(UpcomingParaGenesis {
					genesis_head: HeadData(vec![1]),
					para_kind: ParaKind::Parathread,
				})
			);
			assert_eq!(CurrentCodeHash::<Test>::get(id), Some(code_hash));
			assert_eq!(CodeByHash::<Test>::get(code_hash), Some(code));
			assert_eq!(CodeByHashRefs::<Test>::get(code_hash), 1);
		});
	}

	#[test]
	fn migrate_empty_storage_is_noop() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			<VersionUncheckedMigrateToV1<Test> as UncheckedOnRuntimeUpgrade>::on_runtime_upgrade();
			assert_eq!(UpcomingParasGenesis::<Test>::iter().count(), 0);
		});
	}

	// Drives the full `MigrateToV1` (version-gated): bumps 0 -> 1, migrates both entry kinds, and a
	// second run is a no-op.
	#[test]
	fn migrate_v0_to_v1_bumps_version_and_is_idempotent() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			StorageVersion::new(0).put::<Pallet<Test>>();

			let sentinel = ParaId::from(1);
			let legacy = ParaId::from(2);
			let sentinel_hash = ValidationCode(vec![1, 2, 3]).hash();
			let legacy_code = ValidationCode(vec![7, 8, 9]);
			let legacy_hash = legacy_code.hash();

			v0::UpcomingParasGenesis::<Test>::insert(
				sentinel,
				ParaGenesisArgs {
					genesis_head: HeadData(vec![4]),
					validation_code: ValidationCode(Vec::new()),
					para_kind: ParaKind::Parachain,
				},
			);
			CurrentCodeHash::<Test>::insert(sentinel, sentinel_hash);
			v0::UpcomingParasGenesis::<Test>::insert(
				legacy,
				ParaGenesisArgs {
					genesis_head: HeadData(vec![5]),
					validation_code: legacy_code.clone(),
					para_kind: ParaKind::Parathread,
				},
			);

			MigrateToV1::<Test>::on_runtime_upgrade();

			assert_eq!(Pallet::<Test>::on_chain_storage_version(), 1);
			assert_eq!(UpcomingParasGenesis::<Test>::iter().count(), 2);
			assert_eq!(CurrentCodeHash::<Test>::get(sentinel), Some(sentinel_hash));
			assert_eq!(CurrentCodeHash::<Test>::get(legacy), Some(legacy_hash));
			assert_eq!(CodeByHash::<Test>::get(legacy_hash), Some(legacy_code));
			assert_eq!(CodeByHashRefs::<Test>::get(legacy_hash), 1);

			// Version is already 1, so a second run is gated out: no double code-ref.
			MigrateToV1::<Test>::on_runtime_upgrade();
			assert_eq!(Pallet::<Test>::on_chain_storage_version(), 1);
			assert_eq!(UpcomingParasGenesis::<Test>::iter().count(), 2);
			assert_eq!(CodeByHashRefs::<Test>::get(legacy_hash), 1);
		});
	}

	// pre_upgrade snapshot + post_upgrade checks pass on seeded sentinel and legacy entries.
	#[cfg(feature = "try-runtime")]
	#[test]
	fn try_runtime_checks_pass() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			let sentinel = ParaId::from(1);
			let legacy = ParaId::from(2);
			v0::UpcomingParasGenesis::<Test>::insert(
				sentinel,
				ParaGenesisArgs {
					genesis_head: HeadData(vec![4]),
					validation_code: ValidationCode(Vec::new()),
					para_kind: ParaKind::Parachain,
				},
			);
			CurrentCodeHash::<Test>::insert(sentinel, ValidationCode(vec![1]).hash());
			v0::UpcomingParasGenesis::<Test>::insert(
				legacy,
				ParaGenesisArgs {
					genesis_head: HeadData(vec![5]),
					validation_code: ValidationCode(vec![7, 8, 9]),
					para_kind: ParaKind::Parathread,
				},
			);

			let state = VersionUncheckedMigrateToV1::<Test>::pre_upgrade().unwrap();
			<VersionUncheckedMigrateToV1<Test> as UncheckedOnRuntimeUpgrade>::on_runtime_upgrade();
			VersionUncheckedMigrateToV1::<Test>::post_upgrade(state).unwrap();
		});
	}
}
