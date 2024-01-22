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

//! Tests regarding the functionality of the dispatchables/extrinsics.

use super::*;
use crate::{
	AdjustmentDirection::{Decrease as Dec, Increase as Inc},
	Event,
};
use frame_support::traits::{fungible::Unbalanced, tokens::Preservation::Expendable};
use fungible::{hold::Mutate as HoldMutate, Inspect, Mutate};

/// Alice account ID for more readable tests.
const ALICE: u64 = 1;

#[test]
fn default_indexing_on_new_accounts_should_not_work2() {
	ExtBuilder::default()
		.existential_deposit(10)
		.monied(true)
		.build_and_execute_with(|| {
			// account 5 should not exist
			// ext_deposit is 10, value is 9, not satisfies for ext_deposit
			assert_noop!(
				Balances::transfer_allow_death(Some(1).into(), 5, 9),
				TokenError::BelowMinimum,
			);
			assert_eq!(Balances::free_balance(1), 100);
		});
}

#[test]
fn dust_account_removal_should_work() {
	ExtBuilder::default()
		.existential_deposit(100)
		.monied(true)
		.build_and_execute_with(|| {
			System::inc_account_nonce(&2);
			assert_eq!(System::account_nonce(&2), 1);
			assert_eq!(Balances::total_balance(&2), 2000);
			// index 1 (account 2) becomes zombie
			assert_ok!(Balances::transfer_allow_death(Some(2).into(), 5, 1901));
			assert_eq!(Balances::total_balance(&2), 0);
			assert_eq!(Balances::total_balance(&5), 1901);
			assert_eq!(System::account_nonce(&2), 0);
		});
}

#[test]
fn balance_transfer_works() {
	ExtBuilder::default().build_and_execute_with(|| {
		let _ = Balances::mint_into(&1, 111);
		assert_ok!(Balances::transfer_allow_death(Some(1).into(), 2, 69));
		assert_eq!(Balances::total_balance(&1), 42);
		assert_eq!(Balances::total_balance(&2), 69);
	});
}

#[test]
fn force_transfer_works() {
	ExtBuilder::default().build_and_execute_with(|| {
		let _ = Balances::mint_into(&1, 111);
		assert_noop!(Balances::force_transfer(Some(2).into(), 1, 2, 69), BadOrigin,);
		assert_ok!(Balances::force_transfer(RawOrigin::Root.into(), 1, 2, 69));
		assert_eq!(Balances::total_balance(&1), 42);
		assert_eq!(Balances::total_balance(&2), 69);
	});
}

#[test]
fn balance_transfer_when_on_hold_should_not_work() {
	ExtBuilder::default().build_and_execute_with(|| {
		let _ = Balances::mint_into(&1, 111);
		assert_ok!(Balances::hold(&TestId::Foo, &1, 69));
		assert_noop!(
			Balances::transfer_allow_death(Some(1).into(), 2, 69),
			TokenError::FundsUnavailable,
		);
	});
}

#[test]
fn transfer_keep_alive_works() {
	ExtBuilder::default().existential_deposit(1).build_and_execute_with(|| {
		let _ = Balances::mint_into(&1, 100);
		assert_noop!(
			Balances::transfer_keep_alive(Some(1).into(), 2, 100),
			TokenError::NotExpendable
		);
		assert_eq!(Balances::total_balance(&1), 100);
		assert_eq!(Balances::total_balance(&2), 0);
	});
}

#[test]
fn transfer_keep_alive_all_free_succeed() {
	ExtBuilder::default().existential_deposit(100).build_and_execute_with(|| {
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 1, 300));
		assert_ok!(Balances::hold(&TestId::Foo, &1, 100));
		assert_ok!(Balances::transfer_keep_alive(Some(1).into(), 2, 100));
		assert_eq!(Balances::total_balance(&1), 200);
		assert_eq!(Balances::total_balance(&2), 100);
	});
}

