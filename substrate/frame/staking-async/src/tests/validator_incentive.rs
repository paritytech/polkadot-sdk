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
use crate::session_rotation::Eras;

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
		let alice = 11;
		// nominator
		let bob = 101;

		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		let budget = pallet_dap::BudgetConfig {
			staker_rewards: Perbill::from_percent(45),
			validator_self_stake_incentive: Perbill::from_percent(5),
			buffer: Perbill::from_percent(50),
		};
		pallet_dap::BudgetAllocation::<Test>::put(budget);

		// Era 2 has validator weights set by election
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let alice_balance_before = asset::total_balance::<Test>(&alice);

		// WHEN: Payout rewards
		mock::make_all_reward_payment(2);

		// THEN: Validator receives both staker reward and incentive bonus
		let events = staking_events_since_last_call();

		let staker_reward_amount = events
			.iter()
			.find_map(|e| match e {
				Event::Rewarded { stash, amount, .. } if *stash == alice => Some(*amount),
				_ => None,
			})
			.expect("Validator should receive staker reward");

		let incentive_amount = events
			.iter()
			.find_map(|e| match e {
				Event::ValidatorIncentivePaid { validator_stash, amount, .. }
					if *validator_stash == alice =>
				{
					Some(*amount)
				},
				_ => None,
			})
			.expect("Validator should receive incentive bonus");

		assert!(staker_reward_amount > 0);
		assert!(incentive_amount > 0);

		// alice balance increased by correct amount
		let alice_balance_after = asset::total_balance::<Test>(&alice);
		assert_eq!(
			alice_balance_after - alice_balance_before,
			staker_reward_amount + incentive_amount
		);

		// Bob (the nominator) also received staker reward
		assert!(events
			.iter()
			.any(|e| matches!(e, Event::Rewarded { stash, .. } if *stash == bob)));

		// But Bob did not receive any incentive reward
		assert!(
			!events.iter().any(
				|e| matches!(e, Event::ValidatorIncentivePaid { validator_stash, .. } if *validator_stash == bob)
			),
			"Nominator should not receive validator incentive"
		);
	});
}

#[test]
fn nominator_reward_is_proportional_to_staker_budget() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Validator with nominator, validator incentive enabled
		let alice = 11; // validator
		let bob = 101; // nominator

		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		let budget_with_incentive = pallet_dap::BudgetConfig {
			staker_rewards: Perbill::from_percent(40),
			validator_self_stake_incentive: Perbill::from_percent(10),
			buffer: Perbill::from_percent(50),
		};
		pallet_dap::BudgetAllocation::<Test>::put(budget_with_incentive);

		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		// WHEN: Rewards distributed
		make_all_reward_payment(1);
		let events = staking_events_since_last_call();

		// THEN: Both receive rewards, validator gets more
		let nominator_reward = events
			.iter()
			.find_map(|e| match e {
				Event::Rewarded { stash, amount, .. } if *stash == bob => Some(*amount),
				_ => None,
			})
			.expect("Nominator should receive reward");

		let validator_reward = events
			.iter()
			.find_map(|e| match e {
				Event::Rewarded { stash, amount, .. } if *stash == alice => Some(*amount),
				_ => None,
			})
			.expect("Validator should receive reward");

		assert!(nominator_reward > 0);
		assert!(validator_reward > 0);
		assert!(validator_reward > nominator_reward);
	});
}

#[test]
fn multiple_validators_share_incentive_pot_correctly() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Two validators with equal reward points
		let alice = 11; // validator
		let bob = 21; // validator

		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		let budget_with_incentive = pallet_dap::BudgetConfig {
			staker_rewards: Perbill::from_percent(45),
			validator_self_stake_incentive: Perbill::from_percent(5),
			buffer: Perbill::from_percent(50),
		};
		pallet_dap::BudgetAllocation::<Test>::put(budget_with_incentive);

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
		let pot_account = <Test as Config>::EraPotAccountProvider::era_pot_account(
			2,
			EraPotType::ValidatorSelfStake,
		);
		let remaining = Balances::free_balance(&pot_account);
		assert_eq!(remaining, 0, "Pot should be empty, has {}", remaining);

		let total_claimed = pot_snapshot - remaining;
		let expected_total = alice_expected_share + bob_expected_share;

		// CLAUDE: We always round down reward right? So total claimed should never be
		// greater than expected total.
		let diff = if total_claimed > expected_total {
			total_claimed - expected_total
		} else {
			expected_total - total_claimed
		};
		assert!(
			diff < 5,
			"Total claimed ({}) vs expected ({}), diff: {}",
			total_claimed,
			expected_total,
			diff
		);
	});
}

