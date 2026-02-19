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
use frame_support::traits::fungible::Mutate;
use sp_staking::{Stake, StakingInterface};

#[test]
fn ledger_update_creates_hold() {
	ExtBuilder::default().has_stakers(true).build_and_execute(|| {
		// GIVEN alice who is a nominator with old currency
		let alice = 300;
		bond_nominator(alice, 1000, vec![11]);
		assert_eq!(asset::staked::<Test>(&alice), 1000);
		assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 0);
		// migrate alice currency to legacy locks
		testing_utils::migrate_to_old_currency::<Test>(alice);
		// no more holds
		assert_eq!(asset::staked::<Test>(&alice), 0);
		assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 1000);
		assert_eq!(
			<Staking as StakingInterface>::stake(&alice),
			Ok(Stake { total: 1000, active: 1000 })
		);

		// any ledger mutation should create a hold
		hypothetically!({
			// give some extra balance to alice.
			let _ = asset::mint_into_existing::<Test>(&alice, 100);

			// WHEN new fund is bonded to ledger.
			assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(alice), 100));

			// THEN new hold is created
			assert_eq!(asset::staked::<Test>(&alice), 1000 + 100);
			assert_eq!(
				<Staking as StakingInterface>::stake(&alice),
				Ok(Stake { total: 1100, active: 1100 })
			);

			// old locked balance is untouched
			assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 1000);
		});

		hypothetically!({
			// WHEN new fund is unbonded from ledger.
			assert_ok!(Staking::unbond(RuntimeOrigin::signed(alice), 100));

			// THEN hold is updated.
			assert_eq!(asset::staked::<Test>(&alice), 1000);
			assert_eq!(
				<Staking as StakingInterface>::stake(&alice),
				Ok(Stake { total: 1000, active: 900 })
			);

			// old locked balance is untouched
			assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 1000);
		});

		// WHEN alice currency is migrated.
		assert_ok!(Staking::migrate_currency(RuntimeOrigin::signed(1), alice));

		// THEN hold is updated.
		assert_eq!(asset::staked::<Test>(&alice), 1000);
		assert_eq!(
			<Staking as StakingInterface>::stake(&alice),
			Ok(Stake { total: 1000, active: 1000 })
		);

		// ensure cannot migrate again.
		assert_noop!(
			Staking::migrate_currency(RuntimeOrigin::signed(1), alice),
			Error::<Test>::AlreadyMigrated
		);

		// locked balance is removed
		assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 0);
	});
}

#[test]
fn migrate_removes_old_lock() {
	ExtBuilder::default().has_stakers(true).build_and_execute(|| {
		// GIVEN alice who is a nominator with old currency
		let alice = 300;
		bond_nominator(alice, 1000, vec![11]);
		testing_utils::migrate_to_old_currency::<Test>(alice);
		assert_eq!(asset::staked::<Test>(&alice), 0);
		assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 1000);
		let pre_migrate_consumer = System::consumers(&alice);
		System::reset_events();

		// WHEN alice currency is migrated.
		assert_ok!(Staking::migrate_currency(RuntimeOrigin::signed(1), alice));

		// THEN
		// the extra consumer from old code is removed.
		assert_eq!(System::consumers(&alice), pre_migrate_consumer - 1);
		// ensure no lock
		assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 0);
		// ensure stake and hold are same.
		assert_eq!(
			<Staking as StakingInterface>::stake(&alice),
			Ok(Stake { total: 1000, active: 1000 })
		);
		assert_eq!(asset::staked::<Test>(&alice), 1000);
		// ensure events are emitted.
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::CurrencyMigrated { stash: alice, force_withdraw: 0 }]
		);

		// ensure cannot migrate again.
		assert_noop!(
			Staking::migrate_currency(RuntimeOrigin::signed(1), alice),
			Error::<Test>::AlreadyMigrated
		);
	});
}
#[test]
fn cannot_hold_all_stake() {
	// When there is not enough funds to hold all stake, part of the stake if force withdrawn.
	// At end of the migration, the stake and hold should be same.
	ExtBuilder::default().has_stakers(true).build_and_execute(|| {
		// GIVEN alice who is a nominator with old currency.
		let alice = 300;
		let stake = 1000;
		bond_nominator(alice, stake, vec![11]);
		testing_utils::migrate_to_old_currency::<Test>(alice);
		assert_eq!(asset::staked::<Test>(&alice), 0);
		assert_eq!(Balances::balance_locked(STAKING_ID, &alice), stake);
		// ledger has 1000 staked.
		assert_eq!(
			<Staking as StakingInterface>::stake(&alice),
			Ok(Stake { total: stake, active: stake })
		);

		// Get rid of the extra ED to emulate all their balance including ED is staked.
		assert_ok!(Balances::transfer_allow_death(
			RuntimeOrigin::signed(alice),
			10,
			ExistentialDeposit::get()
		));

		let expected_force_withdraw = ExistentialDeposit::get();

		// ledger mutation would fail in this case before migration because of failing hold.
		assert_noop!(
			Staking::unbond(RuntimeOrigin::signed(alice), 100),
			Error::<Test>::NotEnoughFunds
		);

		// clear events
		System::reset_events();

		// WHEN alice currency is migrated.
		assert_ok!(Staking::migrate_currency(RuntimeOrigin::signed(1), alice));

		// THEN
		let expected_hold = stake - expected_force_withdraw;
		// ensure no lock
		assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 0);
		// ensure stake and hold are same.
		assert_eq!(
			<Staking as StakingInterface>::stake(&alice),
			Ok(Stake { total: expected_hold, active: expected_hold })
		);
		assert_eq!(asset::staked::<Test>(&alice), expected_hold);
		// ensure events are emitted.
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::CurrencyMigrated {
				stash: alice,
				force_withdraw: expected_force_withdraw
			}]
		);

		// ensure cannot migrate again.
		assert_noop!(
			Staking::migrate_currency(RuntimeOrigin::signed(1), alice),
			Error::<Test>::AlreadyMigrated
		);

		// unbond works after migration.
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(alice), 100));
	});
}

