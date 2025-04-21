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
use crate::{
	session_rotation::EraElectionPlanner, tests::Test, UnbondingQueueConfig, UnbondingQueueParams,
};
use sp_npos_elections::Support;
use sp_runtime::{traits::Zero, Perbill};

#[test]
fn get_min_lowest_stake_works() {
	ExtBuilder::default()
		.set_status(31, StakerStatus::<AccountId>::Validator)
		.validator_count(3)
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
					back_of_unbonding_queue_era: Zero::zero(),
				})
			));

			// Check the era we are working with.
			assert_eq!(Staking::get_min_lowest_stake(), 0);
			Session::roll_until_active_era(2);
			let current_era = Staking::current_era();
			assert_eq!(current_era, 2);
			assert_eq!(Staking::get_min_lowest_stake(), 500);

			// Start initial era and verify setup.
			assert_eq!(
				UnbondingQueueParams::<Test>::get().unwrap(),
				UnbondingQueueConfig {
					min_slashable_share: Perbill::from_percent(50),
					lowest_ratio: Perbill::from_percent(34),
					unbond_period_lower_bound: 1,
					back_of_unbonding_queue_era: Zero::zero(),
				}
			);

			// Populate some `EraLowestRatioTotalStake` entries to test the function.
			let bonding_duration = <Test as Config>::BondingDuration::get();
			for i in current_era + 1..bonding_duration {
				assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(11), 100));
				assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(21), 100));
				assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(31), 100));
				assert_eq!(Staking::get_min_lowest_stake(), 500);
				Session::roll_until_active_era(i + 1);
				assert_eq!(Staking::get_min_lowest_stake(), 500);
			}

			// After this the lowest value will have been removed, and next iterations the
			// number will increase by 100 each era.
			let current_era = Staking::current_era();
			for i in current_era + 1..bonding_duration {
				assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(11), 100));
				assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(21), 100));
				assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(31), 100));
				assert_eq!(Staking::get_min_lowest_stake(), (500 + 100 * (i - 1)).into());
				Session::roll_until_active_era(i + 1);
				assert_eq!(Staking::get_min_lowest_stake(), (500 + 100 * i).into());
			}
		});
}

#[test]
fn get_min_lowest_stake_with_many_validators_works() {
	ExtBuilder::default()
		.validator_count(9)
		.multi_page_election_provider(3)
		// We already have the following validators:
		// 11 - 1000
		// 21 - 1000
		// 31 - 500
		.set_status(31, StakerStatus::<AccountId>::Validator) // 1000
		.set_status(41, StakerStatus::<AccountId>::Validator) // 1000
		.set_status(51, StakerStatus::<AccountId>::Validator) // 1000
		.add_staker(201, 1000, StakerStatus::<AccountId>::Validator)
		.add_staker(202, 1000, StakerStatus::<AccountId>::Validator)
		// And we add two more
		.add_staker(61, 100, StakerStatus::<AccountId>::Validator)
		.add_staker(71, 200, StakerStatus::<AccountId>::Validator)
		.set_status(101, StakerStatus::<AccountId>::Nominator(vec![61])) // 500 extras to nominate candidate 61
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
					back_of_unbonding_queue_era: Zero::zero(),
				})
			));

			assert_eq!(Staking::get_min_lowest_stake(), 0);
			Session::roll_until_active_era(2);

			// There are 9 validators, so one third of them would be 3.
			// Hence, the lowest third would be composed of the following validators:
			// 61 -> 100 (own) + 500 (nominated)
			// 71 -> 200
			// 31 -> 500
			// Summing up 1300 total.
			assert_eq!(Staking::get_min_lowest_stake(), 1300);
		});
}

#[test]
fn calculate_lowest_total_stake_works() {
	ExtBuilder::default().has_stakers(false).build_and_execute(|| {
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
				back_of_unbonding_queue_era: Zero::zero(),
			})
		));

		Session::roll_until_active_era(2);
		Session::roll_until_active_era(3);
		Session::roll_until_active_era(4);
		assert_eq!(current_era(), 4);
		// There are no stakers
		assert_eq!(EraLowestRatioTotalStake::<Test>::get().into_inner(), vec![0, 0, 0]);
		assert_eq!(Staking::get_min_lowest_stake(), 0);

		// Create validators with different stakes for the next era.
		let exposures = to_bounded_supports(vec![
			(1, Support { total: 1000, voters: vec![] }),
			(2, Support { total: 2000, voters: vec![] }),
			(3, Support { total: 3000, voters: vec![] }),
			(4, Support { total: 4000, voters: vec![] }),
		]);

		// Trigger new era to calculate the lowest proportion.
		assert_ok!(EraElectionPlanner::<T>::do_elect_paged_inner(
			exposures.clone().try_into().unwrap()
		));
		Session::roll_until_active_era(5);
		assert_eq!(EraLowestRatioTotalStake::<Test>::get().into_inner(), vec![0, 0, 1000]);
		// The lowest proportion is 33% of 4 validators ~ 1.32.
		// Hence, the lowest 1 validator with 1000.
		assert_eq!(Staking::get_min_lowest_stake(), 0);

		assert_ok!(EraElectionPlanner::<Test>::do_elect_paged_inner(exposures.clone()));
		Session::roll_until_active_era(6);
		assert_eq!(EraLowestRatioTotalStake::<Test>::get().into_inner(), vec![0, 1000, 1000]);
		assert_eq!(Staking::get_min_lowest_stake(), 0);

		// Ensure old entry is pruned after bonding duration (3 eras).
		assert_ok!(EraElectionPlanner::<Test>::do_elect_paged_inner(exposures.clone()));
		Session::roll_until_active_era(7);
		assert_eq!(EraLowestRatioTotalStake::<Test>::get().into_inner(), vec![1000, 1000, 1000]);
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
			back_of_unbonding_queue_era: 0,
		};

		// Must be the maximum because the minimum lowest stake is zero, which defaults to
		// the maximum unbonding period.
		assert_eq!(EraLowestRatioTotalStake::<Test>::get().into_inner(), vec![]);
		assert_eq!(Staking::get_unbond_eras_delta(1, config), 3);

		let config = UnbondingQueueConfig {
			min_slashable_share: Perbill::from_percent(0),
			lowest_ratio: Perbill::from_percent(34),
			unbond_period_lower_bound: 1,
			back_of_unbonding_queue_era: 0,
		};

		// Now the minimum lowest stake is also zero because the slashable share is zero, so
		// again it must be the maximum unbonding period.
		assert_ok!(EraLowestRatioTotalStake::<Test>::try_append(1000));
		assert_eq!(Staking::get_unbond_eras_delta(1, config), 3);
	});
}

