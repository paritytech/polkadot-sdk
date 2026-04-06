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

//! Tests for validator self-stake incentive (liquid payout).

use super::*;
use crate::{
	asset,
	session_rotation::{EraElectionPlanner, Eras, Rotator},
};

/// Sets up the default validator self-stake incentive config used across tests.
fn setup_incentive_config() {
	assert_ok!(Staking::set_validator_self_stake_incentive_config(
		RuntimeOrigin::root(),
		ConfigOp::Set(30_000),
		ConfigOp::Set(100_000),
		ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
	));
}

/// Sets up incentive config and a budget allocation with the given percentages.
fn setup_incentive_with_budget(staker_pct: u32, incentive_pct: u32) {
	setup_incentive_config();
	let buffer_pct = 100u32.saturating_sub(staker_pct).saturating_sub(incentive_pct);
	let mut entries = vec![(staker_reward_key(), staker_pct)];
	if incentive_pct > 0 {
		entries.push((validator_incentive_key(), incentive_pct));
	}
	if buffer_pct > 0 {
		entries.push((buffer_key(), buffer_pct));
	}
	pallet_dap::BudgetAllocation::<Test>::put(build_budget(&entries));
}

/// Finds the staker reward amount for a given stash from events.
fn staker_reward_for(stash: AccountId, events: &[Event<Test>]) -> Option<Balance> {
	events.iter().find_map(|e| match e {
		Event::Rewarded { stash: s, amount, .. } if *s == stash => Some(*amount),
		_ => None,
	})
}

/// Finds the validator incentive amount for a given stash from events.
fn incentive_paid_for(stash: AccountId, events: &[Event<Test>]) -> Option<Balance> {
	incentive_paid_details(stash, events).map(|(amount, _)| amount)
}

/// Finds the validator incentive amount and destination for a given stash from events.
fn incentive_paid_details(
	stash: AccountId,
	events: &[Event<Test>],
) -> Option<(Balance, RewardDestination<AccountId>)> {
	events.iter().find_map(|e| match e {
		Event::ValidatorIncentivePaid { validator_stash, amount, dest, .. }
			if *validator_stash == stash =>
		{
			Some((*amount, *dest))
		},
		_ => None,
	})
}

// ===== Config extrinsic tests =====

#[test]
fn set_validator_self_stake_incentive_config_works() {
	ExtBuilder::default().build_and_execute(|| {
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));
		assert_eq!(OptimumSelfStake::<Test>::get(), 30_000);
		assert_eq!(HardCapSelfStake::<Test>::get(), 100_000);
		assert_eq!(SelfStakeSlopeFactor::<Test>::get(), Perbill::from_rational(1u32, 2u32));

		assert_storage_noop!(assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
		)));

		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Remove,
			ConfigOp::Remove,
			ConfigOp::Remove,
		));
		assert!(!OptimumSelfStake::<Test>::exists());
		assert!(!HardCapSelfStake::<Test>::exists());
		assert!(!SelfStakeSlopeFactor::<Test>::exists());
	});
}

#[test]
fn set_validator_self_stake_incentive_config_requires_admin() {
	ExtBuilder::default().build_and_execute(|| {
		let admin = 1;

		assert_noop!(
			Staking::set_validator_self_stake_incentive_config(
				RuntimeOrigin::signed(2),
				ConfigOp::Set(30_000),
				ConfigOp::Set(100_000),
				ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
			),
			DispatchError::BadOrigin
		);

		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::signed(admin),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));
	});
}

#[test]
fn set_validator_self_stake_incentive_config_rejects_optimum_greater_than_cap() {
	ExtBuilder::default().build_and_execute(|| {
		assert_noop!(
			Staking::set_validator_self_stake_incentive_config(
				RuntimeOrigin::root(),
				ConfigOp::Set(100_000),
				ConfigOp::Set(50_000),
				ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
			),
			Error::<Test>::OptimumGreaterThanCap
		);
	});
}

#[test]
fn set_validator_self_stake_incentive_config_accepts_equal_values() {
	ExtBuilder::default().build_and_execute(|| {
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(50_000),
			ConfigOp::Set(50_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));
		assert_eq!(OptimumSelfStake::<Test>::get(), 50_000);
		assert_eq!(HardCapSelfStake::<Test>::get(), 50_000);
	});
}

// ===== Reward distribution tests =====

#[test]
fn validator_receives_both_staker_and_incentive_rewards() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator
		let bob = 101; // nominator

		setup_incentive_with_budget(45, 5);

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let alice_balance_before = asset::total_balance::<Test>(&alice);

		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		let staker_reward =
			staker_reward_for(alice, &events).expect("Validator should receive staker reward");
		let incentive =
			incentive_paid_for(alice, &events).expect("Validator should receive incentive bonus");

		let alice_balance_after = asset::total_balance::<Test>(&alice);
		assert_eq!(alice_balance_after - alice_balance_before, staker_reward + incentive);

		// Nominator receives staker reward but not incentive.
		assert!(staker_reward_for(bob, &events).is_some());
		assert!(incentive_paid_for(bob, &events).is_none());
	});
}

