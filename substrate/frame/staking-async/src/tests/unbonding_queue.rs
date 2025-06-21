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
use crate::{tests::Test, UnbondingQueueConfig, UnbondingQueueParams};
use frame_support::traits::fungible::Inspect;
use sp_runtime::Perbill;
use std::collections::BTreeMap;

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
					unbond_period_lower_bound: 2,
				})
			));

			// Initial conditions.
			assert_eq!(Staking::current_era(), 1);
			assert_eq!(ErasLowestRatioTotalStake::<T>::iter().collect::<Vec<_>>(), vec![]);

			// Setup to nominate.
			assert_ok!(Staking::bond(RuntimeOrigin::signed(999), 100, RewardDestination::Stash));
			assert_ok!(Staking::nominate(RuntimeOrigin::signed(999), vec![31]));

			// Era 1 -> 2
			Session::roll_until_active_era(2);
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(2, 500)])
			);

			// Era 2 -> 3
			assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(999), 100));
			Session::roll_until_active_era(3);
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(2, 500), (3, 600)])
			);

			// Era 3 -> 4
			assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(999), 100));
			Session::roll_until_active_era(4);
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(2, 500), (3, 600), (4, 700)])
			);

			// Era 4 -> 5
			assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(999), 100));
			Session::roll_until_active_era(5);
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(3, 600), (4, 700), (5, 800)])
			);

			// Era 5 -> 6
			assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(999), 100));
			Session::roll_until_active_era(6);
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(4, 700), (5, 800), (6, 900)])
			);
		});
}

#[test]
fn calculate_lowest_total_stake_works() {
	ExtBuilder::default()
		.has_stakers(false)
		.validator_count(4)
		.has_unbonding_queue_config(true)
		.build_and_execute(|| {
			Session::roll_until_active_era(4);
			assert_eq!(current_era(), 4);
			// There are no stakers
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(2, 0), (3, 0), (4, 0)])
			);
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
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(3, 0), (4, 0), (5, 1000)])
			);

			Session::roll_until_active_era(6);
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(4, 0), (5, 1000), (6, 1000)])
			);

			// Ensure old entry is pruned after bonding duration (3 eras).
			Session::roll_until_active_era(7);
			assert_eq!(
				ErasLowestRatioTotalStake::<T>::iter().collect::<BTreeMap<_, _>>(),
				BTreeMap::from([(5, 1000), (6, 1000), (7, 1000)])
			);
		});
}

#[test]
fn correct_unbond_era_is_being_calculated_without_config_set() {
	ExtBuilder::default().build_and_execute(|| {
		// Start at era 1 with known minimum lowest stake
		assert_eq!(UnbondingQueueParams::<Test>::get(), None);
		let current_era = Staking::current_era();
		assert_eq!(current_era, 1);

		// The first attempt before unbonding should yield no unbonds.
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![]);

		// First unbond
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 1, previous_unbonded_stake: 0 }]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![(1 + 3, 10)]);
		assert_eq!(TotalUnbondInEra::<T>::get(1), Some(10));

		// Second unbond
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 20, era: 1, previous_unbonded_stake: 10 }]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![(1 + 3, 20)]);
		assert_eq!(TotalUnbondInEra::<T>::get(1), Some(20));
	});
}

#[test]
fn rebonding_after_one_era_and_unbonding_should_place_the_new_unbond_era_in_the_queue() {
	ExtBuilder::default().has_unbonding_queue_config(true).build_and_execute(|| {
		// Start at era 1 with known minimum lowest stake
		let current_era = Staking::current_era();
		assert_eq!(current_era, 1);

		// First unbond
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![]);
		assert_eq!(TotalUnbondInEra::<T>::get(2), None);

		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 1, previous_unbonded_stake: 0 }]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![(1 + 2, 10)]);
		assert_eq!(TotalUnbondInEra::<T>::get(1), Some(10));

		// Second unbond
		Session::roll_until_active_era(2);
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 500));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![
				UnlockChunk { value: 10, era: 1, previous_unbonded_stake: 0 },
				UnlockChunk { value: 500, era: 2, previous_unbonded_stake: 0 }
			]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![(1 + 2, 10), (2 + 2, 500)]);
		assert_eq!(TotalUnbondInEra::<T>::get(1), Some(10));
		assert_eq!(TotalUnbondInEra::<T>::get(2), Some(500));

		assert_ok!(Staking::rebond(RuntimeOrigin::signed(11), 490));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![
				UnlockChunk { value: 10, era: 1, previous_unbonded_stake: 0 },
				UnlockChunk { value: 10, era: 2, previous_unbonded_stake: 0 }
			]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![(1 + 2, 10), (2 + 2, 10)]);
		assert_eq!(TotalUnbondInEra::<T>::get(1), Some(10));
		assert_eq!(TotalUnbondInEra::<T>::get(2), Some(10));

		// Rebond so that the last chunk gets removed and part of the previous one gets subtracted.
		assert_ok!(Staking::rebond(RuntimeOrigin::signed(11), 15));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 5, era: 1, previous_unbonded_stake: 0 }]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![(1 + 2, 5)]);
		assert_eq!(TotalUnbondInEra::<T>::get(1), Some(5));
		assert_eq!(TotalUnbondInEra::<T>::get(2), None);
	});
}

