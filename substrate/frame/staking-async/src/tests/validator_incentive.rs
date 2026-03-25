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
use crate::{asset, session_rotation::Eras};

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

#[test]
fn set_validator_self_stake_incentive_config_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Setting all parameters works
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)), // 0.5
		));
		assert_eq!(OptimumSelfStake::<Test>::get(), 30_000);
		assert_eq!(HardCapSelfStake::<Test>::get(), 100_000);
		assert_eq!(SelfStakeSlopeFactor::<Test>::get(), Perbill::from_rational(1u32, 2u32));

		// Noop does nothing
		assert_storage_noop!(assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
		)));

		// Removing works
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
		// as setup in mock
		let admin = 1;

		// Non-admin origin should fail
		assert_noop!(
			Staking::set_validator_self_stake_incentive_config(
				RuntimeOrigin::signed(2),
				ConfigOp::Set(30_000),
				ConfigOp::Set(100_000),
				ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
			),
			DispatchError::BadOrigin
		);

		// Admin origin should work
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::signed(admin),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));
	});
}

#[test]
fn set_validator_self_stake_incentive_config_partial_update() {
	ExtBuilder::default().build_and_execute(|| {
		// Set initial values
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		// Update only optimum_self_stake
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(50_000),
			ConfigOp::Noop,
			ConfigOp::Noop,
		));
		assert_eq!(OptimumSelfStake::<Test>::get(), 50_000);
		assert_eq!(HardCapSelfStake::<Test>::get(), 100_000);
		assert_eq!(SelfStakeSlopeFactor::<Test>::get(), Perbill::from_rational(1u32, 2u32));

		// Update only slope factor
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Set(Perbill::from_rational(3u32, 4u32)),
		));
		assert_eq!(OptimumSelfStake::<Test>::get(), 50_000);
		assert_eq!(HardCapSelfStake::<Test>::get(), 100_000);
		assert_eq!(SelfStakeSlopeFactor::<Test>::get(), Perbill::from_rational(3u32, 4u32));
	});
}

#[test]
fn set_validator_self_stake_incentive_config_rejects_optimum_greater_than_cap() {
	ExtBuilder::default().build_and_execute(|| {
		// Setting both with optimum > cap should fail
		assert_noop!(
			Staking::set_validator_self_stake_incentive_config(
				RuntimeOrigin::root(),
				// optimum
				ConfigOp::Set(100_000),
				// hard cap
				ConfigOp::Set(50_000),
				ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
			),
			Error::<Test>::OptimumGreaterThanCap
		);
	});
}

#[test]
fn set_validator_self_stake_incentive_config_rejects_setting_optimum_greater_than_existing_cap() {
	ExtBuilder::default().build_and_execute(|| {
		// Set initial config with valid values
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		// Try to update optimum to be greater than existing cap should fail
		assert_noop!(
			Staking::set_validator_self_stake_incentive_config(
				RuntimeOrigin::root(),
				// optimum
				ConfigOp::Set(150_000),
				// existing hard cap is 100_000
				ConfigOp::Noop,
				ConfigOp::Noop,
			),
			Error::<Test>::OptimumGreaterThanCap
		);
	});
}

#[test]
fn set_validator_self_stake_incentive_config_rejects_setting_cap_less_than_existing_optimum() {
	ExtBuilder::default().build_and_execute(|| {
		// Set initial config with valid values
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(50_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		// Try to update cap to be less than existing optimum should fail
		assert_noop!(
			Staking::set_validator_self_stake_incentive_config(
				RuntimeOrigin::root(),
				// existing optimum is 50_000
				ConfigOp::Noop,
				// hard cap
				ConfigOp::Set(30_000),
				ConfigOp::Noop,
			),
			Error::<Test>::OptimumGreaterThanCap
		);
	});
}

#[test]
fn set_validator_self_stake_incentive_config_accepts_equal_values() {
	ExtBuilder::default().build_and_execute(|| {
		// Setting both with optimum = cap should succeed
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(50_000),
			ConfigOp::Set(50_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		assert_eq!(OptimumSelfStake::<Test>::get(), 50_000);
		assert_eq!(HardCapSelfStake::<Test>::get(), 50_000);
		assert_eq!(SelfStakeSlopeFactor::<Test>::get(), Perbill::from_rational(1u32, 2u32));
	});
}

#[test]
fn set_validator_self_stake_incentive_config_allows_removing_parameters() {
	ExtBuilder::default().build_and_execute(|| {
		// Set initial config
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		// Removing optimum while keeping cap should succeed (no validation needed)
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Remove,
			ConfigOp::Noop,
			ConfigOp::Noop,
		));
		assert!(!OptimumSelfStake::<Test>::exists());
		assert_eq!(HardCapSelfStake::<Test>::get(), 100_000);

		// Set optimum again
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Noop,
			ConfigOp::Noop,
		));

		// Removing cap while keeping optimum should succeed (no validation needed)
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Remove,
			ConfigOp::Noop,
		));
		assert_eq!(OptimumSelfStake::<Test>::get(), 30_000);
		assert!(!HardCapSelfStake::<Test>::exists());
	});
}

