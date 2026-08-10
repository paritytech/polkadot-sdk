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
		// THEN: bob is the only validator with points, so under the weighted-mean
		// formula his share (w_b · 1) / (w_b · 1) = 1 — he receives the full budget.
		// Pot is depleted (modulo Perbill rounding dust).
		assert!(staker_reward_for(bob, &events).unwrap() > 0);
		assert_eq!(incentive_paid_for(bob, &events), Some(budget));
		assert_eq!(Balances::free_balance(&pot), 0);
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

#[test]
fn incentive_sum_across_multiple_exposure_pages_equals_share_times_budget() {
	// With `exposures_page_size(1)`, alice's extra nominators force her exposure to
	// span ≥ 2 pages. Each page emits its own `ValidatorIncentivePaid` event prorated
	// by `page_stake_part`; the sum across pages must equal `share × budget` (± dust)
	// because Σ page_stake_part = 1.
	ExtBuilder::default()
		.exposures_page_size(1)
		.add_staker(102, 250, StakerStatus::Nominator(vec![11]))
		.add_staker(103, 250, StakerStatus::Nominator(vec![11]))
		.build_and_execute(|| {
			let alice = 11; // validator with multi-page exposure
			let bob = 21; // validator with single-page exposure

			// GIVEN: incentive enabled; both validators earn equal points.
			setup_incentive_with_budget(45, 5);
			Session::roll_until_active_era(2);
			Eras::<Test>::reward_active_era(vec![(alice, 1), (bob, 1)]);
			Session::roll_until_active_era(3);
			let _ = staking_events_since_last_call();

			let alice_pages = Eras::<Test>::exposure_page_count(2, &alice);
			assert!(alice_pages >= 2, "expected alice to have ≥ 2 pages, got {alice_pages}");

			// Equal own-stake & equal points → share = w_a / (w_a + w_b).
			let alice_weight = ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();
			let sum_weight = ErasSumValidatorIncentiveWeight::<Test>::get(2);
			let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
			let expected_total = Perbill::from_rational(alice_weight, sum_weight).mul_floor(budget);

			// WHEN: payout all pages.
			make_all_reward_payment(2);
			let events = staking_events_since_last_call();

			// THEN: one ValidatorIncentivePaid event per page.
			let alice_amounts: Vec<Balance> = events
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
			assert_eq!(
				alice_amounts.len() as u32,
				alice_pages,
				"expected one incentive event per page"
			);

			// THEN: sum across pages equals share × budget within Perbill rounding dust.
			let total_paid: Balance = alice_amounts.iter().sum();
			assert_eq_error_rate!(total_paid, expected_total, 4);
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

#[test]
fn validator_with_points_but_zero_weight_gets_no_incentive() {
	// A validator that earns reward points but whose incentive weight is zero
	// (e.g., elected with own=0 on every page — see `store_stakers_info`, which
	// only inserts a weight when own > 0) must be gated out of the incentive
	// payout. The other earner picks up the full budget by virtue of being the
	// only validator with non-zero weighted points.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator — we'll strip her incentive weight
		let bob = 21; // validator — keeps normal weight

		// GIVEN: incentive enabled; both validators elected normally.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);

		// WHEN: remove alice's weight (simulating own=0 in election); keep the sum
		// invariant consistent so try_state passes.
		let alice_weight =
			ErasValidatorIncentiveWeight::<Test>::take(2, alice).expect("weight stored");
		ErasSumValidatorIncentiveWeight::<Test>::mutate(2, |s| *s = s.saturating_sub(alice_weight));

		// Both validators earn the same reward points.
		Eras::<Test>::reward_active_era(vec![(alice, 1), (bob, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		// WHEN: payout era 2.
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: alice — gated out, no incentive.
		assert_eq!(incentive_paid_for(alice, &events), None);
		// staker reward is independent of incentive weight → alice still gets one.
		assert!(staker_reward_for(alice, &events).is_some());

		// THEN: bob is the only non-zero-weight earner → share = 1 → full budget.
		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		assert_eq!(incentive_paid_for(bob, &events), Some(budget));
	});
}

// ===== Defensive path tests =====

// ===== VestingEpochStartBlocks snapshot tests =====

#[test]
fn vesting_epoch_start_blocks_never_set_in_liquid_mode() {
	// In liquid mode (VestingBondingPeriods = 0), VestingEpochStartBlocks must remain empty
	// regardless of how many eras or bonding boundaries are crossed.
	ExtBuilder::default().build_and_execute(|| {
		assert!(VestingEpochStartBlocks::<Test>::iter().next().is_none());

		Session::roll_until_active_era(1);
		assert!(VestingEpochStartBlocks::<Test>::iter().next().is_none());

		Session::roll_until_active_era(3); // first boundary
		assert!(VestingEpochStartBlocks::<Test>::iter().next().is_none());

		Session::roll_until_active_era(6); // second boundary
		assert!(VestingEpochStartBlocks::<Test>::iter().next().is_none());
	});
}

#[test]
fn vesting_epoch_start_blocks_seeded_on_first_era_in_vesting_mode() {
	// With VestingBondingPeriods > 0, an entry for the current bonding window is seeded on
	// the very first era rotation, well before the first bonding-duration boundary (era 3
	// in the mock). Era 1 belongs to bonding period 0.
	ExtBuilder::default().build_and_execute(|| {
		VestingBondingPeriods::set(1);
		assert!(VestingEpochStartBlocks::<Test>::iter().next().is_none());

		// Advance one era (BondingDuration = 3, so no boundary yet — the seeding fires because
		// the map is empty, not because of a boundary crossing).
		Session::roll_until_active_era(2);
		assert!(VestingEpochStartBlocks::<Test>::get(0).is_some(), "period 0 should be seeded");
	});
}

#[test]
fn vesting_epoch_start_blocks_snapshotted_at_bonding_duration_boundary() {
	// A new entry is inserted at each bonding-duration boundary (era % 3 == 0).
	ExtBuilder::default().build_and_execute(|| {
		VestingBondingPeriods::set(1);

		let block_before = System::block_number();
		Session::roll_until_active_era(3);

		let snapshot = VestingEpochStartBlocks::<Test>::get(1);
		assert!(snapshot.is_some(), "period 1 should be set after first boundary");
		assert!(snapshot.unwrap() >= block_before);
	});
}

#[test]
fn vesting_epoch_start_blocks_recorded_per_bonding_period() {
	// Each bonding-duration boundary records a fresh entry, keyed by the new period index.
	ExtBuilder::default().build_and_execute(|| {
		VestingBondingPeriods::set(1);

		Session::roll_until_active_era(3);
		let first_snapshot = VestingEpochStartBlocks::<Test>::get(1).unwrap();

		Session::roll_until_active_era(6);
		let second_snapshot = VestingEpochStartBlocks::<Test>::get(2).unwrap();

		assert!(
			second_snapshot > first_snapshot,
			"period 2 ({second_snapshot}) should start after period 1 ({first_snapshot})"
		);
	});
}

#[test]
fn vesting_epoch_start_blocks_unchanged_between_boundaries() {
	ExtBuilder::default().build_and_execute(|| {
		VestingBondingPeriods::set(1);

		Session::roll_until_active_era(3);
		let snapshot_at_3 = VestingEpochStartBlocks::<Test>::get(1).unwrap();

		// Era 4 and 5 are still in period 1; the entry must not be touched.
		Session::roll_until_active_era(4);
		assert_eq!(VestingEpochStartBlocks::<Test>::get(1).unwrap(), snapshot_at_3);
		Session::roll_until_active_era(5);
		assert_eq!(VestingEpochStartBlocks::<Test>::get(1).unwrap(), snapshot_at_3);
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
		assert!(!events.iter().any(|e| matches!(
			e,
			Event::Unexpected(UnexpectedKind::ValidatorIncentiveDropped { .. })
		)));

		// THEN: a single vesting schedule exists for alice with the full incentive locked
		// and starting at the epoch start for era 2's bonding window (period 0, eras 0..2).
		let schedules =
			pallet_vesting::Vesting::<Test>::get(alice).expect("vesting schedule expected");
		assert_eq!(schedules.len(), 1);
		assert_eq!(schedules[0].0.locked(), incentive);
		let bonding_period = 2 / BondingDuration::get();
		assert_eq!(
			schedules[0].0.starting_block(),
			VestingEpochStartBlocks::<Test>::get(bonding_period).unwrap(),
		);
	});
}

#[test]
fn incentive_merges_into_existing_vesting_schedule_within_epoch() {
	// Two payouts whose eras belong to the same bonding-duration window must merge
	// into the same vesting schedule — the `add_to_vesting_merge` path.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;

		VestingBondingPeriods::set(1);
		setup_incentive_with_budget(45, 5);

		// Reward era 3 and claim it from era 4.
		Session::roll_until_active_era(3);
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		Session::roll_until_active_era(4);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(3);
		let first_events = staking_events_since_last_call();
		assert!(incentive_paid_for(alice, &first_events).unwrap() > 0);
		let bonding_period = 3 / BondingDuration::get();
		let epoch_start = VestingEpochStartBlocks::<Test>::get(bonding_period).unwrap();

		// Reward era 4 (same bonding period as era 3) and claim it from era 5.
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		Session::roll_until_active_era(5);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(4);
		let second_events = staking_events_since_last_call();
		let second_incentive = incentive_paid_for(alice, &second_events).unwrap();
		assert!(second_incentive > 0);

		// THEN: still one schedule (merge path was taken, not create) and its
		// starting_block is unchanged. The `locked` field is intentionally not asserted
		// because `merge_vesting_info_preserving_start` back-calculates it from
		// `target_locked_now + per_block * elapsed`, which differs from the simple sum.
		let schedules = pallet_vesting::Vesting::<Test>::get(alice).unwrap();
		assert_eq!(schedules.len(), 1, "second payout should merge, not create");
		assert_eq!(schedules[0].0.starting_block(), epoch_start);
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
		assert_eq!(schedules[0].0.locked(), incentive);

		// The schedule starts at era 2's bonding window — which began before the payout was
		// claimed — so by `now` some of the incentive has already vested. The usable balance
		// must equal `free - still_locked_now`.
		let still_locked = schedules[0]
			.0
			.locked_at::<sp_runtime::traits::ConvertInto>(System::block_number());
		let unlocked_so_far = incentive.saturating_sub(still_locked);
		assert!(still_locked > 0, "the incentive must not be fully unlocked yet");
		assert_eq!(Balances::free_balance(&recipient), free_before + staker_reward + incentive);
		assert_eq!(
			Balances::usable_balance(&recipient),
			usable_before + staker_reward + unlocked_so_far,
		);

		// Transferring more than what's currently usable must fail — the vesting lock keeps
		// the still-locked portion of the incentive from being spent.
		let usable_now = Balances::usable_balance(&recipient);
		assert!(
			Balances::transfer_allow_death(RuntimeOrigin::signed(recipient), alice, usable_now + 1)
				.is_err(),
			"transfer exceeding the usable balance should fail due to the vesting lock",
		);
	});
}

#[test]
fn cross_epoch_payouts_create_distinct_vesting_schedules() {
	// Per-epoch design check: payouts for eras in different bonding-duration windows must
	// land in distinct schedules, because each era's payout merges into its own window's slot.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;
		// Use 2 bonding periods so neither schedule fully vests before the second payout is
		// claimed. This also mirrors production, where vesting duration spans many bonding windows.
		VestingBondingPeriods::set(2);
		setup_incentive_with_budget(45, 5);

		// Reward eras 2 and 3.
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		Session::roll_until_active_era(3);
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		let _ = staking_events_since_last_call();

		let period0_start = VestingEpochStartBlocks::<Test>::get(0).unwrap();
		let period1_start = VestingEpochStartBlocks::<Test>::get(1).unwrap();
		assert!(
			period1_start > period0_start,
			"period 1 should start after period 0 (crossed the boundary at era 3)"
		);

		// Pay era 2 (period 0) first.
		make_all_reward_payment(2);
		let _ = staking_events_since_last_call();
		let after_first = pallet_vesting::Vesting::<Test>::get(alice).unwrap();
		assert_eq!(after_first.len(), 1);
		assert_eq!(after_first[0].0.starting_block(), period0_start);

		// Advance one era so era 3 is claimable, then pay era 3 (period 1) — its lookup
		// returns a *different* epoch start, so a second schedule is created.
		Session::roll_until_active_era(4);
		let _ = staking_events_since_last_call();
		make_all_reward_payment(3);
		let _ = staking_events_since_last_call();

		let after_second = pallet_vesting::Vesting::<Test>::get(alice).unwrap();
		assert_eq!(
			after_second.len(),
			2,
			"different bonding periods should produce distinct schedules"
		);
		assert!(after_second.iter().any(|(s, _)| s.starting_block() == period0_start));
		assert!(after_second.iter().any(|(s, _)| s.starting_block() == period1_start));
	});
}

