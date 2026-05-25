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

//! Idempotent migration to install PSM instances on a fresh deployment.
//!
//! Reads a runtime-supplied list of PSM instances and their approved externals, and
//! materialises them in storage. Already-installed instances and already-approved
//! externals are skipped, so the migration is safe to run more than once.
//!
//! # Usage
//!
//! ```ignore
//! pub type Migrations = (
//!     pallet_psm::migrations::init::InitializePsm<Runtime, PsmInitialConfig>,
//!     // ... other migrations
//! );
//! ```
//!
//! Where `PsmInitialConfig` implements [`InitialPsmConfig`].

use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use alloc::vec::Vec as _;
use frame_support::{
	pallet_prelude::{Get, Weight},
	traits::fungibles::metadata::Inspect as FungiblesMetadataInspect,
};
use sp_runtime::Permill;

use crate::{
	pallet::{
		AssetCeilingWeight, BalanceOf, CircuitBreakerLevel, ExternalAssetInfo, ExternalAssets,
		MintingFee, PsmInfo, Psms, RedemptionFee, MAX_DECIMALS_DIFF,
	},
	Config, Pallet,
};

#[cfg(feature = "try-runtime")]
use frame_support::ensure;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

const LOG_TARGET: &str = "runtime::psm::migration";

/// Configuration trait for the [`InitializePsm`] migration.
pub trait InitialPsmConfig<T: Config> {
	/// PSM instances to install: `(internal_asset, fee_destination, max_debt)`.
	fn psms() -> Vec<(T::AssetId, T::AccountId, BalanceOf<T>)>;

	/// Externals to approve, per instance:
	/// `(internal_asset, external_asset, minting_fee, redemption_fee, ceiling_weight)`.
	fn externals() -> Vec<(T::AssetId, T::AssetId, Permill, Permill, Permill)>;
}

/// Idempotent migration that installs PSM instances and approved externals.
///
/// On each run:
/// 1. For every entry in `psms()` not already present in `Psms`, inserts a fresh [`PsmInfo`] (with
///    `external_count = 0`).
/// 2. For every entry in `externals()` whose `(internal, external)` pair is not yet approved on the
///    corresponding PSM, inserts the per-external storage and bumps the parent
///    [`PsmInfo::external_count`].
/// 3. Ensures the derived PSM account and fee-destination account exist.
///
/// Safe to run multiple times — existing state is not overwritten.
pub struct InitializePsm<T, I>(core::marker::PhantomData<(T, I)>);

