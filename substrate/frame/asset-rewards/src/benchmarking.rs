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
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{
		fungibles::{Create, Inspect, Mutate},
		EnsureOrigin,
	},
};
use frame_system::{RawOrigin, RawOrigin::Root};
use sp_runtime::{traits::One, SaturatedConversion};
use sp_std::prelude::*;

/// Benchmark Helper
pub trait BenchmarkHelper<AssetId, AccountId> {
	/// Convert a u32 to an AssetId
	fn to_asset_id(seed: u32) -> AssetId;
	/// Convert a [u8; 32] to an AccountId
	fn to_account_id(seed: [u8; 32]) -> AccountId;
	/// Return the ID of the asset whos minimum balance is sufficient for an account to exist
	fn sufficient_asset() -> AssetId;
}

/// Create and mint the minimum amount of the sufficient asset.
fn create_and_mint_sufficient<T: Config>(caller: &T::AccountId)
where
	T::Assets: Create<T::AccountId> + Mutate<T::AccountId>,
{
	let sufficient_asset = T::BenchmarkHelper::sufficient_asset();
	create_and_mint_asset::<T>(
		&caller,
		&sufficient_asset.clone(),
		T::Assets::minimum_balance(sufficient_asset),
	);
}

/// Create the `asset` and mint the `amount` for the `caller`.
fn create_and_mint_asset<T: Config>(caller: &T::AccountId, asset: &T::AssetId, amount: T::Balance)
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

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<T>::events();
	let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
	// compare to the last event record
	let frame_system::EventRecord { event, .. } = &events[events.len() - 1];
	assert_eq!(event, &system_event);
}

