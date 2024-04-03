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
use frame_support::{assert_err, assert_ok, hypothetically, traits::fungible::NativeOrWithId};
use sp_runtime::traits::BadOrigin;

/// Creates a basic pool with values:
/// - Staking asset: 1
/// - Reward asset: Native
/// - Reward rate per block: 100
/// - Expiry block: 100
/// - Admin: 1
///
/// Useful to reduce boilerplate in tests when it's not important to customise or reusing pool
/// params.
fn create_default_pool() {
	let staking_asset_id = NativeOrWithId::<u32>::WithId(1);
	assert_ok!(StakingRewards::create_pool(
		RuntimeOrigin::signed(1),
		Box::new(staking_asset_id),
		Box::new(NativeOrWithId::<u32>::Native),
		100,
		100u64,
		None
	));
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
	use super::*;

	#[test]
	fn success() {
		new_test_ext().execute_with(|| {
			let user = 1;
			let staking_asset_id = NativeOrWithId::<u32>::Native;
			let reward_asset_id = NativeOrWithId::<u32>::WithId(1);
			let reward_rate_per_block = 100;
			let expiry_block = 200u64;

			assert_eq!(NextPoolId::<MockRuntime>::get(), 0);
			assert_ok!(StakingRewards::create_pool(
				RuntimeOrigin::signed(user),
				Box::new(staking_asset_id.clone()),
				Box::new(reward_asset_id.clone()),
				reward_rate_per_block,
				expiry_block,
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
					expiry_block,
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
						expiry_block,
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
				expiry_block,
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
					expiry_block,
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
							expiry_block,
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
							expiry_block,
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
					10u64,
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
					10u64,
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
					10u64,
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
			let expiry_block = 100u64;
			assert_err!(
				StakingRewards::create_pool(
					RuntimeOrigin::signed(user),
					Box::new(staking_asset_id.clone()),
					Box::new(reward_asset_id.clone()),
					reward_rate_per_block,
					expiry_block,
					Some(999)
				),
				BadOrigin
			);
		});
	}

	#[test]
	fn fails_for_bad_expiry_block() {
		new_test_ext().execute_with(|| {
			let user = 1;
			let staking_asset_id = NativeOrWithId::<u32>::Native;
			let reward_asset_id = NativeOrWithId::<u32>::WithId(1);
			let reward_rate_per_block = 100;
			let expiry_block = 100u64;
			System::set_block_number(expiry_block + 1u64);
			assert_err!(
				StakingRewards::create_pool(
					RuntimeOrigin::signed(user),
					Box::new(staking_asset_id.clone()),
					Box::new(reward_asset_id.clone()),
					reward_rate_per_block,
					expiry_block,
					None
				),
				Error::<MockRuntime>::ExpiryBlockMustBeInTheFuture
			);
		});
	}
}

mod stake {
	use super::*;

	#[test]
	fn success() {
		new_test_ext().execute_with(|| {
			let user = 1;
			create_default_pool();
			let pool_id = 0;

			// User stakes tokens
			assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(user), pool_id, 1000));

			// Check that the user's staked amount is updated
			assert_eq!(PoolStakers::<MockRuntime>::get(pool_id, user).unwrap().amount, 1000);

			// Event is emitted.
			assert_eq!(
				*events().last().unwrap(),
				Event::<MockRuntime>::Staked { who: user, amount: 1000, pool_id: 0 }
			);

			// Check that the pool's total tokens staked is updated
			assert_eq!(Pools::<MockRuntime>::get(pool_id).unwrap().total_tokens_staked, 1000);

			// TODO: Check user's frozen balance is updated

			// User stakes more tokens
			assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(user), pool_id, 500));

			// Event is emitted.
			assert_eq!(
				*events().last().unwrap(),
				Event::<MockRuntime>::Staked { who: user, amount: 500, pool_id: 0 }
			);

			// Check that the user's staked amount is updated
			assert_eq!(PoolStakers::<MockRuntime>::get(pool_id, user).unwrap().amount, 1500);

			// Check that the pool's total tokens staked is updated
			assert_eq!(Pools::<MockRuntime>::get(pool_id).unwrap().total_tokens_staked, 1500);

			// TODO: Check user's frozen balance is updated

			// Event is emitted.
			assert_eq!(events(), []);
		});
	}

	#[test]
	fn fails_for_non_existent_pool() {
		new_test_ext().execute_with(|| {
			let user = 1;
			assert_err!(
				StakingRewards::stake(RuntimeOrigin::signed(user), 999, 1000),
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
			let user = 1;
			create_default_pool();
			let pool_id = 0;

			// User stakes tokens
			assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(user), pool_id, 1000));

			// User unstakes tokens
			assert_ok!(StakingRewards::unstake(RuntimeOrigin::signed(user), pool_id, 500));

			// Event is emitted.
			assert_eq!(
				*events().last().unwrap(),
				Event::<MockRuntime>::Unstaked { who: user, amount: 500, pool_id: 0 }
			);

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
			let user = 1;
			create_default_pool();
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

mod set_pool_admin {
	use super::*;

	#[test]
	fn success() {
		new_test_ext().execute_with(|| {
			let admin = 1;
			let new_admin = 2;
			let pool_id = 0;
			create_default_pool();

			// Modify the pool admin
			assert_ok!(StakingRewards::set_pool_admin(
				RuntimeOrigin::signed(admin),
				pool_id,
				new_admin
			));

			// Check state
			assert_eq!(
				*events().last().unwrap(),
				Event::<MockRuntime>::PoolAdminModified { pool_id, new_admin }
			);
			assert_eq!(Pools::<MockRuntime>::get(pool_id).unwrap().admin, new_admin);
		});
	}

	#[test]
	fn fails_for_non_existent_pool() {
		new_test_ext().execute_with(|| {
			let admin = 1;
			let new_admin = 2;
			let non_existent_pool_id = 999;

			assert_err!(
				StakingRewards::set_pool_admin(
					RuntimeOrigin::signed(admin),
					non_existent_pool_id,
					new_admin
				),
				Error::<MockRuntime>::NonExistentPool
			);
		});
	}

	#[test]
	fn fails_for_non_admin() {
		new_test_ext().execute_with(|| {
			let new_admin = 2;
			let non_admin = 3;
			let pool_id = 0;
			create_default_pool();

			assert_err!(
				StakingRewards::set_pool_admin(
					RuntimeOrigin::signed(non_admin),
					pool_id,
					new_admin
				),
				BadOrigin
			);
		});
	}
}

mod set_pool_expiry_block {
	use super::*;

	#[test]
	fn success() {
		new_test_ext().execute_with(|| {
			let admin = 1;
			let pool_id = 0;
			let new_expiry_block = 200u64;
			create_default_pool();

			// Modify the pool expiry block
			assert_ok!(StakingRewards::set_pool_expiry_block(
				RuntimeOrigin::signed(admin),
				pool_id,
				new_expiry_block
			));

			// Check state
			assert_eq!(
				*events().last().unwrap(),
				Event::<MockRuntime>::PoolExpiryBlockModified { pool_id, new_expiry_block }
			);
			assert_eq!(Pools::<MockRuntime>::get(pool_id).unwrap().expiry_block, new_expiry_block);
		});
	}

	#[test]
	fn fails_for_non_existent_pool() {
		new_test_ext().execute_with(|| {
			let admin = 1;
			let non_existent_pool_id = 999;
			let new_expiry_block = 200u64;

			assert_err!(
				StakingRewards::set_pool_expiry_block(
					RuntimeOrigin::signed(admin),
					non_existent_pool_id,
					new_expiry_block
				),
				Error::<MockRuntime>::NonExistentPool
			);
		});
	}

	#[test]
	fn fails_for_non_admin() {
		new_test_ext().execute_with(|| {
			let non_admin = 2;
			let pool_id = 0;
			let new_expiry_block = 200u64;
			create_default_pool();

			assert_err!(
				StakingRewards::set_pool_expiry_block(
					RuntimeOrigin::signed(non_admin),
					pool_id,
					new_expiry_block
				),
				BadOrigin
			);
		});
	}

	#[test]
	fn fails_for_expiry_block_in_the_past() {
		new_test_ext().execute_with(|| {
			let admin = 1;
			let pool_id = 0;
			create_default_pool();

			System::set_block_number(5000);

			assert_err!(
				StakingRewards::set_pool_expiry_block(RuntimeOrigin::signed(admin), pool_id, 2u64),
				Error::<MockRuntime>::ExpiryBlockMustBeInTheFuture
			);
		});
	}
}

/// Assert that an amount has been hypothetically earned by a staker.
fn assert_hypothetically_earned(
	staker: u128,
	expected_earned: u128,
	pool_id: u32,
	reward_asset_id: NativeOrWithId<u32>,
) {
	hypothetically!({
		// Get the pre-harvest balance.
		let balance_before: <MockRuntime as Config>::Balance =
			<<MockRuntime as Config>::Assets>::balance(reward_asset_id.clone(), &staker);

		// Harvest the rewards.
		assert_ok!(StakingRewards::harvest_rewards(RuntimeOrigin::signed(staker), pool_id, None));

		// Sanity check: staker rewards are reset to 0.
		assert_eq!(PoolStakers::<MockRuntime>::get(pool_id, staker).unwrap().rewards, 0);

		// Check that the staker has earned the expected amount.
		let balance_after =
			<<MockRuntime as Config>::Assets>::balance(reward_asset_id.clone(), &staker);
		assert_eq!(balance_after - balance_before, expected_earned);
	});
}

/// This integration test
/// 1. Considers 2 stakers each staking and unstaking at different intervals, asserts their
///    claimable rewards are adjusted as expected, and that harvesting works.
/// 2. Checks that rewards are correctly halted after the pool's expiry block, and resume when the
///    pool is extended.
/// 3. Checks that reward rates adjustment works correctly.
///
/// Note: There are occasionally off by 1 errors due to rounding. In practice this is
/// insignificant.
#[test]
fn integration() {
	new_test_ext().execute_with(|| {
		let admin = 1;
		let staker1 = 10u128;
		let staker2 = 20;
		let staking_asset_id = NativeOrWithId::<u32>::WithId(1);
		let reward_asset_id = NativeOrWithId::<u32>::Native;
		let reward_rate_per_block = 100;
		let expiry_block = 25u64.into();
		assert_ok!(StakingRewards::create_pool(
			RuntimeOrigin::signed(admin),
			Box::new(staking_asset_id.clone()),
			Box::new(reward_asset_id.clone()),
			reward_rate_per_block,
			expiry_block,
			None
		));
		let pool_id = 0;
		let pool_account_id = StakingRewards::pool_account_id(&pool_id).unwrap();
		<<MockRuntime as Config>::Assets>::set_balance(
			reward_asset_id.clone(),
			&pool_account_id,
			100_000,
		);
		<<MockRuntime as Config>::Assets>::set_balance(staking_asset_id.clone(), &staker1, 100_000);
		<<MockRuntime as Config>::Assets>::set_balance(staking_asset_id.clone(), &staker2, 100_000);

		// Block 7: Staker 1 stakes 100 tokens.
		System::set_block_number(7);
		assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(staker1), pool_id, 100));
		// At this point
		// - Staker 1 has earned 0 tokens.
		// - Staker 1 is earning 100 tokens per block.

		// Check that Staker 1 has earned 0 tokens.
		assert_hypothetically_earned(staker1, 0, pool_id, reward_asset_id.clone());

		// Block 9: Staker 2 stakes 100 tokens.
		System::set_block_number(9);
		assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(staker2), pool_id, 100));
		// At this point
		// - Staker 1 has earned 200 (100*2) tokens.
		// - Staker 2 has earned 0 tokens.
		// - Staker 1 is earning 50 tokens per block.
		// - Staker 2 is earning 50 tokens per block.

		// Check that Staker 1 has earned 200 tokens and Staker 2 has earned 0 tokens.
		assert_hypothetically_earned(staker1, 200, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 0, pool_id, reward_asset_id.clone());

		// Block 12: Staker 1 stakes an additional 100 tokens.
		System::set_block_number(12);
		assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(staker1), pool_id, 100));
		// At this point
		// - Staker 1 has earned 350 (200 + (50 * 3)) tokens.
		// - Staker 2 has earned 150 (50 * 3) tokens.
		// - Staker 1 is earning 66.66 tokens per block.
		// - Staker 2 is earning 33.33 tokens per block.

		// Check that Staker 1 has earned 350 tokens and Staker 2 has earned 150 tokens.
		assert_hypothetically_earned(staker1, 349, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 149, pool_id, reward_asset_id.clone());

		// Block 22: Staker 1 unstakes 100 tokens.
		System::set_block_number(22);
		assert_ok!(StakingRewards::unstake(RuntimeOrigin::signed(staker1), pool_id, 100));
		// - Staker 1 has earned 1016 (350 + 66.66 * 10) tokens.
		// - Staker 2 has earned 483 (150 + 33.33 * 10) tokens.
		// - Staker 1 is earning 50 tokens per block.
		// - Staker 2 is earning 50 tokens per block.
		assert_hypothetically_earned(staker1, 1015, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 483, pool_id, reward_asset_id.clone());

		// Block 23: Staker 1 unstakes 100 tokens.
		System::set_block_number(23);
		assert_ok!(StakingRewards::unstake(RuntimeOrigin::signed(staker1), pool_id, 100));
		// - Staker 1 has earned 1065 (1015 + 50) tokens.
		// - Staker 2 has earned 533 (483 + 50) tokens.
		// - Staker 1 is earning 0 tokens per block.
		// - Staker 2 is earning 100 tokens per block.
		assert_hypothetically_earned(staker1, 1064, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 533, pool_id, reward_asset_id.clone());

		// Block 50: Stakers should only have earned 2 blocks worth of tokens (expiry is 25).
		System::set_block_number(50);
		// - Staker 1 has earned 1065 tokens.
		// - Staker 2 has earned 733 (533 + 2 * 100) tokens.
		// - Staker 1 is earning 0 tokens per block.
		// - Staker 2 is earning 0 tokens per block.
		assert_hypothetically_earned(staker1, 1064, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 733, pool_id, reward_asset_id.clone());

		// Block 51: Extend the pool expiry block to 60.
		System::set_block_number(51);
		// - Staker 1 is earning 0 tokens per block.
		// - Staker 2 is earning 100 tokens per block.
		assert_ok!(StakingRewards::set_pool_expiry_block(
			RuntimeOrigin::signed(admin),
			pool_id,
			60u64
		));
		assert_hypothetically_earned(staker1, 1064, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 733, pool_id, reward_asset_id.clone());

		// Block 53: Check rewards are resumed.
		// - Staker 1 has earned 1065 tokens.
		// - Staker 2 has earned 933 (733 + 2 * 100) tokens.
		// - Staker 2 is earning 100 tokens per block.
		System::set_block_number(53);
		assert_hypothetically_earned(staker1, 1064, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 933, pool_id, reward_asset_id.clone());

		// Block 55: Halve the block reward.
		// - Staker 1 has earned 1065 tokens.
		// - Staker 2 has earned 1133 (933 + 2 * 100) tokens.
		// - Staker 2 is earning 50 tokens per block.
		System::set_block_number(55);
		assert_ok!(StakingRewards::set_pool_reward_rate_per_block(
			RuntimeOrigin::signed(admin),
			pool_id,
			50
		));
		assert_hypothetically_earned(staker1, 1064, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 1133, pool_id, reward_asset_id.clone());

		// Block 57: Staker2 harvests their rewards.
		System::set_block_number(57);
		// - Staker 2 has earned 1233 (1133 + 2 * 50) tokens.
		assert_hypothetically_earned(staker2, 1233, pool_id, reward_asset_id.clone());
		// Get the pre-harvest balance.
		let balance_before: <MockRuntime as Config>::Balance =
			<<MockRuntime as Config>::Assets>::balance(reward_asset_id.clone(), &staker2);
		assert_ok!(StakingRewards::harvest_rewards(RuntimeOrigin::signed(staker2), pool_id, None));
		let balance_after =
			<<MockRuntime as Config>::Assets>::balance(reward_asset_id.clone(), &staker2);
		assert_eq!(balance_after - balance_before, 1233u128);

		// Block 60: Check rewards were adjusted correctly.
		// - Staker 1 has earned 1065 tokens.
		// - Staker 2 has earned 149 (3 * 50) tokens.
		System::set_block_number(60);
		assert_hypothetically_earned(staker1, 1064, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 149, pool_id, reward_asset_id.clone());

		// Finally, check events.
		assert_eq!(
			events(),
			[
				Event::PoolCreated {
					creator: admin,
					pool_id,
					staking_asset_id,
					reward_asset_id,
					reward_rate_per_block: 100,
					expiry_block: 25,
					admin
				},
				Event::Staked { who: staker1, pool_id, amount: 100 },
				Event::Staked { who: staker2, pool_id, amount: 100 },
				Event::Staked { who: staker1, pool_id, amount: 100 },
				Event::Unstaked { who: staker1, pool_id, amount: 100 },
				Event::Unstaked { who: staker1, pool_id, amount: 100 },
				Event::PoolExpiryBlockModified { pool_id, new_expiry_block: 60 },
				Event::PoolRewardRateModified { pool_id, new_reward_rate_per_block: 50 },
				Event::RewardsHarvested { who: staker2, staker: staker2, pool_id, amount: 1233 }
			]
		);
	});
}
