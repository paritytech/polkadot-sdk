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
use frame_support::{
	assert_err, assert_noop, assert_ok, hypothetically,
	traits::{
		fungible,
		fungible::NativeOrWithId,
		fungibles,
		tokens::{Fortitude, Preservation},
	},
};
use sp_runtime::{traits::BadOrigin, ArithmeticError, TokenError};

const DEFAULT_STAKED_ASSET_ID: NativeOrWithId<u32> = NativeOrWithId::<u32>::WithId(1);
const DEFAULT_REWARD_ASSET_ID: NativeOrWithId<u32> = NativeOrWithId::<u32>::Native;
const DEFAULT_REWARD_RATE_PER_BLOCK: u128 = 100;
const DEFAULT_EXPIRE_AFTER: u64 = 200;
const DEFAULT_ADMIN: u128 = 1;

/// Creates a basic pool with values:
/// - Staking asset: 1
/// - Reward asset: Native
/// - Reward rate per block: 100
/// - Lifetime: 100
/// - Admin: 1
///
/// Useful to reduce boilerplate in tests when it's not important to customise or reuse pool
/// params.
pub fn create_default_pool() {
	assert_ok!(StakingRewards::create_pool(
		RuntimeOrigin::root(),
		Box::new(DEFAULT_STAKED_ASSET_ID.clone()),
		Box::new(DEFAULT_REWARD_ASSET_ID.clone()),
		DEFAULT_REWARD_RATE_PER_BLOCK,
		DispatchTime::After(DEFAULT_EXPIRE_AFTER),
		Some(DEFAULT_ADMIN)
	));
}