#[benchmarks(where T::Assets: Create<T::AccountId> + Mutate<T::AccountId>)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn create_pool() {
		use super::*;

		let root_acc = T::PermissionedOrigin::ensure_origin(Root.into()).unwrap();
		let staked_asset = T::BenchmarkHelper::to_asset_id(1);
		let reward_asset = T::BenchmarkHelper::to_asset_id(2);
		create_and_mint_asset::<T>(
			&root_acc,
			&staked_asset,
			T::Assets::minimum_balance(staked_asset.clone()),
		);
		create_and_mint_asset::<T>(
			&root_acc,
			&reward_asset,
			T::Assets::minimum_balance(reward_asset.clone()),
		);

		#[extrinsic_call]
		_(
			Root,
			Box::new(staked_asset.clone()),
			Box::new(reward_asset.clone()),
			100u32.into(),
			200u32.into(),
			None,
		);

		assert_last_event::<T>(
			Event::PoolCreated {
				creator: root_acc.clone(),
				admin: root_acc,
				staked_asset_id: staked_asset,
				reward_asset_id: reward_asset,
				reward_rate_per_block: 100u32.into(),
				expiry_block: 200u32.into(),
				pool_id: 0u32.into(),
			}
			.into(),
		);
	}

	#[benchmark]
	fn stake() {
		use super::*;

		let staker = T::BenchmarkHelper::to_account_id([1u8; 32]);
		let staked_asset = T::BenchmarkHelper::to_asset_id(1);
		let reward_asset = T::BenchmarkHelper::to_asset_id(0);
		create_and_mint_asset::<T>(
			&staker,
			&staked_asset,
			T::Assets::minimum_balance(staked_asset.clone()),
		);
		create_and_mint_asset::<T>(
			&staker,
			&reward_asset,
			T::Assets::minimum_balance(reward_asset.clone()),
		);

		assert_ok!(AssetRewards::<T>::create_pool(
			Root.into(),
			Box::new(staked_asset.clone()),
			Box::new(reward_asset.clone()),
			100u32.into(),
			200u32.into(),
			None,
		));

		#[extrinsic_call]
		_(RawOrigin::Signed(staker.clone()), 0u32.into(), 100u32.into());

		assert_last_event::<T>(
			Event::Staked { who: staker, pool_id: 0u32.into(), amount: 100u32.into() }.into(),
		);
	}

	#[benchmark]
	fn unstake() {
		use super::*;

		let staker = T::BenchmarkHelper::to_account_id([1u8; 32]);
		let staked_asset = T::BenchmarkHelper::to_asset_id(1);
		let reward_asset = T::BenchmarkHelper::to_asset_id(0);
		create_and_mint_asset::<T>(
			&staker,
			&staked_asset,
			T::Assets::minimum_balance(staked_asset.clone()),
		);
		create_and_mint_asset::<T>(
			&staker,
			&reward_asset,
			T::Assets::minimum_balance(reward_asset.clone()),
		);

		assert_ok!(AssetRewards::<T>::create_pool(
			Root.into(),
			Box::new(staked_asset.clone()),
			Box::new(reward_asset.clone()),
			100u32.into(),
			200u32.into(),
			None,
		));

		assert_ok!(AssetRewards::<T>::stake(
			RawOrigin::Signed(staker.clone()).into(),
			0u32.into(),
			100u32.into()
		));

		#[extrinsic_call]
		_(RawOrigin::Signed(staker.clone()), 0u32.into(), 100u32.into());

		assert_last_event::<T>(
			Event::Unstaked { who: staker, pool_id: 0u32.into(), amount: 100u32.into() }.into(),
		);
	}

	#[benchmark]
	fn harvest_rewards() {
		use super::*;

		let block_number_before = frame_system::Pallet::<T>::block_number();
		let staker = T::BenchmarkHelper::to_account_id([2u8; 32]);
		let staked_asset = T::BenchmarkHelper::to_asset_id(1);
		let reward_asset = T::BenchmarkHelper::to_asset_id(2);
		create_and_mint_asset::<T>(
			&staker,
			&staked_asset,
			T::Assets::minimum_balance(staked_asset.clone()),
		);
		create_and_mint_asset::<T>(
			&staker,
			&reward_asset,
			T::Assets::minimum_balance(reward_asset.clone()),
		);
		assert_ok!(AssetRewards::<T>::create_pool(
			Root.into(),
			Box::new(staked_asset.clone()),
			Box::new(reward_asset.clone()),
			100u32.into(),
			200u32.into(),
			None,
		));

		let pool_acc = AssetRewards::<T>::pool_account_id(&0u32).unwrap();
		create_and_mint_sufficient::<T>(&pool_acc);
		create_and_mint_asset::<T>(&pool_acc, &reward_asset, 100_000u32.into());
		assert_ok!(AssetRewards::<T>::stake(
			RawOrigin::Signed(staker.clone()).into(),
			0u32.into(),
			100u32.into()
		));

		#[extrinsic_call]
		_(RawOrigin::Signed(staker.clone()), 0u32.into(), None);

		let block_number_after = frame_system::Pallet::<T>::block_number();

		// In tests a block doesn't pass but when running benchmarks for node-template one does.
		// Not sure why, but adding this to correctly calculate the harvested amount.
		//
		// TODO: Before merging understand this
		let blocks_elapsed = block_number_after - block_number_before;

		assert_last_event::<T>(
			Event::RewardsHarvested {
				who: staker.clone(),
				staker,
				pool_id: 0u32.into(),
				amount: (blocks_elapsed * 100u8.into()).saturated_into::<u32>().into(),
			}
			.into(),
		);
	}

	#[benchmark]
	fn set_pool_reward_rate_per_block() {
		use super::*;

		let acc = T::BenchmarkHelper::to_account_id([3u8; 32]);
		let staked_asset = T::BenchmarkHelper::to_asset_id(1);
		let reward_asset = T::BenchmarkHelper::to_asset_id(5);
		create_and_mint_asset::<T>(
			&acc,
			&staked_asset,
			T::Assets::minimum_balance(staked_asset.clone()),
		);
		create_and_mint_asset::<T>(
			&acc,
			&reward_asset,
			T::Assets::minimum_balance(reward_asset.clone()),
		);

		assert_ok!(AssetRewards::<T>::create_pool(
			Root.into(),
			Box::new(staked_asset.clone()),
			Box::new(reward_asset.clone()),
			100u32.into(),
			200u32.into(),
			None,
		));

		#[extrinsic_call]
		_(Root, 0u32.into(), 5u32.into());

		assert_last_event::<T>(
			Event::PoolRewardRateModified {
				pool_id: 0u32.into(),
				new_reward_rate_per_block: 5u32.into(),
			}
			.into(),
		);
	}

	#[benchmark]
	fn set_pool_admin() {
		use super::*;

		let new_admin = T::BenchmarkHelper::to_account_id([2u8; 32]);
		let staked_asset = T::BenchmarkHelper::to_asset_id(1);
		let reward_asset = T::BenchmarkHelper::to_asset_id(0);
		create_and_mint_asset::<T>(
			&new_admin,
			&staked_asset,
			T::Assets::minimum_balance(staked_asset.clone()),
		);
		create_and_mint_asset::<T>(
			&new_admin,
			&reward_asset,
			T::Assets::minimum_balance(reward_asset.clone()),
		);

		assert_ok!(AssetRewards::<T>::create_pool(
			Root.into(),
			Box::new(staked_asset.clone()),
			Box::new(reward_asset.clone()),
			100u32.into(),
			200u32.into(),
			None,
		));

		#[extrinsic_call]
		_(Root, 0u32.into(), new_admin.clone());

		assert_last_event::<T>(Event::PoolAdminModified { pool_id: 0u32.into(), new_admin }.into());
	}

	#[benchmark]
	fn set_pool_expiry_block() {
		use super::*;

		let staked_asset = T::BenchmarkHelper::to_asset_id(2);
		let reward_asset = T::BenchmarkHelper::to_asset_id(5);
		let acc = T::BenchmarkHelper::to_account_id([5u8; 32]);
		create_and_mint_asset::<T>(
			&acc,
			&staked_asset,
			T::Assets::minimum_balance(staked_asset.clone()),
		);
		create_and_mint_asset::<T>(
			&acc,
			&reward_asset,
			T::Assets::minimum_balance(reward_asset.clone()),
		);

		assert_ok!(AssetRewards::<T>::create_pool(
			Root.into(),
			Box::new(staked_asset.clone()),
			Box::new(reward_asset.clone()),
			100u32.into(),
			200u32.into(),
			None,
		));

		#[extrinsic_call]
		_(Root, 0u32.into(), 1000u32.into());

		assert_last_event::<T>(
			Event::PoolExpiryBlockModified {
				pool_id: 0u32.into(),
				new_expiry_block: 1000u32.into(),
			}
			.into(),
		);
	}

	#[benchmark]
	fn deposit_reward_tokens() {
		use super::*;

		let acc = T::BenchmarkHelper::to_account_id([3u8; 32]);
		let staked_asset = T::BenchmarkHelper::to_asset_id(1);
		let reward_asset = T::BenchmarkHelper::to_asset_id(4);
		create_and_mint_asset::<T>(&acc, &reward_asset, 100_000u32.into());
		create_and_mint_asset::<T>(&acc, &staked_asset, 100_000u32.into());

		assert_ok!(AssetRewards::<T>::create_pool(
			Root.into(),
			Box::new(staked_asset.clone()),
			Box::new(reward_asset.clone()),
			100u32.into(),
			200u32.into(),
			None,
		));

		let pool_acc = AssetRewards::<T>::pool_account_id(&0u32).unwrap();
		create_and_mint_sufficient::<T>(&pool_acc);

		let balance_before = T::Assets::balance(reward_asset.clone(), &acc);

		#[extrinsic_call]
		_(RawOrigin::Signed(acc.clone()), 0u32.into(), 10_000u32.into());

		let balance_after = T::Assets::balance(reward_asset.clone(), &acc);

		assert_eq!(balance_after, balance_before - 10_000u32.into());
	}

	#[benchmark]
	fn withdraw_reward_tokens() {
		use super::*;

		let acc = T::BenchmarkHelper::to_account_id([3u8; 32]);
		let staked_asset = T::BenchmarkHelper::to_asset_id(1);
		let reward_asset = T::BenchmarkHelper::to_asset_id(2);
		create_and_mint_asset::<T>(&acc, &staked_asset, 10000u32.into());
		create_and_mint_asset::<T>(&acc, &reward_asset, 10000u32.into());

		assert_ok!(AssetRewards::<T>::create_pool(
			Root.into(),
			Box::new(staked_asset.clone()),
			Box::new(reward_asset.clone()),
			100u32.into(),
			200u32.into(),
			None,
		));
		let pool_acc = AssetRewards::<T>::pool_account_id(&0u32).unwrap();
		create_and_mint_sufficient::<T>(&pool_acc);

		let balance_before = T::Assets::balance(reward_asset.clone(), &acc);

		assert_ok!(AssetRewards::<T>::deposit_reward_tokens(
			RawOrigin::Signed(acc.clone()).into(),
			0u32.into(),
			100u32.into()
		));

		#[extrinsic_call]
		_(Root, 0u32.into(), 50u32.into(), acc.clone());

		let balance_after = T::Assets::balance(reward_asset.clone(), &acc);

		// Deposited 100, withdrew 50
		assert_eq!(balance_after, balance_before - 50u32.into());
	}

	impl_benchmark_test_suite!(AssetRewards, crate::mock::new_test_ext(), crate::mock::MockRuntime);
}
