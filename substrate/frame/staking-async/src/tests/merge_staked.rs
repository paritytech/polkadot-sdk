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
use crate::{asset, session_rotation::Eras};
use frame_support::traits::Currency;
use mock::Session;

const STASH_ALICE: AccountId = 60;
const STASH_BOB: AccountId = 61;
const E2E_STASH_ALICE: AccountId = 102;
const E2E_STASH_BOB: AccountId = 101;

fn e2e_ext_builder() -> ExtBuilder {
	ExtBuilder::default()
		.nominate(true)
		.session_per_era(3)
		.set_nominators_slashable(false)
		.add_staker(E2E_STASH_ALICE, 1_000, StakerStatus::<AccountId>::Nominator(vec![11, 21]))
}

fn allow_merge() {
	AreNominatorsSlashable::<T>::put(false);
	let oldest_slashable_era =
		active_era().saturating_sub(BondingDuration::get().saturating_sub(1)).max(1);
	for era in oldest_slashable_era..=active_era() {
		ErasNominatorsSlashable::<T>::insert(era, false);
	}
}

fn setup_nominator(stash: AccountId, amount: Balance, validator: AccountId) {
	allow_merge();
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
			Error::<T>::NotANominator
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
fn merge_staked_while_nominators_are_slashable_fails() {
	ExtBuilder::default().build_and_execute(|| {
		setup_nominator(STASH_ALICE, 100, 11);
		setup_nominator(STASH_BOB, 50, 11);
		AreNominatorsSlashable::<T>::put(true);

		assert_noop!(
			Staking::merge_staked(RuntimeOrigin::signed(STASH_ALICE), STASH_BOB, 30),
			Error::<T>::SlashingRisk
		);
	});
}

#[test]
fn merge_staked_with_slashable_history_fails() {
	ExtBuilder::default().build_and_execute(|| {
		setup_nominator(STASH_ALICE, 100, 11);
		setup_nominator(STASH_BOB, 50, 11);
		ErasNominatorsSlashable::<T>::insert(active_era(), true);

		assert_noop!(
			Staking::merge_staked(RuntimeOrigin::signed(STASH_ALICE), STASH_BOB, 30),
			Error::<T>::SlashingRisk
		);
	});
}

#[test]
fn merge_staked_for_recent_validator_fails() {
	ExtBuilder::default().build_and_execute(|| {
		setup_nominator(STASH_ALICE, 100, 11);
		setup_nominator(STASH_BOB, 50, 11);
		LastValidatorEra::<T>::insert(STASH_ALICE, active_era());

		assert_noop!(
			Staking::merge_staked(RuntimeOrigin::signed(STASH_ALICE), STASH_BOB, 30),
			Error::<T>::SlashingRisk
		);
	});
}

#[test]
fn merge_staked_with_relevant_offence_queue_fails() {
	ExtBuilder::default().build_and_execute(|| {
		setup_nominator(STASH_ALICE, 100, 11);
		setup_nominator(STASH_BOB, 50, 11);
		OffenceQueueEras::<T>::put(WeakBoundedVec::force_from(vec![active_era()], None));

		assert_noop!(
			Staking::merge_staked(RuntimeOrigin::signed(STASH_ALICE), STASH_BOB, 30),
			Error::<T>::SlashingRisk
		);
		OffenceQueueEras::<T>::kill();
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
fn merge_staked_e2e_rejects_slashable_source() {
	e2e_ext_builder().set_nominators_slashable(true).build_and_execute(|| {
		Session::roll_until_active_era(2);

		assert_noop!(
			Staking::merge_staked(RuntimeOrigin::signed(E2E_STASH_ALICE), E2E_STASH_BOB, 1_000,),
			Error::<T>::SlashingRisk
		);

		let alice_before = Staking::ledger(E2E_STASH_ALICE.into()).unwrap().active;
		add_slash_with_percent(11, 10);
		Session::roll_next();
		assert!(Staking::ledger(E2E_STASH_ALICE.into()).unwrap().active < alice_before);
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

		allow_merge();
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

		allow_merge();
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