#[test]
fn transfer_all_works_1() {
	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		// setup
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 1, 200));
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 2, 0));
		// transfer all and allow death
		assert_ok!(Balances::transfer_all(Some(1).into(), 2, false));
		assert_eq!(Balances::total_balance(&1), 0);
		assert_eq!(Balances::total_balance(&2), 200);
	});
}

#[test]
fn transfer_all_works_2() {
	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		// setup
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 1, 200));
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 2, 0));
		// transfer all and keep alive
		assert_ok!(Balances::transfer_all(Some(1).into(), 2, true));
		assert_eq!(Balances::total_balance(&1), 100);
		assert_eq!(Balances::total_balance(&2), 100);
	});
}

#[test]
fn transfer_all_works_3() {
	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		// setup
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 1, 210));
		assert_ok!(Balances::hold(&TestId::Foo, &1, 10));
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 2, 0));
		// transfer all and allow death w/ reserved
		assert_ok!(Balances::transfer_all(Some(1).into(), 2, false));
		assert_eq!(Balances::total_balance(&1), 110);
		assert_eq!(Balances::total_balance(&2), 100);
	});
}

#[test]
fn transfer_all_works_4() {
	ExtBuilder::default().existential_deposit(100).build().execute_with(|| {
		// setup
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 1, 210));
		assert_ok!(Balances::hold(&TestId::Foo, &1, 10));
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 2, 0));
		// transfer all and keep alive w/ reserved
		assert_ok!(Balances::transfer_all(Some(1).into(), 2, true));
		assert_eq!(Balances::total_balance(&1), 110);
		assert_eq!(Balances::total_balance(&2), 100);
	});
}

#[test]
fn set_balance_handles_killing_account() {
	ExtBuilder::default().build_and_execute_with(|| {
		let _ = Balances::mint_into(&1, 111);
		assert_ok!(frame_system::Pallet::<Test>::inc_consumers(&1));
		assert_noop!(
			Balances::force_set_balance(RuntimeOrigin::root(), 1, 0),
			DispatchError::ConsumerRemaining,
		);
	});
}

#[test]
fn set_balance_handles_total_issuance() {
	ExtBuilder::default().build_and_execute_with(|| {
		let old_total_issuance = Balances::total_issuance();
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 1337, 69));
		assert_eq!(Balances::total_issuance(), old_total_issuance + 69);
		assert_eq!(Balances::total_balance(&1337), 69);
		assert_eq!(Balances::free_balance(&1337), 69);
	});
}

#[test]
fn upgrade_accounts_should_work() {
	ExtBuilder::default()
		.existential_deposit(1)
		.monied(true)
		.build_and_execute_with(|| {
			System::inc_providers(&7);
			assert_ok!(<Test as Config>::AccountStore::try_mutate_exists(
				&7,
				|a| -> DispatchResult {
					*a = Some(AccountData {
						free: 5,
						reserved: 5,
						frozen: Zero::zero(),
						flags: crate::types::ExtraFlags::old_logic(),
					});
					Ok(())
				}
			));
			assert!(!Balances::account(&7).flags.is_new_logic());
			assert_eq!(System::providers(&7), 1);
			assert_eq!(System::consumers(&7), 0);
			assert_ok!(Balances::upgrade_accounts(Some(1).into(), vec![7]));
			assert!(Balances::account(&7).flags.is_new_logic());
			assert_eq!(System::providers(&7), 1);
			assert_eq!(System::consumers(&7), 1);

			<Balances as frame_support::traits::ReservableCurrency<_>>::unreserve(&7, 5);
			assert_ok!(<Balances as fungible::Mutate<_>>::transfer(&7, &1, 10, Expendable));
			assert_eq!(Balances::total_balance(&7), 0);
			assert_eq!(System::providers(&7), 0);
			assert_eq!(System::consumers(&7), 0);
		});
}