#[test]
fn changing_budget_allocation_affects_rewards() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator

		// Era 1: 50% staker, 0% incentive.
		setup_incentive_with_budget(50, 0);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(1);
		let era1_events = staking_events_since_last_call();
		assert!(staker_reward_for(alice, &era1_events).is_some());
		assert!(incentive_paid_for(alice, &era1_events).is_none());

		// Era 2: 40% staker, 10% incentive.
		setup_incentive_with_budget(40, 10);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(2);
		let era2_events = staking_events_since_last_call();
		assert!(staker_reward_for(alice, &era2_events).is_some());
		assert!(incentive_paid_for(alice, &era2_events).is_some());

		assert_eq!(ErasValidatorIncentiveAllocation::<Test>::get(1), 0);
		assert!(ErasValidatorIncentiveAllocation::<Test>::get(2) > 0);
	});
}

#[test]
fn validator_with_zero_reward_points_no_payout() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator
		let bob = 21; // validator

		setup_incentive_with_budget(45, 5);
		Eras::<Test>::reward_active_era(vec![(bob, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(1);
		let events = staking_events_since_last_call();

		// Alice (no reward points) gets nothing.
		assert!(staker_reward_for(alice, &events).is_none());
		assert!(incentive_paid_for(alice, &events).is_none());
		assert!(staker_reward_for(bob, &events).is_some());
	});
}

#[test]
fn very_small_self_stake_weight() {
	ExtBuilder::default().build_and_execute(|| {
		// Validator with self-stake of 1000 (mock default), below optimum of 30_000.
		// w(1000) = √1000 ≈ 31
		let alice = 11; // validator

		setup_incentive_with_budget(45, 5);

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let weight = ErasValidatorIncentive::<Test>::get(2, alice).unwrap();
		assert_eq!(weight, 31); // √1000 ≈ 31

		make_all_reward_payment(2);
		let events = staking_events_since_last_call();
		assert!(incentive_paid_for(alice, &events).is_some());
	});
}

#[test]
fn validator_incentive_with_account_destination() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator
		let reward_account = 999;

		assert_ok!(Staking::set_payee(
			RuntimeOrigin::signed(alice),
			RewardDestination::Account(reward_account)
		));

		setup_incentive_with_budget(45, 5);

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let reward_balance_before = asset::total_balance::<Test>(&reward_account);

		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		let (incentive, dest) =
			incentive_paid_details(alice, &events).expect("Should receive incentive");
		assert_eq!(dest, RewardDestination::Account(reward_account));

		let reward_balance_after = asset::total_balance::<Test>(&reward_account);
		assert!(reward_balance_after - reward_balance_before >= incentive);
	});
}

#[test]
fn multi_page_election_does_not_overwrite_incentive_weight() {
	ExtBuilder::default().exposures_page_size(1).build_and_execute(|| {
		let alice = 11; // validator
		setup_incentive_config();

		Session::roll_to_next_session();
		let planned_era = Rotator::<Test>::planned_era();

		// Scenario 1: own-stake on page 1, page 2 has only nominators.
		hypothetically!({
			let page1 = bounded_vec![(
				alice,
				Exposure {
					total: 1000 + 250,
					own: 1000,
					others: vec![IndividualExposure { who: 101, value: 250 }]
				},
			)];
			EraElectionPlanner::<Test>::store_stakers_info(page1, planned_era);

			let weight = ErasValidatorIncentive::<Test>::get(planned_era, alice).unwrap();
			let total = ErasTotalValidatorWeight::<Test>::get(planned_era);
			assert!(weight > 0);

			let page2 = bounded_vec![(
				alice,
				Exposure {
					total: 250,
					own: 0,
					others: vec![IndividualExposure { who: 102, value: 250 }]
				},
			)];
			EraElectionPlanner::<Test>::store_stakers_info(page2, planned_era);

			// Weight must not be overwritten by page 2 (own=0).
			assert_eq!(ErasValidatorIncentive::<Test>::get(planned_era, alice).unwrap(), weight);
			assert_eq!(ErasTotalValidatorWeight::<Test>::get(planned_era), total);
		});

		// Scenario 2: own-stake arrives on page 2 (not page 1).
		hypothetically!({
			let page1 = bounded_vec![(
				alice,
				Exposure {
					total: 250,
					own: 0,
					others: vec![IndividualExposure { who: 101, value: 250 }]
				},
			)];
			EraElectionPlanner::<Test>::store_stakers_info(page1, planned_era);
			assert_eq!(ErasValidatorIncentive::<Test>::get(planned_era, alice), None);

			let page2 = bounded_vec![(
				alice,
				Exposure {
					total: 1000 + 250,
					own: 1000,
					others: vec![IndividualExposure { who: 102, value: 250 }]
				},
			)];
			EraElectionPlanner::<Test>::store_stakers_info(page2, planned_era);

			let weight = ErasValidatorIncentive::<Test>::get(planned_era, alice).unwrap();
			assert!(weight > 0);
			assert_eq!(ErasTotalValidatorWeight::<Test>::get(planned_era), weight);
		});
	});
}
