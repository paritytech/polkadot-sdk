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

//! Tests for validator self-stake incentive (hold-based auto-pay).

use super::*;
use crate::{
	asset,
	maturation::matured_fraction,
	session_rotation::{EraElectionPlanner, Eras, Rotator},
};

/// Idle hold remaining right after auto-pay delivers `incentive` for `era`: a freshly-merged
/// bucket already has `matured_fraction` of it releasable if its period started earlier.
fn expected_hold_after_delivery(incentive: Balance, era: EraIndex) -> Balance {
	let bonding_duration = BondingDuration::get();
	let period = era.checked_div(bonding_duration).unwrap_or(0);
	let matured = matured_fraction(period, era, VestingBondingPeriods::get(), bonding_duration)
		.mul_floor(incentive);
	incentive - matured
}

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
		let _ = staking_events_since_last_call();
		let alice_before = asset::total_balance::<Test>(&alice);

		// WHEN: era 2 ends — incentive is auto-paid as a hold (no claim needed).
		Session::roll_until_active_era(3);
		let auto_pay_events = staking_events_since_last_call();

		// THEN: validator's incentive landed as a `ValidatorIncentive` hold (most of it still
		// vesting; a sliver is already matured since period 0 started before era 2).
		let incentive = incentive_paid_for(alice, &auto_pay_events).expect("incentive bonus");
		assert_eq!(
			asset::incentive_on_hold::<Test>(&alice),
			expected_hold_after_delivery(incentive, 2)
		);
		assert_eq!(asset::total_balance::<Test>(&alice) - alice_before, incentive);
		assert!(incentive_paid_for(bob, &auto_pay_events).is_none());

		// THEN: era pot deducted by exactly the sum of all incentives paid out (alice and the
		// other elected validator, 21, who earns equal points and so an equal share).
		let era_pot = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
			2,
			RewardKind::ValidatorSelfStake,
		));
		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		let total_incentive_paid: Balance = auto_pay_events
			.iter()
			.filter_map(|e| match e {
				Event::ValidatorIncentivePaid { amount, .. } => Some(*amount),
				_ => None,
			})
			.sum();
		assert_eq!(Balances::free_balance(&era_pot), budget - total_incentive_paid);

		// AND: the staker-reward payout path (unaffected by the incentive change) still requires
		// an explicit claim.
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();
		let staker = staker_reward_for(alice, &events).expect("staker reward");

		// THEN: nominator gets staker reward only (no incentive).
		// Bob (500 stake) gets less than alice (1000 stake) from staker rewards.
		let bob_reward = staker_reward_for(bob, &events).expect("nominator should receive reward");
		assert!(
			bob_reward < staker,
			"nominator ({bob_reward}) should get less than validator ({staker})"
		);

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
		let _ = staking_events_since_last_call();
		// WHEN: era 1 ends — no incentive budget, so nothing is auto-paid.
		Session::roll_until_active_era(2);
		let era1 = staking_events_since_last_call();
		assert!(incentive_paid_for(alice, &era1).is_none());

		// WHEN: governance enables 10% incentive for era 2.
		setup_incentive_with_budget(40, 10);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		let _ = staking_events_since_last_call();
		// WHEN: era 2 ends — incentive is auto-paid.
		Session::roll_until_active_era(3);
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

		// Alice and bob both elected with equal self-stake, so both have equal weights
		// and the sum counts both.
		let bob_weight = ErasValidatorIncentiveWeight::<Test>::get(2, bob).unwrap();
		assert_eq!(ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap(), bob_weight);
		assert_eq!(ErasSumValidatorIncentiveWeight::<Test>::get(2), 2 * bob_weight);

		let pot: AccountId = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
			2,
			RewardKind::ValidatorSelfStake,
		));
		let _ = staking_events_since_last_call();

		// WHEN: era 2 ends — budget is snapshotted and incentive is auto-paid.
		Session::roll_until_active_era(3);
		let auto_pay_events = staking_events_since_last_call();
		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		assert_eq!(budget, 750);

		// THEN: alice gets no incentive share — no reward points, even though she was
		// elected and has self-stake.
		assert_eq!(incentive_paid_for(alice, &auto_pay_events), None);
		// THEN: bob is the only validator with points, so under the weighted-mean
		// formula his share (w_b · 1) / (w_b · 1) = 1 — he receives the full budget.
		// Pot is depleted (modulo Perbill rounding dust).
		assert_eq!(incentive_paid_for(bob, &auto_pay_events), Some(budget));
		assert_eq!(Balances::free_balance(&pot), 0);

		// AND: staker rewards are unaffected — alice still gets nothing, bob gets his share.
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();
		assert_eq!(staker_reward_for(alice, &events), None);
		assert!(staker_reward_for(bob, &events).unwrap() > 0);
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

		// THEN: weight = √1000 ≈ 31.
		let incentive_weight = ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();
		assert_eq!(incentive_weight, 31);

		// THEN: incentive is auto-paid at era end. Two validators have equal weight so each
		// gets half of the incentive budget (750 = 5% of era issuance 15_000).
		let _ = staking_events_since_last_call();
		Session::roll_until_active_era(3);
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
		let _ = staking_events_since_last_call();
		let before = asset::total_balance::<Test>(&reward_account);

		// WHEN: era 2 ends — incentive is auto-paid to the custom account as a hold.
		Session::roll_until_active_era(3);
		let auto_pay_events = staking_events_since_last_call();

		// THEN: event records custom account; incentive landed as a hold (an `Account(x)`
		// destination is release-only — no stash ledger to bond it into).
		let (incentive, dest) = incentive_paid_details(alice, &auto_pay_events).expect("incentive");
		assert_eq!(dest, RewardDestination::Account(reward_account));
		assert_eq!(
			asset::incentive_on_hold::<Test>(&reward_account),
			expected_hold_after_delivery(incentive, 2)
		);
		assert_eq!(asset::total_balance::<Test>(&reward_account) - before, incentive);

		// AND: staker reward also goes to the custom account (separate, on-demand claim).
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();
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
fn validator_incentive_delivered_in_one_shot_at_era_end() {
	// Unlike staker rewards, the incentive is no longer prorated per exposure page: auto-pay
	// delivers the full-era share in a single hold at era end, regardless of paging.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator

		// GIVEN: incentive enabled.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);

		let validator_incentive_weight =
			ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();
		let sum_incentive_weight = ErasSumValidatorIncentiveWeight::<Test>::get(2);
		let _ = staking_events_since_last_call();

		// WHEN: era 2 ends — budget is snapshotted and incentive auto-paid.
		Session::roll_until_active_era(3);
		let events = staking_events_since_last_call();

		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		let expected_total =
			Perbill::from_rational(validator_incentive_weight, sum_incentive_weight)
				.mul_floor(budget);

		// THEN: exactly one `ValidatorIncentivePaid` event for alice, matching share × budget.
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
		assert_eq!(alice_amounts.len(), 1, "expected a single incentive delivery per era");
		assert_eq!(alice_amounts[0], expected_total);
	});
}

