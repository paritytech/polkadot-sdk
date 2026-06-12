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

// ===== Config extrinsic tests =====

#[test]
fn config_set_noop_remove_works() {
	ExtBuilder::default().build_and_execute(|| {
		// WHEN: set all params.
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		// THEN: values stored and event emitted.
		assert_eq!(OptimumSelfStake::<Test>::get(), 30_000);
		assert_eq!(HardCapSelfStake::<Test>::get(), 100_000);
		assert_eq!(SelfStakeSlopeFactor::<Test>::get(), Perbill::from_rational(1u32, 2u32));
		assert!(staking_events_since_last_call().iter().any(|e| matches!(
			e,
			Event::ValidatorIncentiveConfigSet {
				optimum_self_stake: 30_000,
				hard_cap_self_stake: 100_000,
				slope_factor,
			} if *slope_factor == Perbill::from_rational(1u32, 2u32)
		)));

		// WHEN: noop, THEN: values remain the same
		assert_storage_noop!(assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Noop,
			ConfigOp::Noop,
			ConfigOp::Noop,
		)));

		// WHEN: remove all.
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Remove,
			ConfigOp::Remove,
			ConfigOp::Remove,
		));

		// THEN: storage cleared.
		assert!(!OptimumSelfStake::<Test>::exists());
		assert!(!HardCapSelfStake::<Test>::exists());
		assert!(!SelfStakeSlopeFactor::<Test>::exists());
	});
}

#[test]
fn config_requires_admin_origin() {
	ExtBuilder::default().build_and_execute(|| {
		let admin = 1; // as set in mock

		// WHEN: non-admin calls.
		assert_noop!(
			Staking::set_validator_self_stake_incentive_config(
				RuntimeOrigin::signed(2),
				ConfigOp::Set(30_000),
				ConfigOp::Set(100_000),
				ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
			),
			DispatchError::BadOrigin
		);

		// WHEN: admin calls.
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::signed(admin),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));
	});
}

#[test]
fn config_validates_optimum_le_cap() {
	ExtBuilder::default().build_and_execute(|| {
		// WHEN: optimum > cap → rejected.
		assert_noop!(
			Staking::set_validator_self_stake_incentive_config(
				RuntimeOrigin::root(),
				ConfigOp::Set(100_000),
				ConfigOp::Set(50_000),
				ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
			),
			Error::<Test>::OptimumGreaterThanCap
		);

		// WHEN: optimum == cap → accepted.
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(50_000),
			ConfigOp::Set(50_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));
	});
}

// ===== Reward distribution tests =====

#[test]
fn validator_receives_both_staker_and_incentive_rewards() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator
		let bob = 101; // nominator

		// GIVEN: incentive budget enabled (45% staker, 5% incentive).
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		// GIVEN: era pot starts with full snapshotted budget (nothing paid yet).
		let era_pot = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
			2,
			RewardKind::ValidatorSelfStake,
		));
		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		assert_eq!(Balances::free_balance(&era_pot), budget);

		// WHEN: payout.
		let alice_before = asset::total_balance::<Test>(&alice);
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: validator gets both staker reward + incentive bonus.
		let staker = staker_reward_for(alice, &events).expect("staker reward");
		let incentive = incentive_paid_for(alice, &events).expect("incentive bonus");
		assert_eq!(asset::total_balance::<Test>(&alice) - alice_before, staker + incentive);

		// THEN: nominator gets staker reward only (no incentive).
		// Bob (500 stake) gets less than alice (1000 stake) from staker rewards.
		let bob_reward = staker_reward_for(bob, &events).expect("nominator should receive reward");
		assert!(
			bob_reward < staker,
			"nominator ({bob_reward}) should get less than validator ({staker})"
		);
		assert!(incentive_paid_for(bob, &events).is_none());

		// THEN: era pot deducted by exactly the sum of all incentives paid out.
		let total_incentive_paid: Balance = events
			.iter()
			.filter_map(|e| match e {
				Event::ValidatorIncentivePaid { amount, .. } => Some(*amount),
				_ => None,
			})
			.sum();
		assert_eq!(Balances::free_balance(&era_pot), budget - total_incentive_paid);

		// General pot retains ED after snapshot drained it.
		assert_eq!(Balances::free_balance(&general_incentive_pot()), ExistentialDeposit::get());
	});
}

