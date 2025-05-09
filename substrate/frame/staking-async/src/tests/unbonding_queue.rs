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

use super::*;
use crate::{pallet::normalize_era, tests::Test, UnbondingQueueConfig, UnbondingQueueParams};
use frame_support::traits::fungible::Inspect;
use sp_runtime::Perbill;

#[test]
fn get_min_lowest_stake_works() {
	ExtBuilder::default()
		.set_stake(11, 10_000)
		.set_stake(21, 11_000)
		.set_stake(31, 400)
		.validator_count(3)
		.nominate(false)
		.build_and_execute(|| {
			assert_ok!(Staking::set_staking_configs(
				RuntimeOrigin::root(),
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Set(UnbondingQueueConfig {
					min_slashable_share: Perbill::from_percent(50),
					lowest_ratio: Perbill::from_percent(34),
					unbond_period_lower_bound: 1,
				})
			));

			// Initial conditions.
			assert_eq!(Staking::current_era(), 1);
			assert_eq!(EraLowestRatioTotalStake::<T>::get().into_inner(), vec![]);
			assert_eq!(Staking::get_min_lowest_stake(), 0);

			// Setup to nominate.
			assert_ok!(Staking::bond(RuntimeOrigin::signed(999), 100, RewardDestination::Stash));
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(999), vec![31]));

			// Era 1 -> 2
			Session::roll_until_active_era(2);
			assert_eq!(EraLowestRatioTotalStake::<T>::get().into_inner(), vec![500]);
			assert_eq!(Staking::get_min_lowest_stake(), 500);

			// Era 2 -> 3
			assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(999), 100));
			Session::roll_until_active_era(3);
			assert_eq!(EraLowestRatioTotalStake::<T>::get().into_inner(), vec![500, 600]);
			assert_eq!(Staking::get_min_lowest_stake(), 500);

			// Era 3 -> 4
			assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(999), 100));
			Session::roll_until_active_era(4);
			assert_eq!(EraLowestRatioTotalStake::<T>::get().into_inner(), vec![500, 600, 700]);
			assert_eq!(Staking::get_min_lowest_stake(), 500);

			// Era 4 -> 5
			assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(999), 100));
			Session::roll_until_active_era(5);
			assert_eq!(EraLowestRatioTotalStake::<T>::get().into_inner(), vec![600, 700, 800]);
			assert_eq!(Staking::get_min_lowest_stake(), 600);

			// Era 5 -> 6
			assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(999), 100));
			Session::roll_until_active_era(6);
			assert_eq!(EraLowestRatioTotalStake::<T>::get().into_inner(), vec![700, 800, 900]);
			assert_eq!(Staking::get_min_lowest_stake(), 700);
		});
}

#[test]
fn get_min_lowest_stake_with_many_validators_works() {
	ExtBuilder::default()
		.validator_count(9)
		.multi_page_election_provider(3)
		// We already have the following validators:
		// 11 -> 1000
		// 21 -> 1000
		.set_status(31, StakerStatus::<AccountId>::Validator) // 1000
		.set_status(41, StakerStatus::<AccountId>::Validator) // 1000
		.set_status(51, StakerStatus::<AccountId>::Validator) // 1000
		.add_staker(61, 100, StakerStatus::<AccountId>::Validator)
		.add_staker(71, 200, StakerStatus::<AccountId>::Validator)
		.add_staker(81, 1000, StakerStatus::<AccountId>::Validator)
		.add_staker(91, 1000, StakerStatus::<AccountId>::Validator)
		// Add nominators
		.set_status(101, StakerStatus::<AccountId>::Nominator(vec![11])) // 500
		.add_staker(102, 500, StakerStatus::<AccountId>::Nominator(vec![21]))
		.add_staker(103, 500, StakerStatus::<AccountId>::Nominator(vec![31]))
		.add_staker(104, 500, StakerStatus::<AccountId>::Nominator(vec![41]))
		.add_staker(105, 500, StakerStatus::<AccountId>::Nominator(vec![51]))
		.add_staker(106, 500, StakerStatus::<AccountId>::Nominator(vec![61]))
		.add_staker(107, 500, StakerStatus::<AccountId>::Nominator(vec![71]))
		.add_staker(108, 500, StakerStatus::<AccountId>::Nominator(vec![81]))
		.add_staker(109, 500, StakerStatus::<AccountId>::Nominator(vec![91]))
		.build_and_execute(|| {
			assert_ok!(Staking::set_staking_configs(
				RuntimeOrigin::root(),
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Set(UnbondingQueueConfig {
					min_slashable_share: Perbill::from_percent(50),
					lowest_ratio: Perbill::from_percent(34),
					unbond_period_lower_bound: 1,
				})
			));

			assert_eq!(Staking::get_min_lowest_stake(), 0);
			Session::roll_until_active_era(2);

			// There are 9 validators, so one third of them would be 3.
			// Hence, the lowest third would be composed of the following validators:
			// 61 -> 100 (own) + 500 (nomination from 106)
			// 71 -> 200 (own) + 500 (nomination from 107)
			// 31 -> 500 (own) + 500 (nomination from 103)
			// Summing up 2300 total.
			assert_eq!(Staking::get_min_lowest_stake(), 2300);
		});
}