#[test]
fn incentive_unaffected_by_multiple_exposure_pages() {
	// With `exposures_page_size(1)`, alice's extra nominators force her exposure to span ≥ 2
	// pages. Unlike staker rewards, incentive auto-pay is not prorated per page — it always
	// delivers the full `share × budget` in one hold at era end, regardless of paging.
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

			let alice_pages = Eras::<Test>::exposure_page_count(2, &alice);
			assert!(alice_pages >= 2, "expected alice to have ≥ 2 pages, got {alice_pages}");

			// Equal own-stake & equal points → share = w_a / (w_a + w_b).
			let alice_weight = ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();
			let sum_weight = ErasSumValidatorIncentiveWeight::<Test>::get(2);
			let _ = staking_events_since_last_call();

			// WHEN: era 2 ends — budget is snapshotted and incentive auto-paid.
			Session::roll_until_active_era(3);
			let events = staking_events_since_last_call();

			let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
			let expected_total = Perbill::from_rational(alice_weight, sum_weight).mul_floor(budget);

			// THEN: exactly one incentive delivery for alice, unaffected by her page count.
			assert_eq!(incentive_paid_for(alice, &events), Some(expected_total));
		});
}

// ===== Edge cases =====

#[test]
fn chilling_after_era_end_does_not_affect_already_delivered_incentive() {
	// Auto-pay delivers era 2's incentive automatically when era 2 ends, using the
	// `ErasValidatorIncentiveWeight` snapshotted at election time — chilling afterwards must
	// not claw back (or otherwise affect) what was already held.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator

		// GIVEN: alice earns weight in era 2.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		let _ = staking_events_since_last_call();
		assert!(ErasValidatorIncentiveWeight::<Test>::get(2, alice).is_some());

		// WHEN: era 2 ends — alice's incentive is auto-paid.
		Session::roll_until_active_era(3);
		let events = staking_events_since_last_call();
		let incentive = incentive_paid_for(alice, &events).expect("incentive auto-paid at era end");
		let hold_after_delivery = asset::incentive_on_hold::<Test>(&alice);
		assert_eq!(hold_after_delivery, expected_hold_after_delivery(incentive, 2));

		// WHEN: alice chills afterwards.
		assert_ok!(Staking::chill(RuntimeOrigin::signed(alice)));
		assert!(!Validators::<Test>::contains_key(&alice));

		// THEN: her already-delivered incentive hold is untouched.
		assert_eq!(asset::incentive_on_hold::<Test>(&alice), hold_after_delivery);
	});
}

