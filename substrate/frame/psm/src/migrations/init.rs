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

//! Idempotent migration to initialize PSM parameters for post-genesis deployment.
//!
//! This migration sets initial values for all configurable PSM parameters when
//! adding the pallet to an existing chain. Already-configured assets are skipped,
//! making it safe to run multiple times.
//!
//! # Usage
//!
//! Include in your runtime migrations:
//!
//! ```ignore
//! pub type Migrations = (
//!     pallet_psm::migrations::init::InitializePsm<Runtime, PsmInitialConfig>,
//!     // ... other migrations
//! );
//! ```
//!
//! Where `PsmInitialConfig` implements [`InitialPsmConfig`].

use alloc::collections::btree_map::BTreeMap;
#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
use frame_support::{
	pallet_prelude::{Get, Weight},
	traits::{
		fungible::metadata::Inspect as FungibleMetadataInspect,
		fungibles::metadata::Inspect as FungiblesMetadataInspect,
	},
};
use sp_runtime::Permill;

use crate::{
	pallet::{
		AssetCeilingWeight, AssetDecimals, CircuitBreakerLevel, ExternalAssets, MaxPsmDebtOfTotal,
		MintingFee, RedemptionFee, StableDecimals, MAX_DECIMALS_DIFF,
	},
	Config, Pallet,
};

#[cfg(feature = "try-runtime")]
use frame_support::ensure;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

const LOG_TARGET: &str = "runtime::psm::migration";

/// Configuration trait for initial PSM parameters.
///
/// Implement this trait in your runtime to provide the initial values used by
/// [`InitializePsm`].
pub trait InitialPsmConfig<T: Config> {
	/// Max PSM debt as a fraction of MaximumIssuance.
	fn max_psm_debt_of_total() -> Permill;

	/// Per-asset configuration:
	/// - minting fee
	/// - redemption fee
	/// - asset ceiling weight
	///
	/// Keys also define the set of approved external assets.
	fn asset_configs() -> BTreeMap<T::AssetId, (Permill, Permill, Permill)>;
}

/// Idempotent migration to initialize PSM pallet parameters.
///
/// This migration:
/// 1. Sets `MaxPsmDebtOfTotal`
/// 2. For each configured external asset, checks if it already exists. If not, adds it with
///    `AllEnabled` status and the configured fees and ceiling weight.
/// 3. Ensures the PSM and fee destination accounts exist
///
/// Safe to run multiple times — existing assets are not overwritten.
pub struct InitializePsm<T, I>(core::marker::PhantomData<(T, I)>);