impl<T: Config, I: InitialPsmConfig<T>> frame_support::traits::OnRuntimeUpgrade
	for InitializePsm<T, I>
{
	fn on_runtime_upgrade() -> Weight {
		log::info!(
			target: LOG_TARGET,
			"Running InitializePsm: installing PSM instances and approved externals"
		);

		let mut reads = 0u64;
		let mut writes = 0u64;

		for (internal_asset, fee_destination, max_debt) in I::psms() {
			reads += 1;
			if Psms::<T>::contains_key(&internal_asset) {
				log::info!(
					target: LOG_TARGET,
					"PSM for {:?} already installed, skipping",
					internal_asset,
				);
				continue;
			}

			let internal_decimals = T::Fungibles::decimals(internal_asset.clone());
			Psms::<T>::insert(
				&internal_asset,
				PsmInfo::<T> {
					fee_destination: fee_destination.clone(),
					max_debt,
					internal_decimals,
					external_count: 0,
				},
			);
			writes += 1;

			Pallet::<T>::ensure_account_exists(&Pallet::<T>::psm_account(&internal_asset));
			Pallet::<T>::ensure_account_exists(&fee_destination);
			writes += 2;

			log::info!(
				target: LOG_TARGET,
				"Installed PSM for {:?} (decimals={})",
				internal_asset,
				internal_decimals,
			);
		}

		for (internal_asset, external_asset, minting_fee, redemption_fee, ceiling_weight) in
			I::externals()
		{
			reads += 2;
			let Some(mut info) = Psms::<T>::get(&internal_asset) else {
				log::error!(
					target: LOG_TARGET,
					"External {:?} configured for unregistered PSM {:?}; skipping",
					external_asset,
					internal_asset,
				);
				continue;
			};
			if ExternalAssets::<T>::contains_key(&internal_asset, &external_asset) {
				log::info!(
					target: LOG_TARGET,
					"External {:?} already approved on PSM {:?}; skipping",
					external_asset,
					internal_asset,
				);
				continue;
			}
			if info.external_count >= T::MaxExternalAssetsPerPsm::get() {
				log::error!(
					target: LOG_TARGET,
					"PSM {:?} already at MaxExternalAssetsPerPsm; cannot add {:?}",
					internal_asset,
					external_asset,
				);
				continue;
			}

			let asset_decimals = T::Fungibles::decimals(external_asset.clone());
			let diff = asset_decimals.abs_diff(info.internal_decimals) as u32;
			if diff > MAX_DECIMALS_DIFF {
				log::error!(
					target: LOG_TARGET,
					"External {:?} decimals diff ({}) exceeds MAX_DECIMALS_DIFF ({}); skipping",
					external_asset,
					diff,
					MAX_DECIMALS_DIFF,
				);
				continue;
			}

			ExternalAssets::<T>::insert(
				&internal_asset,
				&external_asset,
				ExternalAssetInfo {
					status: CircuitBreakerLevel::AllEnabled,
					decimals: asset_decimals,
				},
			);
			MintingFee::<T>::insert(&internal_asset, &external_asset, minting_fee);
			RedemptionFee::<T>::insert(&internal_asset, &external_asset, redemption_fee);
			AssetCeilingWeight::<T>::insert(&internal_asset, &external_asset, ceiling_weight);
			info.external_count = info.external_count.saturating_add(1);
			Psms::<T>::insert(&internal_asset, info);
			writes += 5;

			log::info!(
				target: LOG_TARGET,
				"Approved external {:?} on PSM {:?} (decimals={})",
				external_asset,
				internal_asset,
				asset_decimals,
			);
		}

		log::info!(target: LOG_TARGET, "InitializePsm complete");
		T::DbWeight::get().reads_writes(reads, writes)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, TryRuntimeError> {
		Ok(alloc::vec::Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: alloc::vec::Vec<u8>) -> Result<(), TryRuntimeError> {
		for (internal_asset, fee_destination, max_debt) in I::psms() {
			let info = Psms::<T>::get(&internal_asset).ok_or("PsmInfo missing after migration")?;
			ensure!(info.max_debt == max_debt, "max_debt mismatch after migration");
			ensure!(
				info.fee_destination == fee_destination,
				"fee_destination mismatch after migration"
			);
			ensure!(
				frame_system::Pallet::<T>::account_exists(&Pallet::<T>::psm_account(
					&internal_asset
				)),
				"PSM derived account does not exist after migration"
			);
		}

		for (internal_asset, external_asset, minting_fee, redemption_fee, ceiling_weight) in
			I::externals()
		{
			let stored = ExternalAssets::<T>::get(&internal_asset, &external_asset)
				.ok_or("External asset missing after migration")?;
			ensure!(
				stored.status == CircuitBreakerLevel::AllEnabled,
				"External asset is not AllEnabled after migration"
			);
			ensure!(
				MintingFee::<T>::get(&internal_asset, &external_asset) == minting_fee,
				"MintingFee mismatch after migration"
			);
			ensure!(
				RedemptionFee::<T>::get(&internal_asset, &external_asset) == redemption_fee,
				"RedemptionFee mismatch after migration"
			);
			ensure!(
				AssetCeilingWeight::<T>::get(&internal_asset, &external_asset) == ceiling_weight,
				"AssetCeilingWeight mismatch after migration"
			);
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{
		new_test_ext, Test, INSURANCE_FUND, INTERNAL_ASSET_ID, INTERNAL_UNIT, USDC_ASSET_ID,
		USDT_ASSET_ID,
	};

	const TEST_MAX_DEBT: u128 = 5_000_000 * INTERNAL_UNIT;

	struct TestPsmConfig;

	impl InitialPsmConfig<Test> for TestPsmConfig {
		fn psms() -> Vec<(u32, u64, u128)> {
			alloc::vec![(INTERNAL_ASSET_ID, INSURANCE_FUND, TEST_MAX_DEBT)]
		}

		fn externals() -> Vec<(u32, u32, Permill, Permill, Permill)> {
			alloc::vec![
				(
					INTERNAL_ASSET_ID,
					USDC_ASSET_ID,
					Permill::from_parts(5_000),
					Permill::from_parts(5_000),
					Permill::from_percent(50),
				),
				(
					INTERNAL_ASSET_ID,
					USDT_ASSET_ID,
					Permill::from_parts(3_000),
					Permill::from_parts(7_000),
					Permill::from_percent(50),
				),
			]
		}
	}

	fn clear_all_psm_state() {
		Psms::<Test>::remove(INTERNAL_ASSET_ID);
		ExternalAssets::<Test>::remove(INTERNAL_ASSET_ID, USDC_ASSET_ID);
		ExternalAssets::<Test>::remove(INTERNAL_ASSET_ID, USDT_ASSET_ID);
		MintingFee::<Test>::remove(INTERNAL_ASSET_ID, USDC_ASSET_ID);
		MintingFee::<Test>::remove(INTERNAL_ASSET_ID, USDT_ASSET_ID);
		RedemptionFee::<Test>::remove(INTERNAL_ASSET_ID, USDC_ASSET_ID);
		RedemptionFee::<Test>::remove(INTERNAL_ASSET_ID, USDT_ASSET_ID);
		AssetCeilingWeight::<Test>::remove(INTERNAL_ASSET_ID, USDC_ASSET_ID);
		AssetCeilingWeight::<Test>::remove(INTERNAL_ASSET_ID, USDT_ASSET_ID);
	}

	#[test]
	fn initialize_psm_installs_instance_and_externals() {
		use frame_support::traits::OnRuntimeUpgrade;

		new_test_ext().execute_with(|| {
			clear_all_psm_state();

			InitializePsm::<Test, TestPsmConfig>::on_runtime_upgrade();

			let info = Psms::<Test>::get(INTERNAL_ASSET_ID).expect("PsmInfo populated");
			assert_eq!(info.fee_destination, INSURANCE_FUND);
			assert_eq!(info.max_debt, TEST_MAX_DEBT);
			assert_eq!(
				info.internal_decimals,
				<<Test as Config>::Fungibles as FungiblesMetadataInspect<u64>>::decimals(
					INTERNAL_ASSET_ID
				),
			);
			assert_eq!(info.external_count, 2);

			for (_, external_asset, minting_fee, redemption_fee, ceiling_weight) in
				TestPsmConfig::externals()
			{
				let stored = ExternalAssets::<Test>::get(INTERNAL_ASSET_ID, external_asset)
					.expect("external asset present");
				assert_eq!(stored.status, CircuitBreakerLevel::AllEnabled);
				assert_eq!(
					stored.decimals,
					<<Test as Config>::Fungibles as FungiblesMetadataInspect<u64>>::decimals(
						external_asset
					),
				);
				assert_eq!(MintingFee::<Test>::get(INTERNAL_ASSET_ID, external_asset), minting_fee);
				assert_eq!(
					RedemptionFee::<Test>::get(INTERNAL_ASSET_ID, external_asset),
					redemption_fee
				);
				assert_eq!(
					AssetCeilingWeight::<Test>::get(INTERNAL_ASSET_ID, external_asset),
					ceiling_weight
				);
			}
		});
	}

	#[test]
	fn initialize_psm_is_idempotent() {
		use frame_support::traits::OnRuntimeUpgrade;

		new_test_ext().execute_with(|| {
			clear_all_psm_state();

			InitializePsm::<Test, TestPsmConfig>::on_runtime_upgrade();
			InitializePsm::<Test, TestPsmConfig>::on_runtime_upgrade();

			let info = Psms::<Test>::get(INTERNAL_ASSET_ID).expect("PsmInfo populated");
			assert_eq!(info.external_count, 2);
		});
	}
}