#[test]
fn set_validator_self_stake_incentive_config_allows_setting_optimum_when_cap_is_zero() {
	ExtBuilder::default().build_and_execute(|| {
		// Setting optimum when cap is zero (not configured) should succeed
		// because the config is incomplete and won't be used
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(100_000),
			ConfigOp::Noop, // cap remains 0
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		assert_eq!(OptimumSelfStake::<Test>::get(), 100_000);
		assert_eq!(HardCapSelfStake::<Test>::get(), 0); // Still zero
	});
}

#[test]
fn set_validator_self_stake_incentive_config_allows_setting_cap_when_optimum_is_zero() {
	ExtBuilder::default().build_and_execute(|| {
		// Setting cap when optimum is zero (not configured) should succeed
		// because the config is incomplete and won't be used
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Noop, // optimum remains 0
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		assert_eq!(OptimumSelfStake::<Test>::get(), 0); // Still zero
		assert_eq!(HardCapSelfStake::<Test>::get(), 100_000);
	});
}

#[test]
fn validator_receives_both_staker_and_incentive_rewards() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Validator with incentive budget enabled
		let alice = 11; // validator
		let bob = 101; // nominator

		setup_incentive_with_budget(45, 5);

		// Era 2 has validator weights set by election
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let alice_balance_before = asset::total_balance::<Test>(&alice);

		// WHEN: Payout rewards
		make_all_reward_payment(2);

		// THEN: Validator receives both staker reward and incentive bonus
		let events = staking_events_since_last_call();

		let staker_reward =
			staker_reward_for(alice, &events).expect("Validator should receive staker reward");
		let incentive =
			incentive_paid_for(alice, &events).expect("Validator should receive incentive bonus");

		// Alice balance increased by correct amount
		let alice_balance_after = asset::total_balance::<Test>(&alice);
		assert_eq!(alice_balance_after - alice_balance_before, staker_reward + incentive);

		// Bob (the nominator) received staker reward but not incentive
		assert!(staker_reward_for(bob, &events).is_some());
		assert!(incentive_paid_for(bob, &events).is_none());
	});
}

#[test]
fn nominator_reward_is_proportional_to_staker_budget() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Validator with nominator, validator incentive enabled
		let alice = 11; // validator
		let bob = 101; // nominator

		setup_incentive_with_budget(40, 10);

		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		// WHEN: Rewards distributed
		make_all_reward_payment(1);
		let events = staking_events_since_last_call();

		// THEN: Both receive rewards, validator gets more (higher stake = 1000 vs 500)
		let nominator_reward =
			staker_reward_for(bob, &events).expect("Nominator should receive reward");
		let validator_reward =
			staker_reward_for(alice, &events).expect("Validator should receive reward");

		assert!(
			validator_reward > nominator_reward,
			"Validator ({validator_reward}) should earn more than nominator ({nominator_reward})"
		);
	});
}