#[test]
fn overstaked_and_partially_unbonding() {
	ExtBuilder::default().has_stakers(true).build_and_execute(|| {
		// GIVEN alice who is a nominator with T::OldCurrency.
		let alice = 300;
		// 1000 + ED
		let _ = Balances::make_free_balance_be(&alice, 1001);
		let stake = 600;
		let reserved_by_another_pallet = 400;
		assert_ok!(Staking::bond(
			RuntimeOrigin::signed(alice),
			stake,
			RewardDestination::Staked
		));

		// AND Alice is partially unbonding.
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(alice), 300));

		// AND Alice has some funds reserved with another pallet.
		assert_ok!(Balances::reserve(&alice, reserved_by_another_pallet));

		// convert stake to T::OldCurrency.
		testing_utils::migrate_to_old_currency::<Test>(alice);
		assert_eq!(asset::staked::<Test>(&alice), 0);
		assert_eq!(Balances::balance_locked(STAKING_ID, &alice), stake);

		// ledger has correct amount staked.
		assert_eq!(
			<Staking as StakingInterface>::stake(&alice),
			Ok(Stake { total: stake, active: stake - 300 })
		);

		// Alice becomes overstaked by withdrawing some staked balance.
		assert_ok!(Balances::transfer_allow_death(
			RuntimeOrigin::signed(alice),
			10,
			reserved_by_another_pallet
		));

		let expected_force_withdraw = reserved_by_another_pallet;

		// ledger mutation would fail in this case before migration because of failing hold.
		assert_noop!(
			Staking::unbond(RuntimeOrigin::signed(alice), 100),
			Error::<Test>::NotEnoughFunds
		);

		// clear events
		System::reset_events();

		// WHEN alice currency is migrated.
		assert_ok!(Staking::migrate_currency(RuntimeOrigin::signed(1), alice));

		// THEN
		let expected_hold = stake - expected_force_withdraw;
		// ensure no lock
		assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 0);
		// ensure stake and hold are same.
		assert_eq!(
			<Staking as StakingInterface>::stake(&alice),
			// expected stake is 0 since force withdrawn (400) is taken out completely of
			// active stake.
			Ok(Stake { total: expected_hold, active: 0 })
		);

		assert_eq!(asset::staked::<Test>(&alice), expected_hold);
		// ensure events are emitted.
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::CurrencyMigrated {
				stash: alice,
				force_withdraw: expected_force_withdraw
			}]
		);

		// ensure cannot migrate again.
		assert_noop!(
			Staking::migrate_currency(RuntimeOrigin::signed(1), alice),
			Error::<Test>::AlreadyMigrated
		);

		// unbond works after migration.
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(alice), 100));
	});
}

#[test]
fn virtual_staker_consumer_provider_dec() {
	// Ensure virtual stakers consumer and provider count is decremented.
	ExtBuilder::default().has_stakers(true).build_and_execute(|| {
		// 200 virtual bonds
		bond_virtual_nominator(200, 201, 500, vec![11, 21]);

		// previously the virtual nominator had a provider inc by the delegation system as
		// well as a consumer by this pallet.
		System::inc_providers(&200);
		System::inc_consumers(&200).expect("has provider, can consume");

		hypothetically!({
			// migrate 200
			assert_ok!(Staking::migrate_currency(RuntimeOrigin::signed(1), 200));

			// ensure account does not exist in system anymore.
			assert_eq!(System::consumers(&200), 0);
			assert_eq!(System::providers(&200), 0);
			assert!(!System::account_exists(&200));

			// ensure cannot migrate again.
			assert_noop!(
				Staking::migrate_currency(RuntimeOrigin::signed(1), 200),
				Error::<Test>::AlreadyMigrated
			);
		});

		hypothetically!({
			// 200 has an erroneously extra provider
			System::inc_providers(&200);

			// causes migration to fail.
			assert_noop!(
				Staking::migrate_currency(RuntimeOrigin::signed(1), 200),
				Error::<Test>::BadState
			);
		});

		// 200 is funded for more than ED by a random account.
		assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(999), 200, 10));

		// it has an extra provider now.
		assert_eq!(System::providers(&200), 2);

		// migrate 200
		assert_ok!(Staking::migrate_currency(RuntimeOrigin::signed(1), 200));

		// 1 provider is left, consumers is 0.
		assert_eq!(System::providers(&200), 1);
		assert_eq!(System::consumers(&200), 0);

		// ensure cannot migrate again.
		assert_noop!(
			Staking::migrate_currency(RuntimeOrigin::signed(1), 200),
			Error::<Test>::AlreadyMigrated
		);
	});
}

