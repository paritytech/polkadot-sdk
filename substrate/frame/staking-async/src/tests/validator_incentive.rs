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

// ===== Helpers =====

fn setup_incentive_config() {
	assert_ok!(Staking::set_validator_self_stake_incentive_config(
		RuntimeOrigin::root(),
		ConfigOp::Set(30_000),
		ConfigOp::Set(100_000),
		ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
	));
}

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

fn staker_reward_for(stash: AccountId, events: &[Event<Test>]) -> Option<Balance> {
	events.iter().find_map(|e| match e {
		Event::Rewarded { stash: s, amount, .. } if *s == stash => Some(*amount),
		_ => None,
	})
}

fn incentive_paid_for(stash: AccountId, events: &[Event<Test>]) -> Option<Balance> {
	incentive_paid_details(stash, events).map(|(amount, _)| amount)
}

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
fn config_set_noop_remove_works() {
	ExtBuilder::default().build_and_execute(|| {
		// WHEN: set all params.
		assert_ok!(Staking::set_validator_self_stake_incentive_config(
			RuntimeOrigin::root(),
			ConfigOp::Set(30_000),
			ConfigOp::Set(100_000),
			ConfigOp::Set(Perbill::from_rational(1u32, 2u32)),
		));

		// THEN: values stored.
		assert_eq!(OptimumSelfStake::<Test>::get(), 30_000);
		assert_eq!(HardCapSelfStake::<Test>::get(), 100_000);
		assert_eq!(SelfStakeSlopeFactor::<Test>::get(), Perbill::from_rational(1u32, 2u32));

		// WHEN: noop.
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

		// WHEN: payout.
		let alice_before = asset::total_balance::<Test>(&alice);
		make_all_reward_payment(2);
		let events = staking_events_since_last_call();

		// THEN: validator gets both staker reward + incentive bonus.
		let staker = staker_reward_for(alice, &events).expect("staker reward");
		let incentive = incentive_paid_for(alice, &events).expect("incentive bonus");
		assert_eq!(asset::total_balance::<Test>(&alice) - alice_before, staker + incentive);

		// THEN: nominator gets staker reward only.
		assert!(staker_reward_for(bob, &events).is_some());
		assert!(incentive_paid_for(bob, &events).is_none());
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
		assert!(ErasValidatorIncentiveBudget::<Test>::get(2) > 0);
	});
}

#[test]
fn zero_reward_points_means_no_payout() {
	ExtBuilder::default().build_and_execute(|| {
		let alice = 11; // validator (no reward points)
		let bob = 21; // validator (has reward points)

		// GIVEN: only bob earns points.
		setup_incentive_with_budget(45, 5);
		Eras::<Test>::reward_active_era(vec![(bob, 1)]);
		Session::roll_until_active_era(2);
		let _ = staking_events_since_last_call();

		// WHEN: payout.
		make_all_reward_payment(1);
		let events = staking_events_since_last_call();

		// THEN: alice gets nothing, bob gets staker reward.
		assert!(staker_reward_for(alice, &events).is_none());
		assert!(incentive_paid_for(alice, &events).is_none());
		assert!(staker_reward_for(bob, &events).is_some());
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
		let weight = ErasValidatorIncentiveWeight::<Test>::get(2, alice).unwrap();
		assert_eq!(weight, 31);

		// THEN: incentive is paid.
		let _ = staking_events_since_last_call();
		make_all_reward_payment(2);
		assert!(incentive_paid_for(alice, &staking_events_since_last_call()).is_some());
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
		assert!(asset::total_balance::<Test>(&reward_account) - before >= incentive);
	});
}

// ===== Multi-page election =====

#[test]
fn multi_page_election_does_not_overwrite_incentive_weight() {
	ExtBuilder::default().exposures_page_size(1).build_and_execute(|| {
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
			let weight = ErasValidatorIncentiveWeight::<Test>::get(planned_era, alice).unwrap();
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

			// THEN: weight not overwritten by page 2 (own=0).
			assert_eq!(
				ErasValidatorIncentiveWeight::<Test>::get(planned_era, alice).unwrap(),
				weight
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

			// THEN: weight set from overview when own-stake arrives.
			let weight = ErasValidatorIncentiveWeight::<Test>::get(planned_era, alice).unwrap();
			assert!(weight > 0);
		});
	});
}