#[test]
fn multiple_validators_share_incentive_pot_correctly() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Two validators with equal reward points
		let alice = 11; // validator
		let bob = 21; // validator

		setup_incentive_with_budget(45, 5);

		Eras::<Test>::reward_active_era(vec![(alice, 1), (bob, 1)]);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (bob, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let pot_snapshot = ErasValidatorIncentiveAllocation::<Test>::get(2);
		assert!(pot_snapshot > 0);

		let alice_weight = ErasValidatorIncentive::<Test>::get(2, alice).unwrap();
		let bob_weight = ErasValidatorIncentive::<Test>::get(2, bob).unwrap();
		let total_weight = ErasTotalValidatorWeight::<Test>::get(2);

		let alice_expected_share =
			Perbill::from_rational(alice_weight, total_weight).mul_floor(pot_snapshot);
		let bob_expected_share =
			Perbill::from_rational(bob_weight, total_weight).mul_floor(pot_snapshot);

		// WHEN: Both validators claim rewards
		make_all_reward_payment(2);

		// THEN: Pot is depleted and total matches expected
		let pot_account = <Test as Config>::EraPots::era_pot_account(
			2,
			EraPotType::ValidatorSelfStake,
		);
		let remaining = Balances::free_balance(&pot_account);
		assert_eq!(remaining, 0, "Pot should be empty, has {}", remaining);

		let total_claimed = pot_snapshot - remaining;
		let expected_total = alice_expected_share + bob_expected_share;

		// Rewards are always rounded down, so total_claimed <= expected_total
		assert!(
			total_claimed <= expected_total,
			"Total claimed ({}) should not exceed expected ({})",
			total_claimed,
			expected_total
		);
		let diff = expected_total - total_claimed;
		assert!(diff < 5, "Rounding dust too large: {}", diff);
	});
}

#[test]
fn validator_incentive_prorated_across_pages() {
	// Verifies that validator incentive is prorated across multiple pages:
	// - ValidatorIncentivePaid event is emitted once per page
	// - Sum of all page incentives equals the validator's total share from the pot
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Validator with incentive enabled
		let alice = 11; // validator

		setup_incentive_with_budget(45, 5);

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let validator_weight = ErasValidatorIncentive::<Test>::get(2, alice).unwrap();
		let total_weight = ErasTotalValidatorWeight::<Test>::get(2);
		let pot_allocation = ErasValidatorIncentiveAllocation::<Test>::get(2);
		let validator_share = Perbill::from_rational(validator_weight, total_weight);
		let expected_total_incentive = validator_share.mul_floor(pot_allocation);

		// WHEN: All pages paid out
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: Validator receives ValidatorIncentivePaid events (one per page)
		let incentive_events: Vec<Balance> = events
			.iter()
			.filter_map(|e| match e {
				Event::ValidatorIncentivePaid { validator_stash, amount, .. }
					if *validator_stash == alice =>
				{
					Some(*amount)
				},
				_ => None,
			})
			.collect();

		// Sum of all page incentives should equal expected total (within rounding error)
		let total_incentive_paid: Balance = incentive_events.iter().sum();
		assert!(
			total_incentive_paid <= expected_total_incentive,
			"Total incentive paid ({}) should not exceed expected ({})",
			total_incentive_paid,
			expected_total_incentive
		);
		let diff = expected_total_incentive - total_incentive_paid;
		assert!(diff < 5, "Rounding dust too large: {}", diff);
	});
}

#[test]
fn validator_with_zero_reward_points_no_payout_triggered() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Alice and Bob are validators, but only Bob has reward points
		let alice = 11; // validator
		let bob = 21; // validator

		setup_incentive_with_budget(45, 5);

		Eras::<Test>::reward_active_era(vec![(bob, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		// WHEN: Payouts triggered
		make_all_reward_payment(1);
		let events = staking_events_since_last_call();

		// THEN: Alice (no reward points) gets nothing
		assert!(staker_reward_for(alice, &events).is_none());
		assert!(incentive_paid_for(alice, &events).is_none());

		// Bob (has reward points) gets staker reward
		assert!(staker_reward_for(bob, &events).is_some());

		// Both have self-stake weight allocated, but only Bob's is paid out
		assert!(ErasValidatorIncentive::<Test>::get(1, alice).is_some());
		assert!(ErasValidatorIncentive::<Test>::get(1, bob).is_some());
	});
}

#[test]
fn changing_budget_allocation_affects_rewards() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Era 1 with no validator incentive
		let alice = 11; // validator

		// Era 1: 50% staker, 0% incentive
		setup_incentive_with_budget(50, 0);

		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(1);
		let era1_events = staking_events_since_last_call();

		assert!(staker_reward_for(alice, &era1_events).is_some());
		assert!(incentive_paid_for(alice, &era1_events).is_none());

		// WHEN: Era 2 with validator incentive enabled (40% staker, 10% incentive)
		setup_incentive_with_budget(40, 10);

		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(2);
		let era2_events = staking_events_since_last_call();

		// THEN: Era 2 has both staker and incentive rewards
		assert!(staker_reward_for(alice, &era2_events).is_some());
		assert!(incentive_paid_for(alice, &era2_events).is_some());

		assert_eq!(ErasValidatorIncentiveAllocation::<Test>::get(1), 0);
		assert!(ErasValidatorIncentiveAllocation::<Test>::get(2) > 0);
	});
}