#[test]
fn no_incentive_when_budget_is_zero() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator

		// GIVEN: 50% staker, 0% incentive.
		setup_incentive_with_budget(50, 0);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		// WHEN: payout.
		make_all_reward_payment(1);
		let events = staking_events_since_last_call();

		// THEN: staker reward yes, incentive no.
		assert!(staker_reward_for(alice, &events).is_some());
		assert!(incentive_paid_for(alice, &events).is_none());
		assert_eq!(ErasValidatorIncentiveBudget::<Test>::get(1), 0);
	});
}

#[test]
fn enabling_incentive_budget_mid_flight() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator

		// GIVEN: era 1 has no incentive budget.
		setup_incentive_with_budget(50, 0);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();
		make_all_reward_payment(1);
		let era1 = staking_events_since_last_call();
		assert!(incentive_paid_for(alice, &era1).is_none());

		// WHEN: governance enables 10% incentive for era 2.
		setup_incentive_with_budget(40, 10);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();
		make_all_reward_payment(2);
		let era2 = staking_events_since_last_call();

		// THEN: era 2 has incentive.
		assert!(incentive_paid_for(alice, &era2).is_some());
		assert_eq!(ErasValidatorIncentiveBudget::<Test>::get(1), 0);
		// 10% of total inflation goes to incentive pot
		let actual_budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		let expected = Perbill::from_percent(10).mul_floor(total_payout_for(time_per_era()));
		assert_eq_error_rate!(actual_budget, expected, 1);
	});
}

#[test]
fn zero_reward_points_means_no_payout() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator (no reward points)
		let bob = 21; // validator (has reward points)

		// GIVEN: incentive enabled; roll to era 2 so its election runs with config.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		// Only bob earns points in era 2.
		Eras::<Test>::reward_active_era(vec![(bob, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		// Alice and bob both elected with equal self-stake, so both have equal weights
		// and the sum counts both.
		let bob_weight = ErasValidatorIncentiveWeight::<Test>::get(2, bob).unwrap();
		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		assert_eq!(ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap(), bob_weight);
		assert_eq!(ErasSumValidatorIncentiveWeight::<Test>::get(2), 2 * bob_weight);
		assert_eq!(budget, 750);

		// WHEN: payout era 2.
		let pot: AccountId = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
			2,
			RewardKind::ValidatorSelfStake,
		));
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: alice gets nothing — no reward points => no staker reward and no
		// incentive share, even though she was elected and has self-stake.
		assert_eq!(staker_reward_for(alice, &events), None);
		assert_eq!(incentive_paid_for(alice, &events), None);
		// THEN: bob gets staker reward, plus half the incentive budget (his weight /
		// sum_weight = 1/2). Alice's half stays unclaimed in the pot .
		assert!(staker_reward_for(bob, &events).unwrap() > budget / 2);
		assert_eq!(incentive_paid_for(bob, &events), Some(budget / 2));
		// Reward that alice forfeited
		assert_eq!(Balances::free_balance(&pot), budget / 2);
	});
}

#[test]
fn incentive_weight_stored_correctly() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator, self-stake = 1000 (mock default)

		// GIVEN: incentive config with optimum=30_000, cap=100_000.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);

		// THEN: weight = √1000 ≈ 31.
		let incentive_weight = ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();
		assert_eq!(incentive_weight, 31);

		// THEN: incentive is paid. Two validators have equal weight so each gets half of the
		// incentive budget (750 = 5% of era issuance 15_000).
		let _ = staking_events_since_last_call();
		make_all_reward_payment(2);
		assert_eq!(incentive_paid_for(alice, &staking_events_since_last_call()), Some(375));
	});
}

#[test]
fn incentive_paid_to_custom_account() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator
		let reward_account = 999;

		// GIVEN: payee set to custom account.
		assert_ok!(Staking::set_payee(
			RuntimeOrigin::signed(alice),
			RewardDestination::Account(reward_account)
		));
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();
		let before = asset::total_balance::<Test>(&reward_account);

		// WHEN: payout.
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: event records custom account, balance increased.
		let (incentive, dest) = incentive_paid_details(alice, &events).expect("incentive");
		assert_eq!(dest, RewardDestination::Account(reward_account));
		// Staker reward also goes to the custom account, so balance increase includes both.
		let staker = staker_reward_for(alice, &events).expect("staker reward");
		assert_eq!(asset::total_balance::<Test>(&reward_account) - before, staker + incentive);
	});
}

