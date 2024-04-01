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

use crate::{mock::*, *};
use frame_support::{assert_err, assert_ok, traits::fungible::NativeOrWithId};

fn create_tokens(owner: u128, tokens: Vec<NativeOrWithId<u32>>) {
	create_tokens_with_ed(owner, tokens, 1)
}

fn create_tokens_with_ed(owner: u128, tokens: Vec<NativeOrWithId<u32>>, ed: u128) {
	for token_id in tokens {
		let asset_id = match token_id {
			NativeOrWithId::WithId(id) => id,
			_ => unreachable!("invalid token"),
		};
		assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, false, ed));
	}
}

fn events() -> Vec<Event<MockRuntime>> {
	let result = System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| {
			if let mock::RuntimeEvent::StakingRewards(inner) = e {
				Some(inner)
			} else {
				None
			}
		})
		.collect();

	System::reset_events();

	result
}

fn pools() -> Vec<(u32, PoolInfo<u128, NativeOrWithId<u32>, u128, u64>)> {
	Pools::<MockRuntime>::iter().collect()
}

mod create_pool {
	use sp_runtime::traits::BadOrigin;

	use super::*;

	#[test]
	fn success() {
		new_test_ext().execute_with(|| {
			// Setup
			let user = 1;
			let staking_asset_id = NativeOrWithId::<u32>::Native;
			let reward_asset_id = NativeOrWithId::<u32>::WithId(1);
			let reward_rate_per_block = 100;

			create_tokens(user, vec![reward_asset_id.clone()]);
			assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), user, 1000));

			// Create a pool with default admin.
			assert_eq!(NextPoolId::<MockRuntime>::get(), 0);
			assert_ok!(StakingRewards::create_pool(
				RuntimeOrigin::signed(user),
				Box::new(staking_asset_id.clone()),
				Box::new(reward_asset_id.clone()),
				reward_rate_per_block,
				None
			));

			// Event is emitted.
			assert_eq!(
				events(),
				[Event::<MockRuntime>::PoolCreated {
					creator: user,
					pool_id: 0,
					staking_asset_id: staking_asset_id.clone(),
					reward_asset_id: reward_asset_id.clone(),
					reward_rate_per_block,
					admin: user,
				}]
			);

			// State is updated correctly.
			assert_eq!(NextPoolId::<MockRuntime>::get(), 1);
			assert_eq!(
				pools(),
				vec![(
					0,
					PoolInfo {
						staking_asset_id: staking_asset_id.clone(),
						reward_asset_id: reward_asset_id.clone(),
						reward_rate_per_block,
						admin: user,
						total_tokens_staked: 0,
						reward_per_token_stored: 0,
						last_update_block: 0
					}
				)]
			);

			// Create another pool with explicit admin.
			let admin = 2;
			assert_ok!(StakingRewards::create_pool(
				RuntimeOrigin::signed(user),
				Box::new(staking_asset_id.clone()),
				Box::new(reward_asset_id.clone()),
				reward_rate_per_block,
				Some(admin)
			));

			// Event is emitted.
			assert_eq!(
				events(),
				[Event::<MockRuntime>::PoolCreated {
					creator: user,
					pool_id: 1,
					staking_asset_id: staking_asset_id.clone(),
					reward_asset_id: reward_asset_id.clone(),
					reward_rate_per_block,
					admin,
				}]
			);

			// State is updated correctly.
			assert_eq!(NextPoolId::<MockRuntime>::get(), 2);
			assert_eq!(
				pools(),
				vec![
					(
						0,
						PoolInfo {
							staking_asset_id: staking_asset_id.clone(),
							reward_asset_id: reward_asset_id.clone(),
							reward_rate_per_block,
							admin: user,
							total_tokens_staked: 0,
							reward_per_token_stored: 0,
							last_update_block: 0
						}
					),
					(
						1,
						PoolInfo {
							staking_asset_id,
							reward_asset_id,
							reward_rate_per_block,
							admin,
							total_tokens_staked: 0,
							reward_per_token_stored: 0,
							last_update_block: 0
						}
					)
				]
			);
		});
	}

	#[test]
	fn fails_for_non_existent_asset() {
		new_test_ext().execute_with(|| {
			let valid_asset = NativeOrWithId::<u32>::WithId(1);
			let invalid_asset = NativeOrWithId::<u32>::WithId(200);

			assert_err!(
				StakingRewards::create_pool(
					RuntimeOrigin::signed(1),
					Box::new(valid_asset.clone()),
					Box::new(invalid_asset.clone()),
					10,
					None
				),
				Error::<MockRuntime>::NonExistentAsset
			);

			assert_err!(
				StakingRewards::create_pool(
					RuntimeOrigin::signed(1),
					Box::new(invalid_asset.clone()),
					Box::new(valid_asset.clone()),
					10,
					None
				),
				Error::<MockRuntime>::NonExistentAsset
			);

			assert_err!(
				StakingRewards::create_pool(
					RuntimeOrigin::signed(1),
					Box::new(invalid_asset.clone()),
					Box::new(invalid_asset.clone()),
					10,
					None
				),
				Error::<MockRuntime>::NonExistentAsset
			);
		})
	}

	#[test]
	fn fails_for_not_admin() {
		new_test_ext().execute_with(|| {
			let user = 100;
			let staking_asset_id = NativeOrWithId::<u32>::Native;
			let reward_asset_id = NativeOrWithId::<u32>::WithId(1);
			let reward_rate_per_block = 100;
			create_tokens(user, vec![reward_asset_id.clone()]);
			assert_err!(
				StakingRewards::create_pool(
					RuntimeOrigin::signed(user),
					Box::new(staking_asset_id.clone()),
					Box::new(reward_asset_id.clone()),
					reward_rate_per_block,
					Some(999)
				),
				BadOrigin
			);
		});
	}
}

