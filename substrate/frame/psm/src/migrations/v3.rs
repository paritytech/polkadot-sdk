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

//! Storage v2 -> v3 migration.
//!
//! In v3 per-instance configuration (fee destination, debt ceiling, internal-asset decimals
//! snapshot) is collapsed into a single [`PsmInfo`] record stored in [`Psms`], keyed by the
//! internal asset id. The standalone storage values `MaxPsmDebtOfTotal` and
//! `InternalDecimals` are removed, as are the `Config` items `FeeDestination` and
//! `MaximumIssuance`.
//!
//! This migration:
//! 1. Reads the legacy storage values (`MaxPsmDebtOfTotal`, `InternalDecimals`) via
//!    [`storage_alias`].
//! 2. Reads the legacy `MaximumIssuance` and `FeeDestination` from the runtime through the
//!    [`MigrateV2ToV3Config`] trait (these were `Config` constants and have no on-chain record).
//! 3. Inserts `Psms[InternalAssetId]` with `fee_destination = legacy_fee_destination` and `max_debt
//!    = legacy_max_issuance × legacy ratio`.
//! 4. Clears the legacy storage values.
//!
//! # Usage
//!
//! ```ignore
//! pub struct PsmV3MigrationConfig;
//! impl pallet_psm::migrations::v3::MigrateV2ToV3Config<Runtime> for PsmV3MigrationConfig {
//!     fn legacy_max_issuance() -> Balance { /* pre-v3 Config::MaximumIssuance value */ }
//!     fn legacy_fee_destination() -> AccountId { /* pre-v3 Config::FeeDestination value */ }
//! }
//!
//! pub type Migrations = (
//!     pallet_psm::migrations::v3::MigrateV2ToV3<Runtime, PsmV3MigrationConfig>,
//!     // ... other migrations
//! );
//! ```

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
use frame_support::{
	migrations::VersionedMigration,
	pallet_prelude::Weight,
	storage_alias,
	traits::{
		fungibles::metadata::Inspect as FungiblesMetadataInspect, Get, UncheckedOnRuntimeUpgrade,
	},
};
use sp_runtime::Permill;

use crate::{
	pallet::{BalanceOf, PsmInfo, Psms},
	Config, Pallet,
};

#[cfg(feature = "try-runtime")]
use frame_support::ensure;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

const LOG_TARGET: &str = "runtime::psm::migration::v3";

/// Legacy v2 storage layout. Used by the migration to read items that no longer exist in
/// the live pallet schema.
pub mod v2 {
	use super::*;
	use frame_support::pallet_prelude::{OptionQuery, ValueQuery};

	#[storage_alias]
	pub type MaxPsmDebtOfTotal<T: Config> = StorageValue<Pallet<T>, Permill, ValueQuery>;

	#[storage_alias]
	pub type InternalDecimals<T: Config> = StorageValue<Pallet<T>, u8, OptionQuery>;
}

/// Runtime-supplied legacy values that lived in `Config` before v3 and so cannot be
/// recovered from on-chain storage.
pub trait MigrateV2ToV3Config<T: Config> {
	/// The pre-v3 value of `Config::MaximumIssuance`.
	fn legacy_max_issuance() -> BalanceOf<T>;
	/// The pre-v3 value of `Config::FeeDestination`.
	fn legacy_fee_destination() -> T::AccountId;
}

/// Version-gated v2 -> v3 migration. Bumps the pallet on-chain storage version from 2 to 3
/// after running [`InnerMigrateV2ToV3`].
pub type MigrateV2ToV3<T, I> = VersionedMigration<
	2,
	3,
	InnerMigrateV2ToV3<T, I>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

/// Version-unchecked migration logic. Exposed only for use by [`MigrateV2ToV3`].
///
/// Should never be placed directly into a runtime's migrations tuple — use the versioned
/// alias [`MigrateV2ToV3`] so the on-chain storage version is checked and bumped.
pub struct InnerMigrateV2ToV3<T, I>(core::marker::PhantomData<(T, I)>);

