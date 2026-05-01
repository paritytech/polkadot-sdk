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

//! Test the behavior of a runtime when both `Fungible` and `Currency` traits are in use and are
//! being mixed.
//!
//! The primitives that we have and can mix are:
//!
//! * locks
//! * reserves
//! * holds
//! * freezes
//!
//! All permutations of which are:
//!
//! * Two primitives combined
//! 	* locks + reserves
//! 	* locks + holds
//! 	* locks + freezes
//! 	* reserves + holds
//! 	* reserves + freezes
//! 	* holds + freezes
//!
//! * Three primitives combined
//! 	* locks + reserves + holds
//! 	* locks + reserves + freezes
//! 	* locks + holds + freezes
//! 	* reserves + holds + freezes
//!
//! * All four primitives combined
//! 	* locks + reserves + holds + freezes
//!
//! For each test, after creating the primitive, we check:
//!
//! * The account data triplet.
//! * What `can_reserve` returns and where is the boundary.
//! * What `can_hold` returns and where is the boundary.

use super::*;
use frame_support::traits::{
	fungible::{InspectHold, MutateFreeze, MutateHold},
	Currency, LockIdentifier, LockableCurrency, ReservableCurrency, WithdrawReasons,
};

fn subject() -> AccountId {
	let subject = 1;
	Balances::make_free_balance_be(&subject, 100);
	subject
}

const ID: LockIdentifier = *b"1       ";

fn b(x: AccountId) -> (Balance, Balance, Balance) {
	let a = get_test_account_data(x);
	(a.free, a.reserved, a.frozen)
}

fn ensure_max_reserve(who: AccountId, amount: Balance) {
	assert!(!<Balances as ReservableCurrency<_>>::can_reserve(&who, amount.max(1) * 2));
	assert!(!<Balances as ReservableCurrency<_>>::can_reserve(&who, amount + 1));
	assert!(<Balances as ReservableCurrency<_>>::can_reserve(&who, amount));
	assert!(<Balances as ReservableCurrency<_>>::can_reserve(&who, amount.saturating_sub(1)));
	assert!(<Balances as ReservableCurrency<_>>::can_reserve(&who, amount / 2));
}

fn ensure_max_hold(who: AccountId, amount: Balance) {
	assert!(<Balances as InspectHold<_>>::ensure_can_hold(&TestId::Foo, &who, amount.max(1) * 2)
		.is_err());
	assert!(<Balances as InspectHold<_>>::ensure_can_hold(&TestId::Foo, &who, amount + 1).is_err());
	assert!(<Balances as InspectHold<_>>::ensure_can_hold(&TestId::Foo, &who, amount).is_ok());
	assert!(<Balances as InspectHold<_>>::ensure_can_hold(
		&TestId::Foo,
		&who,
		amount.saturating_sub(1)
	)
	.is_ok());
	assert!(<Balances as InspectHold<_>>::ensure_can_hold(&TestId::Foo, &who, amount / 2).is_ok());
}

// Two primitives combined

#[test]
fn locks_and_reserves() {
	ExtBuilder::default()
		.monied(false)
		.existential_deposit(1)
		.build_and_execute_with(|| {
			let who = subject();

			<Balances as LockableCurrency<_>>::set_lock(ID, &who, 50, WithdrawReasons::all());
			assert_eq!(b(who), (100, 0, 50));

			// Can reserve up to 99 (leaving 1 for ED)
			ensure_max_reserve(who, 99);
			// Can hold up to 99 (leaving 1 for ED)
			ensure_max_hold(who, 99);

			assert_ok!(<Balances as ReservableCurrency<_>>::reserve(&who, 30));
			assert_eq!(b(who), (70, 30, 50));

			// Can hold or reserve up to 69 more (leaving 1 for ED)
			let expected = 69;
			ensure_max_reserve(who, expected);
			ensure_max_hold(who, expected);
		});
}

#[test]
fn locks_and_holds() {
	ExtBuilder::default()
		.monied(false)
		.existential_deposit(1)
		.build_and_execute_with(|| {
			let who = subject();

			// Lock 60 tokens
			<Balances as LockableCurrency<_>>::set_lock(ID, &who, 60, WithdrawReasons::all());
			assert_eq!(b(who), (100, 0, 60));

			ensure_max_hold(who, 99);
			ensure_max_reserve(who, 99);

			// Hold 40 tokens
			assert_ok!(<Balances as MutateHold<_>>::hold(&TestId::Foo, &who, 40));
			assert_eq!(b(who), (60, 40, 60));

			// Can hold or reserve up to 59 more (leaving 1 for ED)
			let expected = 59;
			ensure_max_hold(who, expected);
			ensure_max_reserve(who, expected);
		});
}