#[test]
// CLAUDE: we should check this together in one of the above test, don't think we need a separate
// one for it. Wdyt? Remove if you agree.
fn validator_incentive_only_paid_to_validators() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Validator with nominator, incentive budget enabled
		let alice = 11;
		let bob = 101;

		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		let budget_with_incentive = pallet_dap::BudgetConfig {
			staker_rewards: Perbill::from_percent(45),
			validator_self_stake_incentive: Perbill::from_percent(5),
			buffer: Perbill::from_percent(50),
		};
		pallet_dap::BudgetAllocation::<Test>::put(budget_with_incentive);

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let validator_weight = ErasValidatorIncentive::<Test>::get(2, alice).unwrap();
		let total_weight = ErasTotalValidatorWeight::<Test>::get(2);
		let pot_allocation = ErasValidatorIncentiveAllocation::<Test>::get(2);
		let validator_share = Perbill::from_rational(validator_weight, total_weight);
		let expected_incentive = validator_share.mul_floor(pot_allocation);

		// WHEN: All pages paid out
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: Validator receives exactly ONE ValidatorIncentivePaid event
		// (regardless of how many pages of Rewarded events there are)
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

		assert_eq!(incentive_events.len(), 1, "Should have exactly 1 ValidatorIncentivePaid event");
		assert_eq!(incentive_events[0], expected_incentive);
	});
}

#[test]
fn validator_with_zero_reward_points_no_payout_triggered() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Alice and Bob are validators with self stake weight but only Bob has reward points
		let alice = 11; // validator
		let bob = 21; // validator

		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		let budget_with_incentive = pallet_dap::BudgetConfig {
			staker_rewards: Perbill::from_percent(45),
			validator_self_stake_incentive: Perbill::from_percent(5),
			buffer: Perbill::from_percent(50),
		};
		pallet_dap::BudgetAllocation::<Test>::put(budget_with_incentive);

		Eras::<Test>::reward_active_era(vec![(bob, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		// WHEN: Payouts triggered
		make_all_reward_payment(1);
		let events = staking_events_since_last_call();

		// THEN: Only Bob gets payout (Alice has no reward points)
		let alice_reward = events.iter().find_map(|e| match e {
			Event::Rewarded { stash, amount, .. } if *stash == alice => Some(*amount),
			_ => None,
		});

		let bob_reward = events
			.iter()
			.find_map(|e| match e {
				Event::Rewarded { stash, amount, .. } if *stash == bob => Some(*amount),
				_ => None,
			})
			.expect("Bob should receive reward");

		assert!(alice_reward.is_none(), "Alice should not receive staker reward");
		assert!(bob_reward > 0);

		// Alice should not receive validator incentive either
		let alice_incentive = events.iter().find_map(|e| match e {
			Event::ValidatorIncentivePaid { validator_stash, amount, .. }
				if *validator_stash == alice =>
			{
				Some(*amount)
			},
			_ => None,
		});
		assert!(alice_incentive.is_none(), "Alice should not receive validator incentive");

		// Verify Bob receives validator incentive
		let bob_incentive_events: Vec<_> = events
			.iter()
			.filter(|e| {
				matches!(e, Event::ValidatorIncentivePaid { validator_stash, .. } if *validator_stash == bob)
			})
			.collect();

		// Check if any validator got incentive at all
		let any_incentive =
			events.iter().any(|e| matches!(e, Event::ValidatorIncentivePaid { .. }));

		if any_incentive {
			assert!(
				!bob_incentive_events.is_empty(),
				"Bob should receive validator incentive when he has reward points"
			);
		}

		// Both have self stake weight allocated, but only Bob's is paid out
		let alice_weight = ErasValidatorIncentive::<Test>::get(1, alice);
		let bob_weight = ErasValidatorIncentive::<Test>::get(1, bob);
		assert!(alice_weight.is_some());
		assert!(bob_weight.is_some());
	});
}

