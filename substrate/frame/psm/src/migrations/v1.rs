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

use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use frame_support::{
	pallet_prelude::{Get, StorageVersion, Weight},
	traits::{GetStorageVersion, UncheckedOnRuntimeUpgrade},
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

	/// Approved external stablecoin asset IDs.
	fn external_asset_ids() -> Vec<T::AssetId>;

	/// Per-asset configuration:
	/// - minting fee
	/// - redemption fee
	/// - asset ceiling weight
	fn asset_configs() -> BTreeMap<T::AssetId, (Permill, Permill, Permill)>;
}

/// Migration to initialize PSM pallet parameters (V0 -> V1).
///
/// This migration:
/// 1. Sets `MaxPsmDebtOfTotal`
/// 2. Sets approved external assets with `AllEnabled` status
/// 3. Sets per-asset fee and ceiling-weight configuration
/// 4. Ensures the PSM account exists
///
/// Only runs if the on-chain storage version is 0 (uninitialized).
pub struct MigrateToV1<T, I>(core::marker::PhantomData<(T, I)>);

impl<T: Config, I: InitialPsmConfig<T>> UncheckedOnRuntimeUpgrade for MigrateToV1<T, I> {
	fn on_runtime_upgrade() -> Weight {
		let on_chain_version = Pallet::<T>::on_chain_storage_version();
		if on_chain_version != 0 {
			log::info!(
				target: LOG_TARGET,
				"Skipping migration: on-chain version is {:?}, expected 0",
				on_chain_version
			);
			return T::DbWeight::get().reads(1);
		}

		log::info!(
			target: LOG_TARGET,
			"Running MigrateToV1: initializing PSM pallet parameters"
		);

		let external_asset_ids = I::external_asset_ids();
		let asset_configs = I::asset_configs();

		MaxPsmDebtOfTotal::<T>::put(I::max_psm_debt_of_total());

		for asset_id in &external_asset_ids {
			ExternalAssets::<T>::insert(asset_id, CircuitBreakerLevel::AllEnabled);
		}

		for (asset_id, (minting_fee, redemption_fee, max_asset_debt_ratio)) in &asset_configs {
			MintingFee::<T>::insert(asset_id, minting_fee);
			RedemptionFee::<T>::insert(asset_id, redemption_fee);
			AssetCeilingWeight::<T>::insert(asset_id, max_asset_debt_ratio);
		}

		Pallet::<T>::ensure_psm_account_exists();

		StorageVersion::new(1).put::<Pallet<T>>();

		log::info!(
			target: LOG_TARGET,
			"MigrateToV1 complete"
		);

		let writes = 3u64
			.saturating_add(external_asset_ids.len() as u64)
			.saturating_add((asset_configs.len() as u64).saturating_mul(3));
		T::DbWeight::get().reads_writes(1, writes)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		let on_chain_version = Pallet::<T>::on_chain_storage_version();
		ensure!(on_chain_version == 0, "Expected storage version 0 before migration");
		Ok(Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
		let on_chain_version = Pallet::<T>::on_chain_storage_version();
		ensure!(on_chain_version == 1, "Expected storage version 1 after migration");

		ensure!(
			MaxPsmDebtOfTotal::<T>::get() == I::max_psm_debt_of_total(),
			"MaxPsmDebtOfTotal mismatch after migration"
		);

		for asset_id in I::external_asset_ids() {
			ensure!(
				ExternalAssets::<T>::get(asset_id) == Some(CircuitBreakerLevel::AllEnabled),
				"External asset missing or not AllEnabled after migration"
			);
		}

		for (asset_id, (minting_fee, redemption_fee, ceiling_weight)) in I::asset_configs() {
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

		fn external_asset_ids() -> Vec<u32> {
			vec![USDC_ASSET_ID, USDT_ASSET_ID, 77]
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
			StorageVersion::new(0).put::<Pallet<Test>>();

			MaxPsmDebtOfTotal::<Test>::kill();
			ExternalAssets::<Test>::remove(USDC_ASSET_ID);
			ExternalAssets::<Test>::remove(USDT_ASSET_ID);
			ExternalAssets::<Test>::remove(77);
			MintingFee::<Test>::remove(USDC_ASSET_ID);
			MintingFee::<Test>::remove(USDT_ASSET_ID);
			RedemptionFee::<Test>::remove(USDC_ASSET_ID);
			RedemptionFee::<Test>::remove(USDT_ASSET_ID);
			AssetCeilingWeight::<Test>::remove(USDC_ASSET_ID);
			AssetCeilingWeight::<Test>::remove(USDT_ASSET_ID);

			let _weight = MigrateToV1::<Test, TestPsmConfig>::on_runtime_upgrade();

			assert_eq!(MaxPsmDebtOfTotal::<Test>::get(), TestPsmConfig::max_psm_debt_of_total());

			for asset_id in TestPsmConfig::external_asset_ids() {
				assert_eq!(
					ExternalAssets::<Test>::get(asset_id),
					Some(CircuitBreakerLevel::AllEnabled)
				);
			}

			for (asset_id, (minting_fee, redemption_fee, ceiling_weight)) in
				TestPsmConfig::asset_configs()
			{
				assert_eq!(MintingFee::<Test>::get(asset_id), minting_fee);
				assert_eq!(RedemptionFee::<Test>::get(asset_id), redemption_fee);
				assert_eq!(AssetCeilingWeight::<Test>::get(asset_id), ceiling_weight);
			}

			assert_eq!(Pallet::<Test>::on_chain_storage_version(), 1);
		});
	}

	#[test]
	fn migration_skipped_if_already_v1() {
		new_test_ext().execute_with(|| {
			StorageVersion::new(1).put::<Pallet<Test>>();
			let before = MaxPsmDebtOfTotal::<Test>::get();

			let weight = MigrateToV1::<Test, TestPsmConfig>::on_runtime_upgrade();

			assert_eq!(MaxPsmDebtOfTotal::<Test>::get(), before);
			assert_eq!(weight, <Test as frame_system::Config>::DbWeight::get().reads(1));
		});
	}
}
