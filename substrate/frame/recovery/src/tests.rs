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

//! Tests for the module.

use crate::{mock::*, *};
use frame::{deps::sp_runtime::bounded_vec, testing_prelude::*};

#[test]
fn basic_setup_works() {
	new_test_ext().execute_with(|| {
		// Nothing in storage to start
		assert_eq!(Recovery::proxy(&2), None);
		assert_eq!(Recovery::active_recovery(&1, &2), None);
		assert_eq!(Recovery::recovery_config(&1), None);
		// Everyone should have starting balance of 100
		assert_eq!(Balances::free_balance(1), 100);
	});
}

#[test]
fn set_recovered_works() {
	new_test_ext().execute_with(|| {
		// Not accessible by a normal user
		assert_noop!(Recovery::set_recovered(RuntimeOrigin::signed(1), 5, 1), BadOrigin);
		// Root can set a recovered account though
		assert_ok!(Recovery::set_recovered(RuntimeOrigin::root(), 5, 1));
		// Account 1 should now be able to make a call through account 5
		let call = Box::new(RuntimeCall::Balances(BalancesCall::transfer_allow_death {
			dest: 1,
			value: 100,
		}));
		assert_ok!(Recovery::as_recovered(RuntimeOrigin::signed(1), 5, call));
		// Account 1 has successfully drained the funds from account 5
		assert_eq!(Balances::free_balance(1), 200);
		assert_eq!(Balances::free_balance(5), 0);
	});
}

#[test]
fn recovery_life_cycle_works() {
	new_test_ext().execute_with(|| {
		let friends = vec![2, 3, 4];
		let threshold = 3;
		let delay_period = 10;
		// Account 5 sets up a recovery configuration on their account
		assert_ok!(Recovery::create_recovery(
			RuntimeOrigin::signed(5),
			friends,
			threshold,
			delay_period
		));
		// Some time has passed, and the user lost their keys!
		System::run_to_block::<AllPalletsWithSystem>(10);
		// Using account 1, the user begins the recovery process to recover the lost account
		assert_ok!(Recovery::initiate_recovery(RuntimeOrigin::signed(1), 5));
		// Off chain, the user contacts their friends and asks them to vouch for the recovery
		// attempt
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(2), 5, 1));
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(3), 5, 1));
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(4), 5, 1));
		// We met the threshold, lets try to recover the account...?
		assert_noop!(
			Recovery::claim_recovery(RuntimeOrigin::signed(1), 5),
			Error::<Test>::DelayPeriod
		);
		// We need to wait at least the delay_period number of blocks before we can recover
		System::run_to_block::<AllPalletsWithSystem>(20);
		assert_ok!(Recovery::claim_recovery(RuntimeOrigin::signed(1), 5));
		// Account 1 can use account 5 to close the active recovery process, claiming the deposited
		// funds used to initiate the recovery process into account 5.
		let call = Box::new(RuntimeCall::Recovery(RecoveryCall::close_recovery { rescuer: 1 }));
		assert_ok!(Recovery::as_recovered(RuntimeOrigin::signed(1), 5, call));
		// Account 1 can then use account 5 to remove the recovery configuration, claiming the
		// deposited funds used to create the recovery configuration into account 5.
		let call = Box::new(RuntimeCall::Recovery(RecoveryCall::remove_recovery {}));
		assert_ok!(Recovery::as_recovered(RuntimeOrigin::signed(1), 5, call));
		// Account 1 should now be able to make a call through account 5 to get all of their funds
		assert_eq!(Balances::free_balance(5), 110);
		let call = Box::new(RuntimeCall::Balances(BalancesCall::transfer_allow_death {
			dest: 1,
			value: 110,
		}));
		assert_ok!(Recovery::as_recovered(RuntimeOrigin::signed(1), 5, call));
		// All funds have been fully recovered!
		assert_eq!(Balances::free_balance(1), 200);
		assert_eq!(Balances::free_balance(5), 0);
		// Remove the proxy link.
		assert_ok!(Recovery::cancel_recovered(RuntimeOrigin::signed(1), 5));

		// All storage items are removed from the module
		assert!(!<ActiveRecoveries<Test>>::contains_key(&5, &1));
		assert!(!<Recoverable<Test>>::contains_key(&5));
		assert!(!<Proxy<Test>>::contains_key(&1));
	});
}