#[test]
fn lowering_nominator_rewards_via_budget_adjustment() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator
		let bob = 101; // nominator

		// GIVEN: Era 1 baseline with 45% staker rewards
		setup_incentive_with_budget(45, 0);

		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(1);
		let era1_events = staking_events_since_last_call();
		let bob_reward_era1 =
			staker_reward_for(bob, &era1_events).expect("Bob should receive reward");

		// WHEN: Era 2 with reduced staker budget (30% staker, 15% incentive)
		setup_incentive_with_budget(30, 15);

		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(2);
		let era2_events = staking_events_since_last_call();
		let bob_reward_era2 =
			staker_reward_for(bob, &era2_events).expect("Bob should receive reward");

		// THEN: Staker budget 45% -> 30% is a 33% reduction. Check at least 30% decrease.
		assert!(bob_reward_era2 < bob_reward_era1);
		let decrease_pct =
			Perbill::from_rational(bob_reward_era1 - bob_reward_era2, bob_reward_era1);
		assert!(decrease_pct >= Perbill::from_percent(30));
	});
}

#[test]
fn extreme_budget_scenarios_validator_heavy() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Very high validator incentive (40%), low staker budget (10%)
		let alice = 11; // validator
		let bob = 101; // nominator

		setup_incentive_with_budget(10, 40);

		// Era 2 has validator weights set by election
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		// WHEN: Rewards distributed for era 2
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: Incentive (40% budget) should exceed staker reward (10% budget)
		let alice_staker_reward =
			staker_reward_for(alice, &events).expect("Alice should receive staker reward");
		let bob_staker_reward =
			staker_reward_for(bob, &events).expect("Bob should receive staker reward");
		let alice_incentive =
			incentive_paid_for(alice, &events).expect("Alice should receive incentive");

		// Alice's total should be >2x Bob's (who only gets staker reward)
		let alice_total = alice_staker_reward + alice_incentive;
		assert!(
			alice_total > bob_staker_reward * 2,
			"Alice total ({alice_total}) should be >2x Bob's staker reward ({bob_staker_reward})"
		);

		// 40% incentive budget vs 10% staker budget
		assert!(
			alice_incentive > alice_staker_reward,
			"Incentive ({alice_incentive}) should exceed staker reward ({alice_staker_reward})"
		);
	});
}

#[test]
fn nominator_apy_decreases_as_validator_incentive_increases() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator
		let bob = 101; // nominator

		// Scenario 1: 50% staker, 0% incentive
		setup_incentive_with_budget(50, 0);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();
		make_all_reward_payment(1);
		let events1 = staking_events_since_last_call();
		let bob_s1 = staker_reward_for(bob, &events1).expect("Bob should receive reward");

		// Scenario 2: 40% staker, 10% incentive
		setup_incentive_with_budget(40, 10);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();
		make_all_reward_payment(2);
		let events2 = staking_events_since_last_call();
		let bob_s2 = staker_reward_for(bob, &events2).expect("Bob should receive reward");

		// Scenario 3: 25% staker, 25% incentive
		setup_incentive_with_budget(25, 25);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(4);
		let _ = staking_events_since_last_call();
		make_all_reward_payment(3);
		let events3 = staking_events_since_last_call();
		let bob_s3 = staker_reward_for(bob, &events3).expect("Bob should receive reward");

		// THEN: Nominator rewards decrease as validator incentive increases
		assert!(bob_s1 > bob_s2, "S1: {bob_s1}, S2: {bob_s2}");
		assert!(bob_s2 > bob_s3, "S2: {bob_s2}, S3: {bob_s3}");

		// 50% -> 25% staker budget means ~2x reduction in nominator reward
		let ratio = bob_s1 as f64 / bob_s3 as f64;
		assert!(ratio > 1.5, "Ratio: {ratio}");
	});
}

// ===== Tests for RewardDestination variants =====

