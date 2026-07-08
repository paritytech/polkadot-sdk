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

//! Tests for `merge_staked`.

use super::*;
use crate::{asset, session_rotation::Eras, UnappliedSlash};
use frame_support::traits::Currency;
use mock::Session;
use sp_runtime::{bounded_vec, Perbill};

const STASH_ALICE: AccountId = 60;
const STASH_BOB: AccountId = 61;
const E2E_STASH_ALICE: AccountId = 102;
const E2E_STASH_BOB: AccountId = 101;

fn e2e_ext_builder() -> ExtBuilder {
	ExtBuilder::default()
		.nominate(true)
		.session_per_era(3)
		.add_staker(
			E2E_STASH_ALICE,
			1_000,
			StakerStatus::<AccountId>::Nominator(vec![11, 21]),
		)
}

fn setup_nominator(stash: AccountId, amount: Balance, validator: AccountId) {
	let _ = asset::set_stakeable_balance::<T>(&stash, amount);
	Balances::make_free_balance_be(&stash, amount + ExistentialDeposit::get());
	assert_ok!(Staking::bond(RuntimeOrigin::signed(stash), amount, RewardDestination::Staked));
	assert_ok!(Staking::nominate(RuntimeOrigin::signed(stash), vec![validator]));
}

#[test]
fn merge_staked_partial_transfer_moves_hold_balance() {
	ExtBuilder::default().build_and_execute(|| {
		setup_nominator(STASH_ALICE, 100, 11);
		setup_nominator(STASH_BOB, 50, 11);

		let bob_free_before = asset::free_to_stake::<T>(&STASH_BOB);

		assert_ok!(Staking::merge_staked(RuntimeOrigin::signed(STASH_ALICE), STASH_BOB, 30));

		assert_eq!(asset::staked::<T>(&STASH_ALICE), 70);
		assert_eq!(asset::staked::<T>(&STASH_BOB), 80);
		assert_eq!(asset::free_to_stake::<T>(&STASH_BOB), bob_free_before);

		let ledger_bob = Ledger::<T>::get(&STASH_BOB).unwrap();
		assert_eq!(ledger_bob.active, 80);
		assert_eq!(ledger_bob.total, 80);

		let ledger_alice = Ledger::<T>::get(&STASH_ALICE).unwrap();
		assert_eq!(ledger_alice.active, 70);
		assert_eq!(ledger_alice.total, 70);
	});
}

#[test]
fn merge_staked_full_transfer_kills_source_stash() {
	ExtBuilder::default().build_and_execute(|| {
		setup_nominator(STASH_ALICE, 100, 11);
		setup_nominator(STASH_BOB, 50, 11);

		assert_ok!(Staking::merge_staked(RuntimeOrigin::signed(STASH_ALICE), STASH_BOB, 100));

		assert_eq!(Staking::bonded(&STASH_ALICE), None);
		assert!(Ledger::<T>::get(&STASH_ALICE).is_none());
		assert!(!Nominators::<T>::contains_key(&STASH_ALICE));
		assert_eq!(asset::staked::<T>(&STASH_ALICE), 0);

		let ledger_bob = Ledger::<T>::get(&STASH_BOB).unwrap();
		assert_eq!(ledger_bob.active, 150);
		assert_eq!(ledger_bob.total, 150);
		assert_eq!(asset::staked::<T>(&STASH_BOB), 150);
	});
}

#[test]
fn merge_staked_to_self_fails() {
	ExtBuilder::default().build_and_execute(|| {
		setup_nominator(STASH_ALICE, 100, 11);
		assert_noop!(
			Staking::merge_staked(RuntimeOrigin::signed(STASH_ALICE), STASH_ALICE, 50),
			Error::<T>::MergeIdentical
		);
	});
}

#[test]
fn merge_staked_zero_amount_fails() {
	ExtBuilder::default().build_and_execute(|| {
		setup_nominator(STASH_ALICE, 100, 11);
		setup_nominator(STASH_BOB, 50, 11);
		assert_noop!(
			Staking::merge_staked(RuntimeOrigin::signed(STASH_ALICE), STASH_BOB, 0),
			Error::<T>::InvalidMergeAmount
		);
	});
}

#[test]
fn merge_staked_exceeds_active_fails() {
	ExtBuilder::default().build_and_execute(|| {
		setup_nominator(STASH_ALICE, 100, 11);
		setup_nominator(STASH_BOB, 50, 11);
		assert_noop!(
			Staking::merge_staked(RuntimeOrigin::signed(STASH_ALICE), STASH_BOB, 150),
			Error::<T>::InvalidMergeAmount
		);
	});
}

