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

#![cfg(test)]

use super::{mock::*, *};
use frame_support::{assert_noop, assert_ok, pallet_prelude::Pays};
use pallet_balances::Error as BalancesError;
use sp_runtime::MultiAddress::Id;

#[test]
fn claiming_should_work() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Indices::claim(Some(0).into(), 0),
			BalancesError::<Test, _>::InsufficientBalance
		);
		assert_ok!(Indices::claim(Some(1).into(), 0));
		assert_noop!(Indices::claim(Some(2).into(), 0), Error::<Test>::InUse);
		assert_eq!(Balances::reserved_balance(1), 1);
	});
}

#[test]
fn freeing_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(Indices::claim(Some(1).into(), 0));
		assert_ok!(Indices::claim(Some(2).into(), 1));
		assert_noop!(Indices::free(Some(0).into(), 0), Error::<Test>::NotOwner);
		assert_noop!(Indices::free(Some(1).into(), 1), Error::<Test>::NotOwner);
		assert_noop!(Indices::free(Some(1).into(), 2), Error::<Test>::NotAssigned);
		assert_ok!(Indices::free(Some(1).into(), 0));
		assert_eq!(Balances::reserved_balance(1), 0);
		assert_noop!(Indices::free(Some(1).into(), 0), Error::<Test>::NotAssigned);
	});
}

#[test]
fn freezing_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(Indices::claim(Some(1).into(), 0));
		assert_noop!(Indices::freeze(Some(1).into(), 1), Error::<Test>::NotAssigned);
		assert_noop!(Indices::freeze(Some(2).into(), 0), Error::<Test>::NotOwner);
		assert_ok!(Indices::freeze(Some(1).into(), 0));
		assert_noop!(Indices::freeze(Some(1).into(), 0), Error::<Test>::Permanent);

		assert_noop!(Indices::free(Some(1).into(), 0), Error::<Test>::Permanent);
		assert_noop!(Indices::transfer(Some(1).into(), Id(2), 0), Error::<Test>::Permanent);
	});
}

#[test]
fn indexing_lookup_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(Indices::claim(Some(1).into(), 0));
		assert_ok!(Indices::claim(Some(2).into(), 1));
		assert_eq!(Indices::lookup_index(0), Some(1));
		assert_eq!(Indices::lookup_index(1), Some(2));
		assert_eq!(Indices::lookup_index(2), None);
	});
}

#[test]
fn reclaim_index_on_accounts_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(Indices::claim(Some(1).into(), 0));
		assert_ok!(Indices::free(Some(1).into(), 0));
		assert_ok!(Indices::claim(Some(2).into(), 0));
		assert_eq!(Indices::lookup_index(0), Some(2));
		assert_eq!(Balances::reserved_balance(2), 1);
	});
}

#[test]
fn transfer_index_on_accounts_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(Indices::claim(Some(1).into(), 0));
		assert_noop!(Indices::transfer(Some(1).into(), Id(2), 1), Error::<Test>::NotAssigned);
		assert_noop!(Indices::transfer(Some(2).into(), Id(3), 0), Error::<Test>::NotOwner);
		assert_ok!(Indices::transfer(Some(1).into(), Id(3), 0));
		assert_eq!(Balances::reserved_balance(1), 0);
		assert_eq!(Balances::reserved_balance(3), 1);
		assert_eq!(Indices::lookup_index(0), Some(3));
	});
}

#[test]
fn force_transfer_index_on_preowned_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(Indices::claim(Some(1).into(), 0));
		assert_ok!(Indices::force_transfer(RuntimeOrigin::root(), Id(3), 0, false));
		assert_eq!(Balances::reserved_balance(1), 0);
		assert_eq!(Balances::reserved_balance(3), 0);
		assert_eq!(Indices::lookup_index(0), Some(3));
	});
}

#[test]
fn force_transfer_index_on_free_should_work() {
	new_test_ext().execute_with(|| {
		assert_ok!(Indices::force_transfer(RuntimeOrigin::root(), Id(3), 0, false));
		assert_eq!(Balances::reserved_balance(3), 0);
		assert_eq!(Indices::lookup_index(0), Some(3));
	});
}