#[test]
fn fallback_materializes_so_pre_upgrade_claims_merge() {
	// When a bonding period has no entry (its era predates the pallet upgrade), the first
	// claim records `now` into the map for that period. A later claim from a different
	// block for *the same* pre-upgrade period must then merge into the same schedule.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;

		VestingBondingPeriods::set(1);
		setup_incentive_with_budget(45, 5);

		// Reward eras 3 and 4 — both in bonding period 1 (with BondingDuration = 3).
		Session::roll_until_active_era(3);
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		Session::roll_until_active_era(4);
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		Session::roll_until_active_era(5);
		let _ = staking_events_since_last_call();

		// Simulate a pre-upgrade window: wipe the entry that the snapshot logic produced
		// for period 1. From the payout path's perspective, eras 3 and 4 now look like
		// eras whose epoch was never recorded.
		VestingEpochStartBlocks::<Test>::remove(1);
		assert!(VestingEpochStartBlocks::<Test>::get(1).is_none());

		// First fallback claim: era 3 → no entry → materializes period 1 = current block.
		make_all_reward_payment(3);
		let _ = staking_events_since_last_call();
		let materialized = VestingEpochStartBlocks::<Test>::get(1)
			.expect("fallback must have materialized an entry");
		let after_first = pallet_vesting::Vesting::<Test>::get(alice).expect("schedule");
		assert_eq!(after_first.len(), 1);
		assert_eq!(after_first[0].0.starting_block(), materialized);

		// Advance the block number so a re-fallback would pick a *different* `now`.
		System::set_block_number(System::block_number() + 50);

		// Second claim for the same pre-upgrade period must merge, not create.
		make_all_reward_payment(4);
		let _ = staking_events_since_last_call();
		let after_second = pallet_vesting::Vesting::<Test>::get(alice).unwrap();
		assert_eq!(
			after_second.len(),
			1,
			"second claim in the same pre-upgrade window must merge into the materialized schedule",
		);
		assert_eq!(after_second[0].0.starting_block(), materialized);

		// The materialized entry itself must not have been rewritten by the second claim.
		assert_eq!(VestingEpochStartBlocks::<Test>::get(1).unwrap(), materialized);
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
			!events.iter().any(|e| matches!(
				e,
				Event::Unexpected(UnexpectedKind::ValidatorIncentiveDropped { .. })
			)),
			"ValidatorIncentiveDropped should not be emitted on success"
		);
		assert!(
			events.iter().any(|e| matches!(e, Event::ValidatorIncentivePaid { .. })),
			"ValidatorIncentivePaid should be emitted on success"
		);
	});
}