// ===== Multi-page election =====

#[test]
fn multi_page_election_does_not_overwrite_incentive_weight() {
	ExtBuilder::default()
		.multi_page_election_provider(3)
		.exposures_page_size(1)
		.build_and_execute(|| {
			let alice = 11; // validator
			setup_incentive_config();

			Session::roll_to_next_session();
			let planned_era = Rotator::<Test>::planned_era();

			// GIVEN/WHEN: page 1 has own-stake, page 2 has only nominators.
			hypothetically!({
				let page1 = bounded_vec![(
					alice,
					Exposure {
						total: 1250,
						own: 1000,
						others: vec![IndividualExposure { who: 101, value: 250 }]
					},
				)];
				EraElectionPlanner::<Test>::store_stakers_info(page1, planned_era);
				let incentive_weight =
					ErasValidatorIncentiveWeight::<Test>::get(planned_era, alice).unwrap();
				assert_eq!(incentive_weight, 31); // √1000 ≈ 31

				let page2 = bounded_vec![(
					alice,
					Exposure {
						total: 250,
						own: 0,
						others: vec![IndividualExposure { who: 102, value: 250 }]
					},
				)];
				EraElectionPlanner::<Test>::store_stakers_info(page2, planned_era);

				// THEN: incentive weight not overwritten by page 2 (own=0).
				assert_eq!(
					ErasValidatorIncentiveWeight::<Test>::get(planned_era, alice).unwrap(),
					incentive_weight
				);
			});

			// GIVEN/WHEN: own-stake arrives on page 2 instead.
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
				assert_eq!(ErasValidatorIncentiveWeight::<Test>::get(planned_era, alice), None);

				let page2 = bounded_vec![(
					alice,
					Exposure {
						total: 1250,
						own: 1000,
						others: vec![IndividualExposure { who: 102, value: 250 }]
					},
				)];
				EraElectionPlanner::<Test>::store_stakers_info(page2, planned_era);

				// THEN: incentive weight set from overview when own-stake arrives.
				let incentive_weight =
					ErasValidatorIncentiveWeight::<Test>::get(planned_era, alice).unwrap();
				assert_eq!(incentive_weight, 31); // √1000 ≈ 31
			});
		});
}

// ===== Pot distribution and proration =====

#[test]
fn multiple_validators_share_incentive_pot_correctly() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator
		let bob = 21; // validator

		// GIVEN: two validators with equal reward points, incentive budget enabled.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (bob, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		// 5% of total inflation for one era (±1 from drip rounding).
		let pot_snapshot = ErasValidatorIncentiveBudget::<Test>::get(2);
		let expected = Perbill::from_percent(5).mul_floor(total_payout_for(time_per_era()));
		assert_eq_error_rate!(pot_snapshot, expected, 1);

		let alice_incentive_weight = ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();
		let bob_incentive_weight = ErasValidatorIncentiveWeight::<Test>::get(2, bob).unwrap();
		let sum_incentive_weight = ErasSumValidatorIncentiveWeight::<Test>::get(2);

		let alice_expected = Perbill::from_rational(alice_incentive_weight, sum_incentive_weight)
			.mul_floor(pot_snapshot);
		let bob_expected = Perbill::from_rational(bob_incentive_weight, sum_incentive_weight)
			.mul_floor(pot_snapshot);

		// WHEN: both validators claim.
		make_all_reward_payment(2);

		// THEN: pot is depleted (within rounding dust).
		let pot_account = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
			2,
			RewardKind::ValidatorSelfStake,
		));
		let remaining = Balances::free_balance(&pot_account);
		let total_claimed = pot_snapshot - remaining;
		let expected_total = alice_expected + bob_expected;
		assert!(total_claimed <= expected_total);
		assert!(expected_total - total_claimed < 5, "Rounding dust too large");
	});
}