#[test]
fn payee_change_before_era_end_uses_new_destination() {
	// Auto-pay resolves the payee at era-end (when the incentive is actually delivered), not
	// at points-crediting time — so a payee change earlier in the era takes effect.
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

		// WHEN: payee changes to new_account before era 2 ends.
		assert_ok!(Staking::set_payee(
			RuntimeOrigin::signed(alice),
			RewardDestination::Account(new_account)
		));
		let old_before = asset::total_balance::<Test>(&old_account);
		let new_before = asset::total_balance::<Test>(&new_account);
		let _ = staking_events_since_last_call();

		// WHEN: era 2 ends — auto-pay resolves the payee as of now.
		Session::roll_until_active_era(3);
		let events = staking_events_since_last_call();

		// THEN: incentive goes to new_account (payee at auto-pay time), held there.
		let (incentive, dest) = incentive_paid_details(alice, &events).expect("incentive");
		assert_eq!(dest, RewardDestination::Account(new_account));
		assert_eq!(asset::total_balance::<Test>(&old_account), old_before);
		assert_eq!(
			asset::incentive_on_hold::<Test>(&new_account),
			expected_hold_after_delivery(incentive, 2)
		);
		assert_eq!(asset::total_balance::<Test>(&new_account) - new_before, incentive);
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

		// WHEN: alice's payee is missing before auto-pay runs.
		Payee::<Test>::remove(&alice);
		let _ = staking_events_since_last_call();

		// WHEN: era 2 ends — auto-pay runs.
		Session::roll_until_active_era(3);

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
		let _ = staking_events_since_last_call();

		// WHEN: era 2 ends — incentive is auto-paid.
		Session::roll_until_active_era(3);
		let auto_pay_events = staking_events_since_last_call();

		// THEN: alice — gated out, no incentive.
		assert_eq!(incentive_paid_for(alice, &auto_pay_events), None);
		// THEN: bob is the only non-zero-weight earner → share = 1 → full budget.
		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		assert_eq!(incentive_paid_for(bob, &auto_pay_events), Some(budget));

		// AND: staker reward is independent of incentive weight → alice still gets one.
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();
		assert!(staker_reward_for(alice, &events).is_some());
	});
}

// ===== Idle bucket / maturation tests =====
//
// `auto_pay_incentive` is exercised directly (bypassing full era rotation) so a bucket's
// maturation can be driven to specific eras without setting up an election every step.