#[test]
fn incentive_not_paid_when_pot_is_empty() {
	// When the incentive pot is empty the pay() call fails (the currency transfer inside
	// the vesting adapter has nothing to move).  Neither ValidatorIncentivePaid nor any
	// other successful-delivery event is emitted.
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
			!events.iter().any(|e| matches!(e, Event::ValidatorIncentivePaid { .. })),
			"ValidatorIncentivePaid should not be emitted when pot is empty"
		);
	});
}

#[test]
#[cfg_attr(debug_assertions, should_panic(expected = "Defensive failure has been triggered!"))]
fn reward_active_era_defends_individual_map_capacity() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;
		let bob = 21;

		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);

		#[cfg(not(debug_assertions))]
		let alice_weight = ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();

		// Reward points are keyed by a bounded `individual` map. If that map cannot record a
		// validator, the point delta must not enter `ErasSumWeightedPoints` either; otherwise
		// payout and try-state recomputation would see different denominators.
		MaxValidatorSet::set(1);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (bob, 1)]);

		#[cfg(not(debug_assertions))]
		{
			assert_eq!(ErasRewardPoints::<Test>::get(2).individual.len(), 1);
			assert_eq!(ErasSumWeightedPoints::<Test>::get(2), alice_weight);
		}
	});
}