#[test]
fn validator_incentive_with_account_destination() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Validator with RewardDestination::Account(other)
		let alice = 11; // validator
		let reward_account = 999; // custom reward account

		assert_ok!(Staking::set_payee(
			RuntimeOrigin::signed(alice),
			RewardDestination::Account(reward_account)
		));

		setup_incentive_with_budget(45, 5);

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let reward_account_balance_before = asset::total_balance::<Test>(&reward_account);

		// WHEN: Payout rewards
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: Incentive event records the custom account destination
		let (incentive, dest) = incentive_paid_details(alice, &events)
			.expect("Validator should receive incentive");
		assert_eq!(dest, RewardDestination::Account(reward_account));

		// Custom account receives both staker rewards and validator incentive
		let reward_account_balance_after = asset::total_balance::<Test>(&reward_account);
		let total_received = reward_account_balance_after - reward_account_balance_before;
		assert!(
			total_received >= incentive,
			"Custom reward account should receive at least the incentive ({incentive}), got {total_received}",
		);
	});
}

#[test]
fn validator_incentive_with_staked_destination() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Validator with RewardDestination::Staked
		let alice = 11; // validator

		assert_ok!(Staking::set_payee(RuntimeOrigin::signed(alice), RewardDestination::Staked));
		setup_incentive_with_budget(45, 5);

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let alice_balance_before = asset::total_balance::<Test>(&alice);

		// WHEN: Payout rewards
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: Incentive event records Staked destination and balance increases
		let (incentive, dest) =
			incentive_paid_details(alice, &events).expect("Validator should receive incentive");
		assert_eq!(dest, RewardDestination::Staked);
		assert!(incentive > 0, "Incentive amount should be non-zero");

		let alice_balance_after = asset::total_balance::<Test>(&alice);
		assert!(
			alice_balance_after > alice_balance_before,
			"Alice balance should increase from incentive payout"
		);
	});
}

// ===== Tests for edge cases =====

#[test]
fn validator_chills_before_payout() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Validator earns weight in era 2, then chills before payout
		let alice = 11; // validator

		setup_incentive_with_budget(45, 5);

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		assert!(ErasValidatorIncentive::<Test>::get(2, alice).is_some());

		// WHEN: Alice chills before claiming payout
		assert_ok!(Staking::chill(RuntimeOrigin::signed(alice)));
		assert!(!Validators::<Test>::contains_key(&alice));

		// THEN: Alice can still claim incentive for era 2
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		assert!(
			incentive_paid_for(alice, &events).is_some(),
			"Chilled validator should still receive incentive for era they were active"
		);
	});
}

#[test]
fn all_validators_zero_reward_points() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Validators with self-stake weight but zero reward points
		setup_incentive_with_budget(45, 5);

		// Don't call reward_active_era — no reward points
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		// WHEN: Try to claim payouts
		make_all_reward_payment(1);
		let events = staking_events_since_last_call();

		// THEN: No incentive paid
		let any_incentive =
			events.iter().any(|e| matches!(e, Event::ValidatorIncentivePaid { .. }));
		assert!(!any_incentive, "No incentive when no validators earned reward points");
	});
}

#[test]
fn validator_payee_changes_before_payout() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Validator with payee set to old_account during weight calculation
		let alice = 11; // validator
		let old_account = 888;
		let new_account = 999;

		assert_ok!(Staking::set_payee(
			RuntimeOrigin::signed(alice),
			RewardDestination::Account(old_account)
		));

		setup_incentive_with_budget(45, 5);

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		// WHEN: Payee changes to new_account before payout
		assert_ok!(Staking::set_payee(
			RuntimeOrigin::signed(alice),
			RewardDestination::Account(new_account)
		));

		let old_balance_before = asset::total_balance::<Test>(&old_account);
		let new_balance_before = asset::total_balance::<Test>(&new_account);

		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: Incentive uses payee at payout time (new_account)
		let (incentive, dest) =
			incentive_paid_details(alice, &events).expect("Validator should receive incentive");
		assert_eq!(dest, RewardDestination::Account(new_account));

		assert_eq!(asset::total_balance::<Test>(&old_account), old_balance_before);
		let new_balance_after = asset::total_balance::<Test>(&new_account);
		assert!(
			new_balance_after - new_balance_before >= incentive,
			"New account should receive at least the incentive amount"
		);
	});
}