#[test]
fn poke_deposit_should_fail_for_unassigned_index() {
	new_test_ext().execute_with(|| {
		assert_noop!(Indices::poke_deposit(Some(1).into(), 0), Error::<Test>::NotAssigned);
	});
}

#[test]
fn poke_deposit_should_fail_for_wrong_owner() {
	new_test_ext().execute_with(|| {
		assert_ok!(Indices::claim(Some(1).into(), 0));
		assert_noop!(Indices::poke_deposit(Some(2).into(), 0), Error::<Test>::NotOwner);
	});
}

#[test]
fn poke_deposit_should_fail_for_permanent_index() {
	new_test_ext().execute_with(|| {
		assert_ok!(Indices::claim(Some(1).into(), 0));
		assert_ok!(Indices::freeze(Some(1).into(), 0));
		assert_noop!(Indices::poke_deposit(Some(1).into(), 0), Error::<Test>::Permanent);
	});
}

#[test]
fn poke_deposit_should_fail_for_insufficient_balance() {
	new_test_ext().execute_with(|| {
		assert_ok!(Indices::claim(Some(1).into(), 0));

		// Set deposit higher than available balance
		IndexDeposit::set(1000);

		assert_noop!(
			Indices::poke_deposit(Some(1).into(), 0),
			BalancesError::<Test, _>::InsufficientBalance
		);
	});
}

#[test]
fn poke_deposit_should_work_when_deposit_increases() {
	new_test_ext().execute_with(|| {
		assert_ok!(Indices::claim(Some(1).into(), 0));
		assert_eq!(Balances::reserved_balance(1), 1);

		// Change deposit to 3
		IndexDeposit::set(3);

		// poke_deposit should work and be free
		let initial_balance = Balances::free_balance(1);
		let result = Indices::poke_deposit(Some(1).into(), 0);
		assert_ok!(result.as_ref());
		let post_info = result.unwrap();
		assert_eq!(post_info.pays_fee, Pays::No);
		assert_eq!(Balances::reserved_balance(1), 3);

		// Balance should only reduce by the deposit difference
		assert_eq!(Balances::free_balance(1), initial_balance - 2);

		System::assert_has_event(
			Event::DepositPoked { who: 1, index: 0, old_deposit: 1, new_deposit: 3 }.into(),
		);
	});
}

#[test]
fn poke_deposit_should_work_when_deposit_decreases() {
	new_test_ext().execute_with(|| {
		// Set initial deposit to 3
		IndexDeposit::set(3);
		assert_ok!(Indices::claim(Some(1).into(), 0));
		assert_eq!(Balances::reserved_balance(1), 3);

		// Change deposit to 1
		IndexDeposit::set(1);

		let initial_balance = Balances::free_balance(1);
		let result = Indices::poke_deposit(Some(1).into(), 0);
		assert_ok!(result.as_ref());
		let post_info = result.unwrap();
		assert_eq!(post_info.pays_fee, Pays::No);
		assert_eq!(Balances::reserved_balance(1), 1);

		// Balance should increase by the unreserved amount
		assert_eq!(Balances::free_balance(1), initial_balance + 2);

		System::assert_has_event(
			Event::DepositPoked { who: 1, index: 0, old_deposit: 3, new_deposit: 1 }.into(),
		);
	});
}

#[test]
fn poke_deposit_should_charge_fee_when_deposit_unchanged() {
	new_test_ext().execute_with(|| {
		assert_ok!(Indices::claim(Some(1).into(), 0));
		assert_eq!(Balances::reserved_balance(1), 1);

		// poke_deposit with same deposit amount
		let result = Indices::poke_deposit(Some(1).into(), 0);
		assert_ok!(result.as_ref());
		// Verify fee payment
		let post_info = result.unwrap();
		assert_eq!(post_info.pays_fee, Pays::Yes);

		// Reserved balance should remain the same
		assert_eq!(Balances::reserved_balance(1), 1);

		// Verify no DepositPoked event was emitted
		assert!(!System::events().iter().any(|record| matches!(
			record.event,
			RuntimeEvent::Indices(Event::DepositPoked { .. })
		)));
	});
}