// ===== Performance scaling integration tests =====

#[test]
fn incentive_scales_with_relative_performance() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator
		let bob = 21; // validator

		// GIVEN: two validators with equal incentive weight, incentive budget enabled.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		// Bob earns twice as many points as alice.
		Eras::<Test>::reward_active_era(vec![(alice, 1), (bob, 2)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let alice_weight = ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();
		let bob_weight = ErasValidatorIncentiveWeight::<Test>::get(2, bob).unwrap();
		let sum_weight = ErasSumValidatorIncentiveWeight::<Test>::get(2);
		assert_eq!(alice_weight, bob_weight);
		assert_eq!(sum_weight, alice_weight + bob_weight);

		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		let pot = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
			2,
			RewardKind::ValidatorSelfStake,
		));

		// WHEN: payout era 2.
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: equal weights, points 1 & 2 → denominator = w·(1 + 2) = 3w.
		// bob share = 2/3, alice share = 1/3. Full budget is distributed.
		let bob_expected = Perbill::from_rational(2u32, 3u32).mul_floor(budget);
		let alice_expected = Perbill::from_rational(1u32, 3u32).mul_floor(budget);

		assert_eq!(incentive_paid_for(bob, &events), Some(bob_expected));
		assert_eq!(incentive_paid_for(alice, &events), Some(alice_expected));

		// THEN: pot is depleted (only Perbill rounding dust remains, no forfeit).
		let total_paid = bob_expected + alice_expected;
		assert_eq!(Balances::free_balance(&pot), budget - total_paid);
		assert!(budget - total_paid < 5, "Rounding dust too large");
	});
}

#[test]
fn outlier_top_performer_scales_others_down() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator
		let bob = 21; // validator

		// GIVEN: two validators with equal weight, incentive budget enabled.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		// Alice earns 10× more points than bob.
		Eras::<Test>::reward_active_era(vec![(alice, 10), (bob, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		let pot = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
			2,
			RewardKind::ValidatorSelfStake,
		));

		// WHEN: payout era 2.
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: equal weights, points 10 & 1 → denominator = w·11.
		// alice share = 10/11, bob share = 1/11. Full budget is distributed.
		let alice_expected = Perbill::from_rational(10u32, 11u32).mul_floor(budget);
		let bob_expected = Perbill::from_rational(1u32, 11u32).mul_floor(budget);

		assert_eq!(incentive_paid_for(alice, &events), Some(alice_expected));
		assert_eq!(incentive_paid_for(bob, &events), Some(bob_expected));

		// THEN: pot residue is dust only.
		let total_paid = alice_expected + bob_expected;
		assert_eq!(Balances::free_balance(&pot), budget - total_paid);
		assert!(budget - total_paid < 5, "Rounding dust too large");
	});
}

#[test]
fn uniform_performance_distributes_full_budget() {
	// Under the weighted-mean / proportional-split formula, equal performance ⇒
	// each validator's share is 1/N and the full budget is distributed (no
	// forfeit-to-pot in this regime, just Perbill rounding dust).
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator
		let bob = 21; // validator

		// GIVEN: two validators with equal weight and equal points (5 each).
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 5), (bob, 5)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		let pot = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
			2,
			RewardKind::ValidatorSelfStake,
		));

		// WHEN: payout era 2.
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: total paid equals budget within rounding dust (≤ a few units).
		let total_paid: Balance = events
			.iter()
			.filter_map(|e| match e {
				Event::ValidatorIncentivePaid { amount, .. } => Some(*amount),
				_ => None,
			})
			.sum();
		assert!(total_paid <= budget);
		assert!(budget - total_paid < 5, "Rounding dust too large: {}", budget - total_paid);
		// Pot residue is just dust, not redistribution leftovers.
		assert_eq!(Balances::free_balance(&pot), budget - total_paid);
	});
}

#[test]
fn zero_performer_alongside_unequal_others() {
	ExtBuilder::default().validator_count(3).build_and_execute(|| {
		let alice = 11; // validator (will earn 0 points)
		let bob = 21; // validator (will earn 5 points)
		let carol = 31; // validator (will earn 2 points)

		// GIVEN: three validators elected, incentive budget enabled.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		// Alice gets 0 points (will be gated out); bob and carol earn unequal points.
		Eras::<Test>::reward_active_era(vec![(bob, 5), (carol, 2)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		let pot = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
			2,
			RewardKind::ValidatorSelfStake,
		));
		// Equal stake → equal weights for bob and carol; alice's weight is multiplied
		// by zero points so it doesn't enter the denominator either way.
		let bob_weight = ErasValidatorIncentiveWeight::<Test>::get(2, bob).unwrap();
		let carol_weight = ErasValidatorIncentiveWeight::<Test>::get(2, carol).unwrap();

		// WHEN: payout era 2.
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: alice gated out (zero points → no incentive).
		assert_eq!(incentive_paid_for(alice, &events), None);

		// THEN: denominator = w_bob·5 + w_carol·2; alice contributes 0. The full
		// budget is split proportionally between bob and carol.
		let denom = bob_weight * 5 + carol_weight * 2;
		let bob_expected = Perbill::from_rational(bob_weight * 5, denom).mul_floor(budget);
		let carol_expected = Perbill::from_rational(carol_weight * 2, denom).mul_floor(budget);

		assert_eq!(incentive_paid_for(bob, &events), Some(bob_expected));
		assert_eq!(incentive_paid_for(carol, &events), Some(carol_expected));

		// THEN: pot residue is dust only — full budget distributed across bob+carol.
		let total_paid = bob_expected + carol_expected;
		assert_eq!(Balances::free_balance(&pot), budget - total_paid);
		assert!(budget - total_paid < 5, "Rounding dust too large");
	});
}