/// The same as [`create_default_pool`], but with the admin parameter set to the creator.
pub fn create_default_pool_permissioned_admin() {
	assert_ok!(StakingRewards::create_pool(
		RuntimeOrigin::root(),
		Box::new(DEFAULT_STAKED_ASSET_ID.clone()),
		Box::new(DEFAULT_REWARD_ASSET_ID.clone()),
		DEFAULT_REWARD_RATE_PER_BLOCK,
		DispatchTime::After(DEFAULT_EXPIRE_AFTER),
		Some(PermissionedAccountId::get()),
	));
}

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
		assert_ok!(StakingRewards::harvest_rewards(RuntimeOrigin::signed(staker), pool_id, None),);

		// Sanity check: staker rewards are reset to 0 if some `amount` is still staked, otherwise
		// the storage item removed.
		if let Some(staker_pool) = PoolStakers::<MockRuntime>::get(pool_id, staker) {
			assert!(staker_pool.rewards == 0);
			assert!(staker_pool.amount > 0);
		}

		// Check that the staker has earned the expected amount.
		let balance_after =
			<<MockRuntime as Config>::Assets>::balance(reward_asset_id.clone(), &staker);
		assert_eq!(balance_after - balance_before, expected_earned);
	});
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
			assert_eq!(NextPoolId::<MockRuntime>::get(), 0);

			System::set_block_number(10);
			let expected_expiry_block = DEFAULT_EXPIRE_AFTER + 10;

			// Create a pool with default values, and no admin override so [`PermissionedAccountId`]
			// is admin.
			assert_ok!(StakingRewards::create_pool(
				RuntimeOrigin::root(),
				Box::new(DEFAULT_STAKED_ASSET_ID),
				Box::new(DEFAULT_REWARD_ASSET_ID),
				DEFAULT_REWARD_RATE_PER_BLOCK,
				DispatchTime::After(DEFAULT_EXPIRE_AFTER),
				Some(PermissionedAccountId::get())
			));

			// Event is emitted.
			assert_eq!(
				events(),
				[Event::<MockRuntime>::PoolCreated {
					creator: PermissionedAccountId::get(),
					pool_id: 0,
					staked_asset_id: DEFAULT_STAKED_ASSET_ID,
					reward_asset_id: DEFAULT_REWARD_ASSET_ID,
					reward_rate_per_block: DEFAULT_REWARD_RATE_PER_BLOCK,
					expiry_block: expected_expiry_block,
					admin: PermissionedAccountId::get(),
				}]
			);

			// State is updated correctly.
			assert_eq!(NextPoolId::<MockRuntime>::get(), 1);
			assert_eq!(
				pools(),
				vec![(
					0,
					PoolInfo {
						staked_asset_id: DEFAULT_STAKED_ASSET_ID,
						reward_asset_id: DEFAULT_REWARD_ASSET_ID,
						reward_rate_per_block: DEFAULT_REWARD_RATE_PER_BLOCK,
						expiry_block: expected_expiry_block,
						admin: PermissionedAccountId::get(),
						total_tokens_staked: 0,
						reward_per_token_stored: 0,
						last_update_block: 0,
						account: StakingRewards::pool_account_id(&0),
					}
				)]
			);

			// Create another pool with explicit admin and other overrides.
			let admin = 2;
			let staked_asset_id = NativeOrWithId::<u32>::WithId(10);
			let reward_asset_id = NativeOrWithId::<u32>::WithId(20);
			let reward_rate_per_block = 250;
			let expiry_block = 500;
			let expected_expiry_block = expiry_block + 10;
			assert_ok!(StakingRewards::create_pool(
				RuntimeOrigin::root(),
				Box::new(staked_asset_id.clone()),
				Box::new(reward_asset_id.clone()),
				reward_rate_per_block,
				DispatchTime::After(expiry_block),
				Some(admin)
			));

			// Event is emitted.
			assert_eq!(
				events(),
				[Event::<MockRuntime>::PoolCreated {
					creator: PermissionedAccountId::get(),
					pool_id: 1,
					staked_asset_id: staked_asset_id.clone(),
					reward_asset_id: reward_asset_id.clone(),
					reward_rate_per_block,
					admin,
					expiry_block: expected_expiry_block,
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
							staked_asset_id: DEFAULT_STAKED_ASSET_ID,
							reward_asset_id: DEFAULT_REWARD_ASSET_ID,
							reward_rate_per_block: DEFAULT_REWARD_RATE_PER_BLOCK,
							admin: PermissionedAccountId::get(),
							expiry_block: DEFAULT_EXPIRE_AFTER + 10,
							total_tokens_staked: 0,
							reward_per_token_stored: 0,
							last_update_block: 0,
							account: StakingRewards::pool_account_id(&0),
						}
					),
					(
						1,
						PoolInfo {
							staked_asset_id,
							reward_asset_id,
							reward_rate_per_block,
							admin,
							total_tokens_staked: 0,
							expiry_block: expected_expiry_block,
							reward_per_token_stored: 0,
							last_update_block: 0,
							account: StakingRewards::pool_account_id(&1),
						}
					)
				]
			);
		});
	}

	#[test]
	fn success_same_assets() {
		new_test_ext().execute_with(|| {
			assert_eq!(NextPoolId::<MockRuntime>::get(), 0);

			System::set_block_number(10);
			let expected_expiry_block = DEFAULT_EXPIRE_AFTER + 10;

			// Create a pool with the same staking and reward asset.
			let asset = NativeOrWithId::<u32>::Native;
			assert_ok!(StakingRewards::create_pool(
				RuntimeOrigin::root(),
				Box::new(asset.clone()),
				Box::new(asset.clone()),
				DEFAULT_REWARD_RATE_PER_BLOCK,
				DispatchTime::After(DEFAULT_EXPIRE_AFTER),
				Some(PermissionedAccountId::get())
			));

			// Event is emitted.
			assert_eq!(
				events(),
				[Event::<MockRuntime>::PoolCreated {
					creator: PermissionedAccountId::get(),
					pool_id: 0,
					staked_asset_id: asset.clone(),
					reward_asset_id: asset.clone(),
					reward_rate_per_block: DEFAULT_REWARD_RATE_PER_BLOCK,
					expiry_block: expected_expiry_block,
					admin: PermissionedAccountId::get(),
				}]
			);

			// State is updated correctly.
			assert_eq!(NextPoolId::<MockRuntime>::get(), 1);
			assert_eq!(
				pools(),
				vec![(
					0,
					PoolInfo {
						staked_asset_id: asset.clone(),
						reward_asset_id: asset,
						reward_rate_per_block: DEFAULT_REWARD_RATE_PER_BLOCK,
						expiry_block: expected_expiry_block,
						admin: PermissionedAccountId::get(),
						total_tokens_staked: 0,
						reward_per_token_stored: 0,
						last_update_block: 0,
						account: StakingRewards::pool_account_id(&0),
					}
				)]
			);
		})
	}

	#[test]
	fn fails_for_non_existent_asset() {
		new_test_ext().execute_with(|| {
			let valid_asset = NativeOrWithId::<u32>::WithId(1);
			let invalid_asset = NativeOrWithId::<u32>::WithId(200);

			assert_err!(
				StakingRewards::create_pool(
					RuntimeOrigin::root(),
					Box::new(valid_asset.clone()),
					Box::new(invalid_asset.clone()),
					10,
					DispatchTime::After(10u64),
					None
				),
				Error::<MockRuntime>::NonExistentAsset
			);

			assert_err!(
				StakingRewards::create_pool(
					RuntimeOrigin::root(),
					Box::new(invalid_asset.clone()),
					Box::new(valid_asset.clone()),
					10,
					DispatchTime::After(10u64),
					None
				),
				Error::<MockRuntime>::NonExistentAsset
			);

			assert_err!(
				StakingRewards::create_pool(
					RuntimeOrigin::root(),
					Box::new(invalid_asset.clone()),
					Box::new(invalid_asset.clone()),
					10,
					DispatchTime::After(10u64),
					None
				),
				Error::<MockRuntime>::NonExistentAsset
			);
		})
	}

	#[test]
	fn fails_for_not_permissioned() {
		new_test_ext().execute_with(|| {
			let user = 100;
			let staked_asset_id = NativeOrWithId::<u32>::Native;
			let reward_asset_id = NativeOrWithId::<u32>::WithId(1);
			let reward_rate_per_block = 100;
			let expiry_block = 100u64;
			assert_err!(
				StakingRewards::create_pool(
					RuntimeOrigin::signed(user),
					Box::new(staked_asset_id.clone()),
					Box::new(reward_asset_id.clone()),
					reward_rate_per_block,
					DispatchTime::After(expiry_block),
					None
				),
				BadOrigin
			);
		});
	}

	#[test]
	fn create_pool_with_caller_admin() {
		new_test_ext().execute_with(|| {
			assert_eq!(NextPoolId::<MockRuntime>::get(), 0);

			System::set_block_number(10);
			let expected_expiry_block = DEFAULT_EXPIRE_AFTER + 10;

			assert_ok!(StakingRewards::create_pool(
				RuntimeOrigin::root(),
				Box::new(DEFAULT_STAKED_ASSET_ID),
				Box::new(DEFAULT_REWARD_ASSET_ID),
				DEFAULT_REWARD_RATE_PER_BLOCK,
				DispatchTime::After(DEFAULT_EXPIRE_AFTER),
				None,
			));

			assert_eq!(
				events(),
				[Event::<MockRuntime>::PoolCreated {
					creator: PermissionedAccountId::get(),
					pool_id: 0,
					staked_asset_id: DEFAULT_STAKED_ASSET_ID,
					reward_asset_id: DEFAULT_REWARD_ASSET_ID,
					reward_rate_per_block: DEFAULT_REWARD_RATE_PER_BLOCK,
					expiry_block: expected_expiry_block,
					admin: PermissionedAccountId::get(),
				}]
			);

			assert_eq!(Pools::<MockRuntime>::get(0).unwrap().admin, PermissionedAccountId::get());
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
			let initial_balance = <Assets as fungibles::Inspect<u128>>::reducible_balance(
				1,
				&user,
				Preservation::Expendable,
				Fortitude::Force,
			);

			// User stakes tokens
			assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(user), pool_id, 1000));

			// Check that the user's staked amount is updated
			assert_eq!(PoolStakers::<MockRuntime>::get(pool_id, user).unwrap().amount, 1000);

			// Event is emitted.
			assert_eq!(
				*events().last().unwrap(),
				Event::<MockRuntime>::Staked { staker: user, amount: 1000, pool_id: 0 }
			);

			// Check that the pool's total tokens staked is updated
			assert_eq!(Pools::<MockRuntime>::get(pool_id).unwrap().total_tokens_staked, 1000);

			// Check user's frozen balance is updated
			assert_eq!(
				<Assets as fungibles::Inspect<u128>>::reducible_balance(
					1,
					&user,
					Preservation::Expendable,
					Fortitude::Force,
				),
				initial_balance - 1000
			);

			// User stakes more tokens
			assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(user), pool_id, 500));

			// Event is emitted.
			assert_eq!(
				*events().last().unwrap(),
				Event::<MockRuntime>::Staked { staker: user, amount: 500, pool_id: 0 }
			);

			// Check that the user's staked amount is updated
			assert_eq!(PoolStakers::<MockRuntime>::get(pool_id, user).unwrap().amount, 1000 + 500);

			// Check that the pool's total tokens staked is updated
			assert_eq!(Pools::<MockRuntime>::get(pool_id).unwrap().total_tokens_staked, 1000 + 500);

			assert_eq!(
				<Assets as fungibles::Inspect<u128>>::reducible_balance(
					1,
					&user,
					Preservation::Expendable,
					Fortitude::Force,
				),
				initial_balance - 1500
			);

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
		new_test_ext().execute_with(|| {
			let user = 1;
			create_default_pool();
			let pool_id = 0;
			let initial_balance = <Assets as fungibles::Inspect<u128>>::reducible_balance(
				1,
				&user,
				Preservation::Expendable,
				Fortitude::Force,
			);
			assert_err!(
				StakingRewards::stake(RuntimeOrigin::signed(user), pool_id, initial_balance + 1),
				TokenError::FundsUnavailable,
			);
		})
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
			assert_ok!(StakingRewards::unstake(RuntimeOrigin::signed(user), pool_id, 500, None));

			// Event is emitted.
			assert_eq!(
				*events().last().unwrap(),
				Event::<MockRuntime>::Unstaked {
					caller: user,
					staker: user,
					amount: 500,
					pool_id: 0
				}
			);

			// Check that the user's staked amount is updated
			assert_eq!(PoolStakers::<MockRuntime>::get(pool_id, user).unwrap().amount, 500);

			// Check that the pool's total tokens staked is updated
			assert_eq!(Pools::<MockRuntime>::get(pool_id).unwrap().total_tokens_staked, 500);

			// User unstakes remaining tokens
			assert_ok!(StakingRewards::unstake(RuntimeOrigin::signed(user), pool_id, 500, None));

			// Check that the storage items is removed since stake amount and rewards are zero.
			assert!(PoolStakers::<MockRuntime>::get(pool_id, user).is_none());

			// Check that the pool's total tokens staked is zero
			assert_eq!(Pools::<MockRuntime>::get(pool_id).unwrap().total_tokens_staked, 0);
		});
	}

	#[test]
	fn unstake_for_other() {
		new_test_ext().execute_with(|| {
			let staker = 1;
			let caller = 2;
			let pool_id = 0;
			let init_block = System::block_number();

			create_default_pool();

			// User stakes tokens
			assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(staker), pool_id, 1000));

			// Fails to unstake for other since pool is still active
			assert_noop!(
				StakingRewards::unstake(RuntimeOrigin::signed(caller), pool_id, 500, Some(staker)),
				BadOrigin,
			);

			System::set_block_number(init_block + DEFAULT_EXPIRE_AFTER + 1);

			assert_ok!(StakingRewards::unstake(
				RuntimeOrigin::signed(caller),
				pool_id,
				500,
				Some(staker)
			));

			// Event is emitted.
			assert_eq!(
				*events().last().unwrap(),
				Event::<MockRuntime>::Unstaked { caller, staker, amount: 500, pool_id: 0 }
			);
		});
	}

	#[test]
	fn fails_for_non_existent_pool() {
		new_test_ext().execute_with(|| {
			let user = 1;
			let non_existent_pool_id = 999;

			// User tries to unstake tokens from a non-existent pool
			assert_err!(
				StakingRewards::unstake(
					RuntimeOrigin::signed(user),
					non_existent_pool_id,
					500,
					None
				),
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
				StakingRewards::unstake(RuntimeOrigin::signed(user), pool_id, 1500, None),
				Error::<MockRuntime>::NotEnoughTokens
			);
		});
	}
}

