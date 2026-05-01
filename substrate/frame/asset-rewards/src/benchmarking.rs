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

//! Asset Rewards pallet benchmarking.

use super::*;
use crate::Pallet as AssetRewards;
use frame_benchmarking::{v2::*, whitelisted_caller, BenchmarkError};
use frame_support::{
	assert_ok,
	traits::{
		fungibles::{Create, Inspect, Mutate},
		Consideration, EnsureOrigin, Footprint,
	},
};
use frame_system::{Pallet as System, RawOrigin};
use sp_runtime::{traits::One, Saturating};
use sp_std::prelude::*;

/// Benchmark Helper
pub trait BenchmarkHelper<AssetId> {
	/// Returns the staked asset id.
	///
	/// If the asset does not exist, it will be created by the benchmark.
	fn staked_asset() -> AssetId;
	/// Returns the reward asset id.
	///
	/// If the asset does not exist, it will be created by the benchmark.
	fn reward_asset() -> AssetId;
}

fn pool_expire<T: Config>() -> DispatchTime<BlockNumberFor<T>> {
	DispatchTime::At(BlockNumberFor::<T>::from(100u32))
}

fn create_reward_pool<T: Config>() -> Result<T::RuntimeOrigin, BenchmarkError>
where
	T::Assets: Create<T::AccountId> + Mutate<T::AccountId>,
{
	let caller_origin =
		T::CreatePoolOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
	let caller = T::CreatePoolOrigin::ensure_origin(caller_origin.clone()).unwrap();

	let footprint = Footprint::from_mel::<(PoolId, PoolInfoFor<T>)>();
	T::Consideration::ensure_successful(&caller, footprint);

	let staked_asset = T::BenchmarkHelper::staked_asset();
	let reward_asset = T::BenchmarkHelper::reward_asset();

	let min_staked_balance =
		T::Assets::minimum_balance(staked_asset.clone()).max(T::Balance::one());
	if !T::Assets::asset_exists(staked_asset.clone()) {
		assert_ok!(T::Assets::create(
			staked_asset.clone(),
			caller.clone(),
			true,
			min_staked_balance
		));
	}
	let min_reward_balance =
		T::Assets::minimum_balance(reward_asset.clone()).max(T::Balance::one());
	if !T::Assets::asset_exists(reward_asset.clone()) {
		assert_ok!(T::Assets::create(
			reward_asset.clone(),
			caller.clone(),
			true,
			min_reward_balance
		));
	}

	assert_ok!(AssetRewards::<T>::create_pool(
		caller_origin.clone(),
		Box::new(staked_asset),
		Box::new(reward_asset),
		// reward rate per block
		min_reward_balance,
		pool_expire::<T>(),
		Some(caller),
	));

	Ok(caller_origin)
}

fn mint_into<T: Config>(caller: &T::AccountId, asset: &T::AssetId) -> T::Balance
where
	T::Assets: Mutate<T::AccountId>,
{
	let min_balance = T::Assets::minimum_balance(asset.clone());
	assert_ok!(T::Assets::mint_into(
		asset.clone(),
		&caller,
		min_balance.saturating_mul(10u32.into())
	));
	min_balance
}

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	System::<T>::assert_last_event(generic_event.into());
}