#[test]
fn single_validator_earning_points_gets_full_budget() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator (only one earning points)
		let bob = 21; // validator (zero points → gated out)

		// GIVEN: two validators with equal weight, incentive budget enabled.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		// Only alice earns points. Denominator = w_a · 3, numerator = w_a · 3 → share = 1.
		Eras::<Test>::reward_active_era(vec![(alice, 3)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		let pot = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
			2,
			RewardKind::ValidatorSelfStake,
		));

		// WHEN: payout era 2.
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: alice is the only earner → share = 1, receives the entire budget.
		// Bob gated out; his "slot" is implicitly redistributed to alice by the
		// weighted-mean formula (no pot residue beyond dust).
		assert_eq!(incentive_paid_for(alice, &events), Some(budget));
		assert_eq!(incentive_paid_for(bob, &events), None);
		assert_eq!(Balances::free_balance(&pot), 0);
	});
}

// ===== `ErasSumWeightedPoints` incremental-update unit tests =====
//
// These pin the storage-maintenance invariant inside `Eras::reward_active_era`:
// `ErasSumWeightedPoints[era] == Σ_v (ErasValidatorIncentiveWeight[era, v] · ep_v)`.
// They are the unit-level mirror of the parameterized cases the old
// `weighted_points_share_*` tests used to cover on the deleted in-memory helper.
// Share math through payouts is exercised by the integration tests above.

#[test]
fn sum_weighted_points_initial_value_is_zero() {
	ExtBuilder::default().build_and_execute(|| {
		// GIVEN: incentive enabled and a fresh era — but no `reward_active_era` calls yet.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);

		// Sanity: the era is set up (weights exist) so the assertion is non-trivial.
		assert!(ErasValidatorIncentiveWeight::<Test>::get(2, 11).is_some());

		// THEN: ValueQuery default holds — no points credited → denominator is zero.
		assert_eq!(ErasSumWeightedPoints::<Test>::get(2), 0);
	});
}

#[test]
fn sum_weighted_points_single_validator_equals_weight_times_points() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);

		let alice_weight = ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();

		// WHEN: credit alice 7 points.
		Eras::<Test>::reward_active_era(vec![(alice, 7)]);

		// THEN: sum == w_alice · 7.
		assert_eq!(ErasSumWeightedPoints::<Test>::get(2), alice_weight * 7);
	});
}

#[test]
fn sum_weighted_points_uniform_inputs_sum_equally() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;
		let bob = 21;
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);

		// Equal own-stake (mock default for 11/21) → equal weights.
		let w = ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();
		assert_eq!(ErasValidatorIncentiveWeight::<Test>::get(2, bob).unwrap(), w);

		// WHEN: credit each validator 5 points.
		Eras::<Test>::reward_active_era(vec![(alice, 5), (bob, 5)]);

		// THEN: sum == 2 · (w · 5).
		assert_eq!(ErasSumWeightedPoints::<Test>::get(2), 2 * w * 5);
	});
}

#[test]
fn sum_weighted_points_unequal_points_contribute_proportionally() {
	// Three validators with possibly-unequal natural weights. The invariant is
	// `Σ(w_v · ep_v)`; assert it directly without assuming weights are uniform.
	ExtBuilder::default().validator_count(3).build_and_execute(|| {
		let alice = 11;
		let bob = 21;
		let carol = 31;
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);

		let w_a = ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();
		let w_b = ErasValidatorIncentiveWeight::<Test>::get(2, bob).unwrap();
		let w_c = ErasValidatorIncentiveWeight::<Test>::get(2, carol).unwrap();

		// WHEN: credit alice 10, bob 1, carol 1.
		Eras::<Test>::reward_active_era(vec![(alice, 10), (bob, 1), (carol, 1)]);

		// THEN: sum == w_a · 10 + w_b · 1 + w_c · 1.
		assert_eq!(ErasSumWeightedPoints::<Test>::get(2), w_a * 10 + w_b + w_c);
	});
}

#[test]
fn sum_weighted_points_unequal_weights_propagate() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;
		let bob = 21;
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);

		// Inflate alice's weight to 2× her natural value; keep the
		// `ErasSumValidatorIncentiveWeight` invariant consistent so try_state passes.
		let natural = ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();
		ErasValidatorIncentiveWeight::<Test>::insert(2, alice, 2 * natural);
		ErasSumValidatorIncentiveWeight::<Test>::mutate(2, |s| *s = s.saturating_add(natural));

		// WHEN: alice 2 points, bob 1 point.
		Eras::<Test>::reward_active_era(vec![(alice, 2), (bob, 1)]);

		// THEN: sum == 2w · 2 + w · 1 = 5w.
		assert_eq!(ErasSumWeightedPoints::<Test>::get(2), 5 * natural);
	});
}