#[test]
fn malicious_recovery_fails() {
	new_test_ext().execute_with(|| {
		let friends = vec![2, 3, 4];
		let threshold = 3;
		let delay_period = 10;
		// Account 5 sets up a recovery configuration on their account
		assert_ok!(Recovery::create_recovery(
			RuntimeOrigin::signed(5),
			friends,
			threshold,
			delay_period
		));
		// Some time has passed, and account 1 wants to try and attack this account!
		System::run_to_block::<AllPalletsWithSystem>(10);
		// Using account 1, the malicious user begins the recovery process on account 5
		assert_ok!(Recovery::initiate_recovery(RuntimeOrigin::signed(1), 5));
		// Off chain, the user **tricks** their friends and asks them to vouch for the recovery
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(2), 5, 1));
		// shame on you
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(3), 5, 1));
		// shame on you
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(4), 5, 1));
		// shame on you
		// We met the threshold, lets try to recover the account...?
		assert_noop!(
			Recovery::claim_recovery(RuntimeOrigin::signed(1), 5),
			Error::<Test>::DelayPeriod
		);
		// Account 1 needs to wait...
		System::run_to_block::<AllPalletsWithSystem>(19);
		// One more block to wait!
		assert_noop!(
			Recovery::claim_recovery(RuntimeOrigin::signed(1), 5),
			Error::<Test>::DelayPeriod
		);
		// Account 5 checks their account every `delay_period` and notices the malicious attack!
		// Account 5 can close the recovery process before account 1 can claim it
		assert_ok!(Recovery::close_recovery(RuntimeOrigin::signed(5), 1));
		// By doing so, account 5 has now claimed the deposit originally reserved by account 1
		assert_eq!(Balances::total_balance(&1), 90);
		// Thanks for the free money!
		assert_eq!(Balances::total_balance(&5), 110);
		// The recovery process has been closed, so account 1 can't make the claim
		System::run_to_block::<AllPalletsWithSystem>(20);
		assert_noop!(
			Recovery::claim_recovery(RuntimeOrigin::signed(1), 5),
			Error::<Test>::NotStarted
		);
		// Account 5 can remove their recovery config and pick some better friends
		assert_ok!(Recovery::remove_recovery(RuntimeOrigin::signed(5)));
		assert_ok!(Recovery::create_recovery(
			RuntimeOrigin::signed(5),
			vec![22, 33, 44],
			threshold,
			delay_period
		));
	});
}

#[test]
fn create_recovery_handles_basic_errors() {
	new_test_ext().execute_with(|| {
		// No friends
		assert_noop!(
			Recovery::create_recovery(RuntimeOrigin::signed(5), vec![], 1, 0),
			Error::<Test>::NotEnoughFriends
		);
		// Zero threshold
		assert_noop!(
			Recovery::create_recovery(RuntimeOrigin::signed(5), vec![2], 0, 0),
			Error::<Test>::ZeroThreshold
		);
		// Threshold greater than friends length
		assert_noop!(
			Recovery::create_recovery(RuntimeOrigin::signed(5), vec![2, 3, 4], 4, 0),
			Error::<Test>::NotEnoughFriends
		);
		// Too many friends
		assert_noop!(
			Recovery::create_recovery(
				RuntimeOrigin::signed(5),
				vec![1; (MaxFriends::get() + 1) as usize],
				1,
				0
			),
			Error::<Test>::MaxFriends
		);
		// Unsorted friends
		assert_noop!(
			Recovery::create_recovery(RuntimeOrigin::signed(5), vec![3, 2, 4], 3, 0),
			Error::<Test>::NotSorted
		);
		// Duplicate friends
		assert_noop!(
			Recovery::create_recovery(RuntimeOrigin::signed(5), vec![2, 2, 4], 3, 0),
			Error::<Test>::NotSorted
		);
		// Already configured
		assert_ok!(Recovery::create_recovery(RuntimeOrigin::signed(5), vec![2, 3, 4], 3, 10));
		assert_noop!(
			Recovery::create_recovery(RuntimeOrigin::signed(5), vec![2, 3, 4], 3, 10),
			Error::<Test>::AlreadyRecoverable
		);
	});
}