#[test]
fn changing_budget_allocation_affects_rewards() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Era 1 with no validator incentive
		let alice = 11; // validator

		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		let budget_era1 = pallet_dap::BudgetConfig {
			staker_rewards: Perbill::from_percent(50),
			validator_self_stake_incentive: Perbill::from_percent(0),
			buffer: Perbill::from_percent(50),
		};
		pallet_dap::BudgetAllocation::<Test>::put(budget_era1);

		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(1);
		let era1_events = staking_events_since_last_call();
		let alice_reward_era1 = era1_events
			.iter()
			.find_map(|e| match e {
				Event::Rewarded { stash, amount, .. } if *stash == alice => Some(*amount),
				_ => None,
			})
			.expect("Alice should receive reward in era 1");

		// Era 1 should have no incentive event
		let era1_incentive = era1_events
			.iter()
			.find(|e| matches!(e, Event::ValidatorIncentivePaid { era, .. } if *era == 1));
		assert!(era1_incentive.is_none(), "Era 1 should not emit ValidatorIncentivePaid");

		// WHEN: Era 2 with validator incentive enabled
		let budget_era2 = pallet_dap::BudgetConfig {
			staker_rewards: Perbill::from_percent(40),
			validator_self_stake_incentive: Perbill::from_percent(10),
			buffer: Perbill::from_percent(50),
		};
		pallet_dap::BudgetAllocation::<Test>::put(budget_era2);

		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(2);
		let era2_events = staking_events_since_last_call();
		let alice_reward_era2 = era2_events
			.iter()
			.find_map(|e| match e {
				Event::Rewarded { stash, amount, .. } if *stash == alice => Some(*amount),
				_ => None,
			})
			.expect("Alice should receive reward in era 2");

		// Era 2 should have incentive event
		let era2_incentive = era2_events
			.iter()
			.find_map(|e| match e {
				Event::ValidatorIncentivePaid { era, validator_stash, amount, .. }
					if *era == 2 && *validator_stash == alice =>
				{
					Some(*amount)
				},
				_ => None,
			})
			.expect("Era 2 should emit ValidatorIncentivePaid for Alice");

		// THEN: Budget change reflected in allocations and events
		assert!(alice_reward_era1 > 0);
		assert!(alice_reward_era2 > 0);
		assert!(era2_incentive > 0, "Era 2 incentive should be non-zero");

		let era1_validator_allocation = ErasValidatorIncentiveAllocation::<Test>::get(1);
		let era2_validator_allocation = ErasValidatorIncentiveAllocation::<Test>::get(2);

		assert_eq!(era1_validator_allocation, 0, "Era 1 should have 0% validator incentive");
		assert!(era2_validator_allocation > 0, "Era 2 should have 10% validator incentive");
	});
}

#[test]
fn lowering_nominator_rewards_via_budget_adjustment() {
	// Tests that reducing staker budget allocation reduces nominator rewards
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator
		let bob = 101; // nominator

		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		// Era 1: baseline with 45% staker rewards
		let budget_baseline = pallet_dap::BudgetConfig {
			staker_rewards: Perbill::from_percent(45),
			validator_self_stake_incentive: Perbill::from_percent(0),
			buffer: Perbill::from_percent(55),
		};
		pallet_dap::BudgetAllocation::<Test>::put(budget_baseline);

		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(1);
		let bob_reward_era1 = staking_events_since_last_call()
			.iter()
			.find_map(|e| match e {
				Event::Rewarded { stash, amount, .. } if *stash == bob => Some(*amount),
				_ => None,
			})
			.expect("Bob should receive reward");

		// Era 2: reduced staker budget to 30%
		let budget_reduced = pallet_dap::BudgetConfig {
			staker_rewards: Perbill::from_percent(30),
			validator_self_stake_incentive: Perbill::from_percent(15),
			buffer: Perbill::from_percent(55),
		};
		pallet_dap::BudgetAllocation::<Test>::put(budget_reduced);

		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		mock::make_all_reward_payment(2);
		let bob_reward_era2 = staking_events_since_last_call()
			.iter()
			.find_map(|e| match e {
				Event::Rewarded { stash, amount, .. } if *stash == bob => Some(*amount),
				_ => None,
			})
			.expect("Bob should receive reward");

		assert!(bob_reward_era2 < bob_reward_era1);

		// Staker budget decreased from 45% to 30%, which is a 33% reduction (15/45)
		// We check for at least 30% decrease to account for rounding and other factors
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

		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		let extreme_budget = pallet_dap::BudgetConfig {
			staker_rewards: Perbill::from_percent(10),
			validator_self_stake_incentive: Perbill::from_percent(40),
			buffer: Perbill::from_percent(50),
		};
		pallet_dap::BudgetAllocation::<Test>::put(extreme_budget);

		// Era 2 has validator weights set by election
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		// WHEN: Rewards distributed for era 2
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: Verify both staker rewards and validator incentive
		let alice_staker_reward = events
			.iter()
			.find_map(|e| match e {
				Event::Rewarded { stash, amount, .. } if *stash == alice => Some(*amount),
				_ => None,
			})
			.expect("Alice should receive staker reward");

		let bob_staker_reward = events
			.iter()
			.find_map(|e| match e {
				Event::Rewarded { stash, amount, .. } if *stash == bob => Some(*amount),
				_ => None,
			})
			.expect("Bob should receive staker reward");

		// Validator incentive pot is 4x the staker pot, so incentive should be significant
		let validator_allocation = ErasValidatorIncentiveAllocation::<Test>::get(2);
		assert!(validator_allocation > 0, "Validator allocation should be non-zero");

		let alice_validator_incentive_opt = events.iter().find_map(|e| match e {
			Event::ValidatorIncentivePaid { validator_stash, amount, .. }
				if *validator_stash == alice =>
			{
				Some(*amount)
			},
			_ => None,
		});

		// If the validator incentive pot was allocated, Alice should receive it
		if let Some(alice_validator_incentive) = alice_validator_incentive_opt {
			// Alice's total = staker_reward + validator_incentive
			// This total should be much more than Bob's (who only gets staker reward)
			let alice_total = alice_staker_reward + alice_validator_incentive;
			assert!(
				alice_total > bob_staker_reward * 2,
				"Alice total (staker: {} + incentive: {} = {}) should be >2x Bob's staker reward ({})",
				alice_staker_reward,
				alice_validator_incentive,
				alice_total,
				bob_staker_reward
			);

			// Verify the incentive is actually substantial relative to staker reward
			assert!(
				alice_validator_incentive > alice_staker_reward,
				"Validator incentive ({}) should exceed staker reward ({}) with 40% vs 10% budget",
				alice_validator_incentive,
				alice_staker_reward
			);
		} else {
			// If no validator incentive was paid despite allocation, that's a problem
			panic!(
				"Validator incentive was allocated ({}) but not paid. Events: {:?}",
				validator_allocation, events
			);
		}
	});
}