#[test]
fn idle_incentive_recorded_in_bucket_for_its_period() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator

		// GIVEN: incentive enabled, validator has weight.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		let _ = staking_events_since_last_call();

		// WHEN: era 2 ends — incentive is auto-paid.
		Session::roll_until_active_era(3);
		let events = staking_events_since_last_call();
		let incentive = incentive_paid_for(alice, &events).expect("incentive bonus");

		// THEN: recorded in `IdleIncentiveBuckets` under period = era / BondingDuration.
		let period = 2 / BondingDuration::get();
		let buckets = IdleIncentiveBuckets::<Test>::get(&alice);
		let bucket = buckets.get(&period).expect("bucket recorded for the delivery period");
		assert_eq!(bucket.total, incentive);
		// A sliver already matured immediately (period 0 started before era 2).
		assert_eq!(bucket.released, incentive - expected_hold_after_delivery(incentive, 2));
	});
}

#[test]
fn idle_incentive_bucket_matures_and_prunes_after_full_window() {
	// A bucket is keyed by `era / BondingDuration`; deliveries in the same bonding period merge
	// into one bucket, and a bucket is pruned once `releasable_at` has released its `total`.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator
		let window = VestingBondingPeriods::get() * BondingDuration::get();

		// Use the legacy stake-only formula so a single-validator delivery is trivial: weight ==
		// total weight ⇒ alice gets the full budget every time.
		WeightedPointsFormulaStartEra::<Test>::put(u32::MAX);
		let deliver = |era: EraIndex, budget: Balance| {
			ErasValidatorIncentiveBudget::<Test>::insert(era, budget);
			ErasValidatorIncentiveWeight::<Test>::insert(era, alice, 1u128);
			ErasSumValidatorIncentiveWeight::<Test>::insert(era, 1u128);
			let pot = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
				era,
				RewardKind::ValidatorSelfStake,
			));
			let _ =
				<Balances as frame_support::traits::fungible::Mutate<_>>::mint_into(&pot, budget);
			Staking::auto_pay_incentive(era);
		};

		// GIVEN: era 0 delivers into period 0's bucket (period start = era 0, so nothing has
		// matured yet).
		deliver(0, 1_000);
		let buckets = IdleIncentiveBuckets::<Test>::get(&alice);
		assert_eq!(buckets.len(), 1);
		assert_eq!(buckets.get(&0), Some(&IncentiveBucket { total: 1_000, released: 0 }));
		assert_eq!(asset::incentive_on_hold::<Test>(&alice), 1_000);

		// WHEN: a later delivery lands exactly at the window's end, which also drives release for
		// period 0 (auto-pay releases matured idle incentive for accounts it touches).
		deliver(window, 500);

		// THEN: period 0 is fully matured and pruned; only the fresh period's bucket remains.
		let buckets = IdleIncentiveBuckets::<Test>::get(&alice);
		assert_eq!(buckets.len(), 1);
		assert!(buckets.get(&0).is_none(), "period 0 bucket should be pruned once fully matured");
		let new_period = window / BondingDuration::get();
		assert_eq!(buckets.get(&new_period), Some(&IncentiveBucket { total: 500, released: 0 }));

		// AND: the fully-matured 1_000 moved to `free`; the fresh 500 stays held.
		assert_eq!(asset::incentive_on_hold::<Test>(&alice), 500);

		// Restore genesis default so the post-test try-state hook is unaffected.
		WeightedPointsFormulaStartEra::<Test>::put(0);
	});
}

// ===== `release_incentive` extrinsic tests =====

#[test]
fn release_incentive_frees_matured_incentive_for_departed_validator() {
	// A validator who has chilled (left the active set, no longer covered by auto-pay) can still
	// have their vesting idle incentive pulled via the permissionless extrinsic.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator
		let bystander = 999; // unrelated signed caller

		// GIVEN: alice's era-2 incentive is auto-paid at era 3's start.
		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		let _ = staking_events_since_last_call();
		Session::roll_until_active_era(3);
		let incentive = incentive_paid_for(alice, &staking_events_since_last_call())
			.expect("incentive auto-paid");
		let hold_after_delivery = asset::incentive_on_hold::<Test>(&alice);
		let free_before = Balances::free_balance(&alice);

		// GIVEN: alice chills — leaves the active set.
		assert_ok!(Staking::chill(RuntimeOrigin::signed(alice)));

		// WHEN: a bystander releases alice's matured idle incentive on her behalf.
		assert_ok!(Staking::release_incentive(RuntimeOrigin::signed(bystander), alice));

		// THEN: hold decreases and free increases by exactly the newly-matured amount. The bucket
		// was created in era 2's period; the active era (3) has since moved one further than the
		// auto-delivery's own release baseline (era 2), so a bit more has now matured.
		let bonding_duration = BondingDuration::get();
		let period = 2u32.checked_div(bonding_duration).unwrap_or(0);
		let matured_at_release =
			matured_fraction(period, 3, VestingBondingPeriods::get(), bonding_duration)
				.mul_floor(incentive);
		let expected_hold = incentive - matured_at_release;
		assert!(expected_hold < hold_after_delivery, "more should mature by era 3 than era 2");
		let released = hold_after_delivery - expected_hold;
		assert_eq!(asset::incentive_on_hold::<Test>(&alice), expected_hold);
		assert_eq!(Balances::free_balance(&alice), free_before + released);
		assert!(staking_events_since_last_call().iter().any(|e| matches!(
			e,
			Event::ValidatorIncentiveReleased { who, amount }
				if *who == alice && *amount == released
		)));
	});
}

