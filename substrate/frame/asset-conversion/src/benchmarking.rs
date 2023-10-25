// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Asset Conversion pallet benchmarking.

use super::*;
use crate::Pallet as AssetConversion;
use frame_benchmarking::{v2::*, whitelisted_caller};
use frame_support::{
	assert_ok,
	traits::{
		fungible::NativeOrWithId,
		fungibles::{Create, Inspect, Mutate},
	},
};
use frame_system::RawOrigin as SystemOrigin;
use sp_core::Get;
use sp_runtime::traits::StaticLookup;
use sp_std::{marker::PhantomData, prelude::*};

/// Benchmark Helper
pub trait BenchmarkHelper<AssetKind> {
	/// Returns a valid assets pair for the pool creation.
	fn create_pair(seed1: u32, seed2: u32) -> (AssetKind, AssetKind);
}

impl<AssetKind> BenchmarkHelper<AssetKind> for ()
where
	AssetKind: From<u32>,
{
	fn create_pair(seed1: u32, seed2: u32) -> (AssetKind, AssetKind) {
		(seed1.into(), seed2.into())
	}
}

/// Factory for creating a valid asset pairs with [`NativeOrWithId::Native`] always leading in the
/// pair.
pub struct NativeOrWithIdFactory<AssetId>(PhantomData<AssetId>);
impl<AssetId: From<u32> + Ord> BenchmarkHelper<NativeOrWithId<AssetId>>
	for NativeOrWithIdFactory<AssetId>
{
	fn create_pair(_seed1: u32, seed2: u32) -> (NativeOrWithId<AssetId>, NativeOrWithId<AssetId>) {
		(NativeOrWithId::Native, NativeOrWithId::WithId(seed2.into()))
	}
}

const INITIAL_ASSET_BALANCE: u128 = 1_000_000_000_000;
type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;

fn get_lp_token_id<T: Config>() -> T::PoolAssetId
where
	T::PoolAssetId: Into<u32>,
{
	let next_id: u32 = AssetConversion::<T>::get_next_pool_asset_id().into();
	(next_id - 1).into()
}

fn create_asset<T: Config>(asset: &T::AssetKind) -> (T::AccountId, AccountIdLookupOf<T>)
where
	T::Balance: From<u128>,
	T::Assets: Create<T::AccountId> + Mutate<T::AccountId>,
{
	let caller: T::AccountId = whitelisted_caller();
	let caller_lookup = T::Lookup::unlookup(caller.clone());
	if !T::Assets::asset_exists(asset.clone()) {
		assert_ok!(T::Assets::create(asset.clone(), caller.clone(), true, 1.into()));
	}
	assert_ok!(T::Assets::mint_into(asset.clone(), &caller, INITIAL_ASSET_BALANCE.into()));

	(caller, caller_lookup)
}

fn create_asset_and_pool<T: Config>(
	asset1: &T::AssetKind,
	asset2: &T::AssetKind,
) -> (T::PoolAssetId, T::AccountId, AccountIdLookupOf<T>)
where
	T::Balance: From<u128>,
	T::Assets: Create<T::AccountId> + Mutate<T::AccountId>,
	T::PoolAssetId: Into<u32>,
{
	let fee_asset = T::PoolSetupFeeAsset::get();
	let (_, _) = create_asset::<T>(&fee_asset);
	let (_, _) = create_asset::<T>(asset1);
	let (caller, caller_lookup) = create_asset::<T>(asset2);

	assert_ok!(T::Assets::mint_into(
		fee_asset.clone(),
		&caller,
		T::PoolSetupFee::get() + T::Assets::minimum_balance(fee_asset)
	));

	assert_ok!(AssetConversion::<T>::create_pool(
		SystemOrigin::Signed(caller.clone()).into(),
		Box::new(asset1.clone()),
		Box::new(asset2.clone())
	));
	let lp_token = get_lp_token_id::<T>();

	(lp_token, caller, caller_lookup)
}

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<T>::events();
	let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
	// compare to the last event record
	let frame_system::EventRecord { event, .. } = &events[events.len() - 1];
	assert_eq!(event, &system_event);
}