#[test]
fn very_small_self_stake_weight() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Validator with self-stake of 1000 (from mock), below optimum of 30_000.
		// w(1000) = √1000 ≈ 31, so weight is small but non-zero.
		let alice = 11; // validator

		setup_incentive_with_budget(45, 5);

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		// Verify weight was calculated (√1000 ≈ 31)
		let weight = ErasValidatorIncentive::<Test>::get(2, alice).unwrap();
		assert_eq!(weight, 31);

		// WHEN: Payout
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: Incentive is paid (small but non-zero)
		assert!(incentive_paid_for(alice, &events).is_some());
	});
}

/// Finds the held incentive amount from events.
fn incentive_held_for(stash: AccountId, events: &[Event<Test>]) -> Option<Balance> {
	events.iter().find_map(|e| match e {
		Event::ValidatorIncentiveHeld { validator_stash, amount, .. }
			if *validator_stash == stash =>
		{
			Some(*amount)
		},
		_ => None,
	})
}

/// Finds vesting conversion details from events.
fn vesting_converted_for(stash: AccountId, events: &[Event<Test>]) -> Option<(Balance, Balance)> {
	events.iter().find_map(|e| match e {
		Event::IncentiveVestingConverted { validator_stash, liquid, vested, .. }
			if *validator_stash == stash =>
		{
			Some((*liquid, *vested))
		},
		_ => None,
	})
}

#[test]
fn incentive_held_when_vesting_enabled() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Vesting enabled, non-batch-boundary era
		let alice = 11; // validator
		setup_vesting_params(150, 5);
		setup_incentive_with_budget(45, 5);

		// Advance to era 2 so validator weights are set by election
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let alice_balance_before = asset::total_balance::<Test>(&alice);
		assert_eq!(asset::incentive_held::<Test>(&alice), 0);

		// WHEN: Payout era 2 (not a batch boundary: 2 % 3 != 0)
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: Incentive is held, not paid liquid
		let held_amount = incentive_held_for(alice, &events).expect("should emit held event");
		assert!(held_amount > 0);
		assert_eq!(asset::incentive_held::<Test>(&alice), held_amount);

		// Balance increased (transfer from pot) but part is held
		let alice_balance_after = asset::total_balance::<Test>(&alice);
		assert!(alice_balance_after > alice_balance_before);

		// No ValidatorIncentivePaid event (that's for liquid path)
		assert!(incentive_paid_for(alice, &events).is_none());
		// No conversion event (not a batch boundary)
		assert!(vesting_converted_for(alice, &events).is_none());
	});
}

#[test]
fn incentive_accumulates_and_converts_at_batch_boundary() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Vesting enabled with BondingDuration = 3
		// Batch boundaries at eras 3, 6, 9, ...
		let alice = 11; // validator
		setup_vesting_params(150, 5);
		setup_incentive_with_budget(45, 5);

		// Advance through eras 2-6, rewarding each. Era 6 is a batch boundary (6 % 3 == 0).
		for target_era in 2..=6 {
			Session::roll_until_active_era(target_era);
			Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		}
		Session::roll_until_active_era(7);
		let _ = staking_events_since_last_call();

		// Pay eras 2-5, hold accumulates (non-boundary eras for 2,4,5; era 3 is boundary
		// but we claim it with pages left from other eras still unclaimed)
		make_all_reward_payment(2);
		make_all_reward_payment(4);
		make_all_reward_payment(5);
		// Also pay era 3 — batch boundary but hold already accumulated from era 2
		make_all_reward_payment(3);

		// After era 3 payout, conversion should have triggered for eras 2+3 incentive.
		// Remaining from eras 4+5 is still held. Flush events before era 6 payout.
		let _ = staking_events_since_last_call();

		// WHEN: Payout era 6 (batch boundary, 6 % 3 == 0)
		make_all_reward_payment(6);
		let events = staking_events_since_last_call();

		// THEN: Conversion triggered for eras 4+5+6 incentive
		let (liquid, vested) = vesting_converted_for(alice, &events)
			.expect("Should emit IncentiveVestingConverted at era 6");

		// Retroactive unlock = 30% (BondingDuration=3 / vesting_eras=10)
		let total_converted = liquid + vested;
		let expected_liquid = Perbill::from_rational(3u32, 10u32).mul_floor(total_converted);
		assert_eq!(liquid, expected_liquid);
		assert_eq!(vested, total_converted - expected_liquid);

		// Hold should be fully released after conversion
		assert_eq!(asset::incentive_held::<Test>(&alice), 0);
	});
}