#[test]
fn nominator_apy_decreases_as_validator_incentive_increases() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: Three scenarios with increasing validator incentive
		let alice = 11; // validator
		let bob = 101; // nominator

		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		// WHEN: Scenario 1 - no validator incentive
		let budget1 = pallet_dap::BudgetConfig {
			staker_rewards: Perbill::from_percent(50),
			validator_self_stake_incentive: Perbill::from_percent(0),
			buffer: Perbill::from_percent(50),
		};
		pallet_dap::BudgetAllocation::<Test>::put(budget1);

		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();
		make_all_reward_payment(1);
		let bob_reward_scenario1 = staking_events_since_last_call()
			.iter()
			.find_map(|e| match e {
				Event::Rewarded { stash, amount, .. } if *stash == bob => Some(*amount),
				_ => None,
			})
			.expect("Bob should receive reward");

		// WHEN: Scenario 2 - moderate validator incentive
		let budget2 = pallet_dap::BudgetConfig {
			staker_rewards: Perbill::from_percent(40),
			validator_self_stake_incentive: Perbill::from_percent(10),
			buffer: Perbill::from_percent(50),
		};
		pallet_dap::BudgetAllocation::<Test>::put(budget2);

		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();
		mock::make_all_reward_payment(2);
		let bob_reward_scenario2 = staking_events_since_last_call()
			.iter()
			.find_map(|e| match e {
				Event::Rewarded { stash, amount, .. } if *stash == bob => Some(*amount),
				_ => None,
			})
			.expect("Bob should receive reward");

		// WHEN: Scenario 3 - high validator incentive
		let budget3 = pallet_dap::BudgetConfig {
			staker_rewards: Perbill::from_percent(25),
			validator_self_stake_incentive: Perbill::from_percent(25),
			buffer: Perbill::from_percent(50),
		};
		pallet_dap::BudgetAllocation::<Test>::put(budget3);

		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(4);
		let _ = staking_events_since_last_call();
		mock::make_all_reward_payment(3);
		let bob_reward_scenario3 = staking_events_since_last_call()
			.iter()
			.find_map(|e| match e {
				Event::Rewarded { stash, amount, .. } if *stash == bob => Some(*amount),
				_ => None,
			})
			.expect("Bob should receive reward");

		// THEN: Nominator rewards decrease as validator incentive increases
		assert!(
			bob_reward_scenario1 > bob_reward_scenario2,
			"S1: {}, S2: {}",
			bob_reward_scenario1,
			bob_reward_scenario2
		);
		assert!(
			bob_reward_scenario2 > bob_reward_scenario3,
			"S2: {}, S3: {}",
			bob_reward_scenario2,
			bob_reward_scenario3
		);

		let ratio = bob_reward_scenario1 as f64 / bob_reward_scenario3 as f64;
		assert!(ratio > 1.5, "Ratio: {}", ratio);
	});
}