#[test]
fn calculate_lowest_total_stake_works() {
	ExtBuilder::default()
		.has_stakers(false)
		.validator_count(4)
		.build_and_execute(|| {
			assert_ok!(Staking::set_staking_configs(
				RuntimeOrigin::root(),
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Noop,
				ConfigOp::Set(UnbondingQueueConfig {
					min_slashable_share: Perbill::from_percent(50),
					lowest_ratio: Perbill::from_percent(34),
					unbond_period_lower_bound: 1,
				})
			));

			Session::roll_until_active_era(4);
			assert_eq!(current_era(), 4);
			// There are no stakers
			assert_eq!(EraLowestRatioTotalStake::<Test>::get().into_inner(), vec![0, 0, 0]);
			assert_eq!(Staking::get_min_lowest_stake(), 0);
			let ed = Balances::minimum_balance();

			// Validator 1
			assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 1, 1000 + ed));
			assert_ok!(Staking::bond(RuntimeOrigin::signed(1), 1000, RewardDestination::Stash));
			assert_ok!(Staking::validate(RuntimeOrigin::signed(1), ValidatorPrefs::default()));

			// Validator 2
			assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 2, 2000 + ed));
			assert_ok!(Staking::bond(RuntimeOrigin::signed(2), 2000, RewardDestination::Stash));
			assert_ok!(Staking::validate(RuntimeOrigin::signed(2), ValidatorPrefs::default()));

			// Validator 3
			assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 3, 3000 + ed));
			assert_ok!(Staking::bond(RuntimeOrigin::signed(3), 3000, RewardDestination::Stash));
			assert_ok!(Staking::validate(RuntimeOrigin::signed(3), ValidatorPrefs::default()));

			// Validator 4
			assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 4, 4000 + ed));
			assert_ok!(Staking::bond(RuntimeOrigin::signed(4), 4000, RewardDestination::Stash));
			assert_ok!(Staking::validate(RuntimeOrigin::signed(4), ValidatorPrefs::default()));

			// Trigger new era to calculate the lowest proportion.
			Session::roll_until_active_era(5);
			assert_eq!(EraLowestRatioTotalStake::<Test>::get().into_inner(), vec![0, 0, 1000]);
			// The lowest proportion is 33% of 4 validators ~ 1.32.
			// Hence, the lowest 1 validator with 1000.
			assert_eq!(Staking::get_min_lowest_stake(), 0);

			Session::roll_until_active_era(6);
			assert_eq!(EraLowestRatioTotalStake::<Test>::get().into_inner(), vec![0, 1000, 1000]);
			assert_eq!(Staking::get_min_lowest_stake(), 0);

			// Ensure old entry is pruned after bonding duration (3 eras).
			Session::roll_until_active_era(7);
			assert_eq!(
				EraLowestRatioTotalStake::<Test>::get().into_inner(),
				vec![1000, 1000, 1000]
			);
			assert_eq!(Staking::get_min_lowest_stake(), 1000);
		});
}