#[test]
fn create_recovery_works() {
	new_test_ext().execute_with(|| {
		let friends = vec![2, 3, 4];
		let threshold = 3;
		let delay_period = 10;
		// Account 5 sets up a recovery configuration on their account
		assert_ok!(Recovery::create_recovery(
			RuntimeOrigin::signed(5),
			friends.clone(),
			threshold,
			delay_period
		));
		// Deposit is taken, and scales with the number of friends they pick
		// Base 10 + 1 per friends = 13 total reserved
		assert_eq!(Balances::reserved_balance(5), 13);
		// Recovery configuration is correctly stored
		let recovery_config = RecoveryConfig {
			delay_period,
			deposit: 13,
			friends: friends.try_into().unwrap(),
			threshold,
		};
		assert_eq!(Recovery::recovery_config(5), Some(recovery_config));
	});
}

#[test]
fn initiate_recovery_handles_basic_errors() {
	new_test_ext().execute_with(|| {
		// No recovery process set up for the account
		assert_noop!(
			Recovery::initiate_recovery(RuntimeOrigin::signed(1), 5),
			Error::<Test>::NotRecoverable
		);
		// Create a recovery process for next test
		let friends = vec![2, 3, 4];
		let threshold = 3;
		let delay_period = 10;
		assert_ok!(Recovery::create_recovery(
			RuntimeOrigin::signed(5),
			friends.clone(),
			threshold,
			delay_period
		));
		// Same user cannot recover same account twice
		assert_ok!(Recovery::initiate_recovery(RuntimeOrigin::signed(1), 5));
		assert_noop!(
			Recovery::initiate_recovery(RuntimeOrigin::signed(1), 5),
			Error::<Test>::AlreadyStarted
		);
		// No double deposit
		assert_eq!(Balances::reserved_balance(1), 10);
	});
}

#[test]
fn initiate_recovery_works() {
	new_test_ext().execute_with(|| {
		// Create a recovery process for the test
		let friends = vec![2, 3, 4];
		let threshold = 3;
		let delay_period = 10;
		assert_ok!(Recovery::create_recovery(
			RuntimeOrigin::signed(5),
			friends.clone(),
			threshold,
			delay_period
		));
		// Recovery can be initiated
		assert_ok!(Recovery::initiate_recovery(RuntimeOrigin::signed(1), 5));
		// Deposit is reserved
		assert_eq!(Balances::reserved_balance(1), 10);
		// Recovery status object is created correctly
		let recovery_status =
			ActiveRecovery { created: 1, deposit: 10, friends: Default::default() };
		assert_eq!(<ActiveRecoveries<Test>>::get(&5, &1), Some(recovery_status));
		// Multiple users can attempt to recover the same account
		assert_ok!(Recovery::initiate_recovery(RuntimeOrigin::signed(2), 5));
	});
}

#[test]
fn vouch_recovery_handles_basic_errors() {
	new_test_ext().execute_with(|| {
		// Cannot vouch for non-recoverable account
		assert_noop!(
			Recovery::vouch_recovery(RuntimeOrigin::signed(2), 5, 1),
			Error::<Test>::NotRecoverable
		);
		// Create a recovery process for next tests
		let friends = vec![2, 3, 4];
		let threshold = 3;
		let delay_period = 10;
		assert_ok!(Recovery::create_recovery(
			RuntimeOrigin::signed(5),
			friends.clone(),
			threshold,
			delay_period
		));
		// Cannot vouch a recovery process that has not started
		assert_noop!(
			Recovery::vouch_recovery(RuntimeOrigin::signed(2), 5, 1),
			Error::<Test>::NotStarted
		);
		// Initiate a recovery process
		assert_ok!(Recovery::initiate_recovery(RuntimeOrigin::signed(1), 5));
		// Cannot vouch if you are not a friend
		assert_noop!(
			Recovery::vouch_recovery(RuntimeOrigin::signed(22), 5, 1),
			Error::<Test>::NotFriend
		);
		// Cannot vouch twice
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(2), 5, 1));
		assert_noop!(
			Recovery::vouch_recovery(RuntimeOrigin::signed(2), 5, 1),
			Error::<Test>::AlreadyVouched
		);
	});
}