#[benchmarks(where T::Balance: From<u128> + Into<u128>, T::Assets: Create<T::AccountId> + Mutate<T::AccountId>, T::PoolAssetId: Into<u32>,)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn create_pool() {
		let (asset1, asset2) = T::BenchmarkHelper::create_pair(0, 1);
		let (_, _) = create_asset::<T>(&asset1);
		let (caller, _) = create_asset::<T>(&asset2);

		assert_ok!(T::Assets::mint_into(
			T::PoolSetupFeeAsset::get(),
			&caller,
			T::PoolSetupFee::get() + T::Assets::minimum_balance(T::PoolSetupFeeAsset::get())
		));

		#[extrinsic_call]
		_(SystemOrigin::Signed(caller.clone()), Box::new(asset1.clone()), Box::new(asset2.clone()));

		let lp_token = get_lp_token_id::<T>();
		let pool_id = T::PoolLocator::pool_id(&asset1, &asset2).unwrap();
		let pool_account = T::PoolLocator::address(&pool_id).unwrap();
		assert_last_event::<T>(
			Event::PoolCreated { creator: caller.clone(), pool_account, pool_id, lp_token }.into(),
		);
	}

	#[benchmark]
	fn add_liquidity() {
		let (asset1, asset2) = T::BenchmarkHelper::create_pair(0, 1);
		let (lp_token, caller, _) = create_asset_and_pool::<T>(&asset1, &asset2);
		let ed: u128 = T::Assets::minimum_balance(asset1.clone()).into();
		let add_amount = 1000 + ed;

		#[extrinsic_call]
		_(
			SystemOrigin::Signed(caller.clone()),
			Box::new(asset1.clone()),
			Box::new(asset2.clone()),
			add_amount.into(),
			1000.into(),
			0.into(),
			0.into(),
			caller.clone(),
		);

		let pool_account = T::PoolLocator::pool_address(&asset1, &asset2).unwrap();
		let lp_minted =
			AssetConversion::<T>::calc_lp_amount_for_zero_supply(&add_amount.into(), &1000.into())
				.unwrap()
				.into();
		assert_eq!(T::PoolAssets::balance(lp_token, &caller), lp_minted.into());
		assert_eq!(T::Assets::balance(asset1, &pool_account), add_amount.into());
		assert_eq!(T::Assets::balance(asset2, &pool_account), 1000.into());
	}

	#[benchmark]
	fn remove_liquidity() {
		let (asset1, asset2) = T::BenchmarkHelper::create_pair(0, 1);
		let (lp_token, caller, _) = create_asset_and_pool::<T>(&asset1, &asset2);
		let ed: u128 = T::Assets::minimum_balance(asset1.clone()).into();
		let add_amount = 100 * ed;
		let lp_minted =
			AssetConversion::<T>::calc_lp_amount_for_zero_supply(&add_amount.into(), &1000.into())
				.unwrap()
				.into();
		let remove_lp_amount = lp_minted.checked_div(10).unwrap();

		assert_ok!(AssetConversion::<T>::add_liquidity(
			SystemOrigin::Signed(caller.clone()).into(),
			Box::new(asset1.clone()),
			Box::new(asset2.clone()),
			add_amount.into(),
			1000.into(),
			0.into(),
			0.into(),
			caller.clone(),
		));
		let total_supply =
			<T::PoolAssets as Inspect<T::AccountId>>::total_issuance(lp_token.clone());

		#[extrinsic_call]
		_(
			SystemOrigin::Signed(caller.clone()),
			Box::new(asset1),
			Box::new(asset2),
			remove_lp_amount.into(),
			0.into(),
			0.into(),
			caller.clone(),
		);

		let new_total_supply =
			<T::PoolAssets as Inspect<T::AccountId>>::total_issuance(lp_token.clone());
		assert_eq!(new_total_supply, total_supply - remove_lp_amount.into());
	}

	#[benchmark]
	fn swap_exact_tokens_for_tokens() {
		let (asset1, asset2) = T::BenchmarkHelper::create_pair(0, 1);
		let (_, asset3) = T::BenchmarkHelper::create_pair(1, 2);
		let (_, asset4) = T::BenchmarkHelper::create_pair(2, 3);

		let (_, caller, _) = create_asset_and_pool::<T>(&asset1, &asset2);
		let ed: u128 = T::Assets::minimum_balance(asset1.clone()).into();

		assert_ok!(AssetConversion::<T>::add_liquidity(
			SystemOrigin::Signed(caller.clone()).into(),
			Box::new(asset1.clone()),
			Box::new(asset2.clone()),
			(100 * ed).into(),
			200.into(),
			1.into(),
			0.into(),
			caller.clone(),
		));

		let (_, _, _) = create_asset_and_pool::<T>(&asset2, &asset3);
		assert_ok!(AssetConversion::<T>::add_liquidity(
			SystemOrigin::Signed(caller.clone()).into(),
			Box::new(asset2.clone()),
			Box::new(asset3.clone()),
			200.into(),
			2000.into(),
			1.into(),
			0.into(),
			caller.clone(),
		));

		let (_, _, _) = create_asset_and_pool::<T>(&asset3, &asset4);
		assert_ok!(AssetConversion::<T>::add_liquidity(
			SystemOrigin::Signed(caller.clone()).into(),
			Box::new(asset3.clone()),
			Box::new(asset4.clone()),
			2000.into(),
			2000.into(),
			1.into(),
			1.into(),
			caller.clone(),
		));
		let path = vec![
			Box::new(asset1.clone()),
			Box::new(asset2.clone()),
			Box::new(asset3.clone()),
			Box::new(asset4.clone()),
		];

		let swap_amount = ed.into();
		let asset1_balance = T::Assets::balance(asset1.clone(), &caller);

		#[extrinsic_call]
		_(SystemOrigin::Signed(caller.clone()), path, swap_amount, 1.into(), caller.clone(), false);

		let new_asset1_balance = T::Assets::balance(asset1, &caller);
		assert_eq!(new_asset1_balance, asset1_balance - ed.into());
	}

	#[benchmark]
	fn swap_tokens_for_exact_tokens() {
		let (asset1, asset2) = T::BenchmarkHelper::create_pair(0, 1);
		let (_, asset3) = T::BenchmarkHelper::create_pair(1, 2);
		let (_, asset4) = T::BenchmarkHelper::create_pair(2, 3);

		let (_, caller, _) = create_asset_and_pool::<T>(&asset1, &asset2);
		let ed: u128 = T::Assets::minimum_balance(asset1.clone()).into();

		assert_ok!(AssetConversion::<T>::add_liquidity(
			SystemOrigin::Signed(caller.clone()).into(),
			Box::new(asset1.clone()),
			Box::new(asset2.clone()),
			(1000 * ed).into(),
			500.into(),
			1.into(),
			0.into(),
			caller.clone(),
		));

		let (_, _, _) = create_asset_and_pool::<T>(&asset2, &asset3);
		assert_ok!(AssetConversion::<T>::add_liquidity(
			SystemOrigin::Signed(caller.clone()).into(),
			Box::new(asset2.clone()),
			Box::new(asset3.clone()),
			2000.into(),
			2000.into(),
			1.into(),
			0.into(),
			caller.clone(),
		));

		let (_, _, _) = create_asset_and_pool::<T>(&asset3, &asset4);
		assert_ok!(AssetConversion::<T>::add_liquidity(
			SystemOrigin::Signed(caller.clone()).into(),
			Box::new(asset3.clone()),
			Box::new(asset4.clone()),
			2000.into(),
			2000.into(),
			1.into(),
			0.into(),
			caller.clone(),
		));

		let path = vec![
			Box::new(asset1.clone()),
			Box::new(asset2.clone()),
			Box::new(asset3.clone()),
			Box::new(asset4.clone()),
		];

		let asset4_balance = T::Assets::balance(asset4.clone(), &caller);

		#[extrinsic_call]
		_(
			SystemOrigin::Signed(caller.clone()),
			path.clone(),
			100.into(),
			(1000 * ed).into(),
			caller.clone(),
			false,
		);

		let new_asset4_balance = T::Assets::balance(asset4, &caller);
		assert_eq!(new_asset4_balance, asset4_balance + 100.into());
	}

	impl_benchmark_test_suite!(AssetConversion, crate::mock::new_test_ext(), crate::mock::Test);
}