#[test]
fn release_incentive_is_permissionless() {
	// Any signed account may call `release_incentive` for any `target`, not just the target
	// itself.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator, still active (not chilled)
		let bystander = 424_242; // unrelated signed caller

		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		let hold_before = asset::incentive_on_hold::<Test>(&alice);

		// WHEN: bystander (not alice, not her controller) calls release_incentive for alice.
		assert_ok!(Staking::release_incentive(RuntimeOrigin::signed(bystander), alice));

		// THEN: succeeds; incentive moves to alice's free balance, not the caller's.
		assert!(asset::incentive_on_hold::<Test>(&alice) < hold_before);
		assert_eq!(Balances::free_balance(&bystander), 0);
	});
}

#[test]
fn release_incentive_prunes_fully_matured_bucket() {
	// Once a bucket has fully matured, `release_incentive` releases everything held and prunes
	// the bucket from `IdleIncentiveBuckets`.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator
		let window = VestingBondingPeriods::get() * BondingDuration::get();

		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);
		Session::roll_until_active_era(3);
		assert_ok!(Staking::chill(RuntimeOrigin::signed(alice)));

		// WHEN: enough eras pass for the (period-0) bucket to fully mature.
		Session::roll_until_active_era(window + 1);
		let hold_before = asset::incentive_on_hold::<Test>(&alice);
		assert!(hold_before > 0, "bucket should still be idle before release");
		let free_before = Balances::free_balance(&alice);

		assert_ok!(Staking::release_incentive(RuntimeOrigin::signed(999), alice));

		// THEN: fully released to free, and the bucket is pruned.
		assert_eq!(asset::incentive_on_hold::<Test>(&alice), 0);
		assert_eq!(Balances::free_balance(&alice), free_before + hold_before);
		assert!(IdleIncentiveBuckets::<Test>::get(&alice).is_empty());
	});
}

#[test]
fn release_incentive_errors_without_idle_buckets() {
	ExtBuilder::default().build_and_execute(|| {
		let target = 31; // never earned any incentive in this test
		assert!(IdleIncentiveBuckets::<Test>::get(&target).is_empty());

		assert_noop!(
			Staking::release_incentive(RuntimeOrigin::signed(999), target),
			Error::<Test>::NoIncentiveToRelease
		);
	});
}

// ===== Defensive path tests =====