#[test]
fn vouch_recovery_works() {
	new_test_ext().execute_with(|| {
		// Create and initiate a recovery process for the test
		let friends = vec![2, 3, 4];
		let threshold = 3;
		let delay_period = 10;
		assert_ok!(Recovery::create_recovery(
			RuntimeOrigin::signed(5),
			friends.clone(),
			threshold,
			delay_period
		));
		assert_ok!(Recovery::initiate_recovery(RuntimeOrigin::signed(1), 5));
		// Vouching works
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(2), 5, 1));
		// Handles out of order vouches
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(4), 5, 1));
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(3), 5, 1));
		// Final recovery status object is updated correctly
		let recovery_status =
			ActiveRecovery { created: 1, deposit: 10, friends: bounded_vec![2, 3, 4] };
		assert_eq!(<ActiveRecoveries<Test>>::get(&5, &1), Some(recovery_status));
	});
}

#[test]
fn claim_recovery_handles_basic_errors() {
	new_test_ext().execute_with(|| {
		// Cannot claim a non-recoverable account
		assert_noop!(
			Recovery::claim_recovery(RuntimeOrigin::signed(1), 5),
			Error::<Test>::NotRecoverable
		);
		// Create a recovery process for the test
		let friends = vec![2, 3, 4];
		let threshold = 3;
		let delay_period = 10;
		assert_ok!(Recovery::create_recovery(
			RuntimeOrigin::signed(5),
			friends.clone(),
			threshold,
			delay_period
		));
		// Cannot claim an account which has not started the recovery process
		assert_noop!(
			Recovery::claim_recovery(RuntimeOrigin::signed(1), 5),
			Error::<Test>::NotStarted
		);
		assert_ok!(Recovery::initiate_recovery(RuntimeOrigin::signed(1), 5));
		// Cannot claim an account which has not passed the delay period
		assert_noop!(
			Recovery::claim_recovery(RuntimeOrigin::signed(1), 5),
			Error::<Test>::DelayPeriod
		);
		System::run_to_block::<AllPalletsWithSystem>(11);
		// Cannot claim an account which has not passed the threshold number of votes
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(2), 5, 1));
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(3), 5, 1));
		// Only 2/3 is not good enough
		assert_noop!(
			Recovery::claim_recovery(RuntimeOrigin::signed(1), 5),
			Error::<Test>::Threshold
		);
	});
}

#[test]
fn claim_recovery_works() {
	new_test_ext().execute_with(|| {
		// Create, initiate, and vouch recovery process for the test
		let friends = vec![2, 3, 4];
		let threshold = 3;
		let delay_period = 10;
		assert_ok!(Recovery::create_recovery(
			RuntimeOrigin::signed(5),
			friends.clone(),
			threshold,
			delay_period
		));
		assert_ok!(Recovery::initiate_recovery(RuntimeOrigin::signed(1), 5));
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(2), 5, 1));
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(3), 5, 1));
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(4), 5, 1));

		System::run_to_block::<AllPalletsWithSystem>(11);

		// Account can be recovered.
		assert_ok!(Recovery::claim_recovery(RuntimeOrigin::signed(1), 5));
		// Recovered storage item is correctly created
		assert_eq!(<Proxy<Test>>::get(&1), Some(5));
		// Account could be re-recovered in the case that the recoverer account also gets lost.
		assert_ok!(Recovery::initiate_recovery(RuntimeOrigin::signed(4), 5));
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(2), 5, 4));
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(3), 5, 4));
		assert_ok!(Recovery::vouch_recovery(RuntimeOrigin::signed(4), 5, 4));

		System::run_to_block::<AllPalletsWithSystem>(21);

		// Account is re-recovered.
		assert_ok!(Recovery::claim_recovery(RuntimeOrigin::signed(4), 5));
		// Recovered storage item is correctly updated
		assert_eq!(<Proxy<Test>>::get(&4), Some(5));
	});
}