#[test]
fn validator_incentive_prorated_across_pages() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator

		// GIVEN: incentive enabled.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let validator_incentive_weight =
			ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();
		let sum_incentive_weight = ErasSumValidatorIncentiveWeight::<Test>::get(2);
		let pot = ErasValidatorIncentiveBudget::<Test>::get(2);
		let expected_total =
			Perbill::from_rational(validator_incentive_weight, sum_incentive_weight).mul_floor(pot);

		// WHEN: all pages paid out.
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: sum of per-page incentive events equals expected total (within rounding).
		let total_paid: Balance = events
			.iter()
			.filter_map(|e| match e {
				Event::ValidatorIncentivePaid { validator_stash, amount, .. }
					if *validator_stash == alice =>
				{
					Some(*amount)
				},
				_ => None,
			})
			.sum();
		assert!(total_paid <= expected_total);
		assert!(expected_total - total_paid < 5, "Rounding dust too large");
	});
}

// ===== Edge cases =====

#[test]
fn chilled_validator_can_still_claim_past_era() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator

		// GIVEN: alice earns weight in era 2.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();
		assert!(ErasValidatorIncentiveWeight::<Test>::get(2, alice).is_some());

		// WHEN: alice chills before claiming.
		assert_ok!(Staking::chill(RuntimeOrigin::signed(alice)));
		assert!(!Validators::<Test>::contains_key(&alice));

		// THEN: payout for era 2 still works.
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();
		assert!(
			incentive_paid_for(alice, &events).is_some(),
			"Chilled validator should still receive incentive for past era"
		);
	});
}

#[test]
fn payee_change_before_payout_uses_new_destination() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator
		let old_account = 888;
		let new_account = 999;

		// GIVEN: payee set to old_account during era 2.
		assert_ok!(Staking::set_payee(
			RuntimeOrigin::signed(alice),
			RewardDestination::Account(old_account)
		));
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		// WHEN: payee changes to new_account before payout.
		assert_ok!(Staking::set_payee(
			RuntimeOrigin::signed(alice),
			RewardDestination::Account(new_account)
		));
		let old_before = asset::total_balance::<Test>(&old_account);
		let new_before = asset::total_balance::<Test>(&new_account);

		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: incentive goes to new_account (payee at payout time).
		let (incentive, dest) = incentive_paid_details(alice, &events).expect("incentive");
		assert_eq!(dest, RewardDestination::Account(new_account));
		assert_eq!(asset::total_balance::<Test>(&old_account), old_before);
		assert!(asset::total_balance::<Test>(&new_account) - new_before >= incentive);
	});
}

#[test]
fn all_validators_zero_points_no_incentive_paid() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: incentive enabled but no reward points assigned.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		// WHEN: payout attempted.
		make_all_reward_payment(1);
		let events = staking_events_since_last_call();

		// THEN: no incentive events at all.
		assert!(
			!events.iter().any(|e| matches!(e, Event::ValidatorIncentivePaid { .. })),
			"No incentive when no validators earned reward points"
		);
	});
}

#[test]
fn missing_payee_emits_unexpected_and_skips_payout() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator

		// GIVEN: incentive enabled, validator has weight.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);

		// WHEN: alice's payee is missing at payout time.
		Payee::<Test>::remove(&alice);
		let _ = staking_events_since_last_call();
		make_all_reward_payment(2);

		// THEN: alice's incentive is skipped with an Unexpected event; other validators still paid.
		let events = staking_events_since_last_call();
		assert!(events
			.contains(&Event::Unexpected(UnexpectedKind::MissingPayee { era: 2, stash: alice })));
		assert!(incentive_paid_for(alice, &events).is_none());
		assert!(incentive_paid_for(21, &events).is_some());

		// Restore payee so post-test try_state passes.
		Payee::<Test>::insert(alice, RewardDestination::Staked);
	});
}

// ===== Defensive path tests =====

// ===== VestingEpochStart snapshot tests =====

#[test]
fn vesting_epoch_start_never_set_in_liquid_mode() {
	// In liquid mode (VestingBondingPeriods = 0), VestingEpochStart must never be set,
	// regardless of how many eras or bonding boundaries are crossed.
	ExtBuilder::default().build_and_execute(|| {
		assert!(VestingEpochStart::<Test>::get().is_none());

		Session::roll_until_active_era(1);
		assert!(VestingEpochStart::<Test>::get().is_none());

		Session::roll_until_active_era(3); // first boundary
		assert!(VestingEpochStart::<Test>::get().is_none());

		Session::roll_until_active_era(6); // second boundary
		assert!(VestingEpochStart::<Test>::get().is_none());
	});
}