#[test]
fn get_unbond_eras_delta_with_zero_max_unstake_works() {
	ExtBuilder::default().build_and_execute(|| {
		let config = UnbondingQueueConfig {
			min_slashable_share: Perbill::from_percent(50),
			lowest_ratio: Perbill::from_percent(34),
			unbond_period_lower_bound: 1,
		};

		// Must be the maximum because the minimum lowest stake is zero, which defaults to
		// the maximum unbonding period.
		assert_eq!(EraLowestRatioTotalStake::<Test>::get().into_inner(), vec![]);
		assert_eq!(Staking::get_unbonding_delta(1, config), normalize_era(3));

		let config = UnbondingQueueConfig {
			min_slashable_share: Perbill::from_percent(0),
			lowest_ratio: Perbill::from_percent(34),
			unbond_period_lower_bound: 1,
		};

		// Now the minimum lowest stake is also zero because the slashable share is zero, so
		// again it must be the maximum unbonding period.
		assert_ok!(EraLowestRatioTotalStake::<Test>::try_append(1000));
		assert_eq!(Staking::get_unbonding_delta(1, config), normalize_era(3));
	});
}

#[test]
fn get_unbond_eras_delta_works() {
	ExtBuilder::default().build_and_execute(|| {
		let config = UnbondingQueueConfig {
			min_slashable_share: Perbill::from_percent(50),
			lowest_ratio: Perbill::from_percent(34),
			unbond_period_lower_bound: 1,
		};
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Set(config)
		));

		// Set a known minimum stake.
		let min_lowest_stake = 1000;
		assert_ok!(EraLowestRatioTotalStake::<Test>::try_append(min_lowest_stake));

		// Max unstake is 50% of min_lowest_stake = 500.
		let max_unstake = config.min_slashable_share * min_lowest_stake;
		assert_eq!(max_unstake, 500);

		// Test cases with BondingDuration = 3:
		// 500 / 500 * 3 = 3
		assert_eq!(Staking::get_unbonding_delta(max_unstake, config), normalize_era(3));
		// 250 / 500 * 3 = 1.5
		assert_eq!(
			Staking::get_unbonding_delta(max_unstake / 2, config),
			normalize_era(1) + 500000000000
		);
		// 0 / 500 * 3 = 0
		assert_eq!(Staking::get_unbonding_delta(0, config), 0);
		// 1000 / 500 * 3 = 6, but the upper bound is 3
		assert_eq!(Staking::get_unbonding_delta(max_unstake * 2, config), normalize_era(3));
	});
}

#[test]
fn correct_unbond_era_is_being_calculated_1() {
	ExtBuilder::default().build_and_execute(|| {
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Set(UnbondingQueueConfig {
				min_slashable_share: Perbill::from_percent(50),
				lowest_ratio: Perbill::from_percent(34),
				unbond_period_lower_bound: 1,
			})
		));

		let current_era = Staking::current_era();
		assert_eq!(current_era, 1);

		// Set a known minimum lowest stake.
		assert_ok!(EraLowestRatioTotalStake::<Test>::try_append(1000));

		// First unbond of 500 (max_unstake is 500, delta = 3).
		assert_eq!(Staking::get_unbonding_duration(500), 3);
		let unbond_era = Staking::process_unbond_queue_request(current_era, 500);
		assert_eq!(unbond_era, 1 + 3); // 4
		assert_eq!(BackOfUnbondingQueue::<Test>::get(), 4000000000000);

		// Next unbond of 250 (delta = 3).
		assert_eq!(Staking::get_unbonding_duration(250), 3);
		let unbond_era = Staking::process_unbond_queue_request(current_era, 250);
		assert_eq!(unbond_era, 1 + 3); // 4
		assert_eq!(BackOfUnbondingQueue::<Test>::get(), 5500000000000);

		// Unbond with amount requiring lower bound (delta = 0 → use upper bound 3 again)
		assert_eq!(Staking::get_unbonding_duration(100), 3);
		let unbond_era = Staking::process_unbond_queue_request(current_era, 100);
		assert_eq!(unbond_era, 1 + 3); // 4

		// delta = 0.6
		assert_eq!(BackOfUnbondingQueue::<Test>::get(), 6100000000000);
	});
}

