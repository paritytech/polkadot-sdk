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
use frame_support::{assert_ok, traits::fungible::NativeOrWithId};

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

#[test]
fn create_pool_works() {
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
					accumulated_rewards_per_share: 0,
					last_rewarded_block: 0
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
						accumulated_rewards_per_share: 0,
						last_rewarded_block: 0
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
						accumulated_rewards_per_share: 0,
						last_rewarded_block: 0
					}
				)
			]
		);
	});
}