mod harvest_rewards {
	use super::*;

	#[test]
	fn success() {
		new_test_ext().execute_with(|| {
			let staker = 1;
			let pool_id = 0;
			let reward_asset_id = NativeOrWithId::<u32>::Native;
			create_default_pool();

			// Stake
			System::set_block_number(10);
			assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(staker), pool_id, 1000));

			// Harvest
			System::set_block_number(20);
			let balance_before: <MockRuntime as Config>::Balance =
				<<MockRuntime as Config>::Assets>::balance(reward_asset_id.clone(), &staker);
			assert_ok!(StakingRewards::harvest_rewards(
				RuntimeOrigin::signed(staker),
				pool_id,
				None
			));
			let balance_after =
				<<MockRuntime as Config>::Assets>::balance(reward_asset_id.clone(), &staker);

			// Assert
			assert_eq!(
				balance_after - balance_before,
				10 * Pools::<MockRuntime>::get(pool_id).unwrap().reward_rate_per_block
			);
			assert_eq!(
				*events().last().unwrap(),
				Event::<MockRuntime>::RewardsHarvested {
					caller: staker,
					staker,
					pool_id,
					amount: 10 * Pools::<MockRuntime>::get(pool_id).unwrap().reward_rate_per_block
				}
			);
		});
	}

	#[test]
	fn harvest_for_other() {
		new_test_ext().execute_with(|| {
			let caller = 2;
			let staker = 1;
			let pool_id = 0;
			let init_block = System::block_number();

			create_default_pool();

			// Stake
			System::set_block_number(10);
			assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(staker), pool_id, 1000));

			System::set_block_number(20);

			// Fails to harvest for staker since pool is still active
			assert_noop!(
				StakingRewards::harvest_rewards(
					RuntimeOrigin::signed(caller),
					pool_id,
					Some(staker)
				),
				BadOrigin
			);

			System::set_block_number(init_block + DEFAULT_EXPIRE_AFTER + 1);

			// Harvest for staker
			assert_ok!(StakingRewards::harvest_rewards(
				RuntimeOrigin::signed(caller),
				pool_id,
				Some(staker),
			));

			assert!(matches!(
				events().last().unwrap(),
				Event::<MockRuntime>::RewardsHarvested {
					caller,
					staker,
					pool_id,
					..
				} if caller == caller && staker == staker && pool_id == pool_id
			));
		});
	}

	#[test]
	fn fails_for_non_existent_staker() {
		new_test_ext().execute_with(|| {
			let non_existent_staker = 999;

			create_default_pool();
			assert_err!(
				StakingRewards::harvest_rewards(
					RuntimeOrigin::signed(non_existent_staker),
					0,
					None
				),
				Error::<MockRuntime>::NonExistentStaker
			);
		});
	}

	#[test]
	fn fails_for_non_existent_pool() {
		new_test_ext().execute_with(|| {
			let staker = 1;
			let non_existent_pool_id = 999;

			assert_err!(
				StakingRewards::harvest_rewards(
					RuntimeOrigin::signed(staker),
					non_existent_pool_id,
					None,
				),
				Error::<MockRuntime>::NonExistentPool
			);
		});
	}
}