#[test]
fn close_recovery_handles_basic_errors() {
	new_test_ext().execute_with(|| {
		// Cannot close a non-active recovery
		assert_noop!(
			Recovery::close_recovery(RuntimeOrigin::signed(5), 1),
			Error::<Test>::NotStarted
		);
	});
}

#[test]
fn remove_recovery_works() {
	new_test_ext().execute_with(|| {
		// Cannot remove an unrecoverable account
		assert_noop!(
			Recovery::remove_recovery(RuntimeOrigin::signed(5)),
			Error::<Test>::NotRecoverable
		);
		// Create and initiate a recovery process for the test
		let friends = vec![2, 3, 4];
		let threshold = 3;
		let delay_period = 10;
		assert_ok!(Recovery::create_recovery(
			RuntimeOrigin::signed(5),
			friends.clone(),
			threshold,
			delay_period
		));
		assert_ok!(Recovery::initiate_recovery(RuntimeOrigin::signed(1), 5));
		assert_ok!(Recovery::initiate_recovery(RuntimeOrigin::signed(2), 5));
		// Cannot remove a recovery when there are active recoveries.
		assert_noop!(
			Recovery::remove_recovery(RuntimeOrigin::signed(5)),
			Error::<Test>::StillActive
		);
		assert_ok!(Recovery::close_recovery(RuntimeOrigin::signed(5), 1));
		// Still need to remove one more!
		assert_noop!(
			Recovery::remove_recovery(RuntimeOrigin::signed(5)),
			Error::<Test>::StillActive
		);
		assert_ok!(Recovery::close_recovery(RuntimeOrigin::signed(5), 2));
		// Finally removed
		assert_ok!(Recovery::remove_recovery(RuntimeOrigin::signed(5)));
	});
}

#[test]
fn poke_deposit_handles_unsigned_origin() {
	new_test_ext().execute_with(|| {
		assert_noop!(Recovery::poke_deposit(RuntimeOrigin::none(), None), DispatchError::BadOrigin);
	});
}

#[test]
fn poke_deposit_works_for_recovery_config_deposits() {
	new_test_ext().execute_with(|| {
		// Create initial recovery config
		assert_ok!(Recovery::create_recovery(RuntimeOrigin::signed(5), vec![2, 3, 4], 3, 10));

		// Verify initial state
		let old_deposit = Balances::reserved_balance(5);
		// Base 10 + 1 per friend
		assert_eq!(old_deposit, 13);
		let config = Recovery::recovery_config(5).unwrap();
		assert_eq!(config.deposit, old_deposit);

		// Change ConfigDepositBase to trigger deposit update
		ConfigDepositBase::set(20);

		// Poke deposit should work and be free
		let result = Recovery::poke_deposit(RuntimeOrigin::signed(5), None);
		assert_ok!(result.as_ref());
		assert_eq!(result.unwrap(), Pays::No.into());

		// Verify final state
		let new_deposit = Balances::reserved_balance(5);
		// New base 20 + 1 per friend
		assert_eq!(new_deposit, 23);
		let updated_config = Recovery::recovery_config(5).unwrap();
		assert_eq!(updated_config.deposit, new_deposit);

		// Check event was emitted
		System::assert_has_event(
			Event::<Test>::DepositPoked {
				who: 5,
				kind: DepositKind::RecoveryConfig,
				old_deposit,
				new_deposit,
			}
			.into(),
		);
	});
}

#[test]
fn poke_deposit_works_for_active_recovery_deposits() {
	new_test_ext().execute_with(|| {
		// Setup recovery config
		assert_ok!(Recovery::create_recovery(RuntimeOrigin::signed(5), vec![2, 3, 4], 3, 10));
		// Account 1 initiates recovery
		assert_ok!(Recovery::initiate_recovery(RuntimeOrigin::signed(1), 5));

		// Verify initial state
		let old_deposit = Balances::reserved_balance(1);
		assert_eq!(old_deposit, 10);
		let recovery = <ActiveRecoveries<Test>>::get(&5, &1).unwrap();
		assert_eq!(recovery.deposit, old_deposit);

		// Change RecoveryDeposit to trigger update
		let new_deposit = 15;
		RecoveryDeposit::set(new_deposit);

		// Poke deposit should work and be free
		let result = Recovery::poke_deposit(RuntimeOrigin::signed(1), Some(5));
		assert_ok!(result.as_ref());
		assert_eq!(result.unwrap(), Pays::No.into());

		// Verify final state
		assert_eq!(Balances::reserved_balance(1), new_deposit.into());
		let updated_recovery = <ActiveRecoveries<Test>>::get(&5, &1).unwrap();
		assert_eq!(updated_recovery.deposit, new_deposit.into());
		assert_eq!(updated_recovery.friends, recovery.friends);

		// Check event was emitted
		System::assert_has_event(
			Event::<Test>::DepositPoked {
				who: 1,
				kind: DepositKind::ActiveRecoveryFor(5),
				old_deposit,
				new_deposit: new_deposit.into(),
			}
			.into(),
		);
	});
}

