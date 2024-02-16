// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Asset Conversion Ops pallet benchmarking.

use super::*;
use crate::Pallet as AssetConversionOps;
use frame_benchmarking::{v2::*, whitelisted_caller};
use frame_support::{
	assert_ok,
	traits::fungibles::{Create, Inspect, Mutate},
};
use frame_system::RawOrigin as SystemOrigin;
use pallet_asset_conversion::{BenchmarkHelper, Pallet as AssetConversion};
use sp_core::Get;
use sp_runtime::traits::One;
use sp_std::prelude::*;

/// Provides a pair of amounts expected to serve as sufficient initial liquidity for a pool.
fn valid_liquidity_amount<T: Config>(ed1: T::Balance, ed2: T::Balance) -> (T::Balance, T::Balance)
where
	T::Assets: Inspect<T::AccountId>,
{
	let l =
		ed1.max(ed2) + T::MintMinLiquidity::get() + T::MintMinLiquidity::get() + T::Balance::one();
	(l, l)
}

/// Create the `asset` and mint the `amount` for the `caller`.
fn create_asset<T: Config>(caller: &T::AccountId, asset: &T::AssetKind, amount: T::Balance)
where
	T::Assets: Create<T::AccountId> + Mutate<T::AccountId>,
{
	if !T::Assets::asset_exists(asset.clone()) {
		assert_ok!(T::Assets::create(asset.clone(), caller.clone(), true, T::Balance::one()));
	}
	assert_ok!(T::Assets::mint_into(
		asset.clone(),
		&caller,
		amount + T::Assets::minimum_balance(asset.clone())
	));
}

/// Create the designated fee asset for pool creation.
fn create_fee_asset<T: Config>(caller: &T::AccountId)
where
	T::Assets: Create<T::AccountId> + Mutate<T::AccountId>,
{
	let fee_asset = T::PoolSetupFeeAsset::get();
	if !T::Assets::asset_exists(fee_asset.clone()) {
		assert_ok!(T::Assets::create(fee_asset.clone(), caller.clone(), true, T::Balance::one()));
	}
	assert_ok!(T::Assets::mint_into(
		fee_asset.clone(),
		&caller,
		T::Assets::minimum_balance(fee_asset)
	));
}

/// Mint the fee asset for the `caller` sufficient to cover the fee for creating a new pool.
fn mint_setup_fee_asset<T: Config>(
	caller: &T::AccountId,
	asset1: &T::AssetKind,
	asset2: &T::AssetKind,
	lp_token: &T::PoolAssetId,
) where
	T::Assets: Create<T::AccountId> + Mutate<T::AccountId>,
{
	assert_ok!(T::Assets::mint_into(
		T::PoolSetupFeeAsset::get(),
		&caller,
		T::PoolSetupFee::get() +
			T::Assets::deposit_required(asset1.clone()) +
			T::Assets::deposit_required(asset2.clone()) +
			T::PoolAssets::deposit_required(lp_token.clone())
	));
}

/// Creates a pool for a given asset pair.
///
/// This action mints the necessary amounts of the given assets for the `caller` to provide initial
/// liquidity. It returns the LP token ID along with a pair of amounts sufficient for the pool's
/// initial liquidity.
fn create_asset_and_pool<T: Config>(
	caller: &T::AccountId,
	asset1: &T::AssetKind,
	asset2: &T::AssetKind,
) -> (T::PoolAssetId, T::Balance, T::Balance)
where
	T::Assets: Create<T::AccountId> + Mutate<T::AccountId>,
{
	let (liquidity1, liquidity2) = valid_liquidity_amount::<T>(
		T::Assets::minimum_balance(asset1.clone()),
		T::Assets::minimum_balance(asset2.clone()),
	);
	create_asset::<T>(caller, asset1, liquidity1);
	create_asset::<T>(caller, asset2, liquidity2);
	let lp_token = AssetConversion::<T>::get_next_pool_asset_id();

	mint_setup_fee_asset::<T>(caller, asset1, asset2, &lp_token);

	assert_ok!(AssetConversion::<T>::create_pool(
		SystemOrigin::Signed(caller.clone()).into(),
		Box::new(asset1.clone()),
		Box::new(asset2.clone())
	));

	(lp_token, liquidity1, liquidity2)
}

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<T>::events();
	let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
	// compare to the last event record
	let frame_system::EventRecord { event, .. } = &events[events.len() - 1];
	assert_eq!(event, &system_event);
}

#[benchmarks(where T::Assets: Create<T::AccountId> + Mutate<T::AccountId>, T::PoolAssetId: Into<u32>,)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn migrate_to_new_account() {
		let caller: T::AccountId = whitelisted_caller();
		let (asset1, asset2) = T::BenchmarkHelper::create_pair(0, 1);

		create_fee_asset::<T>(&caller);
		let (_, liquidity1, liquidity2) = create_asset_and_pool::<T>(&caller, &asset1, &asset2);

		assert_ok!(AssetConversion::<T>::add_liquidity(
			SystemOrigin::Signed(caller.clone()).into(),
			Box::new(asset1.clone()),
			Box::new(asset2.clone()),
			liquidity1,
			liquidity2,
			T::Balance::one(),
			T::Balance::zero(),
			caller.clone(),
		));

		#[extrinsic_call]
		_(SystemOrigin::Signed(caller.clone()), Box::new(asset1.clone()), Box::new(asset2.clone()));

		let pool_id = T::PoolLocator::pool_id(&asset1, &asset2).unwrap();
		let (prior_account, new_account) = AssetConversionOps::<T>::addresses(&pool_id).unwrap();
		assert_last_event::<T>(
			Event::MigratedToNewAccount { pool_id, new_account, prior_account }.into(),
		);
	}

	impl_benchmark_test_suite!(AssetConversionOps, crate::mock::new_test_ext(), crate::mock::Test);
}