#[test]
fn merge_staked_with_pending_unlock_fails() {
	ExtBuilder::default().build_and_execute(|| {
		setup_nominator(STASH_ALICE, 100, 11);
		setup_nominator(STASH_BOB, 50, 11);
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(STASH_ALICE), 50));
		assert_noop!(
			Staking::merge_staked(RuntimeOrigin::signed(STASH_ALICE), STASH_BOB, 30),
			Error::<T>::HasPendingUnlock
		);
	});
}

#[test]
fn merge_staked_target_not_nominator_fails() {
	ExtBuilder::default().build_and_execute(|| {
		setup_nominator(STASH_ALICE, 100, 11);
		let _ = asset::set_stakeable_balance::<T>(&STASH_BOB, 50);
		Balances::make_free_balance_be(&STASH_BOB, 50 + ExistentialDeposit::get());
		assert_ok!(Staking::bond(RuntimeOrigin::signed(STASH_BOB), 50, RewardDestination::Staked));
		assert_noop!(
			Staking::merge_staked(RuntimeOrigin::signed(STASH_ALICE), STASH_BOB, 30),
			Error::<T>::TargetNotNominator
		);
	});
}

#[test]
fn merge_staked_leaves_insufficient_bond_fails() {
	ExtBuilder::default().build_and_execute(|| {
		setup_nominator(STASH_ALICE, 100, 11);
		setup_nominator(STASH_BOB, 50, 11);
		MinNominatorBond::<T>::set(50u64.into());
		assert_noop!(
			Staking::merge_staked(RuntimeOrigin::signed(STASH_ALICE), STASH_BOB, 60),
			Error::<T>::InsufficientBond
		);
	});
}

#[test]
fn merge_staked_with_pending_slash_fails() {
	ExtBuilder::default().build_and_execute(|| {
		setup_nominator(STASH_ALICE, 100, 11);
		setup_nominator(STASH_BOB, 50, 11);

		let slash = UnappliedSlash::<T> {
			validator: 11,
			own: 0u64.into(),
			others: bounded_vec![(STASH_ALICE, 10u64.into())],
			reporter: None,
			payout: 0u64.into(),
		};
		UnappliedSlashes::<T>::insert(3, (11, Perbill::from_percent(10), 0), slash);

		assert_noop!(
			Staking::merge_staked(RuntimeOrigin::signed(STASH_ALICE), STASH_BOB, 30),
			Error::<T>::PendingSlash
		);
	});
}

#[test]
fn merge_staked_target_with_pending_unlock_accepts_merge() {
	ExtBuilder::default().build_and_execute(|| {
		setup_nominator(STASH_ALICE, 100, 11);
		setup_nominator(STASH_BOB, 50, 11);
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(STASH_BOB), 20));
		assert!(!Ledger::<T>::get(&STASH_BOB).unwrap().unlocking.is_empty());

		assert_ok!(Staking::merge_staked(RuntimeOrigin::signed(STASH_ALICE), STASH_BOB, 30));

		assert_eq!(Ledger::<T>::get(&STASH_ALICE).unwrap().active, 70);
		let bob = Ledger::<T>::get(&STASH_BOB).unwrap();
		assert_eq!(bob.active, 60);
		assert_eq!(bob.total, 80);
		assert!(!bob.unlocking.is_empty());
		assert_eq!(asset::staked::<T>(&STASH_BOB), 80);
	});
}

#[test]
fn merge_staked_e2e_full_merge_then_slash_slashes_target_pro_rata() {
	e2e_ext_builder().build_and_execute(|| {
		Session::roll_until_active_era(2);

		let bob_before = Staking::ledger(E2E_STASH_BOB.into()).unwrap().active;
		assert_ok!(Staking::merge_staked(
			RuntimeOrigin::signed(E2E_STASH_ALICE),
			E2E_STASH_BOB,
			1_000,
		));
		assert!(Ledger::<T>::get(&E2E_STASH_ALICE).is_none());

		let bob_after_merge = Staking::ledger(E2E_STASH_BOB.into()).unwrap().active;
		assert_eq!(bob_after_merge, bob_before + 1_000);

		Session::roll_until_active_era(3);

		let exposure = Staking::eras_stakers(active_era(), &11);
		assert!(!exposure.others.iter().any(|e| e.who == E2E_STASH_ALICE));
		let bob_exposure = exposure
			.others
			.iter()
			.find(|e| e.who == E2E_STASH_BOB)
			.map(|e| e.value)
			.expect("bob is exposed");

		add_slash_with_percent(11, 10);
		Session::roll_next();

		let slash_amount = Perbill::from_percent(10) * exposure.total;
		let expected_bob_slash =
			Perbill::from_rational(bob_exposure, exposure.total) * slash_amount;
		let bob_after_slash = Staking::ledger(E2E_STASH_BOB.into()).unwrap().active;
		assert_eq!(bob_after_merge - bob_after_slash, expected_bob_slash);
		assert!(expected_bob_slash > 0);
	});
}