#[test]
fn poke_deposit_works_for_both_deposits() {
	new_test_ext().execute_with(|| {
		// Setup recovery config for account 5
		assert_ok!(Recovery::create_recovery(RuntimeOrigin::signed(5), vec![2, 3, 4], 3, 10));

		// Account 5 also initiates recovery for another account
		assert_ok!(Recovery::create_recovery(RuntimeOrigin::signed(1), vec![2, 3, 4], 3, 10));
		assert_ok!(Recovery::initiate_recovery(RuntimeOrigin::signed(5), 1));

		// Verify initial storage state
		let initial_config_deposit = 13;
		let initial_recovery_deposit = 10;
		assert_eq!(Recovery::recovery_config(5).unwrap().deposit, initial_config_deposit);
		assert_eq!(
			<ActiveRecoveries<Test>>::get(&1, &5).unwrap().deposit,
			initial_recovery_deposit
		);
		assert_eq!(
			Balances::reserved_balance(5),
			initial_config_deposit + initial_recovery_deposit
		);

		// Change both deposit requirements
		ConfigDepositBase::set(20);
		RecoveryDeposit::set(15);

		// Poke deposits
		let result = Recovery::poke_deposit(RuntimeOrigin::signed(5), Some(1));
		assert_ok!(result.as_ref());
		assert_eq!(result.unwrap(), Pays::No.into());

		// Verify storage and balances were updated
		let new_config_deposit = 23;
		let new_recovery_deposit = 15;
		assert_eq!(Recovery::recovery_config(5).unwrap().deposit, new_config_deposit);
		assert_eq!(<ActiveRecoveries<Test>>::get(&1, &5).unwrap().deposit, new_recovery_deposit);
		assert_eq!(Balances::reserved_balance(5), new_config_deposit + new_recovery_deposit);

		// Check both events were emitted
		System::assert_has_event(
			Event::<Test>::DepositPoked {
				who: 5,
				kind: DepositKind::RecoveryConfig,
				old_deposit: initial_config_deposit,
				new_deposit: new_config_deposit,
			}
			.into(),
		);
		System::assert_has_event(
			Event::<Test>::DepositPoked {
				who: 5,
				kind: DepositKind::ActiveRecoveryFor(1),
				old_deposit: initial_recovery_deposit,
				new_deposit: new_recovery_deposit,
			}
			.into(),
		);
	});
}

#[test]
fn poke_deposit_charges_fee_for_no_deposits() {
	new_test_ext().execute_with(|| {
		let result = Recovery::poke_deposit(RuntimeOrigin::signed(1), None);
		assert_ok!(result.as_ref());
		assert_eq!(result.unwrap(), Pays::Yes.into());

		// No events should be emitted
		assert!(!System::events().iter().any(|record| matches!(
			record.event,
			RuntimeEvent::Recovery(Event::DepositPoked { .. })
		)));
	});
}