#[test]
fn remove_old_lock_when_stake_already_on_hold() {
	// When the hold is already migrated because of interactions with the ledger, we still
	// want to remove the old lock via the explicit `migrate_currency`.
	ExtBuilder::default().has_stakers(true).build_and_execute(|| {
		// GIVEN alice and bob who are bonded with old currency.
		let alice = 300;
		let bob = 301;
		Balances::set_balance(&alice, 3000);
		Balances::set_balance(&bob, 3000);

		mock::start_active_era(1);
		let init_stake = 1000;
		assert_ok!(Staking::bond(
			RuntimeOrigin::signed(alice),
			init_stake,
			RewardDestination::Staked
		));
		assert_ok!(Staking::bond(
			RuntimeOrigin::signed(bob),
			init_stake,
			RewardDestination::Staked
		));

		// convert hold to lock.
		testing_utils::migrate_to_old_currency::<Test>(alice);
		testing_utils::migrate_to_old_currency::<Test>(bob);

		// this returns the hold balance which is 0 because of the above migration.
		assert_eq!(asset::staked::<Test>(&alice), 0);
		assert_eq!(asset::staked::<Test>(&bob), 0);

		// but instead of hold, the balance is locked.
		assert_eq!(Balances::balance_locked(STAKING_ID, &alice), init_stake);
		assert_eq!(Balances::balance_locked(STAKING_ID, &bob), init_stake);

		// ledger has 1000 staked.
		assert_eq!(
			<Staking as StakingInterface>::stake(&alice),
			Ok(Stake { total: init_stake, active: init_stake })
		);
		assert_eq!(
			<Staking as StakingInterface>::stake(&bob),
			Ok(Stake { total: init_stake, active: init_stake })
		);

		// -- WHEN Alice interacts with ledger that updates the hold.
		assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(alice), 500));

		// this will update the ledger and the held balance.
		assert_eq!(asset::staked::<Test>(&alice), init_stake + 500);
		// but the locked balance remains
		assert_eq!(Balances::balance_locked(STAKING_ID, &alice), init_stake);

		// clear events
		System::reset_events();

		// To remove the old locks, alice needs to migrate currency.
		// AND alice currency is migrated.
		assert_ok!(Staking::migrate_currency(RuntimeOrigin::signed(1), alice));

		// THEN
		let expected_hold = init_stake + 500;
		// ensure no lock
		assert_eq!(Balances::balance_locked(STAKING_ID, &alice), 0);
		// ensure stake and hold are same.
		assert_eq!(
			<Staking as StakingInterface>::stake(&alice),
			Ok(Stake { total: expected_hold, active: expected_hold })
		);
		assert_eq!(asset::staked::<Test>(&alice), expected_hold);

		// ensure events are emitted.
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::CurrencyMigrated { stash: alice, force_withdraw: 0 }]
		);

		// ensure cannot migrate again.
		assert_noop!(
			Staking::migrate_currency(RuntimeOrigin::signed(1), alice),
			Error::<Test>::AlreadyMigrated
		);

		// -- WHEN Bob withdraws all stake before migration.
		assert_ok!(Staking::unbond(RuntimeOrigin::signed(bob), init_stake));

		mock::start_active_era(4);
		assert_ok!(Staking::withdraw_unbonded(RuntimeOrigin::signed(bob), 0));

		// assert lock still exists but there is no stake.
		assert_eq!(Balances::balance_locked(STAKING_ID, &bob), init_stake);
		assert_eq!(asset::staked::<Test>(&bob), 0);
		assert_eq!(
			<Staking as StakingInterface>::stake(&bob).unwrap_err(),
			Error::<Test>::NotStash.into()
		);

		// clear events
		System::reset_events();

		// AND Bob wants to remove the old lock.
		assert_ok!(Staking::migrate_currency(RuntimeOrigin::signed(1), bob));

		// THEN ensure no lock
		assert_eq!(Balances::balance_locked(STAKING_ID, &bob), 0);

		// And they cannot migrate again.
		assert_noop!(
			Staking::migrate_currency(RuntimeOrigin::signed(1), bob),
			Error::<Test>::AlreadyMigrated
		);
	});
}