#[test]
fn vesting_duration_zero_pays_liquid() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: VestingDuration = 0 (default), so no hold, pay liquid directly.
		let alice = 11; // validator
		setup_incentive_with_budget(45, 5);

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		// WHEN: Payout era 2
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: Paid liquid via ValidatorIncentivePaid, no hold
		assert!(incentive_paid_for(alice, &events).is_some());
		assert_eq!(asset::incentive_held::<Test>(&alice), 0);
		assert!(vesting_converted_for(alice, &events).is_none());
	});
}

#[test]
fn multiple_batch_cycles() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Vesting enabled, run through two batch cycles
		let alice = 11; // validator
		setup_vesting_params(150, 5);
		setup_incentive_with_budget(45, 5);

		// First cycle: eras 2-3. Era 3 is first batch boundary (3 % 3 == 0).
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(4);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(2);
		make_all_reward_payment(3);
		let events1 = staking_events_since_last_call();

		let (liquid1, vested1) =
			vesting_converted_for(alice, &events1).expect("Should convert at era 3");
		assert!(liquid1 > 0);
		assert!(vested1 > 0);
		assert_eq!(asset::incentive_held::<Test>(&alice), 0);

		// Second cycle: eras 4-6. Era 6 is second batch boundary.
		for target_era in 4..=6 {
			Session::roll_until_active_era(target_era);
			Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		}
		Session::roll_until_active_era(7);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(4);
		make_all_reward_payment(5);
		make_all_reward_payment(6);
		let events2 = staking_events_since_last_call();

		// THEN: Second conversion also happens at era 6
		let (liquid2, vested2) =
			vesting_converted_for(alice, &events2).expect("Should convert at era 6");
		assert!(liquid2 > 0);
		assert!(vested2 > 0);
		assert_eq!(asset::incentive_held::<Test>(&alice), 0);

		// Both cycles apply the same 30% formula: liquid = floor(held * 3/10)
		let total1 = liquid1 + vested1;
		let total2 = liquid2 + vested2;
		let expected_liquid1 = Perbill::from_rational(3u32, 10u32).mul_floor(total1);
		let expected_liquid2 = Perbill::from_rational(3u32, 10u32).mul_floor(total2);
		assert_eq!(liquid1, expected_liquid1);
		assert_eq!(liquid2, expected_liquid2);
	});
}

#[test]
fn withdraw_unbonded_settles_incentive_hold() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Validator with incentive held after payout.
		let alice = 11; // validator
		let reward_account = 999;
		setup_vesting_params(150, 5);
		setup_incentive_with_budget(45, 5);

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		make_all_reward_payment(2);

		let held = asset::incentive_held::<Test>(&alice);
		assert!(held > 0, "incentive should be held after payout");

		// Helper: unbond and withdraw alice.
		let unbond_and_withdraw = || {
			assert_ok!(Staking::chill(RuntimeOrigin::signed(alice)));
			let bonded = Staking::ledger(11.into()).unwrap().active;
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(alice), bonded));
			Session::roll_until_active_era(3 + BondingDuration::get() + 1);
			let _ = staking_events_since_last_call();
			assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(alice), 0));
			staking_events_since_last_call()
		};

		// WHEN/THEN: Payee = Stash — full held amount vested on exit (no retroactive unlock).
		hypothetically!({
			let events = unbond_and_withdraw();
			assert!(Staking::ledger(11.into()).is_err());
			assert_eq!(asset::incentive_held::<Test>(&alice), 0);
			let (liquid, vested) =
				vesting_converted_for(alice, &events).expect("should emit conversion event");
			assert_eq!(liquid, 0, "no retroactive unlock on exit");
			assert_eq!(vested, held, "full amount vested on exit");
		});

		// WHEN/THEN: Payee = Account(other) — hold on reward_account is settled.
		let ed = asset::existential_deposit::<Test>();
		<Balances as frame_support::traits::fungible::Mutate<_>>::mint_into(&reward_account, ed)
			.unwrap();
		assert_ok!(Staking::set_payee(
			RuntimeOrigin::signed(alice),
			RewardDestination::Account(reward_account)
		));
		// Hold migrated to reward_account by set_payee.
		assert_eq!(asset::incentive_held::<Test>(&reward_account), held);

		let _events = unbond_and_withdraw();
		assert!(Staking::ledger(11.into()).is_err());
		assert_eq!(asset::incentive_held::<Test>(&reward_account), 0);
	});
}