#[benchmarks(where T::Assets: Create<T::AccountId> + Mutate<T::AccountId>)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn create_pool() -> Result<(), BenchmarkError> {
		let caller_origin =
			T::CreatePoolOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let caller = T::CreatePoolOrigin::ensure_origin(caller_origin.clone()).unwrap();

		let footprint = Footprint::from_mel::<(PoolId, PoolInfoFor<T>)>();
		T::Consideration::ensure_successful(&caller, footprint);

		let staked_asset = T::BenchmarkHelper::staked_asset();
		let reward_asset = T::BenchmarkHelper::reward_asset();

		let min_balance = T::Assets::minimum_balance(staked_asset.clone()).max(T::Balance::one());
		if !T::Assets::asset_exists(staked_asset.clone()) {
			assert_ok!(T::Assets::create(staked_asset.clone(), caller.clone(), true, min_balance));
		}
		let min_balance = T::Assets::minimum_balance(reward_asset.clone()).max(T::Balance::one());
		if !T::Assets::asset_exists(reward_asset.clone()) {
			assert_ok!(T::Assets::create(reward_asset.clone(), caller.clone(), true, min_balance));
		}

		#[extrinsic_call]
		_(
			caller_origin as T::RuntimeOrigin,
			Box::new(staked_asset.clone()),
			Box::new(reward_asset.clone()),
			min_balance,
			pool_expire::<T>(),
			Some(caller.clone()),
		);

		assert_last_event::<T>(
			Event::PoolCreated {
				creator: caller.clone(),
				admin: caller,
				staked_asset_id: staked_asset,
				reward_asset_id: reward_asset,
				reward_rate_per_block: min_balance,
				expiry_block: pool_expire::<T>()
					.evaluate(T::BlockNumberProvider::current_block_number()),
				pool_id: 0,
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn stake() -> Result<(), BenchmarkError> {
		create_reward_pool::<T>()?;

		let staker: T::AccountId = whitelisted_caller();
		let min_balance = mint_into::<T>(&staker, &T::BenchmarkHelper::staked_asset());

		// stake first to get worth case benchmark.
		assert_ok!(AssetRewards::<T>::stake(
			RawOrigin::Signed(staker.clone()).into(),
			0,
			min_balance
		));

		#[extrinsic_call]
		_(RawOrigin::Signed(staker.clone()), 0, min_balance);

		assert_last_event::<T>(Event::Staked { staker, pool_id: 0, amount: min_balance }.into());

		Ok(())
	}

	#[benchmark]
	fn unstake() -> Result<(), BenchmarkError> {
		create_reward_pool::<T>()?;

		let staker: T::AccountId = whitelisted_caller();
		let min_balance = mint_into::<T>(&staker, &T::BenchmarkHelper::staked_asset());

		assert_ok!(AssetRewards::<T>::stake(
			RawOrigin::Signed(staker.clone()).into(),
			0,
			min_balance,
		));

		#[extrinsic_call]
		_(RawOrigin::Signed(staker.clone()), 0, min_balance, None);

		assert_last_event::<T>(
			Event::Unstaked { caller: staker.clone(), staker, pool_id: 0, amount: min_balance }
				.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn harvest_rewards() -> Result<(), BenchmarkError> {
		create_reward_pool::<T>()?;

		let pool_acc = AssetRewards::<T>::pool_account_id(&0u32);
		let min_reward_balance = mint_into::<T>(&pool_acc, &T::BenchmarkHelper::reward_asset());

		let staker = whitelisted_caller();
		let _ = mint_into::<T>(&staker, &T::BenchmarkHelper::staked_asset());
		assert_ok!(AssetRewards::<T>::stake(
			RawOrigin::Signed(staker.clone()).into(),
			0,
			T::Balance::one(),
		));

		T::BlockNumberProvider::set_block_number(
			T::BlockNumberProvider::current_block_number() + BlockNumberFor::<T>::one(),
		);

		#[extrinsic_call]
		_(RawOrigin::Signed(staker.clone()), 0, None);

		assert_last_event::<T>(
			Event::RewardsHarvested {
				caller: staker.clone(),
				staker,
				pool_id: 0,
				amount: min_reward_balance,
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn set_pool_reward_rate_per_block() -> Result<(), BenchmarkError> {
		let caller_origin = create_reward_pool::<T>()?;

		// stake first to get worth case benchmark.
		{
			let staker: T::AccountId = whitelisted_caller();
			let min_balance = mint_into::<T>(&staker, &T::BenchmarkHelper::staked_asset());

			assert_ok!(AssetRewards::<T>::stake(RawOrigin::Signed(staker).into(), 0, min_balance));
		}

		let new_reward_rate_per_block =
			T::Assets::minimum_balance(T::BenchmarkHelper::reward_asset()).max(T::Balance::one()) +
				T::Balance::one();

		#[extrinsic_call]
		_(caller_origin as T::RuntimeOrigin, 0, new_reward_rate_per_block);

		assert_last_event::<T>(
			Event::PoolRewardRateModified { pool_id: 0, new_reward_rate_per_block }.into(),
		);
		Ok(())
	}

	#[benchmark]
	fn set_pool_admin() -> Result<(), BenchmarkError> {
		let caller_origin = create_reward_pool::<T>()?;
		let new_admin: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		_(caller_origin as T::RuntimeOrigin, 0, new_admin.clone());

		assert_last_event::<T>(Event::PoolAdminModified { pool_id: 0, new_admin }.into());

		Ok(())
	}

	#[benchmark]
	fn set_pool_expiry_block() -> Result<(), BenchmarkError> {
		let create_origin = create_reward_pool::<T>()?;

		// stake first to get worth case benchmark.
		{
			let staker: T::AccountId = whitelisted_caller();
			let min_balance = mint_into::<T>(&staker, &T::BenchmarkHelper::staked_asset());

			assert_ok!(AssetRewards::<T>::stake(RawOrigin::Signed(staker).into(), 0, min_balance));
		}

		let new_expiry_block = pool_expire::<T>()
			.evaluate(T::BlockNumberProvider::current_block_number()) +
			BlockNumberFor::<T>::one();

		#[extrinsic_call]
		_(create_origin as T::RuntimeOrigin, 0, DispatchTime::At(new_expiry_block));

		assert_last_event::<T>(
			Event::PoolExpiryBlockModified { pool_id: 0, new_expiry_block }.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn deposit_reward_tokens() -> Result<(), BenchmarkError> {
		create_reward_pool::<T>()?;
		let caller = whitelisted_caller();

		let reward_asset = T::BenchmarkHelper::reward_asset();
		let pool_acc = AssetRewards::<T>::pool_account_id(&0u32);
		let min_balance = mint_into::<T>(&caller, &reward_asset);

		let balance_before = T::Assets::balance(reward_asset.clone(), &pool_acc);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), 0, min_balance);

		let balance_after = T::Assets::balance(reward_asset, &pool_acc);

		assert_eq!(balance_after, balance_before + min_balance);

		Ok(())
	}

	#[benchmark]
	fn cleanup_pool() -> Result<(), BenchmarkError> {
		let create_origin = create_reward_pool::<T>()?;
		let caller = T::CreatePoolOrigin::ensure_origin(create_origin.clone()).unwrap();

		// deposit rewards tokens to get worth case benchmark.
		{
			let caller = whitelisted_caller();
			let reward_asset = T::BenchmarkHelper::reward_asset();
			let min_balance = mint_into::<T>(&caller, &reward_asset);
			assert_ok!(AssetRewards::<T>::deposit_reward_tokens(
				RawOrigin::Signed(caller).into(),
				0,
				min_balance
			));
		}

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), 0);

		assert_last_event::<T>(Event::PoolCleanedUp { pool_id: 0 }.into());

		Ok(())
	}

	impl_benchmark_test_suite!(AssetRewards, crate::mock::new_test_ext(), crate::mock::MockRuntime);
}
