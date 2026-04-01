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

//! Migration to V1: initialize PSM parameters for post-genesis deployment.
//!
//! This migration sets initial values for all configurable PSM parameters when
//! adding the pallet to an existing chain.
//!
//! # Usage
//!
//! Include in your runtime migrations:
//!
//! ```ignore
//! pub type Migrations = (
//!     pallet_psm::migrations::v1::MigrateToV1<Runtime, PsmInitialConfig>,
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
		fungibles::metadata::Inspect as FungiblesMetadataInspect, UncheckedOnRuntimeUpgrade,
	},
};
use sp_runtime::Permill;

use crate::{
	pallet::{
		AssetCeilingWeight, CircuitBreakerLevel, ExternalAssets, MaxPsmDebtOfTotal, MintingFee,
		RedemptionFee,
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
/// [`MigrateToV1`].
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

/// Migration to initialize PSM pallet parameters (V0 -> V1).
///
/// This migration:
/// 1. Sets `MaxPsmDebtOfTotal`
/// 2. Sets approved external assets with `AllEnabled` status
/// 3. Sets per-asset fee and ceiling-weight configuration
/// 4. Ensures the PSM account exists
pub type MigrateToV1<T, I> = frame_support::migrations::VersionedMigration<
	0,
	1,
	UncheckedMigrateToV1<T, I>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

pub struct UncheckedMigrateToV1<T, I>(core::marker::PhantomData<(T, I)>);

impl<T: Config, I: InitialPsmConfig<T>> UncheckedOnRuntimeUpgrade for UncheckedMigrateToV1<T, I> {
	fn on_runtime_upgrade() -> Weight {
		log::info!(
			target: LOG_TARGET,
			"Running MigrateToV1: initializing PSM pallet parameters"
		);

		let asset_configs = I::asset_configs();

		MaxPsmDebtOfTotal::<T>::put(I::max_psm_debt_of_total());

		let stable_decimals = T::StableAsset::decimals();
		for (asset_id, (minting_fee, redemption_fee, ceiling_weight)) in &asset_configs {
			assert!(
				T::Fungibles::decimals(*asset_id) == stable_decimals,
				"PSM migration: asset {:?} decimals do not match stable asset decimals",
				asset_id,
			);
			ExternalAssets::<T>::insert(asset_id, CircuitBreakerLevel::AllEnabled);
			MintingFee::<T>::insert(asset_id, minting_fee);
			RedemptionFee::<T>::insert(asset_id, redemption_fee);
			AssetCeilingWeight::<T>::insert(asset_id, ceiling_weight);
		}

		Pallet::<T>::ensure_account_exists(&Pallet::<T>::account_id());
		Pallet::<T>::ensure_account_exists(&T::FeeDestination::get());

		log::info!(
			target: LOG_TARGET,
			"MigrateToV1 complete"
		);

		// (MaxPsmDebtOfTotal + 2 accounts) + 4 writes per asset
		let writes = 3u64.saturating_add((asset_configs.len() as u64).saturating_mul(4));
		T::DbWeight::get().writes(writes)
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
	use crate::mock::{new_test_ext, Test, USDC_ASSET_ID, USDT_ASSET_ID};

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

	#[test]
	fn migration_v0_to_v1_works() {
		new_test_ext().execute_with(|| {
			MaxPsmDebtOfTotal::<Test>::kill();
			ExternalAssets::<Test>::remove(USDC_ASSET_ID);
			ExternalAssets::<Test>::remove(USDT_ASSET_ID);
			MintingFee::<Test>::remove(USDC_ASSET_ID);
			MintingFee::<Test>::remove(USDT_ASSET_ID);
			RedemptionFee::<Test>::remove(USDC_ASSET_ID);
			RedemptionFee::<Test>::remove(USDT_ASSET_ID);
			AssetCeilingWeight::<Test>::remove(USDC_ASSET_ID);
			AssetCeilingWeight::<Test>::remove(USDT_ASSET_ID);

			let _weight = UncheckedMigrateToV1::<Test, TestPsmConfig>::on_runtime_upgrade();

			assert_eq!(MaxPsmDebtOfTotal::<Test>::get(), TestPsmConfig::max_psm_debt_of_total());

			for (asset_id, (minting_fee, redemption_fee, ceiling_weight)) in
				TestPsmConfig::asset_configs()
			{
				assert_eq!(
					ExternalAssets::<Test>::get(asset_id),
					Some(CircuitBreakerLevel::AllEnabled)
				);
				assert_eq!(MintingFee::<Test>::get(asset_id), minting_fee);
				assert_eq!(RedemptionFee::<Test>::get(asset_id), redemption_fee);
				assert_eq!(AssetCeilingWeight::<Test>::get(asset_id), ceiling_weight);
			}
		});
	}
}