#[test]
fn vesting_epoch_start_seeded_on_first_era_in_vesting_mode() {
	// With VestingBondingPeriods > 0, VestingEpochStart is seeded on the very first era
	// rotation, well before the first bonding-duration boundary (era 3 in the mock).
	ExtBuilder::default().build_and_execute(|| {
		VestingBondingPeriods::set(1);
		assert!(VestingEpochStart::<Test>::get().is_none());

		// Advance one era (BondingDuration = 3, so no boundary yet — the seeding fires because
		// VestingEpochStart is None, not because of a boundary crossing).
		Session::roll_until_active_era(2);
		assert!(VestingEpochStart::<Test>::get().is_some(), "should be seeded on first era");
	});
}

#[test]
fn vesting_epoch_start_snapshotted_at_bonding_duration_boundary() {
	// VestingEpochStart is refreshed at each bonding-duration boundary (era % 3 == 0).
	ExtBuilder::default().build_and_execute(|| {
		VestingBondingPeriods::set(1);

		let block_before = System::block_number();
		Session::roll_until_active_era(3);

		let snapshot = VestingEpochStart::<Test>::get();
		assert!(snapshot.is_some(), "VestingEpochStart should be set after first boundary");
		assert!(snapshot.unwrap() >= block_before);
	});
}

#[test]
fn vesting_epoch_start_updated_on_next_boundary() {
	ExtBuilder::default().build_and_execute(|| {
		VestingBondingPeriods::set(1);

		Session::roll_until_active_era(3);
		let first_snapshot = VestingEpochStart::<Test>::get().unwrap();

		Session::roll_until_active_era(6);
		let second_snapshot = VestingEpochStart::<Test>::get().unwrap();

		assert!(
			second_snapshot > first_snapshot,
			"second snapshot ({second_snapshot}) should be after first ({first_snapshot})"
		);
	});
}

#[test]
fn vesting_epoch_start_unchanged_between_boundaries() {
	ExtBuilder::default().build_and_execute(|| {
		VestingBondingPeriods::set(1);

		Session::roll_until_active_era(3);
		let snapshot_at_3 = VestingEpochStart::<Test>::get().unwrap();

		// Era 4 and 5 should not change the snapshot.
		Session::roll_until_active_era(4);
		assert_eq!(VestingEpochStart::<Test>::get().unwrap(), snapshot_at_3);
		Session::roll_until_active_era(5);
		assert_eq!(VestingEpochStart::<Test>::get().unwrap(), snapshot_at_3);
	});
}

#[test]
fn incentive_creates_vesting_schedule_end_to_end() {
	// E2E: with VestingBondingPeriods > 0, paying out an era must create a vesting
	// schedule on the validator's stash with the full incentive amount locked.
	// This test exercises the `add_to_vesting_create` path.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;

		VestingBondingPeriods::set(1);
		setup_incentive_with_budget(45, 5);

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		// GIVEN: alice has no vesting schedule yet.
		assert!(pallet_vesting::Vesting::<Test>::get(alice).is_none());

		// WHEN: payout era 2.
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: incentive was paid (not dropped).
		let incentive = incentive_paid_for(alice, &events).expect("incentive should be paid");
		assert!(incentive > 0);
		assert!(!events.iter().any(|e| matches!(e, Event::ValidatorIncentiveDropped { .. })));

		// THEN: a single vesting schedule exists for alice with the full incentive locked
		// and starting at the current VestingEpochStart.
		let schedules =
			pallet_vesting::Vesting::<Test>::get(alice).expect("vesting schedule expected");
		assert_eq!(schedules.len(), 1);
		assert_eq!(schedules[0].locked(), incentive);
		assert_eq!(schedules[0].starting_block(), VestingEpochStart::<Test>::get().unwrap());
	});
}