#[test]
fn merge_staked_e2e_partial_merge_preserves_past_payouts() {
	e2e_ext_builder().build_and_execute(|| {
		Payee::<T>::insert(E2E_STASH_ALICE, RewardDestination::Account(E2E_STASH_ALICE));
		Payee::<T>::insert(E2E_STASH_BOB, RewardDestination::Account(E2E_STASH_BOB));
		Payee::<T>::insert(11, RewardDestination::Account(11));
		Payee::<T>::insert(21, RewardDestination::Account(21));

		Eras::<T>::reward_active_era(vec![(11, 50)]);
		Eras::<T>::reward_active_era(vec![(11, 50)]);
		Eras::<T>::reward_active_era(vec![(21, 50)]);

		let init_alice = asset::total_balance::<T>(&E2E_STASH_ALICE);
		let init_bob = asset::total_balance::<T>(&E2E_STASH_BOB);

		Session::roll_until_active_era(2);
		mock::make_all_reward_payment(1);

		let alice_after_era_1 = asset::total_balance::<T>(&E2E_STASH_ALICE);
		let bob_after_era_1 = asset::total_balance::<T>(&E2E_STASH_BOB);
		assert!(alice_after_era_1 > init_alice);
		assert!(bob_after_era_1 > init_bob);

		assert_ok!(Staking::merge_staked(
			RuntimeOrigin::signed(E2E_STASH_ALICE),
			E2E_STASH_BOB,
			300
		));

		assert_eq!(Ledger::<T>::get(&E2E_STASH_ALICE).unwrap().active, 700);
		assert_eq!(Ledger::<T>::get(&E2E_STASH_BOB).unwrap().active, 800);
		assert_eq!(asset::staked::<T>(&E2E_STASH_ALICE), 700);
		assert_eq!(asset::staked::<T>(&E2E_STASH_BOB), 800);

		Eras::<T>::reward_active_era(vec![(11, 50)]);
		Session::roll_until_active_era(3);
		mock::make_all_reward_payment(2);

		assert!(asset::total_balance::<T>(&E2E_STASH_ALICE) > alice_after_era_1);
		assert!(asset::total_balance::<T>(&E2E_STASH_BOB) > bob_after_era_1);
	});
}

#[test]
fn merge_staked_e2e_full_merge_kills_source_but_keeps_past_rewards() {
	e2e_ext_builder().build_and_execute(|| {
		Payee::<T>::insert(E2E_STASH_ALICE, RewardDestination::Account(E2E_STASH_ALICE));
		Payee::<T>::insert(E2E_STASH_BOB, RewardDestination::Account(E2E_STASH_BOB));
		Payee::<T>::insert(11, RewardDestination::Account(11));
		Payee::<T>::insert(21, RewardDestination::Account(21));

		Eras::<T>::reward_active_era(vec![(11, 50)]);
		Eras::<T>::reward_active_era(vec![(11, 50)]);
		Eras::<T>::reward_active_era(vec![(21, 50)]);

		Session::roll_until_active_era(2);
		mock::make_all_reward_payment(1);

		let init_alice = 1_000 + ExistentialDeposit::get();
		let alice_after_era_1 = asset::total_balance::<T>(&E2E_STASH_ALICE);
		assert!(alice_after_era_1 > init_alice);

		assert_ok!(Staking::merge_staked(
			RuntimeOrigin::signed(E2E_STASH_ALICE),
			E2E_STASH_BOB,
			1_000
		));
		assert!(Ledger::<T>::get(&E2E_STASH_ALICE).is_none());
		assert_eq!(Ledger::<T>::get(&E2E_STASH_BOB).unwrap().active, 1_500);

		// Alice keeps rewards earned before being killed; only active stake moved to Bob.
		assert_eq!(
			asset::total_balance::<T>(&E2E_STASH_ALICE),
			alice_after_era_1.saturating_sub(1_000)
		);
		assert_eq!(asset::staked::<T>(&E2E_STASH_ALICE), 0);

		Eras::<T>::reward_active_era(vec![(11, 50)]);
		Session::roll_until_active_era(3);
		let bob_before_era_2 = asset::total_balance::<T>(&E2E_STASH_BOB);
		mock::make_all_reward_payment(2);

		assert!(asset::total_balance::<T>(&E2E_STASH_BOB) > bob_before_era_2);
	});
}