impl<T: Config, I: MigrateV2ToV3Config<T>> UncheckedOnRuntimeUpgrade for InnerMigrateV2ToV3<T, I> {
	fn on_runtime_upgrade() -> Weight {
		log::info!(
			target: LOG_TARGET,
			"Running MigrateV2ToV3: collapsing per-instance configuration into Psms"
		);

		let ratio = v2::MaxPsmDebtOfTotal::<T>::get();
		let max_issuance = I::legacy_max_issuance();
		let max_debt = ratio.mul_floor(max_issuance);

		let internal_decimals = v2::InternalDecimals::<T>::get()
			.unwrap_or_else(|| T::Fungibles::decimals(T::InternalAssetId::get()));

		Psms::<T>::insert(
			T::InternalAssetId::get(),
			PsmInfo::<T> {
				fee_destination: I::legacy_fee_destination(),
				max_debt,
				internal_decimals,
			},
		);

		v2::MaxPsmDebtOfTotal::<T>::kill();
		v2::InternalDecimals::<T>::kill();

		log::info!(
			target: LOG_TARGET,
			"MigrateV2ToV3 complete: max_debt={:?}, internal_decimals={}",
			max_debt,
			internal_decimals,
		);

		// 2 reads (legacy values) + 1 fallback read for live decimals + 3 writes (Psms +
		// two kills).
		T::DbWeight::get().reads_writes(3, 3)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		Ok(Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
		ensure!(
			Psms::<T>::contains_key(T::InternalAssetId::get()),
			"Psms not populated after v2->v3 migration"
		);
		ensure!(
			!v2::MaxPsmDebtOfTotal::<T>::exists(),
			"Legacy MaxPsmDebtOfTotal still present after v2->v3 migration"
		);
		ensure!(
			!v2::InternalDecimals::<T>::exists(),
			"Legacy InternalDecimals still present after v2->v3 migration"
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		mock::{new_test_ext, Test, ALICE, INSURANCE_FUND, INTERNAL_ASSET_ID, INTERNAL_UNIT},
		Pallet,
	};
	use frame_support::traits::{GetStorageVersion, OnRuntimeUpgrade, StorageVersion};

	struct TestConfig;
	const TEST_MAX_ISSUANCE: u128 = 20_000_000 * INTERNAL_UNIT;
	const TEST_FEE_DEST: u64 = INSURANCE_FUND;

	impl MigrateV2ToV3Config<Test> for TestConfig {
		fn legacy_max_issuance() -> u128 {
			TEST_MAX_ISSUANCE
		}
		fn legacy_fee_destination() -> u64 {
			TEST_FEE_DEST
		}
	}

	/// Roll the mock back into a v2-shaped state: clear v3 storage, write the legacy
	/// values, set on-chain version to 2.
	fn prepare_v2(ratio: Permill, internal_decimals: u8) {
		Psms::<Test>::remove(INTERNAL_ASSET_ID);
		v2::MaxPsmDebtOfTotal::<Test>::put(ratio);
		v2::InternalDecimals::<Test>::put(internal_decimals);
		StorageVersion::new(2).put::<Pallet<Test>>();
	}

	#[test]
	fn migrates_legacy_values_into_psms() {
		new_test_ext().execute_with(|| {
			prepare_v2(Permill::from_percent(50), 6);

			MigrateV2ToV3::<Test, TestConfig>::on_runtime_upgrade();

			let info = Psms::<Test>::get(INTERNAL_ASSET_ID).expect("Psms populated");
			assert_eq!(info.fee_destination, TEST_FEE_DEST);
			assert_eq!(info.max_debt, Permill::from_percent(50).mul_floor(TEST_MAX_ISSUANCE));
			assert_eq!(info.internal_decimals, 6);

			assert!(!v2::MaxPsmDebtOfTotal::<Test>::exists());
			assert!(!v2::InternalDecimals::<Test>::exists());

			assert_eq!(Pallet::<Test>::on_chain_storage_version(), StorageVersion::new(3));
		});
	}

	#[test]
	fn falls_back_to_live_decimals_when_legacy_snapshot_missing() {
		new_test_ext().execute_with(|| {
			prepare_v2(Permill::from_percent(10), 6);
			v2::InternalDecimals::<Test>::kill();

			MigrateV2ToV3::<Test, TestConfig>::on_runtime_upgrade();

			let info = Psms::<Test>::get(INTERNAL_ASSET_ID).expect("Psms populated");
			// Live internal decimals are 6 (mock genesis).
			assert_eq!(info.internal_decimals, 6);
		});
	}

	#[test]
	fn skips_when_already_at_v3() {
		new_test_ext().execute_with(|| {
			// Genesis already ran at v3. Plant a sentinel to detect any overwrite.
			let sentinel =
				PsmInfo::<Test> { fee_destination: ALICE, max_debt: 42, internal_decimals: 42 };
			Psms::<Test>::insert(INTERNAL_ASSET_ID, sentinel.clone());

			MigrateV2ToV3::<Test, TestConfig>::on_runtime_upgrade();

			// Versioned wrapper skipped — Psms entry untouched.
			assert_eq!(Psms::<Test>::get(INTERNAL_ASSET_ID), Some(sentinel));
		});
	}

	#[test]
	fn runs_once_then_skips() {
		new_test_ext().execute_with(|| {
			prepare_v2(Permill::from_percent(25), 6);

			MigrateV2ToV3::<Test, TestConfig>::on_runtime_upgrade();
			let info_after_first = Psms::<Test>::get(INTERNAL_ASSET_ID).unwrap();

			// Plant different legacy values and re-run; the wrapper must skip because
			// on-chain version is already 3.
			v2::MaxPsmDebtOfTotal::<Test>::put(Permill::from_percent(99));
			MigrateV2ToV3::<Test, TestConfig>::on_runtime_upgrade();

			assert_eq!(Psms::<Test>::get(INTERNAL_ASSET_ID), Some(info_after_first));
			// And the legacy storage we just planted should still be there because the
			// migration did not run again.
			assert!(v2::MaxPsmDebtOfTotal::<Test>::exists());
		});
	}
}