#[test]
fn test_withdrawing_with_favorable_global_stake_threshold_should_work() {
	ExtBuilder::default().has_unbonding_queue_config(true).build_and_execute(|| {
		Session::roll_until_active_era(10);
		assert_eq!(Staking::current_era(), 10);

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
			vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
		);

		// Should be able to withdraw after one era.
		assert_eq!(Staking::unbonding_duration(11), vec![(10 + 2, 10)]);
		assert_eq!(TotalUnbondInEra::<T>::get(10), Some(10));

		// Should not have withdrawn any funds yet.
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
		);

		// After one era the user still not be able to withdraw
		Session::roll_until_active_era(11);
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![(10 + 2, 10)]);
		assert_eq!(TotalUnbondInEra::<T>::get(10), Some(10));

		// After two eras the user can withdraw.
		Session::roll_until_active_era(12);
		assert_eq!(Staking::unbonding_duration(11), vec![(10 + 2, 10)]);
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![]);
		// This must not change. It includes the stake to be withdrawn and one already withdrawn.
		assert_eq!(TotalUnbondInEra::<T>::get(10), Some(10));
	});
}

#[test]
fn test_withdrawing_over_global_stake_threshold_should_not_work() {
	ExtBuilder::default().has_unbonding_queue_config(true).build_and_execute(|| {
		Session::roll_until_active_era(10);
		assert_eq!(Staking::current_era(), 10);
		// Assume there was no previous stake in any era.
		let _ = ErasLowestRatioTotalStake::<Test>::clear(u32::MAX, None);

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
			vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![(10 + 3, 10)]);
		assert_eq!(TotalUnbondInEra::<T>::get(10), Some(10));

		// Should not have withdrawn any funds yet.
		Session::roll_until_active_era(11);
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![(10 + 3, 10)]);
		assert_eq!(TotalUnbondInEra::<T>::get(10), Some(10));

		// With the lowest stake set the user should be able to withdraw.
		Session::roll_until_active_era(12);
		assert_eq!(Staking::unbonding_duration(11), vec![(10 + 3, 10)]);
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![(10 + 3, 10)]);
		assert_eq!(TotalUnbondInEra::<T>::get(10), Some(10));

		// After three eras the user is finally able to withdraw.
		Session::roll_until_active_era(13);
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![]);
		assert_eq!(TotalUnbondInEra::<T>::get(10), None);
	});
}

#[test]
fn old_unbonding_chunks_should_be_withdrawable_in_current_era() {
	ExtBuilder::default().has_unbonding_queue_config(true).build_and_execute(|| {
		Session::roll_until_active_era(10);
		assert_eq!(Staking::current_era(), 10);

		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![]);
		assert_eq!(TotalUnbondInEra::<T>::get(10), None);

		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![(10 + 2, 10)]);
		assert_eq!(TotalUnbondInEra::<T>::get(10), Some(10));

		Session::roll_until_active_era(11);
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 10));
		assert_eq!(
			StakingLedger::<Test>::get(StakingAccount::Stash(11))
				.unwrap()
				.unlocking
				.into_inner(),
			vec![
				UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 },
				UnlockChunk { value: 10, era: 11, previous_unbonded_stake: 0 }
			]
		);
		assert_eq!(Staking::unbonding_duration(11), vec![(10 + 2, 10), (11 + 2, 10)]);
		assert_eq!(TotalUnbondInEra::<T>::get(10), Some(10));
		assert_eq!(TotalUnbondInEra::<T>::get(11), Some(10));

		Session::roll_until_active_era(12);
		assert_eq!(Staking::unbonding_duration(11), vec![(10 + 2, 10), (11 + 2, 10)]);
		assert_eq!(TotalUnbondInEra::<T>::get(10), Some(10));
		assert_eq!(TotalUnbondInEra::<T>::get(11), Some(10));

		// Now both unbonds get collapsed in a single entry.
		Session::roll_until_active_era(13);
		assert_eq!(Staking::unbonding_duration(11), vec![(13, 20)]);
		assert_eq!(TotalUnbondInEra::<T>::get(10), None);
		assert_eq!(TotalUnbondInEra::<T>::get(11), Some(10));
	});
}

#[test]
fn increasing_unbond_amount_should_delay_expected_withdrawal() {
	ExtBuilder::default()
		.has_stakers(false)
		.has_unbonding_queue_config(true)
		.build_and_execute(|| {
			let ed = Balances::minimum_balance();

			assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 11, 10_000 + ed));
			assert_ok!(Staking::bond(RuntimeOrigin::signed(11), 10_000, RewardDestination::Stash));
			assert_ok!(Staking::validate(RuntimeOrigin::signed(11), ValidatorPrefs::default()));

			Session::roll_until_active_era(10);
			assert_eq!(Staking::current_era(), 10);

			assert_eq!(
				StakingLedger::<Test>::get(StakingAccount::Stash(11))
					.unwrap()
					.unlocking
					.into_inner(),
				vec![]
			);
			assert_eq!(Staking::unbonding_duration(11), vec![]);
			assert_eq!(TotalUnbondInEra::<T>::get(10), None);

			assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 10));
			assert_eq!(
				StakingLedger::<Test>::get(StakingAccount::Stash(11))
					.unwrap()
					.unlocking
					.into_inner(),
				vec![UnlockChunk { value: 10, era: 10, previous_unbonded_stake: 0 }]
			);
			assert_eq!(Staking::unbonding_duration(11), vec![(10 + 2, 10)]);
			assert_eq!(TotalUnbondInEra::<T>::get(10), Some(10));

			// Unbond a huge amount of stake.
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(11), 8000 - 10));
			assert_eq!(
				StakingLedger::<Test>::get(StakingAccount::Stash(11))
					.unwrap()
					.unlocking
					.into_inner(),
				vec![UnlockChunk { value: 8000, era: 10, previous_unbonded_stake: 10 }]
			);

			// The expected release has been increased.
			assert_eq!(Staking::unbonding_duration(11), vec![(10 + 3, 8000)]);
			assert_eq!(TotalUnbondInEra::<T>::get(10), Some(8000));
		});
}
