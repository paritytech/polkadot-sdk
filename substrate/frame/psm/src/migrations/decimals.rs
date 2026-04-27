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

//! One-shot migration that populates decimal snapshots for a pre-existing PSM
//! deployment.
//!
//! Purpose: chains that approved external assets before the multi-decimal upgrade
//! have entries in `ExternalAssets` but no `AssetDecimals` snapshots, and no
//! `StableDecimals` either. Mint and redeem both require these snapshots and
//! will fail closed (`Error::DecimalsMismatch` / `Error::Unexpected`) until they
//! are populated. This migration reads live metadata and writes the snapshots.
//!
//! Out-of-range assets are handled gracefully: if an existing asset's decimals
//! differ from the stable asset's decimals by more than [`MAX_DECIMALS_DIFF`],
//! the migration still writes its decimals snapshot but flips its circuit
//! breaker to [`CircuitBreakerLevel::AllDisabled`]. The chain keeps upgrading;
//! governance can remove or re-enable the asset later once the off-chain
//! situation is resolved. The `try-runtime` post-upgrade hook verifies this
//! invariant — any out-of-range asset must end up disabled.
//!
//! Safe to run multiple times — already-populated snapshots are not overwritten.
//!
//! # Usage
//!
//! ```ignore
//! pub type Migrations = (
//!     pallet_psm::migrations::decimals::PopulateDecimals<Runtime>,
//!     // ... other migrations
//! );
//! ```

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
use frame_support::{
	migrations::VersionedMigration,
	pallet_prelude::Weight,
	traits::{
		fungible::metadata::Inspect as FungibleMetadataInspect,
		fungibles::metadata::Inspect as FungiblesMetadataInspect, Get, UncheckedOnRuntimeUpgrade,
	},
};

use crate::{
	pallet::{
		AssetDecimals, CircuitBreakerLevel, ExternalAssets, StableDecimals, MAX_DECIMALS_DIFF,
	},
	Config, Pallet,
};

#[cfg(feature = "try-runtime")]
use frame_support::ensure;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

const LOG_TARGET: &str = "runtime::psm::migration::populate_decimals";

/// Version-gated v1 -> v2 migration that fills in decimal snapshots for all
/// pre-existing external assets and the stable asset, and bumps the pallet
/// on-chain storage version from 1 to 2.
pub type PopulateDecimals<T> = VersionedMigration<
	1,
	2,
	InnerPopulateDecimals<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