#[test]
#[docify::export]
fn force_adjust_total_issuance_example() {
	ExtBuilder::default().build_and_execute_with(|| {
		// First we set the TotalIssuance to 64 by giving Alice a balance of 64.
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), ALICE, 64));
		let old_ti = Balances::total_issuance();
		assert_eq!(old_ti, 64, "TI should be 64");

		// Now test the increase:
		assert_ok!(Balances::force_adjust_total_issuance(RawOrigin::Root.into(), Inc, 32));
		let new_ti = Balances::total_issuance();
		assert_eq!(old_ti + 32, new_ti, "Should increase by 32");

		// If Alice tries to call it, it errors:
		assert_noop!(
			Balances::force_adjust_total_issuance(RawOrigin::Signed(ALICE).into(), Inc, 32),
			BadOrigin,
		);
	});
}

#[test]
fn force_adjust_total_issuance_works() {
	ExtBuilder::default().build_and_execute_with(|| {
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 1337, 64));
		let ti = Balances::total_issuance();

		// Increase works:
		assert_ok!(Balances::force_adjust_total_issuance(RawOrigin::Root.into(), Inc, 32));
		assert_eq!(Balances::total_issuance(), ti + 32);
		System::assert_last_event(RuntimeEvent::Balances(Event::TotalIssuanceForced {
			old: 64,
			new: 96,
		}));

		// Decrease works:
		assert_ok!(Balances::force_adjust_total_issuance(RawOrigin::Root.into(), Dec, 64));
		assert_eq!(Balances::total_issuance(), ti - 32);
		System::assert_last_event(RuntimeEvent::Balances(Event::TotalIssuanceForced {
			old: 96,
			new: 32,
		}));
	});
}

#[test]
fn force_adjust_total_issuance_saturates() {
	ExtBuilder::default().build_and_execute_with(|| {
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 1337, 64));
		let ti = Balances::total_issuance();
		let max = Balance::max_value();
		assert_eq!(ti, 64);

		// Increment saturates:
		assert_ok!(Balances::force_adjust_total_issuance(RawOrigin::Root.into(), Inc, max));
		assert_ok!(Balances::force_adjust_total_issuance(RawOrigin::Root.into(), Inc, 123));
		assert_eq!(Balances::total_issuance(), max);

		// Decrement saturates:
		assert_ok!(Balances::force_adjust_total_issuance(RawOrigin::Root.into(), Dec, max));
		assert_ok!(Balances::force_adjust_total_issuance(RawOrigin::Root.into(), Dec, 123));
		assert_eq!(Balances::total_issuance(), 0);
	});
}

#[test]
fn force_adjust_total_issuance_rejects_zero_delta() {
	ExtBuilder::default().build_and_execute_with(|| {
		assert_noop!(
			Balances::force_adjust_total_issuance(RawOrigin::Root.into(), Inc, 0),
			Error::<Test>::DeltaZero,
		);
		assert_noop!(
			Balances::force_adjust_total_issuance(RawOrigin::Root.into(), Dec, 0),
			Error::<Test>::DeltaZero,
		);
	});
}

#[test]
fn force_adjust_total_issuance_rejects_more_than_inactive() {
	ExtBuilder::default().build_and_execute_with(|| {
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), 1337, 64));
		Balances::deactivate(16u32.into());

		assert_eq!(Balances::total_issuance(), 64);
		assert_eq!(Balances::active_issuance(), 48);

		// Works with up to 48:
		assert_ok!(Balances::force_adjust_total_issuance(RawOrigin::Root.into(), Dec, 40),);
		assert_ok!(Balances::force_adjust_total_issuance(RawOrigin::Root.into(), Dec, 8),);
		assert_eq!(Balances::total_issuance(), 16);
		assert_eq!(Balances::active_issuance(), 0);
		// Errors with more than 48:
		assert_noop!(
			Balances::force_adjust_total_issuance(RawOrigin::Root.into(), Dec, 1),
			Error::<Test>::IssuanceDeactivated,
		);
		// Increasing again increases the inactive issuance:
		assert_ok!(Balances::force_adjust_total_issuance(RawOrigin::Root.into(), Inc, 10),);
		assert_eq!(Balances::total_issuance(), 26);
		assert_eq!(Balances::active_issuance(), 10);
	});
}