impl<T: Config, I: InitialPsmConfig<T>> frame_support::traits::OnRuntimeUpgrade
	for InitializePsm<T, I>
{
	fn on_runtime_upgrade() -> Weight {
		log::info!(
			target: LOG_TARGET,
			"Running InitializePsm: initializing PSM pallet parameters"
		);

		let asset_configs = I::asset_configs();
		let mut reads = 0u64;
		let mut writes = 0u64;

		reads += 1;
		if !MaxPsmDebtOfTotal::<T>::exists() {
			MaxPsmDebtOfTotal::<T>::put(I::max_psm_debt_of_total());
			writes += 1;
		}

		// Stable decimals snapshot: populate from live metadata if not yet set.
		// Per-asset snapshots for pre-existing approved assets are owned by
		// `super::decimals::PopulateDecimals` — this migration only touches `AssetDecimals` for
		// assets it adds as new below.
		let stable_decimals = T::StableAsset::decimals();
		reads += 1;
		if !StableDecimals::<T>::exists() {
			StableDecimals::<T>::put(stable_decimals);
			writes += 1;
		}
		for (asset_id, (minting_fee, redemption_fee, ceiling_weight)) in &asset_configs {
			reads += 1;
			// Skip assets that are already configured.
			if ExternalAssets::<T>::contains_key(asset_id) {
				log::info!(
					target: LOG_TARGET,
					"Asset {:?} already configured, skipping",
					asset_id,
				);
				continue;
			}

			let asset_decimals = T::Fungibles::decimals(*asset_id);
			let diff = asset_decimals.abs_diff(stable_decimals) as u32;
			if diff > MAX_DECIMALS_DIFF {
				log::error!(
					target: LOG_TARGET,
					"Asset {:?} decimals diff ({}) exceeds MAX_DECIMALS_DIFF ({}), skipping",
					asset_id,
					diff,
					MAX_DECIMALS_DIFF,
				);
				continue;
			}
			ExternalAssets::<T>::insert(asset_id, CircuitBreakerLevel::AllEnabled);
			AssetDecimals::<T>::insert(asset_id, asset_decimals);
			MintingFee::<T>::insert(asset_id, minting_fee);
			RedemptionFee::<T>::insert(asset_id, redemption_fee);
			AssetCeilingWeight::<T>::insert(asset_id, ceiling_weight);
			writes += 5;

			log::info!(
				target: LOG_TARGET,
				"Configured external asset {:?} (decimals={})",
				asset_id,
				asset_decimals,
			);
		}

		Pallet::<T>::ensure_account_exists(&Pallet::<T>::account_id());
		Pallet::<T>::ensure_account_exists(&T::FeeDestination::get());
		writes += 2;

		log::info!(
			target: LOG_TARGET,
			"InitializePsm complete"
		);

		T::DbWeight::get().reads_writes(reads, writes)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		Ok(Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
		ensure!(
			MaxPsmDebtOfTotal::<T>::get() == I::max_psm_debt_of_total(),
			"MaxPsmDebtOfTotal mismatch after migration"
		);

		for (asset_id, (minting_fee, redemption_fee, ceiling_weight)) in I::asset_configs() {
			ensure!(
				ExternalAssets::<T>::get(asset_id) == Some(CircuitBreakerLevel::AllEnabled),
				"External asset missing or not AllEnabled after migration"
			);
			ensure!(
				MintingFee::<T>::get(asset_id) == minting_fee,
				"MintingFee mismatch after migration"
			);
			ensure!(
				RedemptionFee::<T>::get(asset_id) == redemption_fee,
				"RedemptionFee mismatch after migration"
			);
			ensure!(
				AssetCeilingWeight::<T>::get(asset_id) == ceiling_weight,
				"AssetCeilingWeight mismatch after migration"
			);
		}

		let psm_account = Pallet::<T>::account_id();
		ensure!(
			frame_system::Pallet::<T>::account_exists(&psm_account),
			"PSM account does not exist after migration"
		);

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Assets, Test, ALICE, USDC_ASSET_ID, USDT_ASSET_ID};
	use frame_support::assert_ok;

	struct TestPsmConfig;

	impl InitialPsmConfig<Test> for TestPsmConfig {
		fn max_psm_debt_of_total() -> Permill {
			Permill::from_percent(25)
		}

		fn asset_configs() -> BTreeMap<u32, (Permill, Permill, Permill)> {
			[
				(
					USDC_ASSET_ID,
					(
						Permill::from_parts(5_000),
						Permill::from_parts(5_000),
						Permill::from_percent(50),
					),
				),
				(
					USDT_ASSET_ID,
					(
						Permill::from_parts(3_000),
						Permill::from_parts(7_000),
						Permill::from_percent(50),
					),
				),
			]
			.into_iter()
			.collect()
		}
	}

	fn clear_all_psm_state() {
		MaxPsmDebtOfTotal::<Test>::kill();
		StableDecimals::<Test>::kill();
		ExternalAssets::<Test>::remove(USDC_ASSET_ID);
		ExternalAssets::<Test>::remove(USDT_ASSET_ID);
		MintingFee::<Test>::remove(USDC_ASSET_ID);
		MintingFee::<Test>::remove(USDT_ASSET_ID);
		RedemptionFee::<Test>::remove(USDC_ASSET_ID);
		RedemptionFee::<Test>::remove(USDT_ASSET_ID);
		AssetCeilingWeight::<Test>::remove(USDC_ASSET_ID);
		AssetCeilingWeight::<Test>::remove(USDT_ASSET_ID);
		AssetDecimals::<Test>::remove(USDC_ASSET_ID);
		AssetDecimals::<Test>::remove(USDT_ASSET_ID);
	}

	#[test]
	fn initialize_psm_configures_new_assets() {
		use frame_support::traits::{
			fungible::metadata::Inspect as _, fungibles::metadata::Inspect as _, OnRuntimeUpgrade,
		};

		new_test_ext().execute_with(|| {
			clear_all_psm_state();

			InitializePsm::<Test, TestPsmConfig>::on_runtime_upgrade();

			assert_eq!(MaxPsmDebtOfTotal::<Test>::get(), TestPsmConfig::max_psm_debt_of_total());
			assert_eq!(
				StableDecimals::<Test>::get(),
				Some(<Test as Config>::StableAsset::decimals())
			);

			for (asset_id, (minting_fee, redemption_fee, ceiling_weight)) in
				TestPsmConfig::asset_configs()
			{
				assert_eq!(
					ExternalAssets::<Test>::get(asset_id),
					Some(CircuitBreakerLevel::AllEnabled)
				);
				// New assets get their decimals snapshot.
				assert_eq!(
					AssetDecimals::<Test>::get(asset_id),
					Some(<Test as Config>::Fungibles::decimals(asset_id))
				);
				assert_eq!(MintingFee::<Test>::get(asset_id), minting_fee);
				assert_eq!(RedemptionFee::<Test>::get(asset_id), redemption_fee);
				assert_eq!(AssetCeilingWeight::<Test>::get(asset_id), ceiling_weight);
			}
		});
	}

	#[test]
	fn initialize_psm_populates_stable_decimals_when_missing() {
		use frame_support::traits::{fungible::metadata::Inspect as _, OnRuntimeUpgrade};

		new_test_ext().execute_with(|| {
			// StableDecimals was populated by genesis; clear it to simulate a
			// pre-decimal-snapshot deployment where the migration must seed it.
			StableDecimals::<Test>::kill();

			InitializePsm::<Test, TestPsmConfig>::on_runtime_upgrade();

			assert_eq!(
				StableDecimals::<Test>::get(),
				Some(<Test as Config>::StableAsset::decimals())
			);
		});
	}

	#[test]
	fn initialize_psm_preserves_existing_stable_decimals() {
		use frame_support::traits::OnRuntimeUpgrade;

		new_test_ext().execute_with(|| {
			// Plant a sentinel (non-live) value. The migration must not overwrite.
			StableDecimals::<Test>::put(42u8);

			InitializePsm::<Test, TestPsmConfig>::on_runtime_upgrade();

			assert_eq!(StableDecimals::<Test>::get(), Some(42));
		});
	}

	#[test]
	fn initialize_psm_skips_existing_assets() {
		use frame_support::traits::OnRuntimeUpgrade;

		new_test_ext().execute_with(|| {
			// Pre-configure USDC with custom values; drop its decimals snapshot to simulate a
			// pre-migration partial state. This migration must not touch USDC's snapshot (that is
			// `PopulateDecimals`'s job).
			ExternalAssets::<Test>::insert(USDC_ASSET_ID, CircuitBreakerLevel::MintingDisabled);
			MintingFee::<Test>::insert(USDC_ASSET_ID, Permill::from_percent(10));
			AssetDecimals::<Test>::remove(USDC_ASSET_ID);

			// Remove USDT so it gets configured.
			ExternalAssets::<Test>::remove(USDT_ASSET_ID);
			MintingFee::<Test>::remove(USDT_ASSET_ID);
			RedemptionFee::<Test>::remove(USDT_ASSET_ID);
			AssetCeilingWeight::<Test>::remove(USDT_ASSET_ID);
			AssetDecimals::<Test>::remove(USDT_ASSET_ID);

			InitializePsm::<Test, TestPsmConfig>::on_runtime_upgrade();

			// USDC was not overwritten — including its missing decimals snapshot.
			assert_eq!(
				ExternalAssets::<Test>::get(USDC_ASSET_ID),
				Some(CircuitBreakerLevel::MintingDisabled)
			);
			assert_eq!(MintingFee::<Test>::get(USDC_ASSET_ID), Permill::from_percent(10));
			assert_eq!(AssetDecimals::<Test>::get(USDC_ASSET_ID), None);

			// USDT was newly configured; its decimals snapshot is populated.
			let (_, (minting_fee, redemption_fee, ceiling_weight)) = TestPsmConfig::asset_configs()
				.into_iter()
				.find(|(id, _)| *id == USDT_ASSET_ID)
				.unwrap();
			assert_eq!(
				ExternalAssets::<Test>::get(USDT_ASSET_ID),
				Some(CircuitBreakerLevel::AllEnabled)
			);
			assert!(AssetDecimals::<Test>::get(USDT_ASSET_ID).is_some());
			assert_eq!(MintingFee::<Test>::get(USDT_ASSET_ID), minting_fee);
			assert_eq!(RedemptionFee::<Test>::get(USDT_ASSET_ID), redemption_fee);
			assert_eq!(AssetCeilingWeight::<Test>::get(USDT_ASSET_ID), ceiling_weight);
		});
	}

	#[test]
	fn initialize_psm_skips_assets_with_wrong_decimals() {
		use frame_support::traits::{
			fungibles::{metadata::Mutate as MetadataMutate, Create as FungiblesCreate},
			OnRuntimeUpgrade,
		};

		const WRONG_DECIMALS_ID: u32 = 99;

		new_test_ext().execute_with(|| {
			// Create an asset with 8 decimals (stable asset has 6).
			assert_ok!(<Assets as FungiblesCreate<u64>>::create(WRONG_DECIMALS_ID, ALICE, true, 1));
			assert_ok!(<Assets as MetadataMutate<u64>>::set(
				WRONG_DECIMALS_ID,
				&ALICE,
				b"Wrong".to_vec(),
				b"WRG".to_vec(),
				(MAX_DECIMALS_DIFF + 6 + 1).try_into().unwrap(), // exceeds MAX_DECIMALS_DIFF
			));

			struct MixedDecimalsConfig;
			impl InitialPsmConfig<Test> for MixedDecimalsConfig {
				fn max_psm_debt_of_total() -> Permill {
					Permill::from_percent(50)
				}
				fn asset_configs() -> BTreeMap<u32, (Permill, Permill, Permill)> {
					[
						(
							WRONG_DECIMALS_ID,
							(Permill::zero(), Permill::zero(), Permill::from_percent(50)),
						),
						(
							USDC_ASSET_ID, // 6 decimals — matches stable asset
							(Permill::zero(), Permill::zero(), Permill::from_percent(50)),
						),
					]
					.into_iter()
					.collect()
				}
			}

			ExternalAssets::<Test>::remove(WRONG_DECIMALS_ID);
			ExternalAssets::<Test>::remove(USDC_ASSET_ID);

			InitializePsm::<Test, MixedDecimalsConfig>::on_runtime_upgrade();

			// Wrong decimals asset was skipped.
			assert_eq!(ExternalAssets::<Test>::get(WRONG_DECIMALS_ID), None);

			// Matching decimals asset was configured.
			assert_eq!(
				ExternalAssets::<Test>::get(USDC_ASSET_ID),
				Some(CircuitBreakerLevel::AllEnabled)
			);
		});
	}

	#[test]
	fn initialize_psm_is_idempotent() {
		use frame_support::traits::OnRuntimeUpgrade;

		new_test_ext().execute_with(|| {
			clear_all_psm_state();

			// Run twice.
			InitializePsm::<Test, TestPsmConfig>::on_runtime_upgrade();
			InitializePsm::<Test, TestPsmConfig>::on_runtime_upgrade();

			// Same result as running once.
			assert_eq!(MaxPsmDebtOfTotal::<Test>::get(), TestPsmConfig::max_psm_debt_of_total());
			for (asset_id, _) in TestPsmConfig::asset_configs() {
				assert_eq!(
					ExternalAssets::<Test>::get(asset_id),
					Some(CircuitBreakerLevel::AllEnabled)
				);
				assert!(AssetDecimals::<Test>::get(asset_id).is_some());
			}
		});
	}
}