/// Version-unchecked migration logic. Exposed only for use by [`PopulateDecimals`].
///
/// Should never be placed directly into a runtime's migrations tuple — use the
/// versioned alias [`PopulateDecimals`] so the on-chain storage version is
/// checked and bumped.
pub struct InnerPopulateDecimals<T>(core::marker::PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for InnerPopulateDecimals<T> {
	fn on_runtime_upgrade() -> Weight {
		log::info!(
			target: LOG_TARGET,
			"Running PopulateDecimals: backfilling decimal snapshots"
		);

		let mut reads = 0u64;
		let mut writes = 0u64;

		// Stable asset snapshot — only write if missing.
		reads += 2;
		let stable_decimals = T::StableAsset::decimals();
		if !StableDecimals::<T>::exists() {
			StableDecimals::<T>::put(stable_decimals);
			writes += 1;
		}

		// Per-asset snapshots. Walk every approved external asset.
		for (asset_id, status) in ExternalAssets::<T>::iter() {
			reads += 3; // ExternalAssets iter item + AssetDecimals + Fungibles::decimals reads below
			if AssetDecimals::<T>::contains_key(asset_id) {
				log::info!(
					target: LOG_TARGET,
					"Asset {:?} already has a decimals snapshot, skipping",
					asset_id,
				);
				continue;
			}

			let asset_decimals = T::Fungibles::decimals(asset_id);
			AssetDecimals::<T>::insert(asset_id, asset_decimals);
			writes += 1;

			let diff = asset_decimals.abs_diff(stable_decimals) as u32;
			if diff > MAX_DECIMALS_DIFF {
				// Do not fail the migration. Disable swaps for this asset so
				// mint/redeem cannot operate on an unsupported decimal gap. The
				// snapshot is still written — it preserves the observed state
				// and lets the runtime guard surface the divergence clearly.
				if status != CircuitBreakerLevel::AllDisabled {
					ExternalAssets::<T>::insert(asset_id, CircuitBreakerLevel::AllDisabled);
					writes += 1;
				}
				log::warn!(
					target: LOG_TARGET,
					"Asset {:?} decimals diff ({}) exceeds MAX_DECIMALS_DIFF ({}); disabling",
					asset_id,
					diff,
					MAX_DECIMALS_DIFF,
				);
			} else {
				log::info!(
					target: LOG_TARGET,
					"Populated decimals snapshot for asset {:?} (decimals={})",
					asset_id,
					asset_decimals,
				);
			}
		}

		log::info!(
			target: LOG_TARGET,
			"PopulateDecimals complete"
		);

		T::DbWeight::get().reads_writes(reads, writes)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		Ok(Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
		// Stable asset snapshot present and consistent with live metadata.
		ensure!(
			StableDecimals::<T>::get() == Some(T::StableAsset::decimals()),
			"StableDecimals snapshot missing or stale after migration"
		);

		let stable_decimals = T::StableAsset::decimals();
		for (asset_id, status) in ExternalAssets::<T>::iter() {
			let snapshot = AssetDecimals::<T>::get(asset_id)
				.ok_or("Approved external asset missing decimals snapshot after migration")?;
			ensure!(
				snapshot == T::Fungibles::decimals(asset_id),
				"AssetDecimals snapshot differs from live metadata after migration"
			);
			let diff = snapshot.abs_diff(stable_decimals) as u32;
			if diff > MAX_DECIMALS_DIFF {
				ensure!(
					status == CircuitBreakerLevel::AllDisabled,
					"Out-of-range external asset was not disabled by migration"
				);
			}
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		mock::{
			new_test_ext, RuntimeOrigin, Test, ALICE, DAI_MOCK_ASSET_ID, USDC_ASSET_ID,
			USDT_ASSET_ID,
		},
		Pallet,
	};
	use frame_support::{
		assert_ok,
		traits::{GetStorageVersion, OnRuntimeUpgrade, StorageVersion},
	};

	/// The wrapper only runs when on-chain version is 1. Genesis sets it to 2,
	/// so tests must roll it back to simulate a v1 chain.
	fn prepare_v1() {
		StorageVersion::new(1).put::<Pallet<Test>>();
	}

	#[test]
	fn populate_decimals_backfills_existing_assets() {
		new_test_ext().execute_with(|| {
			// Simulate a pre-migration state: existing assets have ExternalAssets
			// entries but no decimals snapshots (and no StableDecimals).
			prepare_v1();
			StableDecimals::<Test>::kill();
			AssetDecimals::<Test>::remove(USDC_ASSET_ID);
			AssetDecimals::<Test>::remove(USDT_ASSET_ID);

			PopulateDecimals::<Test>::on_runtime_upgrade();

			assert_eq!(StableDecimals::<Test>::get(), Some(6));
			assert_eq!(AssetDecimals::<Test>::get(USDC_ASSET_ID), Some(6));
			assert_eq!(AssetDecimals::<Test>::get(USDT_ASSET_ID), Some(6));
			// Normal status preserved since decimals are in range.
			assert_eq!(
				ExternalAssets::<Test>::get(USDC_ASSET_ID),
				Some(CircuitBreakerLevel::AllEnabled)
			);
			assert_eq!(
				ExternalAssets::<Test>::get(USDT_ASSET_ID),
				Some(CircuitBreakerLevel::AllEnabled)
			);
		});
	}

	#[test]
	fn populate_decimals_does_not_overwrite_existing_snapshots() {
		new_test_ext().execute_with(|| {
			prepare_v1();
			// Genesis already wrote snapshots. Plant a sentinel to verify the
			// migration does not overwrite it.
			AssetDecimals::<Test>::insert(USDC_ASSET_ID, 42u8);

			PopulateDecimals::<Test>::on_runtime_upgrade();

			assert_eq!(AssetDecimals::<Test>::get(USDC_ASSET_ID), Some(42));
		});
	}

	#[test]
	fn populate_decimals_disables_out_of_range_assets() {
		new_test_ext().execute_with(|| {
			// Simulate: DAI_MOCK (18 decimals) was approved under a prior pUSD
			// configuration; then pUSD metadata was changed to something exotic
			// that makes the diff exceed MAX_DECIMALS_DIFF. We fake this by
			// approving DAI and then shifting the stable asset's live decimals
			// through metadata update.
			use crate::mock::{Assets, PUSD_ASSET_ID};
			use frame_support::traits::fungibles::metadata::Mutate as MetadataMutate;

			assert_ok!(Pallet::<Test>::add_external_asset(
				RuntimeOrigin::root(),
				DAI_MOCK_ASSET_ID
			));
			// DAI has 18 decimals; pUSD currently 6; diff = 12 (in range).
			// Shift pUSD to 40 decimals so the diff becomes 22 — still in range
			// (MAX_DECIMALS_DIFF is 24). Push further to make diff too large:
			// setting pUSD to the extreme (say, 45) would push diff = 27, > 24.
			assert_ok!(<Assets as MetadataMutate<u64>>::set(
				PUSD_ASSET_ID,
				&ALICE,
				b"pUSD".to_vec(),
				b"pUSD".to_vec(),
				45,
			));

			// Wipe DAI's snapshot and StableDecimals to force repopulation, then
			// roll back to v1 so the versioned wrapper actually runs.
			AssetDecimals::<Test>::remove(DAI_MOCK_ASSET_ID);
			StableDecimals::<Test>::kill();
			prepare_v1();

			PopulateDecimals::<Test>::on_runtime_upgrade();

			// Snapshot was written regardless.
			assert_eq!(StableDecimals::<Test>::get(), Some(45));
			assert_eq!(AssetDecimals::<Test>::get(DAI_MOCK_ASSET_ID), Some(18));
			// DAI is disabled because 45 - 18 = 27 > MAX_DECIMALS_DIFF (24).
			assert_eq!(
				ExternalAssets::<Test>::get(DAI_MOCK_ASSET_ID),
				Some(CircuitBreakerLevel::AllDisabled)
			);
			// In-range assets stay enabled.
			assert_eq!(
				ExternalAssets::<Test>::get(USDC_ASSET_ID),
				Some(CircuitBreakerLevel::AllEnabled)
			);
		});
	}

	#[test]
	fn populate_decimals_runs_once_then_skips() {
		new_test_ext().execute_with(|| {
			prepare_v1();
			StableDecimals::<Test>::kill();
			AssetDecimals::<Test>::remove(USDC_ASSET_ID);

			// First run: on-chain version is 1, migration executes and bumps to 2.
			PopulateDecimals::<Test>::on_runtime_upgrade();
			assert_eq!(Pallet::<Test>::on_chain_storage_version(), StorageVersion::new(2));
			let stable1 = StableDecimals::<Test>::get();
			let usdc1 = AssetDecimals::<Test>::get(USDC_ASSET_ID);

			// Second run: on-chain version is 2, versioned wrapper skips — state
			// is unchanged.
			PopulateDecimals::<Test>::on_runtime_upgrade();
			assert_eq!(StableDecimals::<Test>::get(), stable1);
			assert_eq!(AssetDecimals::<Test>::get(USDC_ASSET_ID), usdc1);
			assert_eq!(Pallet::<Test>::on_chain_storage_version(), StorageVersion::new(2));
		});
	}

	#[test]
	fn populate_decimals_skips_when_not_on_version_one() {
		new_test_ext().execute_with(|| {
			// Simulate an already-upgraded chain at v2. Wrapper must skip.
			StorageVersion::new(2).put::<Pallet<Test>>();

			AssetDecimals::<Test>::remove(USDC_ASSET_ID);
			PopulateDecimals::<Test>::on_runtime_upgrade();

			// Snapshot not repopulated — migration was skipped.
			assert_eq!(AssetDecimals::<Test>::get(USDC_ASSET_ID), None);
			// Version unchanged.
			assert_eq!(Pallet::<Test>::on_chain_storage_version(), StorageVersion::new(2));
		});
	}
}