#[test]
fn correct_unbond_era_is_being_calculated_2() {
	ExtBuilder::default().build_and_execute(|| {
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Set(UnbondingQueueConfig {
				min_slashable_share: Perbill::from_percent(50),
				lowest_ratio: Perbill::from_percent(34),
				unbond_period_lower_bound: 1,
			})
		));

		// Start at era 1.
		let current_era = Staking::current_era();
		assert_eq!(current_era, 1);

		// Set a known minimum lowest stake.
		assert_ok!(EraLowestRatioTotalStake::<Test>::try_append(1000));
		assert_eq!(BackOfUnbondingQueue::<Test>::get(), 0);

		// Unbond with amount requiring lower bound (delta = 0.06 → use lower bound 1)
		assert_eq!(Staking::get_unbonding_duration(10), 1);
		let unbond_era = Staking::process_unbond_queue_request(current_era, 10);
		assert_eq!(unbond_era, 1 + 1); // 2

		// max(current_era, previous_back) + delta = 1 + 0.06 = 1.06
		assert_eq!(BackOfUnbondingQueue::<Test>::get(), normalize_era(1) + 60000000000);

		// Next unbond of 250 (delta = 1.5 -> rounding up to 2).
		assert_eq!(Staking::get_unbonding_duration(250), 2);
		let unbond_era = Staking::process_unbond_queue_request(current_era, 250);
		assert_eq!(unbond_era, 1 + 2); // 3

		// max(current_era, previous_back) + delta = 1.06 + 1.5 = 1 + 1.56
		assert_eq!(BackOfUnbondingQueue::<Test>::get(), normalize_era(1) + 1560000000000);

		// Last unbond of 500 (max_unstake is 500, delta = 3, and it hits the maximum unbonding
		// period of 3 eras).
		assert_eq!(Staking::get_unbonding_duration(500), 3);
		let unbond_era = Staking::process_unbond_queue_request(current_era, 500);
		assert_eq!(unbond_era, 1 + 3); // 4

		// max(current_era, previous_back) + delta = 2.56 + 3 = 1 + 1.56 + 3
		assert_eq!(
			BackOfUnbondingQueue::<Test>::get(),
			normalize_era(1) + 1560000000000 + 3000000000000
		);
	});
}

#[test]
fn correct_unbond_era_is_being_calculated_without_config_set() {
	ExtBuilder::default().build_and_execute(|| {
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
		));

		// Start at era 1.
		let current_era = Staking::current_era();
		assert_eq!(current_era, 1);
		assert_eq!(UnbondingQueueParams::<Test>::get(), None);

		// Set a known minimum lowest stake.
		assert_ok!(EraLowestRatioTotalStake::<Test>::try_append(1000));

		// Regardless of the amount, the unbonding era should be +3.
		assert_eq!(Staking::get_unbonding_duration(100), 3);
		assert_eq!(Staking::process_unbond_queue_request(current_era, 100), 1 + 3);

		assert_eq!(Staking::get_unbonding_duration(100), 3);
		assert_eq!(Staking::process_unbond_queue_request(current_era, 250), 1 + 3);

		assert_eq!(Staking::get_unbonding_duration(100), 3);
		assert_eq!(Staking::process_unbond_queue_request(current_era, 500), 1 + 3);
	});
}