#[test]
fn vesting_duration_in_eras_equals_bonding_releases_all_liquid() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: vesting_eras == BondingDuration → 100% unlocked at conversion
		let alice = 11; // validator
		VestingDurationBlocks::set(45); // 3 eras * 15 blocks/era
		BlocksPerSession::set(5); // vesting_eras = 45 / (5*3) = 3, same as BondingDuration
		setup_incentive_with_budget(45, 5);

		// Advance to era 2+ for validator weights, accumulate through era 3 (batch boundary)
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(4);
		let _ = staking_events_since_last_call();

		// WHEN: Payout era 2 and 3 (era 3 = batch boundary, vesting_eras == BondingDuration)
		make_all_reward_payment(2);
		make_all_reward_payment(3);
		let events = staking_events_since_last_call();

		// THEN: Everything released liquid (no vesting schedule created)
		let converted = events.iter().find_map(|e| match e {
			Event::IncentiveVestingConverted { validator_stash, liquid, vested }
				if *validator_stash == alice =>
			{
				Some((*liquid, *vested))
			},
			_ => None,
		});
		assert!(
			converted.is_some(),
			"Should release all liquid when vesting_eras == BondingDuration"
		);
		let (liquid, vested) = converted.unwrap();
		assert!(liquid > 0, "liquid portion should be non-zero");
		assert_eq!(vested, 0, "vested should be zero when vesting_eras == BondingDuration");
		assert_eq!(asset::incentive_held::<Test>(&alice), 0);
	});
}

#[test]
fn payee_change_migrates_incentive_hold() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Validator with incentive held on Account(other).
		let alice = 11; // validator
		let other = 888;
		let another = 999;
		setup_vesting_params(150, 5);
		setup_incentive_with_budget(45, 5);

		let ed = asset::existential_deposit::<Test>();
		<Balances as frame_support::traits::fungible::Mutate<_>>::mint_into(&other, ed).unwrap();
		<Balances as frame_support::traits::fungible::Mutate<_>>::mint_into(&another, ed).unwrap();

		assert_ok!(Staking::set_payee(
			RuntimeOrigin::signed(alice),
			RewardDestination::Account(other)
		));

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		make_all_reward_payment(2);

		let held = asset::incentive_held::<Test>(&other);
		assert!(held > 0);

		// WHEN/THEN: Account(other) → Account(another) migrates the hold.
		hypothetically!({
			assert_ok!(Staking::set_payee(
				RuntimeOrigin::signed(alice),
				RewardDestination::Account(another)
			));
			assert_eq!(asset::incentive_held::<Test>(&other), 0);
			assert_eq!(asset::incentive_held::<Test>(&another), held);
		});

		// WHEN/THEN: Account(other) → Stash migrates hold to stash (which has a staking hold too).
		hypothetically!({
			assert_ok!(Staking::set_payee(RuntimeOrigin::signed(alice), RewardDestination::Stash));
			assert_eq!(asset::incentive_held::<Test>(&other), 0);
			assert_eq!(asset::incentive_held::<Test>(&alice), held);
		});

		// WHEN/THEN: Account(other) → None blocked while hold exists.
		assert_noop!(
			Staking::set_payee(RuntimeOrigin::signed(alice), RewardDestination::None),
			Error::<Test>::IncentiveVestingPending
		);
	});
}

#[test]
fn force_unstake_settles_incentive_hold() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Validator with held incentive.
		let alice = 11;
		setup_vesting_params(150, 5);
		setup_incentive_with_budget(45, 5);

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		make_all_reward_payment(2);

		let held = asset::incentive_held::<Test>(&alice);
		assert!(held > 0);
		let balance_before = asset::total_balance::<Test>(&alice);

		// WHEN: Root force-unstakes.
		assert_ok!(Staking::force_unstake(RuntimeOrigin::root(), alice, 0));

		// THEN: Hold cleared, stash killed, balance preserved.
		assert_eq!(asset::incentive_held::<Test>(&alice), 0);
		assert!(Staking::ledger(11.into()).is_err());
		assert!(asset::total_balance::<Test>(&alice) >= balance_before);
	});
}