#[test]
#[should_panic(expected = "Validator incentive transfer failed")]
fn defensive_panic_on_transfer_failure() {
	// `auto_pay_incentive` is invoked directly (bypassing era rotation) so the era-2 incentive
	// pot can be left unfunded — normally it is snapshotted from the general pot and auto-paid
	// in the same era-ending transition, with no window in between for a test to drain it.
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator

		setup_incentive_with_budget(45, 5);
		Session::roll_until_active_era(2);
		Eras::<Test>::reward_active_era(vec![(alice, 1), (21, 1)]);

		// GIVEN: a non-zero budget but an empty era-2 incentive pot.
		ErasValidatorIncentiveBudget::<Test>::insert(2, 1_000);

		// THEN: auto-pay panics on defensive when the incentive transfer fails.
		Staking::auto_pay_incentive(2);
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

		let alice_weight = ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();
		let bob_weight = ErasValidatorIncentiveWeight::<Test>::get(2, bob).unwrap();
		let sum_weight = ErasSumValidatorIncentiveWeight::<Test>::get(2);
		assert_eq!(alice_weight, bob_weight);
		assert_eq!(sum_weight, alice_weight + bob_weight);
		let _ = staking_events_since_last_call();

		// WHEN: era 2 ends — budget is snapshotted and incentive is auto-paid.
		Session::roll_until_active_era(3);
		let events = staking_events_since_last_call();

		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		let pot = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
			2,
			RewardKind::ValidatorSelfStake,
		));

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
		let _ = staking_events_since_last_call();

		// WHEN: era 2 ends — budget is snapshotted and incentive is auto-paid.
		Session::roll_until_active_era(3);
		let events = staking_events_since_last_call();

		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		let pot = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
			2,
			RewardKind::ValidatorSelfStake,
		));

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
		let _ = staking_events_since_last_call();

		// WHEN: era 2 ends — budget is snapshotted and incentive is auto-paid.
		Session::roll_until_active_era(3);
		let events = staking_events_since_last_call();

		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		let pot = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
			2,
			RewardKind::ValidatorSelfStake,
		));

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
		// Equal stake → equal weights for bob and carol; alice's weight is multiplied
		// by zero points so it doesn't enter the denominator either way.
		let bob_weight = ErasValidatorIncentiveWeight::<Test>::get(2, bob).unwrap();
		let carol_weight = ErasValidatorIncentiveWeight::<Test>::get(2, carol).unwrap();
		let _ = staking_events_since_last_call();

		// WHEN: era 2 ends — budget is snapshotted and incentive is auto-paid.
		Session::roll_until_active_era(3);
		let events = staking_events_since_last_call();

		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		let pot = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
			2,
			RewardKind::ValidatorSelfStake,
		));

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
		let _ = staking_events_since_last_call();

		// WHEN: era 2 ends — budget is snapshotted and incentive is auto-paid.
		Session::roll_until_active_era(3);
		let events = staking_events_since_last_call();

		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);
		let pot = <Test as Config>::RewardPots::pot_account(RewardPot::Era(
			2,
			RewardKind::ValidatorSelfStake,
		));

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
		// `calculate_validator_incentive`.
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
// `calculate_validator_incentive`: eras strictly older than the cutoff
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

		// WHEN: pin era 2 as pre-cutoff and wipe its weighted-points denominator to mimic an era
		// whose points were credited before that denominator was maintained.
		WeightedPointsFormulaStartEra::<Test>::put(3);
		ErasSumWeightedPoints::<Test>::remove(2);

		let alice_weight = ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();
		let bob_weight = ErasValidatorIncentiveWeight::<Test>::get(2, bob).unwrap();
		let sum_weight = ErasSumValidatorIncentiveWeight::<Test>::get(2);
		let _ = staking_events_since_last_call();

		// WHEN: era 2 ends — auto-pay runs under the legacy formula.
		Session::roll_until_active_era(3);
		let events = staking_events_since_last_call();

		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);

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
		let _ = staking_events_since_last_call();

		// WHEN: era 2 ends — incentive is auto-paid under the new formula.
		Session::roll_until_active_era(3);
		let events = staking_events_since_last_call();

		let budget = ErasValidatorIncentiveBudget::<Test>::get(2);

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

		// WHEN: corrupt the denominator to zero on what will be a weighted-points era.
		let valid_sum = ErasSumWeightedPoints::<Test>::get(2);
		ErasSumWeightedPoints::<Test>::remove(2);
		let _ = staking_events_since_last_call();

		// WHEN: era 2 ends — auto-pay runs against the corrupted denominator.
		Session::roll_until_active_era(3);
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

		// Model a pre-cutoff era whose weighted-points denominator was not maintained.
		WeightedPointsFormulaStartEra::<Test>::put(3);
		ErasSumWeightedPoints::<Test>::remove(2);
		let _ = staking_events_since_last_call();

		// WHEN: era 2 ends — auto-pay runs under the legacy formula.
		Session::roll_until_active_era(3);
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
