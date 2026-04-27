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

//! Benchmarking setup for pallet-psm

use super::*;
use crate::Pallet as Psm;
use frame_benchmarking::v2::*;
use frame_support::traits::{
	fungible::{metadata::Inspect, Create as FungibleCreate, Inspect as FungibleInspect},
	fungibles::{
		Create as FungiblesCreate, Inspect as FungiblesInspect, Mutate as FungiblesMutate,
	},
	Get,
};
use frame_system::RawOrigin;
use pallet::BalanceOf;
use sp_runtime::{traits::Zero, Permill, Saturating};

/// Offset for benchmark asset IDs, chosen to avoid collision with typical
/// genesis asset IDs (e.g. stable asset ID = 1).
const ASSET_ID_OFFSET: u32 = 100;

/// Ensure the stable asset exists and its decimals snapshot is written.
/// The snapshot is consulted by mint/redeem via `ensure_decimals_match` and by
/// `add_external_asset`; without it those paths fail closed. Returns the live
/// stable decimals so callers can align external-asset metadata with it.
fn ensure_stable_setup<T: Config>() -> u8
where
	T::StableAsset: FungibleCreate<T::AccountId>,
{
	let admin: T::AccountId = whitelisted_caller();
	let _ = frame_system::Pallet::<T>::inc_providers(&admin);
	if T::StableAsset::minimum_balance().is_zero() {
		let _ = T::StableAsset::create(admin, true, 1u32.into());
	}
	let stable_decimals = T::StableAsset::decimals();
	if !crate::StableDecimals::<T>::exists() {
		crate::StableDecimals::<T>::put(stable_decimals);
	}
	stable_decimals
}

/// Set up `n` external assets ready for PSM benchmarks.
///
/// Creates the target asset (`ASSET_ID_OFFSET`) and the stable asset,
/// registers `n` external assets (`ASSET_ID_OFFSET..+n`), and
/// configures ceiling weights so the target can absorb the full mint amount.
///
/// Assets beyond the target are filler, they only populate PSM storage so
/// the iterators in `total_psm_debt()` and `max_asset_debt()` touch `n`
/// entries during `mint()`.
fn setup_assets<T: Config>(n: u32) -> T::AssetId
where
	T::Fungibles: FungiblesCreate<T::AccountId>,
	T::StableAsset: FungibleCreate<T::AccountId>,
	T::AssetId: From<u32>,
{
	let admin: T::AccountId = whitelisted_caller();
	let _ = frame_system::Pallet::<T>::inc_providers(&admin);

	let stable_decimals = ensure_stable_setup::<T>();

	// Target asset: create + set metadata via the runtime-provided benchmark
	// helper. Setting metadata requires reserving a native deposit, which the
	// helper handles by funding `admin` first — something the fungibles traits
	// alone cannot express.
	let target_id: T::AssetId = ASSET_ID_OFFSET.into();
	if !T::Fungibles::asset_exists(target_id) {
		T::BenchmarkHelper::create_asset(target_id, &admin, stable_decimals);
	}

	crate::MaxPsmDebtOfTotal::<T>::put(Permill::from_percent(100));
	// Filler assets only populate PSM storage so mint()'s iterators touch `n`
	// entries. They are never swapped against, so their underlying fungibles
	// asset does not need to exist and no AssetDecimals snapshot is required.
	for i in 0..n {
		let id: T::AssetId = (ASSET_ID_OFFSET + i).into();
		crate::ExternalAssets::<T>::insert(id, CircuitBreakerLevel::AllEnabled);
		crate::AssetCeilingWeight::<T>::insert(id, Permill::from_percent(1));
		crate::PsmDebt::<T>::insert(id, BalanceOf::<T>::from(1u32));
	}
	// Target-specific: dominant weight so it can absorb the full mint amount,
	// and a decimals snapshot so `ensure_decimals_match` passes.
	crate::AssetCeilingWeight::<T>::insert(target_id, Permill::from_percent(100));
	crate::AssetDecimals::<T>::insert(target_id, stable_decimals);

	target_id
}

#[benchmarks(where T::Fungibles: FungiblesCreate<T::AccountId>, T::StableAsset: FungibleCreate<T::AccountId>, T::AssetId: From<u32>)]
mod benchmarks {
	use super::*;

	/// Linear in `n`. The number of registered external assets, because
	/// `total_psm_debt()` iterates `PsmDebt` and `max_asset_debt()` iterates
	/// `AssetCeilingWeight`.
	#[benchmark]
	fn mint(n: Linear<1, { T::MaxExternalAssets::get() }>) -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let asset_id = setup_assets::<T>(n);
		let mint_amount = T::MinSwapAmount::get().saturating_mul(10u32.into());