#[test]
fn incentive_merges_into_existing_vesting_schedule_within_epoch() {
	// E2E: two consecutive payouts inside the same bonding-duration epoch must
	// merge into the same vesting schedule - the `add_to_vesting_merge` path.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;

		VestingBondingPeriods::set(1);
		setup_incentive_with_budget(45, 5);

		// First payout: creates the schedule.
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(2);
		let first_events = staking_events_since_last_call();
		assert!(incentive_paid_for(alice, &first_events).unwrap() > 0);
		let epoch_start = VestingEpochStart::<Test>::get().unwrap();

		// Second payout for an era within the same epoch (BondingDuration = 3, so eras 3-5
		// share an epoch — no boundary crossing yet).
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		Session::roll_until_active_era(4);
		let _ = staking_events_since_last_call();
		assert_eq!(
			VestingEpochStart::<Test>::get().unwrap(),
			epoch_start,
			"epoch start must be unchanged within the same bonding window"
		);

		make_all_reward_payment(3);
		let second_events = staking_events_since_last_call();
		let second_incentive = incentive_paid_for(alice, &second_events).unwrap();
		assert!(second_incentive > 0);

		// THEN: still one schedule (merge path was taken, not create) and its
		// starting_block is unchanged. The `locked` field is intentionally not asserted
		// because `merge_vesting_info_preserving_start` back-calculates it from
		// `target_locked_now + per_block * elapsed`, which differs from the simple sum.
		let schedules = pallet_vesting::Vesting::<Test>::get(alice).unwrap();
		assert_eq!(schedules.len(), 1, "second payout should merge, not create");
		assert_eq!(schedules[0].starting_block(), epoch_start);
	});
}

#[test]
fn incentive_dropped_at_max_vesting_schedules() {
	// Vesting schedule cap test: when an account is already at MAX_VESTING_SCHEDULES, a
	// payout that would create a new schedule instead drops the incentive completely.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;
		VestingBondingPeriods::set(1);
		setup_incentive_with_budget(45, 5);

		// Pre-populate alice with MAX_VESTING_SCHEDULES schedules. Starting blocks are unique
		// and in the past so the upcoming payout takes the create path (not merge) and trips
		// the cap check.
		let max = <Test as pallet_vesting::Config>::MAX_VESTING_SCHEDULES;
		let existing: Vec<_> = (0..max)
			.map(|i| pallet_vesting::VestingInfo::new(1u128, 1u128, (i + 1) as BlockNumber))
			.collect();
		let bounded: frame_support::BoundedVec<_, _> = existing.try_into().unwrap();
		pallet_vesting::Vesting::<Test>::insert(alice, bounded);

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: incentive dropped, no Paid event, schedule count unchanged.
		assert!(events.iter().any(|e| matches!(e, Event::ValidatorIncentiveDropped { .. })));
		assert!(!events.iter().any(|e| matches!(e, Event::ValidatorIncentivePaid { .. })));
		assert_eq!(pallet_vesting::Vesting::<Test>::get(alice).unwrap().len(), max as usize);
	});
}

#[test]
fn vested_incentive_is_locked_immediately_after_payout() {
	// Core security guarantee: after a vested payout, the incentive is locked
	// by the vesting schedule and not spendable. A custom destination is used
	// so no staking lock interferes with the assertion.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;
		let recipient = 999;

		assert_ok!(Staking::set_payee(
			RuntimeOrigin::signed(alice),
			RewardDestination::Account(recipient)
		));

		VestingBondingPeriods::set(1);
		setup_incentive_with_budget(45, 5);

		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let free_before = Balances::free_balance(&recipient);
		let usable_before = Balances::usable_balance(&recipient);

		make_all_reward_payment(2);
		let events = staking_events_since_last_call();
		let incentive = incentive_paid_for(alice, &events).expect("incentive");
		let staker_reward = staker_reward_for(alice, &events).expect("staker reward");

		// Schedule landed on recipient with the full incentive locked.
		let schedules = pallet_vesting::Vesting::<Test>::get(recipient).expect("schedule");
		assert_eq!(schedules.len(), 1);
		assert_eq!(schedules[0].locked(), incentive);

		// Free balance grew by both rewards; usable balance only by the liquid staker portion
		// (the incentive is locked by the vesting schedule).
		assert_eq!(Balances::free_balance(&recipient), free_before + staker_reward + incentive);
		assert_eq!(Balances::usable_balance(&recipient), usable_before + staker_reward);

		// Transferring more than the unlocked portion must fail — the vesting lock prevents
		// the incentive from being spent before it vests.
		assert!(
			Balances::transfer_allow_death(
				RuntimeOrigin::signed(recipient),
				alice,
				usable_before + staker_reward + 1,
			)
			.is_err(),
			"transfer exceeding the unlocked portion should fail due to the vesting lock"
		);
	});
}