#[test]
fn sum_weighted_points_validator_without_weight_excluded_from_sum() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;
		let bob = 21;
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);

		// Strip alice's incentive weight (simulating own=0 at election); keep the sum
		// invariant consistent.
		let alice_weight =
			ErasValidatorIncentiveWeight::<Test>::take(2, alice).expect("weight stored");
		ErasSumValidatorIncentiveWeight::<Test>::mutate(2, |s| *s = s.saturating_sub(alice_weight));
		let bob_weight = ErasValidatorIncentiveWeight::<Test>::get(2, bob).unwrap();

		// WHEN: both credited points, but alice has no weight.
		Eras::<Test>::reward_active_era(vec![(alice, 5), (bob, 3)]);

		// THEN: only bob's contribution lands in the sum; alice is gated out by the
		// `if !weight.is_zero()` check, matching the gate in
		// `calculate_validator_incentive_for_page`.
		assert_eq!(ErasSumWeightedPoints::<Test>::get(2), bob_weight * 3);
	});
}

#[test]
fn sum_weighted_points_zero_points_yields_no_delta() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);

		let before = ErasSumWeightedPoints::<Test>::get(2);

		// WHEN: alice credited zero points.
		Eras::<Test>::reward_active_era(vec![(alice, 0)]);

		// THEN: sum is unchanged (w · 0 contributes nothing; short-circuit also avoids
		// a pointless storage write).
		assert_eq!(ErasSumWeightedPoints::<Test>::get(2), before);
	});
}

#[test]
fn sum_weighted_points_accrues_across_sequential_calls() {
	// The load-bearing property of the incrementally-maintained storage: calls
	// compose, so the denominator at payout time reflects the entire history of
	// `reward_active_era` invocations within the era.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);

		let alice_weight = ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();

		// WHEN: two back-to-back credits for the same validator.
		Eras::<Test>::reward_active_era(vec![(alice, 3)]);
		assert_eq!(ErasSumWeightedPoints::<Test>::get(2), alice_weight * 3);

		Eras::<Test>::reward_active_era(vec![(alice, 4)]);

		// THEN: sum accrued — w · (3 + 4) = w · 7.
		assert_eq!(ErasSumWeightedPoints::<Test>::get(2), alice_weight * 7);
	});
}

// ===== Cutoff-era / legacy-formula fallback tests =====
//
// These pin the [`WeightedPointsFormulaStartEra`] branch in
// `calculate_validator_incentive_for_page`: eras strictly older than the cutoff
// fall back to the legacy stake-only share, so pending pre-cutoff payouts still
// work even when their `ErasSumWeightedPoints` denominator was never populated.

#[test]
fn legacy_formula_used_for_eras_before_cutoff() {
	// Era 2 is modeled as active before the cutoff is recorded at era 3, so it must pay under
	// the legacy `w_i / Σ_j w_j` share. Clear `ErasSumWeightedPoints` to prove the legacy path
	// does not read it.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;
		let bob = 21;

		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		// Unequal points; under the new formula this would yield a 10:1 split,
		// under the legacy formula (equal weights) this is a 1:1 split.
		Eras::<Test>::reward_active_era(vec![(alice, 10), (bob, 1)]);
		Session::roll_until_active_era(3);

		// WHEN: pin era 2 as pre-cutoff and wipe its weighted-points denominator to mimic an era
		// whose points were credited before that denominator was maintained.
		WeightedPointsFormulaStartEra::<Test>::put(3);
		ErasSumWeightedPoints::<Test>::remove(2);
		let _ = staking_events_since_last_call();

		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		let alice_weight = ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();
		let bob_weight = ErasValidatorIncentiveWeight::<Test>::get(2, bob).unwrap();
		let sum_weight = ErasSumValidatorIncentiveWeight::<Test>::get(2);

		// WHEN: payout era 2.
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: split follows the legacy formula — equal weights ⇒ equal shares,
		// regardless of the 10:1 points imbalance.
		let alice_expected = Perbill::from_rational(alice_weight, sum_weight).mul_floor(budget);
		let bob_expected = Perbill::from_rational(bob_weight, sum_weight).mul_floor(budget);
		assert_eq!(incentive_paid_for(alice, &events), Some(alice_expected));
		assert_eq!(incentive_paid_for(bob, &events), Some(bob_expected));
	});
}

#[test]
fn new_formula_used_for_eras_at_and_after_cutoff() {
	// Cutoff at era 2 ⇒ era 2 itself uses the weighted-points formula. With a
	// 2:1 points split between alice and bob (equal weights), shares are 2/3
	// and 1/3.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;
		let bob = 21;

		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		WeightedPointsFormulaStartEra::<Test>::put(2);

		Eras::<Test>::reward_active_era(vec![(alice, 2), (bob, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);

		// WHEN: payout era 2.
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: 2/3 vs 1/3 of the budget.
		let alice_expected = Perbill::from_rational(2u32, 3u32).mul_floor(budget);
		let bob_expected = Perbill::from_rational(1u32, 3u32).mul_floor(budget);
		assert_eq!(incentive_paid_for(alice, &events), Some(alice_expected));
		assert_eq!(incentive_paid_for(bob, &events), Some(bob_expected));
	});
}