		T::Fungibles::mint_into(asset_id, &caller, mint_amount.saturating_mul(2u32.into()))
			.map_err(|_| BenchmarkError::Stop("Failed to fund caller"))?;

		let psm_account = Psm::<T>::account_id();
		let reserve_before = T::Fungibles::balance(asset_id, &psm_account);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), asset_id, mint_amount);

		assert!(T::Fungibles::balance(asset_id, &psm_account) > reserve_before);
		Ok(())
	}

	#[benchmark]
	fn redeem() -> Result<(), BenchmarkError> {
		let caller: T::AccountId = whitelisted_caller();
		let asset_id = setup_assets::<T>(1);
		let setup_amount = T::MinSwapAmount::get().saturating_mul(10u32.into());
		let redeem_amount = T::MinSwapAmount::get();

		T::Fungibles::mint_into(asset_id, &caller, setup_amount.saturating_mul(2u32.into()))
			.map_err(|_| BenchmarkError::Stop("Failed to fund caller"))?;
		Psm::<T>::mint(RawOrigin::Signed(caller.clone()).into(), asset_id, setup_amount)
			.map_err(|_| BenchmarkError::Stop("Failed to setup reserve via mint"))?;

		let psm_account = Psm::<T>::account_id();
		let reserve_before = T::Fungibles::balance(asset_id, &psm_account);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), asset_id, redeem_amount);

		assert!(T::Fungibles::balance(asset_id, &psm_account) < reserve_before);
		Ok(())
	}

	#[benchmark]
	fn set_minting_fee() -> Result<(), BenchmarkError> {
		let asset_id = setup_assets::<T>(1);
		let new_fee = Permill::from_percent(2);

		#[extrinsic_call]
		_(RawOrigin::Root, asset_id, new_fee);

		assert_eq!(crate::MintingFee::<T>::get(asset_id), new_fee);
		Ok(())
	}

	#[benchmark]
	fn set_redemption_fee() -> Result<(), BenchmarkError> {
		let asset_id = setup_assets::<T>(1);
		let new_fee = Permill::from_percent(2);

		#[extrinsic_call]
		_(RawOrigin::Root, asset_id, new_fee);

		assert_eq!(crate::RedemptionFee::<T>::get(asset_id), new_fee);
		Ok(())
	}

	#[benchmark]
	fn set_max_psm_debt() -> Result<(), BenchmarkError> {
		let new_ratio = Permill::from_percent(20);

		#[extrinsic_call]
		_(RawOrigin::Root, new_ratio);

		assert_eq!(crate::MaxPsmDebtOfTotal::<T>::get(), new_ratio);
		Ok(())
	}

	#[benchmark]
	fn set_asset_status() -> Result<(), BenchmarkError> {
		let asset_id = setup_assets::<T>(1);
		let new_status = CircuitBreakerLevel::MintingDisabled;

		#[extrinsic_call]
		_(RawOrigin::Root, asset_id, new_status);

		assert_eq!(crate::ExternalAssets::<T>::get(asset_id), Some(new_status));
		Ok(())
	}

	#[benchmark]
	fn set_asset_ceiling_weight() -> Result<(), BenchmarkError> {
		let asset_id = setup_assets::<T>(1);
		let new_weight = Permill::from_percent(50);

		#[extrinsic_call]
		_(RawOrigin::Root, asset_id, new_weight);

		assert_eq!(crate::AssetCeilingWeight::<T>::get(asset_id), new_weight);
		Ok(())
	}
	#[benchmark]
	fn add_external_asset() -> Result<(), BenchmarkError> {
		// Seed StableDecimals and ensure the stable asset exists; the extrinsic
		// reads the snapshot and compares it against live metadata.
		let stable_decimals = ensure_stable_setup::<T>();
		let caller: T::AccountId = whitelisted_caller();
		let new_asset_id: T::AssetId = ASSET_ID_OFFSET.into();

		T::BenchmarkHelper::create_asset(new_asset_id, &caller, stable_decimals);

		#[extrinsic_call]
		_(RawOrigin::Root, new_asset_id);

		assert!(crate::ExternalAssets::<T>::contains_key(new_asset_id));
		Ok(())
	}

	#[benchmark]
	fn remove_external_asset() -> Result<(), BenchmarkError> {
		let asset_id = setup_assets::<T>(1);
		crate::PsmDebt::<T>::remove(asset_id);

		#[extrinsic_call]
		_(RawOrigin::Root, asset_id);

		assert!(!crate::ExternalAssets::<T>::contains_key(asset_id));
		Ok(())
	}

	impl_benchmark_test_suite!(Psm, crate::mock::new_test_ext(), crate::mock::Test);
}