mod stake {
	use super::*;

	#[test]
	fn success() {
		new_test_ext().execute_with(|| {
			// Setup
			let user = 1;
			let staking_asset_id = NativeOrWithId::<u32>::WithId(1);
			let reward_asset_id = NativeOrWithId::<u32>::Native;
			let reward_rate_per_block = 100;
			create_tokens(user, vec![staking_asset_id.clone()]);

			assert_ok!(StakingRewards::create_pool(
				RuntimeOrigin::signed(user),
				Box::new(staking_asset_id.clone()),
				Box::new(reward_asset_id.clone()),
				reward_rate_per_block,
				None
			));

			let pool_id = 0;

			// User stakes tokens
			assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(user), pool_id, 1000));

			// Check that the user's staked amount is updated
			assert_eq!(PoolStakers::<MockRuntime>::get(pool_id, user).unwrap().amount, 1000);

			// Check that the pool's total tokens staked is updated
			assert_eq!(Pools::<MockRuntime>::get(pool_id).unwrap().total_tokens_staked, 1000);

			// TODO: Check user's frozen balance is updated

			// User stakes more tokens
			assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(user), pool_id, 500));

			// Check that the user's staked amount is updated
			assert_eq!(PoolStakers::<MockRuntime>::get(pool_id, user).unwrap().amount, 1500);

			// Check that the pool's total tokens staked is updated
			assert_eq!(Pools::<MockRuntime>::get(pool_id).unwrap().total_tokens_staked, 1500);

			// TODO: Check user's frozen balance is updated
		});
	}

	#[test]
	fn fails_for_non_existent_pool() {
		new_test_ext().execute_with(|| {
			let user = 1;
			let staking_asset_id = NativeOrWithId::<u32>::WithId(1);
			create_tokens(user, vec![staking_asset_id.clone()]);

			let non_existent_pool_id = 999;

			assert_err!(
				StakingRewards::stake(RuntimeOrigin::signed(user), non_existent_pool_id, 1000),
				Error::<MockRuntime>::NonExistentPool
			);
		});
	}

	#[test]
	fn fails_for_insufficient_balance() {
		// TODO: When we're able to freeze assets.
	}
}

mod unstake {
	use super::*;

	#[test]
	fn success() {
		new_test_ext().execute_with(|| {
			// Setup
			let user = 1;
			let staking_asset_id = NativeOrWithId::<u32>::WithId(1);
			let reward_asset_id = NativeOrWithId::<u32>::WithId(2);
			let reward_rate_per_block = 100;
			create_tokens(user, vec![staking_asset_id.clone(), reward_asset_id.clone()]);

			assert_ok!(StakingRewards::create_pool(
				RuntimeOrigin::signed(user),
				Box::new(staking_asset_id.clone()),
				Box::new(reward_asset_id.clone()),
				reward_rate_per_block,
				None
			));

			let pool_id = 0;

			// User stakes tokens
			assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(user), pool_id, 1000));

			// User unstakes tokens
			assert_ok!(StakingRewards::unstake(RuntimeOrigin::signed(user), pool_id, 500));

			// Check that the user's staked amount is updated
			assert_eq!(PoolStakers::<MockRuntime>::get(pool_id, user).unwrap().amount, 500);

			// Check that the pool's total tokens staked is updated
			assert_eq!(Pools::<MockRuntime>::get(pool_id).unwrap().total_tokens_staked, 500);

			// User unstakes remaining tokens
			assert_ok!(StakingRewards::unstake(RuntimeOrigin::signed(user), pool_id, 500));

			// Check that the user's staked amount is zero
			assert_eq!(PoolStakers::<MockRuntime>::get(pool_id, user).unwrap().amount, 0);

			// Check that the pool's total tokens staked is zero
			assert_eq!(Pools::<MockRuntime>::get(pool_id).unwrap().total_tokens_staked, 0);
		});
	}

	#[test]
	fn fails_for_non_existent_pool() {
		new_test_ext().execute_with(|| {
			// Setup
			let user = 1;
			let non_existent_pool_id = 999;

			// User tries to unstake tokens from a non-existent pool
			assert_err!(
				StakingRewards::unstake(RuntimeOrigin::signed(user), non_existent_pool_id, 500),
				Error::<MockRuntime>::NonExistentPool
			);
		});
	}

	#[test]
	fn fails_for_insufficient_staked_amount() {
		new_test_ext().execute_with(|| {
			// Setup
			let user = 1;
			let staking_asset_id = NativeOrWithId::<u32>::WithId(1);
			let reward_asset_id = NativeOrWithId::<u32>::WithId(2);
			let reward_rate_per_block = 100;

			create_tokens(user, vec![staking_asset_id.clone(), reward_asset_id.clone()]);

			assert_ok!(StakingRewards::create_pool(
				RuntimeOrigin::signed(user),
				Box::new(staking_asset_id.clone()),
				Box::new(reward_asset_id.clone()),
				reward_rate_per_block,
				None
			));

			let pool_id = 0;

			// User stakes tokens
			assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(user), pool_id, 1000));

			// User tries to unstake more tokens than they have staked
			assert_err!(
				StakingRewards::unstake(RuntimeOrigin::signed(user), pool_id, 1500),
				Error::<MockRuntime>::NotEnoughTokens
			);
		});
	}
}