#[test]
fn cross_epoch_payouts_create_distinct_vesting_schedules() {
	// Per-epoch design check: payouts that span a bonding-duration boundary must land in
	// distinct schedules, given that`VestingEpochStart` advances after crossing the boundary.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;
		// Use 2 bonding periods so the schedule outlives the first boundary crossing
		// (with 1 the schedule fully vests within one bonding window and gets pruned).
		// This also mirrors production, where vesting duration spans many bonding windows.
		VestingBondingPeriods::set(2);
		setup_incentive_with_budget(45, 5);

		// Reward eras 2 and 3 — both need to be claimable later.
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		Session::roll_until_active_era(3);
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		let _ = staking_events_since_last_call();

		let epoch1_start = VestingEpochStart::<Test>::get().unwrap();

		// First payout creates the era-3-epoch schedule.
		make_all_reward_payment(2);
		let _ = staking_events_since_last_call();
		let after_first = pallet_vesting::Vesting::<Test>::get(alice).unwrap();
		assert_eq!(after_first.len(), 1);
		assert_eq!(after_first[0].starting_block(), epoch1_start);

		// Cross the next bonding-duration boundary (era 6 in the mock, BondingDuration = 3).
		Session::roll_until_active_era(6);
		let _ = staking_events_since_last_call();
		let epoch2_start = VestingEpochStart::<Test>::get().unwrap();
		assert!(epoch2_start > epoch1_start, "VestingEpochStart should advance across boundary");

		// Second payout uses the new VestingEpochStart → create, not merge.
		make_all_reward_payment(3);
		let _ = staking_events_since_last_call();
		let after_second = pallet_vesting::Vesting::<Test>::get(alice).unwrap();
		assert_eq!(
			after_second.len(),
			2,
			"crossing the bonding-duration boundary should produce a distinct schedule"
		);
		assert!(after_second.iter().any(|s| s.starting_block() == epoch1_start));
		assert!(after_second.iter().any(|s| s.starting_block() == epoch2_start));
	});
}

#[test]
fn incentive_dropped_event_not_emitted_on_success() {
	// On a successful payout, ValidatorIncentiveDropped must NOT be emitted.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;

		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		Session::roll_until_active_era(3);

		make_all_reward_payment(2);

		let events = staking_events_since_last_call();
		assert!(
			!events.iter().any(|e| matches!(e, Event::ValidatorIncentiveDropped { .. })),
			"ValidatorIncentiveDropped should not be emitted on success"
		);
		assert!(
			events.iter().any(|e| matches!(e, Event::ValidatorIncentivePaid { .. })),
			"ValidatorIncentivePaid should be emitted on success"
		);
	});
}

#[test]
fn incentive_dropped_when_pot_is_empty() {
	// When the incentive pot is empty the payout adapter returns an error and the incentive
	// is dropped (no liquid fallback). ValidatorIncentiveDropped is emitted instead of a panic.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;

		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);

		// Drain the incentive pot so the transfer inside the adapter fails.
		let pot = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
			2,
			RewardKind::ValidatorSelfStake,
		));
		let pot_balance = Balances::free_balance(&pot);
		if pot_balance > 0 {
			let _ = <Balances as frame_support::traits::fungible::Mutate<_>>::transfer(
				&pot,
				&999,
				pot_balance,
				frame_support::traits::tokens::Preservation::Expendable,
			);
		}

		make_all_reward_payment(2);

		let events = staking_events_since_last_call();
		assert!(
			events.iter().any(|e| matches!(e, Event::ValidatorIncentiveDropped { .. })),
			"ValidatorIncentiveDropped should be emitted when pot is empty"
		);
		assert!(
			!events.iter().any(|e| matches!(e, Event::ValidatorIncentivePaid { .. })),
			"ValidatorIncentivePaid should not be emitted when pot is empty"
		);
	});
}