#[test]
fn rebonding_should_reduce_back_of_unbonding_queue() {
	ExtBuilder::default().build_and_execute(|| {
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Set(UnbondingQueueConfig {
				min_slashable_share: Perbill::from_percent(50),
				lowest_ratio: Perbill::from_percent(34),
				unbond_period_lower_bound: 1,
			})
		));
		// Start at era 1 with known minimum lowest stake
		BackOfUnbondingQueue::<Test>::set(normalize_era(1));
		let current_era = Staking::current_era();
		assert_eq!(current_era, 1);
		assert_ok!(EraLowestRatioTotalStake::<Test>::try_append(1000));

		// First unbond of 500 (max_unstake is 500, delta = 3)
		let unbond_era = Staking::process_unbond_queue_request(current_era, 500);
		assert_eq!(unbond_era, 1 + 3);
		assert_eq!(BackOfUnbondingQueue::<Test>::get(), normalize_era(1) + normalize_era(3));

		// Rebond 250 should reduce back_of_unbonding_queue by 1.5 eras.
		Staking::process_rebond_queue_request(current_era, 250);
		assert_eq!(BackOfUnbondingQueue::<Test>::get(), normalize_era(1) + 1500000000000);

		// Rebond remaining 250 should reduce back_of_unbonding_queue to its original value.
		Staking::process_rebond_queue_request(current_era, 250);
		assert_eq!(BackOfUnbondingQueue::<Test>::get(), normalize_era(1));
	});
}

#[test]
fn rebonding_after_one_era_should_reduce_back_of_unbonding_queue() {
	ExtBuilder::default().build_and_execute(|| {
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Set(UnbondingQueueConfig {
				min_slashable_share: Perbill::from_percent(50),
				lowest_ratio: Perbill::from_percent(34),
				unbond_period_lower_bound: 1,
			})
		));

		// Start at era 1 with known minimum lowest stake
		BackOfUnbondingQueue::<Test>::set(normalize_era(1));
		let current_era = Staking::current_era();
		assert_eq!(current_era, 1);
		assert_ok!(EraLowestRatioTotalStake::<Test>::try_append(1000));

		// First unbond of 500 (max_unstake is 500, delta = 3)
		let unbond_era = Staking::process_unbond_queue_request(current_era, 500);
		assert_eq!(unbond_era, 1 + 3);
		assert_eq!(BackOfUnbondingQueue::<Test>::get(), normalize_era(1) + normalize_era(3));

		// Now the minimum is 500, not 1000.
		assert_ok!(EraLowestRatioTotalStake::<Test>::try_append(500));

		// Move to next era.
		Session::roll_until_active_era(2);

		// Rebond 250 should reduce back_of_unbonding_queue by the maximum of 3 eras, but it gets
		// limited by the current era.
		Staking::process_rebond_queue_request(2, 250);
		assert_eq!(BackOfUnbondingQueue::<Test>::get(), normalize_era(2));

		// Rebond remaining 250 should not reduce more the queue.
		Staking::process_rebond_queue_request(2, 250);
		assert_eq!(BackOfUnbondingQueue::<Test>::get(), normalize_era(2));
	});
}

#[test]
fn rebonding_after_one_era_and_unbonding_should_place_the_new_unbond_era_in_the_queue() {
	ExtBuilder::default().build_and_execute(|| {
		assert_ok!(Staking::set_staking_configs(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Set(UnbondingQueueConfig {
				min_slashable_share: Perbill::from_percent(50),
				lowest_ratio: Perbill::from_percent(34),
				unbond_period_lower_bound: 1,
			})
		));

		// Start at era 1 with known minimum lowest stake
		BackOfUnbondingQueue::<Test>::set(normalize_era(1));
		let current_era = Staking::current_era();
		assert_eq!(current_era, 1);
		assert_ok!(EraLowestRatioTotalStake::<Test>::try_append(1000));

		// First unbond
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![]
		);
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 2 }]
		);

		// Second unbond
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 500));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 2 }, UnlockChunk { value: 500, era: 4 }]
		);

		assert_ok!(Staking::rebond(RuntimeOrigin::signed(11), 490));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 2 }, UnlockChunk { value: 10, era: 4 }]
		);

		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 20, era: 2 }, UnlockChunk { value: 10, era: 4 }]
		);
	});
}