mod set_pool_admin {
	use super::*;

	#[test]
	fn success_signed_admin() {
		new_test_ext().execute_with(|| {
			let admin = 1;
			let new_admin = 2;
			let pool_id = 0;
			create_default_pool();

			// Modify the pool admin
			assert_ok!(StakingRewards::set_pool_admin(
				RuntimeOrigin::signed(admin),
				pool_id,
				new_admin,
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
	fn success_permissioned_admin() {
		new_test_ext().execute_with(|| {
			let new_admin = 2;
			let pool_id = 0;
			create_default_pool_permissioned_admin();

			// Modify the pool admin
			assert_ok!(StakingRewards::set_pool_admin(RuntimeOrigin::root(), pool_id, new_admin));

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
	fn success_permissioned_admin() {
		new_test_ext().execute_with(|| {
			let pool_id = 0;
			let new_expiry_block = System::block_number() + DEFAULT_EXPIRE_AFTER + 1u64;
			create_default_pool_permissioned_admin();

			assert_ok!(StakingRewards::set_pool_expiry_block(
				RuntimeOrigin::root(),
				pool_id,
				DispatchTime::At(new_expiry_block),
			));

			// Check state
			assert_eq!(Pools::<MockRuntime>::get(pool_id).unwrap().expiry_block, new_expiry_block);
			assert_eq!(
				*events().last().unwrap(),
				Event::<MockRuntime>::PoolExpiryBlockModified { pool_id, new_expiry_block }
			);
		});
	}

	#[test]
	fn success_signed_admin() {
		new_test_ext().execute_with(|| {
			let admin = 1;
			let pool_id = 0;
			let new_expiry_block = System::block_number() + DEFAULT_EXPIRE_AFTER + 1u64;
			create_default_pool();

			assert_ok!(StakingRewards::set_pool_expiry_block(
				RuntimeOrigin::signed(admin),
				pool_id,
				DispatchTime::At(new_expiry_block)
			));

			// Check state
			assert_eq!(Pools::<MockRuntime>::get(pool_id).unwrap().expiry_block, new_expiry_block);
			assert_eq!(
				*events().last().unwrap(),
				Event::<MockRuntime>::PoolExpiryBlockModified { pool_id, new_expiry_block }
			);
		});
	}

	#[test]
	fn extends_reward_accumulation() {
		new_test_ext().execute_with(|| {
			let admin = 1;
			let staker = 2;
			let pool_id = 0;
			let new_expiry_block = 300u64;
			System::set_block_number(10);
			create_default_pool();

			// Regular reward accumulation
			assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(staker), pool_id, 1000));
			System::set_block_number(20);
			assert_hypothetically_earned(
				staker,
				DEFAULT_REWARD_RATE_PER_BLOCK * 10,
				pool_id,
				NativeOrWithId::<u32>::Native,
			);

			// Expiry was block 210, so earned 200 at block 250
			System::set_block_number(250);
			assert_hypothetically_earned(
				staker,
				DEFAULT_REWARD_RATE_PER_BLOCK * 200,
				pool_id,
				NativeOrWithId::<u32>::Native,
			);

			// Extend expiry 50 more blocks
			assert_ok!(StakingRewards::set_pool_expiry_block(
				RuntimeOrigin::signed(admin),
				pool_id,
				DispatchTime::At(new_expiry_block)
			));
			System::set_block_number(350);

			// Staker has been in pool with rewards active for 250 blocks total
			assert_hypothetically_earned(
				staker,
				DEFAULT_REWARD_RATE_PER_BLOCK * 250,
				pool_id,
				NativeOrWithId::<u32>::Native,
			);
		});
	}

	#[test]
	fn fails_to_cutback_expiration() {
		new_test_ext().execute_with(|| {
			let admin = 1;
			let pool_id = 0;
			create_default_pool();

			assert_noop!(
				StakingRewards::set_pool_expiry_block(
					RuntimeOrigin::signed(admin),
					pool_id,
					DispatchTime::After(30)
				),
				Error::<MockRuntime>::ExpiryCut
			);
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
					DispatchTime::After(new_expiry_block)
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
					DispatchTime::After(new_expiry_block)
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
			System::set_block_number(50);
			assert_err!(
				StakingRewards::set_pool_expiry_block(
					RuntimeOrigin::signed(admin),
					pool_id,
					DispatchTime::At(40u64)
				),
				Error::<MockRuntime>::ExpiryBlockMustBeInTheFuture
			);
		});
	}
}

mod set_pool_reward_rate_per_block {
	use super::*;

	#[test]
	fn success_signed_admin() {
		new_test_ext().execute_with(|| {
			let pool_id = 0;
			let new_reward_rate = 200;
			create_default_pool();

			// Pool Admin can modify
			assert_ok!(StakingRewards::set_pool_reward_rate_per_block(
				RuntimeOrigin::signed(DEFAULT_ADMIN),
				pool_id,
				new_reward_rate
			));

			// Check state
			assert_eq!(
				Pools::<MockRuntime>::get(pool_id).unwrap().reward_rate_per_block,
				new_reward_rate
			);

			// Check event
			assert_eq!(
				*events().last().unwrap(),
				Event::<MockRuntime>::PoolRewardRateModified {
					pool_id,
					new_reward_rate_per_block: new_reward_rate
				}
			);
		});
	}

	#[test]
	fn success_permissioned_admin() {
		new_test_ext().execute_with(|| {
			let pool_id = 0;
			let new_reward_rate = 200;
			create_default_pool_permissioned_admin();

			// Root can modify
			assert_ok!(StakingRewards::set_pool_reward_rate_per_block(
				RuntimeOrigin::root(),
				pool_id,
				new_reward_rate
			));

			// Check state
			assert_eq!(
				Pools::<MockRuntime>::get(pool_id).unwrap().reward_rate_per_block,
				new_reward_rate
			);

			// Check event
			assert_eq!(
				*events().last().unwrap(),
				Event::<MockRuntime>::PoolRewardRateModified {
					pool_id,
					new_reward_rate_per_block: new_reward_rate
				}
			);
		});
	}

	#[test]
	fn staker_rewards_are_affected_correctly() {
		new_test_ext().execute_with(|| {
			let admin = 1;
			let staker = 2;
			let pool_id = 0;
			let new_reward_rate = 150;
			create_default_pool();

			// Stake some tokens, and accumulate 10 blocks of rewards at the default pool rate (100)
			System::set_block_number(10);
			assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(staker), pool_id, 1000));
			System::set_block_number(20);

			// Increase the reward rate
			assert_ok!(StakingRewards::set_pool_reward_rate_per_block(
				RuntimeOrigin::signed(admin),
				pool_id,
				new_reward_rate
			));

			// Accumulate 10 blocks of rewards at the new rate
			System::set_block_number(30);

			// Check that rewards are calculated correctly with the updated rate
			assert_hypothetically_earned(
				staker,
				10 * 100 + 10 * new_reward_rate,
				pool_id,
				NativeOrWithId::<u32>::Native,
			);
		});
	}

	#[test]
	fn fails_for_non_existent_pool() {
		new_test_ext().execute_with(|| {
			let admin = 1;
			let non_existent_pool_id = 999;
			let new_reward_rate = 200;

			assert_err!(
				StakingRewards::set_pool_reward_rate_per_block(
					RuntimeOrigin::signed(admin),
					non_existent_pool_id,
					new_reward_rate
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
			let new_reward_rate = 200;
			create_default_pool();

			assert_err!(
				StakingRewards::set_pool_reward_rate_per_block(
					RuntimeOrigin::signed(non_admin),
					pool_id,
					new_reward_rate
				),
				BadOrigin
			);
		});
	}

	#[test]
	fn fails_to_decrease() {
		new_test_ext().execute_with(|| {
			create_default_pool_permissioned_admin();

			assert_noop!(
				StakingRewards::set_pool_reward_rate_per_block(
					RuntimeOrigin::root(),
					0,
					DEFAULT_REWARD_RATE_PER_BLOCK - 1
				),
				Error::<MockRuntime>::RewardRateCut
			);
		});
	}
}

mod deposit_reward_tokens {
	use super::*;

	#[test]
	fn success() {
		new_test_ext().execute_with(|| {
			let depositor = 1;
			let pool_id = 0;
			let amount = 1000;
			let reward_asset_id = NativeOrWithId::<u32>::Native;
			create_default_pool();
			let pool_account_id = StakingRewards::pool_account_id(&pool_id);

			let depositor_balance_before =
				<<MockRuntime as Config>::Assets>::balance(reward_asset_id.clone(), &depositor);
			let pool_balance_before = <<MockRuntime as Config>::Assets>::balance(
				reward_asset_id.clone(),
				&pool_account_id,
			);
			assert_ok!(StakingRewards::deposit_reward_tokens(
				RuntimeOrigin::signed(depositor),
				pool_id,
				amount
			));
			let depositor_balance_after =
				<<MockRuntime as Config>::Assets>::balance(reward_asset_id.clone(), &depositor);
			let pool_balance_after =
				<<MockRuntime as Config>::Assets>::balance(reward_asset_id, &pool_account_id);

			assert_eq!(pool_balance_after - pool_balance_before, amount);
			assert_eq!(depositor_balance_before - depositor_balance_after, amount);
		});
	}

	#[test]
	fn fails_for_non_existent_pool() {
		new_test_ext().execute_with(|| {
			assert_err!(
				StakingRewards::deposit_reward_tokens(RuntimeOrigin::signed(1), 999, 100),
				Error::<MockRuntime>::NonExistentPool
			);
		});
	}

	#[test]
	fn fails_for_insufficient_balance() {
		new_test_ext().execute_with(|| {
			create_default_pool();
			assert_err!(
				StakingRewards::deposit_reward_tokens(RuntimeOrigin::signed(1), 0, 100_000_000),
				ArithmeticError::Underflow
			);
		});
	}
}

mod cleanup_pool {
	use super::*;

	#[test]
	fn success() {
		new_test_ext().execute_with(|| {
			let pool_id = 0;
			let admin = DEFAULT_ADMIN;
			let admin_balance_before = <Balances as fungible::Inspect<u128>>::balance(&admin);

			create_default_pool();
			assert!(Pools::<MockRuntime>::get(pool_id).is_some());

			assert_ok!(StakingRewards::cleanup_pool(RuntimeOrigin::signed(admin), pool_id));

			assert_eq!(
				<Balances as fungible::Inspect<u128>>::balance(&admin),
				// `100_000` initial pool account balance from Genesis config
				admin_balance_before + 100_000,
			);
			assert_eq!(Pools::<MockRuntime>::get(pool_id), None);
			assert_eq!(PoolStakers::<MockRuntime>::iter_prefix_values(pool_id).count(), 0);
			assert_eq!(PoolCost::<MockRuntime>::get(pool_id), None);
		});
	}

	#[test]
	fn success_only_when_pool_empty() {
		new_test_ext().execute_with(|| {
			let pool_id = 0;
			let staker = 20;
			let admin = DEFAULT_ADMIN;

			create_default_pool();

			// stake to prevent pool cleanup
			assert_ok!(StakingRewards::stake(RuntimeOrigin::signed(staker), pool_id, 100));

			assert_noop!(
				StakingRewards::cleanup_pool(RuntimeOrigin::signed(admin), pool_id),
				Error::<MockRuntime>::NonEmptyPool
			);

			// unstake partially
			assert_ok!(StakingRewards::unstake(RuntimeOrigin::signed(staker), pool_id, 50, None));

			assert_noop!(
				StakingRewards::cleanup_pool(RuntimeOrigin::signed(admin), pool_id),
				Error::<MockRuntime>::NonEmptyPool
			);

			// unstake all
			assert_ok!(StakingRewards::unstake(RuntimeOrigin::signed(staker), pool_id, 50, None));

			assert_ok!(StakingRewards::cleanup_pool(RuntimeOrigin::signed(admin), pool_id),);

			assert_eq!(Pools::<MockRuntime>::get(pool_id), None);
			assert_eq!(PoolStakers::<MockRuntime>::iter_prefix_values(pool_id).count(), 0);
			assert_eq!(PoolCost::<MockRuntime>::get(pool_id), None);
		});
	}

	#[test]
	fn fails_on_wrong_origin() {
		new_test_ext().execute_with(|| {
			let caller = 888;
			let pool_id = 0;
			create_default_pool();

			assert_noop!(
				StakingRewards::cleanup_pool(RuntimeOrigin::signed(caller), pool_id),
				BadOrigin
			);
		});
	}
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
		let staked_asset_id = NativeOrWithId::<u32>::WithId(1);
		let reward_asset_id = NativeOrWithId::<u32>::Native;
		let reward_rate_per_block = 100;
		let lifetime = 24u64.into();
		System::set_block_number(1);
		assert_ok!(StakingRewards::create_pool(
			RuntimeOrigin::root(),
			Box::new(staked_asset_id.clone()),
			Box::new(reward_asset_id.clone()),
			reward_rate_per_block,
			DispatchTime::After(lifetime),
			Some(admin)
		));
		let pool_id = 0;

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
		assert_hypothetically_earned(staker1, 350, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 150, pool_id, reward_asset_id.clone());

		// Block 22: Staker 1 unstakes 100 tokens.
		System::set_block_number(22);
		assert_ok!(StakingRewards::unstake(RuntimeOrigin::signed(staker1), pool_id, 100, None));
		// - Staker 1 has earned 1016 (350 + 66.66 * 10) tokens.
		// - Staker 2 has earned 483 (150 + 33.33 * 10) tokens.
		// - Staker 1 is earning 50 tokens per block.
		// - Staker 2 is earning 50 tokens per block.
		assert_hypothetically_earned(staker1, 1016, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 483, pool_id, reward_asset_id.clone());

		// Block 23: Staker 1 unstakes 100 tokens.
		System::set_block_number(23);
		assert_ok!(StakingRewards::unstake(RuntimeOrigin::signed(staker1), pool_id, 100, None));
		// - Staker 1 has earned 1065 (1015 + 50) tokens.
		// - Staker 2 has earned 533 (483 + 50) tokens.
		// - Staker 1 is earning 0 tokens per block.
		// - Staker 2 is earning 100 tokens per block.
		assert_hypothetically_earned(staker1, 1066, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 533, pool_id, reward_asset_id.clone());

		// Block 50: Stakers should only have earned 2 blocks worth of tokens (expiry is 25).
		System::set_block_number(50);
		// - Staker 1 has earned 1065 tokens.
		// - Staker 2 has earned 733 (533 + 2 * 100) tokens.
		// - Staker 1 is earning 0 tokens per block.
		// - Staker 2 is earning 0 tokens per block.
		assert_hypothetically_earned(staker1, 1066, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 733, pool_id, reward_asset_id.clone());

		// Block 51: Extend the pool expiry block to 60.
		System::set_block_number(51);
		// - Staker 1 is earning 0 tokens per block.
		// - Staker 2 is earning 100 tokens per block.
		assert_ok!(StakingRewards::set_pool_expiry_block(
			RuntimeOrigin::signed(admin),
			pool_id,
			DispatchTime::At(60u64),
		));
		assert_hypothetically_earned(staker1, 1066, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 733, pool_id, reward_asset_id.clone());

		// Block 53: Check rewards are resumed.
		// - Staker 1 has earned 1065 tokens.
		// - Staker 2 has earned 933 (733 + 2 * 100) tokens.
		// - Staker 2 is earning 100 tokens per block.
		System::set_block_number(53);
		assert_hypothetically_earned(staker1, 1066, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 933, pool_id, reward_asset_id.clone());

		// Block 55: Increase the block reward.
		// - Staker 1 has earned 1065 tokens.
		// - Staker 2 has earned 1133 (933 + 2 * 100) tokens.
		// - Staker 2 is earning 50 tokens per block.
		System::set_block_number(55);
		assert_ok!(StakingRewards::set_pool_reward_rate_per_block(
			RuntimeOrigin::signed(admin),
			pool_id,
			150
		));
		assert_hypothetically_earned(staker1, 1066, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 1133, pool_id, reward_asset_id.clone());

		// Block 57: Staker2 harvests their rewards.
		System::set_block_number(57);
		// - Staker 2 has earned 1433 (1133 + 2 * 150) tokens.
		assert_hypothetically_earned(staker2, 1433, pool_id, reward_asset_id.clone());
		// Get the pre-harvest balance.
		let balance_before: <MockRuntime as Config>::Balance =
			<<MockRuntime as Config>::Assets>::balance(reward_asset_id.clone(), &staker2);
		assert_ok!(StakingRewards::harvest_rewards(RuntimeOrigin::signed(staker2), pool_id, None));
		let balance_after =
			<<MockRuntime as Config>::Assets>::balance(reward_asset_id.clone(), &staker2);
		assert_eq!(balance_after - balance_before, 1433u128);

		// Block 60: Check rewards were adjusted correctly.
		// - Staker 1 has earned 1065 tokens.
		// - Staker 2 has earned 450 (3 * 150) tokens.
		System::set_block_number(60);
		assert_hypothetically_earned(staker1, 1066, pool_id, reward_asset_id.clone());
		assert_hypothetically_earned(staker2, 450, pool_id, reward_asset_id.clone());

		// Finally, check events.
		assert_eq!(
			events(),
			[
				Event::PoolCreated {
					creator: PermissionedAccountId::get(),
					pool_id,
					staked_asset_id,
					reward_asset_id,
					reward_rate_per_block: 100,
					expiry_block: 25,
					admin,
				},
				Event::Staked { staker: staker1, pool_id, amount: 100 },
				Event::Staked { staker: staker2, pool_id, amount: 100 },
				Event::Staked { staker: staker1, pool_id, amount: 100 },
				Event::Unstaked { caller: staker1, staker: staker1, pool_id, amount: 100 },
				Event::Unstaked { caller: staker1, staker: staker1, pool_id, amount: 100 },
				Event::PoolExpiryBlockModified { pool_id, new_expiry_block: 60 },
				Event::PoolRewardRateModified { pool_id, new_reward_rate_per_block: 150 },
				Event::RewardsHarvested { caller: staker2, staker: staker2, pool_id, amount: 1433 }
			]
		);
	});
}