#[test]
fn new_formula_zero_denominator_emits_unexpected_and_skips_payout() {
	// A post-cutoff era whose `ErasSumWeightedPoints` is zero despite a live budget and a
	// validator with points/weight is a storage inconsistency: the payout must skip the
	// incentive and surface an `Unexpected` event rather than silently pay nothing.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator

		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		Session::roll_until_active_era(3);

		// WHEN: corrupt the denominator to zero on a weighted-points era.
		let valid_sum = ErasSumWeightedPoints::<Test>::get(2);
		ErasSumWeightedPoints::<Test>::remove(2);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: no incentive paid, and the inconsistency is reported.
		assert_eq!(incentive_paid_for(alice, &events), None);
		assert!(events.contains(&Event::Unexpected(
			UnexpectedKind::ValidatorIncentiveWeightMismatch { era: 2 }
		)));

		// Restore valid state so the post-test try-state hook still runs and passes.
		ErasSumWeightedPoints::<Test>::insert(2, valid_sum);
	});
}

#[test]
fn legacy_era_pays_out_even_without_weighted_points_storage() {
	// Regression for pending pre-cutoff payouts: under the weighted-points formula, era 2 would
	// pay zero because `ErasSumWeightedPoints[2] == 0`. With the cutoff in place, the legacy
	// branch must still pay alice her full share.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11;
		let bob = 21;

		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		// Both validators earn points so the caller's zero-points gate is open.
		Eras::<Test>::reward_active_era(vec![(alice, 1), (bob, 1)]);
		Session::roll_until_active_era(3);

		// Model a pre-cutoff era whose weighted-points denominator was not maintained.
		WeightedPointsFormulaStartEra::<Test>::put(3);
		ErasSumWeightedPoints::<Test>::remove(2);
		let _ = staking_events_since_last_call();

		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// Both validators are paid (would have been `None` under the weighted-points formula
		// because the denominator is zero).
		assert!(incentive_paid_for(alice, &events).is_some());
		assert!(incentive_paid_for(bob, &events).is_some());
	});
}

#[test]
fn try_state_skips_weighted_points_check_for_pre_cutoff_eras() {
	use crate::session_rotation::Eras as ErasMod;

	ExtBuilder::default().build_and_execute(|| {
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(11, 1), (21, 1)]);

		// GIVEN: the genesis cutoff makes era 2 a weighted-points era, so a missing
		// denominator must trip try-state.
		ErasSumWeightedPoints::<Test>::remove(2);
		assert!(
			ErasMod::<Test>::do_try_state().is_err(),
			"try-state should flag a missing denominator for weighted-points eras"
		);

		// WHEN: declare era 2 as pre-cutoff (legacy formula territory).
		WeightedPointsFormulaStartEra::<Test>::put(3);

		// THEN: the same missing-denominator state is now expected and accepted.
		assert_ok!(ErasMod::<Test>::do_try_state());
	});
}

#[test]
fn migration_sets_cutoff_to_active_era_plus_one() {
	use crate::migrations::VersionUncheckedSetWeightedPointsFormulaStartEra as Migration;
	use frame_support::traits::UncheckedOnRuntimeUpgrade;

	ExtBuilder::default().build_and_execute(|| {
		Session::roll_until_active_era(3);
		// Model a chain whose storage predates the cutoff item: genesis initializes the test
		// value to 0, so clear it to reproduce the unset state the migration must handle.
		WeightedPointsFormulaStartEra::<Test>::kill();

		Migration::<Test>::on_runtime_upgrade();

		// Active era at upgrade time was 3 ⇒ cutoff = 4. Era 3 (which may already
		// have points credited without a denominator) stays on the legacy formula.
		assert_eq!(WeightedPointsFormulaStartEra::<Test>::get(), Some(4));
	});
}

// ===== System-schedule merge-on-exhaustion tests =====
//
// When all System vesting slots are occupied, `add_to_vesting` falls back to merging
// the incoming amount into the existing System schedule with the closest ending block.
// The payment always succeeds.

/// Total System schedule capacity derived from the vesting config.
const MAX_SYSTEM_VESTING_SCHEDULES: u32 = <Test as pallet_vesting::Config>::MAX_VESTING_SCHEDULES -
	<Test as pallet_vesting::Config>::MAX_PUBLIC_VESTING_SCHEDULES;

/// Insert `n` dummy System vesting schedules for `who` (starting_block = 0, locked = 1).
fn fill_system_vesting_slots(who: &AccountId, n: u32) {
	let dummy = pallet_vesting::VestingInfo::new(
		1_u128, // locked: above MinVestedTransfer
		1_u128, // per_block
		0_u64,  // starting_block: 0 — distinct from real epoch-start blocks (> 0)
	);
	pallet_vesting::Vesting::<Test>::mutate(who, |entry| {
		let schedules = entry.get_or_insert_with(Default::default);
		for _ in 0..n {
			let _ = schedules.try_push((dummy, pallet_vesting::VestingKind::System));
		}
	});
}