#[test]
fn get_unbond_eras_delta_works() {
	ExtBuilder::default().build_and_execute(|| {
		let config = UnbondingQueueConfig {
			min_slashable_share: Perbill::from_percent(50),
			lowest_ratio: Perbill::from_percent(34),
			unbond_period_lower_bound: 1,
			back_of_unbonding_queue_era: 0,
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
		assert_eq!(Staking::get_unbond_eras_delta(max_unstake, config), 3);
		// 250 / 500 * 3 = 1.5 → 1
		assert_eq!(Staking::get_unbond_eras_delta(max_unstake / 2, config), 1);
		// 0 / 500 * 3 = 0
		assert_eq!(Staking::get_unbond_eras_delta(0, config), 0);
		// 1000 / 500 * 3 = 6, but the upper bound is 3
		assert_eq!(Staking::get_unbond_eras_delta(max_unstake * 2, config), 3);
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
				back_of_unbonding_queue_era: 0,
			})
		));

		let current_era = Staking::current_era();
		assert_eq!(current_era, 1);

		// Set a known minimum lowest stake.
		assert_ok!(EraLowestRatioTotalStake::<Test>::try_append(1000));

		// First unbond of 500 (max_unstake is 500, delta = 3).
		let unbond_era = Staking::process_unbond_queue_request(current_era, 500);
		assert_eq!(unbond_era, 1 + 3); // 4
		assert_eq!(UnbondingQueueParams::<Test>::get().unwrap().back_of_unbonding_queue_era, 4);

		// Next unbond of 250 (delta = 3).
		let unbond_era = Staking::process_unbond_queue_request(current_era, 250);
		assert_eq!(unbond_era, 1 + 3); // Theoretically it'd be 1 + 4, but the upper bound is 3
		assert_eq!(UnbondingQueueParams::<Test>::get().unwrap().back_of_unbonding_queue_era, 5);

		// Unbond with amount requiring lower bound (delta = 0 → use upper bound 3 again)
		let unbond_era = Staking::process_unbond_queue_request(current_era, 100);
		assert_eq!(unbond_era, 1 + 3); // 4

		// Back remains 5 because delta = 0: new_back = max(1, 5) + 0 = 5
		assert_eq!(UnbondingQueueParams::<Test>::get().unwrap().back_of_unbonding_queue_era, 5);
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
				back_of_unbonding_queue_era: 0,
			})
		));

		// Start at era 1.
		let current_era = Staking::current_era();
		assert_eq!(current_era, 1);

		// Set a known minimum lowest stake.
		assert_ok!(EraLowestRatioTotalStake::<Test>::try_append(1000));
		assert_eq!(UnbondingQueueParams::<Test>::get().unwrap().back_of_unbonding_queue_era, 0);

		// Unbond with amount requiring lower bound (delta = 0 → use lower bound 1)
		let unbond_era = Staking::process_unbond_queue_request(current_era, 100);
		assert_eq!(unbond_era, 1 + 1); // 2

		// max(current_era, previous_back) + delta = 1 + 0 = 1
		assert_eq!(UnbondingQueueParams::<Test>::get().unwrap().back_of_unbonding_queue_era, 1);

		// Next unbond of 250 (delta = 1.5 -> 1).
		let unbond_era = Staking::process_unbond_queue_request(current_era, 250);
		assert_eq!(unbond_era, 1 + 1); // 2

		// max(current_era, previous_back) + delta = 1 + 1 = 2
		assert_eq!(UnbondingQueueParams::<Test>::get().unwrap().back_of_unbonding_queue_era, 2);

		// Last unbond of 500 (max_unstake is 500, delta = 3, and it hits the maximum unbonding
		// period of 3 eras).
		let unbond_era = Staking::process_unbond_queue_request(current_era, 500);
		assert_eq!(unbond_era, 1 + 3); // 4

		// max(current_era, previous_back) + delta = 2 + 3 = 5
		assert_eq!(UnbondingQueueParams::<Test>::get().unwrap().back_of_unbonding_queue_era, 5);
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
		assert_eq!(Staking::process_unbond_queue_request(current_era, 100), 1 + 3);
		assert_eq!(Staking::process_unbond_queue_request(current_era, 250), 1 + 3);
		assert_eq!(Staking::process_unbond_queue_request(current_era, 500), 1 + 3);
	});
}