#[test]
fn poke_deposit_charges_fee_for_unchanged_deposits() {
	new_test_ext().execute_with(|| {
		assert_ok!(Recovery::create_recovery(RuntimeOrigin::signed(5), vec![2, 3, 4], 3, 10));

		// Verify initial state
		let old_deposit = Balances::reserved_balance(5);
		// Base 10 + 1 per friend
		assert_eq!(old_deposit, 13);
		let config = Recovery::recovery_config(5).unwrap();
		assert_eq!(config.deposit, old_deposit);

		let result = Recovery::poke_deposit(RuntimeOrigin::signed(5), None);
		assert_ok!(result.as_ref());
		assert_eq!(result.unwrap(), Pays::Yes.into());

		// Verify final state
		let new_deposit = Balances::reserved_balance(5);
		// New base 20 + 1 per friend
		assert_eq!(new_deposit, old_deposit);
		let updated_config = Recovery::recovery_config(5).unwrap();
		assert_eq!(updated_config.deposit, old_deposit);
		// No events should be emitted
		assert!(!System::events().iter().any(|record| matches!(
			record.event,
			RuntimeEvent::Recovery(Event::DepositPoked { .. })
		)));
	});
}

#[test]
fn poke_deposit_works_with_multiple_active_recoveries() {
	new_test_ext().execute_with(|| {
		// Setup multiple accounts with recovery
		for i in 1..=3 {
			assert_ok!(Recovery::create_recovery(RuntimeOrigin::signed(i), vec![2, 3, 4], 3, 10));
		}

		// Account 5 initiates recovery for all of them
		let old_deposit = 10;
		for i in 1..=3 {
			assert_ok!(Recovery::initiate_recovery(RuntimeOrigin::signed(5), i));
			// Verify initial state for each recovery
			let recovery = <ActiveRecoveries<Test>>::get(&i, &5).unwrap();
			assert_eq!(recovery.deposit, old_deposit);
		}

		// Initial total reserved = 3 * old_deposit
		let initial_total_reserved = old_deposit * 3;
		assert_eq!(Balances::reserved_balance(5), initial_total_reserved);

		// Change recovery deposit
		let new_deposit = 15;
		RecoveryDeposit::set(new_deposit);

		// Poke deposits
		let result = Recovery::poke_deposit(RuntimeOrigin::signed(5), Some(1));
		assert_ok!(result.as_ref());
		assert_eq!(result.unwrap(), Pays::No.into());

		let result = Recovery::poke_deposit(RuntimeOrigin::signed(5), Some(2));
		assert_ok!(result.as_ref());
		assert_eq!(result.unwrap(), Pays::No.into());

		let result = Recovery::poke_deposit(RuntimeOrigin::signed(5), Some(3));
		assert_ok!(result.as_ref());
		assert_eq!(result.unwrap(), Pays::No.into());

		// Verify final state for each recovery
		for i in 1..=3 {
			let updated_recovery = <ActiveRecoveries<Test>>::get(&i, &5).unwrap();
			assert_eq!(updated_recovery.deposit, new_deposit.into());
		}

		// All deposits should be updated
		let new_total_reserved = new_deposit * 3;
		assert_eq!(Balances::reserved_balance(5), new_total_reserved.into());

		System::assert_has_event(
			Event::<Test>::DepositPoked {
				who: 5,
				kind: DepositKind::ActiveRecoveryFor(1),
				old_deposit,
				new_deposit: new_deposit.into(),
			}
			.into(),
		);

		System::assert_has_event(
			Event::<Test>::DepositPoked {
				who: 5,
				kind: DepositKind::ActiveRecoveryFor(2),
				old_deposit,
				new_deposit: new_deposit.into(),
			}
			.into(),
		);

		System::assert_has_event(
			Event::<Test>::DepositPoked {
				who: 5,
				kind: DepositKind::ActiveRecoveryFor(3),
				old_deposit,
				new_deposit: new_deposit.into(),
			}
			.into(),
		);
	});
}

#[test]
fn poke_deposit_handles_insufficient_balance() {
	new_test_ext().execute_with(|| {
		// Setup recovery config
		assert_ok!(Recovery::create_recovery(RuntimeOrigin::signed(5), vec![2, 3, 4], 3, 10));
		assert_eq!(Balances::reserved_balance(5), 13);

		// Increase required deposit
		ConfigDepositBase::set(200);

		// Should fail due to insufficient balance
		assert_noop!(
			Recovery::poke_deposit(RuntimeOrigin::signed(5), None),
			pallet_balances::Error::<Test>::InsufficientBalance
		);
		// Original deposit should remain unchanged
		assert_eq!(Balances::reserved_balance(5), 13);
	});
}