#[test]
fn locks_and_freezes() {
	ExtBuilder::default()
		.monied(false)
		.existential_deposit(1)
		.build_and_execute_with(|| {
			let who = subject();

			// Lock 40 tokens
			<Balances as LockableCurrency<_>>::set_lock(ID, &who, 40, WithdrawReasons::all());
			assert_eq!(b(who), (100, 0, 40));

			// Freeze 70 tokens
			assert_ok!(<Balances as MutateFreeze<_>>::set_freeze(&TestId::Foo, &who, 70));
			// Frozen takes the max of lock (40) and freeze (70)
			assert_eq!(b(who), (100, 0, 70));

			// Can hold or reserve up to 64 more (leaving 1 for ED)
			let expected = 99;
			ensure_max_hold(who, expected);
			ensure_max_reserve(who, expected);
		});
}

#[test]
fn reserves_and_holds() {
	ExtBuilder::default()
		.monied(false)
		.existential_deposit(1)
		.build_and_execute_with(|| {
			let who = subject();

			// Reserve 30 tokens
			assert_ok!(<Balances as ReservableCurrency<_>>::reserve(&who, 30));
			assert_eq!(b(who), (70, 30, 0));
			ensure_max_reserve(who, 69);
			ensure_max_hold(who, 69);

			// Hold 25 tokens (accumulates with reserve)
			assert_ok!(<Balances as MutateHold<_>>::hold(&TestId::Foo, &who, 25));
			assert_eq!(b(who), (45, 55, 0)); // reserved = 30 + 25 = 55

			// Can hold or reserve up to 44 more (leaving 1 for ED)
			let expected = 44;
			ensure_max_reserve(who, expected);
			ensure_max_hold(who, expected);
		});
}

#[test]
fn reserves_and_freezes() {
	ExtBuilder::default()
		.monied(false)
		.existential_deposit(1)
		.build_and_execute_with(|| {
			let who = subject();

			// Reserve 25 tokens
			assert_ok!(<Balances as ReservableCurrency<_>>::reserve(&who, 25));
			assert_eq!(b(who), (75, 25, 0));

			// Freeze 80 tokens
			assert_ok!(<Balances as MutateFreeze<_>>::set_freeze(&TestId::Foo, &who, 80));
			assert_eq!(b(who), (75, 25, 80));

			// Can hold or reserve up to 74 more (leaving 1 for ED)
			let expected = 74;
			ensure_max_reserve(who, expected);
			ensure_max_hold(who, expected);
		});
}

#[test]
fn holds_and_freezes() {
	ExtBuilder::default()
		.monied(false)
		.existential_deposit(1)
		.build_and_execute_with(|| {
			let who = subject();

			// Hold 35 tokens
			assert_ok!(<Balances as MutateHold<_>>::hold(&TestId::Foo, &who, 35));
			assert_eq!(b(who), (65, 35, 0));

			// Can hold or reserve up to 64 more (leaving 1 for ED)
			let expected = 64;
			ensure_max_reserve(who, expected);
			ensure_max_hold(who, expected);

			// Freeze 90 tokens
			assert_ok!(<Balances as MutateFreeze<_>>::set_freeze(&TestId::Foo, &who, 90));
			assert_eq!(b(who), (65, 35, 90));

			// Can hold or reserve up to 64 more (leaving 1 for ED)
			let expected = 64;
			ensure_max_hold(who, expected);
			ensure_max_reserve(who, expected);
		});
}

// Three primitives combined

#[test]
fn locks_reserves_and_holds() {
	ExtBuilder::default()
		.monied(false)
		.existential_deposit(1)
		.build_and_execute_with(|| {
			let who = subject();

			// Lock 60 tokens
			<Balances as LockableCurrency<_>>::set_lock(ID, &who, 60, WithdrawReasons::all());
			assert_eq!(b(who), (100, 0, 60));

			// Reserve 20 tokens
			assert_ok!(<Balances as ReservableCurrency<_>>::reserve(&who, 20));
			assert_eq!(b(who), (80, 20, 60));

			// Can hold or reserve up to 79 more (leaving 1 for ED)
			let expected = 79;
			ensure_max_reserve(who, expected);
			ensure_max_hold(who, expected);

			// Hold 15 tokens (accumulates with reserve)
			assert_ok!(<Balances as MutateHold<_>>::hold(&TestId::Foo, &who, 15));
			assert_eq!(b(who), (65, 35, 60)); // reserved = 20 + 15 = 35

			// Can hold or reserve up to 64 more (leaving 1 for ED)
			let expected = 64;
			ensure_max_reserve(who, expected);
			ensure_max_hold(who, expected);
		});
}