#[test]
fn incentive_merged_into_existing_schedule_when_system_slots_exhausted() {
	// When all System vesting slots are occupied, `add_to_vesting` must not drop the
	// payment and instead merge its amount to the schedule with the closest end block.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11_u64;

		VestingBondingPeriods::set(1);
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		Session::roll_until_active_era(3);
		let _ = staking_events_since_last_call();

		// GIVEN: all System vesting slots for alice are exhausted.
		fill_system_vesting_slots(&alice, MAX_SYSTEM_VESTING_SCHEDULES);

		// WHEN: payout era 2.
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: payment succeeds — no Dropped event.
		assert!(
			!events.iter().any(|e| matches!(
				e,
				Event::Unexpected(UnexpectedKind::ValidatorIncentiveDropped { stash, .. })
				if *stash == alice
			)),
			"ValidatorIncentiveDropped must NOT be emitted: merge-on-exhaustion must succeed"
		);

		let paid_amount = events
			.iter()
			.find_map(|e| match e {
				Event::ValidatorIncentivePaid { validator_stash, amount, .. }
					if *validator_stash == alice =>
				{
					Some(*amount)
				},
				_ => None,
			})
			.expect("ValidatorIncentivePaid must be emitted even when slots are exhausted");

		assert!(paid_amount > 0, "paid amount must be positive");

		// THEN: alice's System schedules contain the incentive amount (merged into one slot).
		let schedules_after = pallet_vesting::Vesting::<Test>::get(&alice).unwrap_or_default();
		let system_schedules_after: Vec<_> = schedules_after
			.iter()
			.filter(|(_, k)| *k == pallet_vesting::VestingKind::System)
			.collect();

		// At least one active System schedule must exist.
		assert!(
			!system_schedules_after.is_empty(),
			"at least one System schedule must exist after merge"
		);

		// The merged schedule's locked amount must be at least the incentive.
		let max_locked =
			system_schedules_after.iter().map(|(vi, _)| vi.locked()).max().unwrap_or(0);
		assert!(
			max_locked >= paid_amount,
			"merged schedule locked ({max_locked}) must be >= incentive ({paid_amount})"
		);

		// No new slot was inserted: slot count must be ≤ what we pre-filled.
		assert!(
			system_schedules_after.len() as u32 <= MAX_SYSTEM_VESTING_SCHEDULES,
			"merge must not insert a new slot beyond the cap"
		);
	});
}

#[test]
fn consecutive_payouts_with_full_slots_both_succeed() {
	// The case for two consecutive eras, both paid while System slots are full.
	// Both must emit `ValidatorIncentivePaid`, while the incentive amounts must
	// accumulate in the merged schedule and not be silently discarded.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11_u64;

		VestingBondingPeriods::set(1);
		setup_incentive_with_budget(45, 5);

		// --- Era 2 ---
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);

		// --- Era 3 (same bonding period as era 2) ---
		Session::roll_until_active_era(3);
		Eras::<Test>::reward_active_era(vec![(alice, 1)]);
		Session::roll_until_active_era(4);
		let _ = staking_events_since_last_call();

		// GIVEN: all System vesting slots for alice are exhausted *before* the first payout.
		fill_system_vesting_slots(&alice, MAX_SYSTEM_VESTING_SCHEDULES);
		assert_eq!(
			pallet_vesting::Vesting::<Test>::get(&alice)
				.unwrap_or_default()
				.iter()
				.filter(|(_, k)| *k == pallet_vesting::VestingKind::System)
				.count() as u32,
			MAX_SYSTEM_VESTING_SCHEDULES,
			"System slots should be full before first payout"
		);

		// WHEN: payout era 2 (first consecutive payout at full capacity).
		make_all_reward_payment(2);
		let era2_events = staking_events_since_last_call();

		// THEN: no Dropped event for era 2.
		assert!(
			!era2_events.iter().any(|e| matches!(
				e,
				Event::Unexpected(UnexpectedKind::ValidatorIncentiveDropped { stash, .. })
				if *stash == alice
			)),
			"era 2: ValidatorIncentiveDropped must NOT be emitted when slots are exhausted"
		);

		let era2_paid = era2_events
			.iter()
			.find_map(|e| match e {
				Event::ValidatorIncentivePaid { validator_stash, amount, .. }
					if *validator_stash == alice =>
				{
					Some(*amount)
				},
				_ => None,
			})
			.expect("era 2: ValidatorIncentivePaid must be emitted even when slots are exhausted");
		assert!(era2_paid > 0, "era 2: paid amount must be positive");

		// Slot count must still be at cap after the first merge.
		let system_count_after_era2 = pallet_vesting::Vesting::<Test>::get(&alice)
			.unwrap_or_default()
			.iter()
			.filter(|(_, k)| *k == pallet_vesting::VestingKind::System)
			.count() as u32;
		assert!(
			system_count_after_era2 <= MAX_SYSTEM_VESTING_SCHEDULES,
			"slot count must not exceed cap after era 2 merge"
		);

		// WHEN: payout era 3 (second consecutive payout, slots still full after era 2 merge).
		make_all_reward_payment(3);
		let era3_events = staking_events_since_last_call();

		// THEN: no Dropped event for era 3 either.
		assert!(
			!era3_events.iter().any(|e| matches!(
				e,
				Event::Unexpected(UnexpectedKind::ValidatorIncentiveDropped { stash, .. })
				if *stash == alice
			)),
			"era 3: ValidatorIncentiveDropped must NOT be emitted when slots are exhausted"
		);

		let era3_paid = era3_events
			.iter()
			.find_map(|e| match e {
				Event::ValidatorIncentivePaid { validator_stash, amount, .. }
					if *validator_stash == alice =>
				{
					Some(*amount)
				},
				_ => None,
			})
			.expect("era 3: ValidatorIncentivePaid must be emitted even when slots are exhausted");
		assert!(era3_paid > 0, "era 3: paid amount must be positive");

		// Slot count stays at cap after the second merge too.
		let system_count_after_era3 = pallet_vesting::Vesting::<Test>::get(&alice)
			.unwrap_or_default()
			.iter()
			.filter(|(_, k)| *k == pallet_vesting::VestingKind::System)
			.count() as u32;
		assert!(
			system_count_after_era3 <= MAX_SYSTEM_VESTING_SCHEDULES,
			"slot count must not exceed cap after era 3 merge"
		);
	});
}