#[test]
fn locks_reserves_and_freezes() {
	ExtBuilder::default()
		.monied(false)
		.existential_deposit(1)
		.build_and_execute_with(|| {
			let who = subject();

			// Lock 40 tokens
			<Balances as LockableCurrency<_>>::set_lock(ID, &who, 40, WithdrawReasons::all());
			assert_eq!(b(who), (100, 0, 40));

			// Reserve 25 tokens
			assert_ok!(<Balances as ReservableCurrency<_>>::reserve(&who, 25));
			assert_eq!(b(who), (75, 25, 40));

			// Freeze 80 tokens (max of lock 40 and freeze 80)
			assert_ok!(<Balances as MutateFreeze<_>>::set_freeze(&TestId::Foo, &who, 80));
			assert_eq!(b(who), (75, 25, 80));

			// Can hold or reserve up to 74 more (leaving 1 for ED)
			let expected = 74;
			ensure_max_reserve(who, expected);
			ensure_max_hold(who, expected);
		});
}

#[test]
fn locks_holds_and_freezes() {
	ExtBuilder::default()
		.monied(false)
		.existential_deposit(1)
		.build_and_execute_with(|| {
			let who = subject();

			// Lock 50 tokens
			<Balances as LockableCurrency<_>>::set_lock(ID, &who, 50, WithdrawReasons::all());
			assert_eq!(b(who), (100, 0, 50));

			// Hold 30 tokens
			assert_ok!(<Balances as MutateHold<_>>::hold(&TestId::Foo, &who, 30));
			assert_eq!(b(who), (70, 30, 50));

			// Freeze 75 tokens (max of lock 50 and freeze 75)
			assert_ok!(<Balances as MutateFreeze<_>>::set_freeze(&TestId::Foo, &who, 75));
			assert_eq!(b(who), (70, 30, 75));

			// Can hold or reserve up to 69 more (leaving 1 for ED)
			let expected = 69;
			ensure_max_hold(who, expected);
			ensure_max_reserve(who, expected);
		});
}

#[test]
fn reserves_holds_and_freezes() {
	ExtBuilder::default()
		.monied(false)
		.existential_deposit(1)
		.build_and_execute_with(|| {
			let who = subject();

			// Reserve 20 tokens
			assert_ok!(<Balances as ReservableCurrency<_>>::reserve(&who, 20));
			assert_eq!(b(who), (80, 20, 0));

			// Can hold or reserve up to 79 more (leaving 1 for ED)
			let expected = 79;
			ensure_max_reserve(who, expected);
			ensure_max_hold(who, expected);

			// Hold 25 tokens (accumulates with reserve)
			assert_ok!(<Balances as MutateHold<_>>::hold(&TestId::Foo, &who, 25));
			assert_eq!(b(who), (55, 45, 0)); // reserved = 20 + 25 = 45

			// Can hold or reserve up to 54 more (leaving 1 for ED)
			let expected = 54;
			ensure_max_reserve(who, expected);
			ensure_max_hold(who, expected);

			// Freeze 90 tokens
			assert_ok!(<Balances as MutateFreeze<_>>::set_freeze(&TestId::Foo, &who, 90));
			assert_eq!(b(who), (55, 45, 90));

			// Can hold or reserve up to 54 more (leaving 1 for ED)
			let expected = 54;
			ensure_max_hold(who, expected);
			ensure_max_reserve(who, expected);
		});
}

// All four primitives combined

#[test]
fn locks_reserves_holds_and_freezes() {
	ExtBuilder::default()
		.monied(false)
		.existential_deposit(1)
		.build_and_execute_with(|| {
			let who = subject();

			// Lock 40 tokens
			<Balances as LockableCurrency<_>>::set_lock(ID, &who, 40, WithdrawReasons::all());
			assert_eq!(b(who), (100, 0, 40));

			// Can hold or reserve up to 99 more (leaving 1 for ED)
			let expected = 99;
			ensure_max_reserve(who, expected);
			ensure_max_hold(who, expected);

			// Reserve 20 tokens
			assert_ok!(<Balances as ReservableCurrency<_>>::reserve(&who, 20));
			assert_eq!(b(who), (80, 20, 40));

			// Can hold or reserve up to 79 more (leaving 1 for ED)
			let expected = 79;
			ensure_max_reserve(who, expected);
			ensure_max_hold(who, expected);

			// Hold 15 tokens (accumulates with reserve)
			assert_ok!(<Balances as MutateHold<_>>::hold(&TestId::Foo, &who, 15));
			assert_eq!(b(who), (65, 35, 40)); // reserved = 20 + 15 = 35

			// Can hold or reserve up to 64 more (leaving 1 for ED)
			let expected = 64;
			ensure_max_reserve(who, expected);
			ensure_max_hold(who, expected);

			// Freeze 85 tokens (max of lock 40 and freeze 85)
			assert_ok!(<Balances as MutateFreeze<_>>::set_freeze(&TestId::Foo, &who, 85));
			assert_eq!(b(who), (65, 35, 85));

			// Can hold or reserve up to 64 more (leaving 1 for ED)
			let expected = 64;
			ensure_max_hold(who, expected);
			ensure_max_reserve(who, expected);
		});
}
