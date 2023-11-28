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

use crate::traits::{
	fungible::{Balanced, Inspect},
	tokens::{imbalance::Imbalance as ImbalanceT, Fortitude, Precision, Preservation},
};
use core::fmt::Debug;
use frame_support::traits::tokens::fungible::imbalance::{Credit, Debt};
use sp_arithmetic::{traits::AtLeast8BitUnsigned, ArithmeticError};
use sp_runtime::{traits::Bounded, TokenError};

/// Tests issuing and resolving [`Credit`] imbalances with [`Balanced::issue`] and
/// [`Balanced::resolve`].
pub fn issue_and_resolve_credit<T, AccountId>()
where
	T: Balanced<AccountId>,
	<T as Inspect<AccountId>>::Balance: AtLeast8BitUnsigned + Debug,
	AccountId: AtLeast8BitUnsigned,
{
	let account = AccountId::from(0);
	assert_eq!(T::total_issuance(), 0.into());
	assert_eq!(T::balance(&account), 0.into());

	// Account that doesn't exist yet can't be credited below the minimum balance
	let credit: Credit<AccountId, T> = T::issue(T::minimum_balance() - 1.into());
	// issue temporarily increases total issuance
	assert_eq!(T::total_issuance(), credit.peek());
	match T::resolve(&account, credit) {
		Ok(_) => panic!("Balanced::resolve should have failed"),
		Err(c) => assert_eq!(c.peek(), T::minimum_balance() - 1.into()),
	};
	// Credit was unused and dropped from total issuance
	assert_eq!(T::total_issuance(), 0.into());
	assert_eq!(T::balance(&account), 0.into());

	// Credit account with minimum balance
	let credit: Credit<AccountId, T> = T::issue(T::minimum_balance());
	match T::resolve(&account, credit) {
		Ok(()) => {},
		Err(_) => panic!("resolve failed"),
	};
	assert_eq!(T::total_issuance(), T::minimum_balance());
	assert_eq!(T::balance(&account), T::minimum_balance());

	// Now that account has been created, it can be credited with an amount below the minimum
	// balance.
	let total_issuance_before = T::total_issuance();
	let balance_before = T::balance(&account);
	let amount = T::minimum_balance() - 1.into();
	let credit: Credit<AccountId, T> = T::issue(amount);
	match T::resolve(&account, credit) {
		Ok(()) => {},
		Err(_) => panic!("resolve failed"),
	};
	assert_eq!(T::total_issuance(), total_issuance_before + amount);
	assert_eq!(T::balance(&account), balance_before + amount);

	// Unhandled issuance is dropped from total issuance
	// `let _ = ...` immediately drops the issuance, so everything should be unchanged when
	// logic gets to the assertions.
	let total_issuance_before = T::total_issuance();
	let balance_before = T::balance(&account);
	let _ = T::issue(5.into());
	assert_eq!(T::total_issuance(), total_issuance_before);
	assert_eq!(T::balance(&account), balance_before);
}

/// Tests issuing and resolving [`Debt`] imbalances with [`Balanced::rescind`] and
/// [`Balanced::settle`].
pub fn rescind_and_settle_debt<T, AccountId>()
where
	T: Balanced<AccountId>,
	<T as Inspect<AccountId>>::Balance: AtLeast8BitUnsigned + Debug,
	AccountId: AtLeast8BitUnsigned,
{
	// Credit account with some balance
	let account = AccountId::from(0);
	let initial_bal = T::minimum_balance() + 10.into();
	let credit = T::issue(initial_bal);
	match T::resolve(&account, credit) {
		Ok(()) => {},
		Err(_) => panic!("resolve failed"),
	};
	assert_eq!(T::total_issuance(), initial_bal);
	assert_eq!(T::balance(&account), initial_bal);

	// Rescind some balance
	let rescind_amount = 2.into();
	let debt: Debt<AccountId, T> = T::rescind(rescind_amount);
	assert_eq!(debt.peek(), rescind_amount);
	match T::settle(&account, debt, Preservation::Expendable) {
		Ok(c) => {
			// We settled the full debt and account was not dusted, so there is no left over
			// credit.
			assert_eq!(c.peek(), 0.into());
		},
		Err(_) => panic!("settle failed"),
	};
	assert_eq!(T::total_issuance(), initial_bal - rescind_amount);
	assert_eq!(T::balance(&account), initial_bal - rescind_amount);

	// Unhandled debt is added from total issuance
	// `let _ = ...` immediately drops the debt, so everything should be unchanged when
	// logic gets to the assertions.
	let _ = T::rescind(T::minimum_balance());
	assert_eq!(T::total_issuance(), initial_bal - rescind_amount);
	assert_eq!(T::balance(&account), initial_bal - rescind_amount);

	// Preservation::Preserve will not allow the account to be dusted on settle
	let balance_before = T::balance(&account);
	let total_issuance_before = T::total_issuance();
	let rescind_amount = balance_before - T::minimum_balance() + 1.into();
	let debt: Debt<AccountId, T> = T::rescind(rescind_amount);
	assert_eq!(debt.peek(), rescind_amount);
	// The new debt is temporarily removed from total_issuance
	assert_eq!(T::total_issuance(), total_issuance_before - debt.peek().into());
	match T::settle(&account, debt, Preservation::Preserve) {
		Ok(_) => panic!("Balanced::settle should have failed"),
		Err(d) => assert_eq!(d.peek(), rescind_amount),
	};
	// The debt is added back to total_issuance because it was dropped, leaving the operation a
	// noop.
	assert_eq!(T::total_issuance(), total_issuance_before);
	assert_eq!(T::balance(&account), balance_before);

	// Preservation::Expendable allows the account to be dusted on settle
	let debt: Debt<AccountId, T> = T::rescind(rescind_amount);
	match T::settle(&account, debt, Preservation::Expendable) {
		Ok(c) => {
			// Dusting happens internally, there is no left over credit.
			assert_eq!(c.peek(), 0.into());
		},
		Err(_) => panic!("settle failed"),
	};
	// The account is dusted and debt dropped from total_issuance
	assert_eq!(T::total_issuance(), 0.into());
	assert_eq!(T::balance(&account), 0.into());
}

/// Tests [`Balanced::deposit`].
pub fn deposit<T, AccountId>()
where
	T: Balanced<AccountId>,
	<T as Inspect<AccountId>>::Balance: AtLeast8BitUnsigned + Debug,
	AccountId: AtLeast8BitUnsigned,
{
	// Cannot deposit < minimum balance into non-existent account
	let account = AccountId::from(0);
	let amount = T::minimum_balance() - 1.into();
	match T::deposit(&account, amount, Precision::Exact) {
		Ok(_) => panic!("Balanced::deposit should have failed"),
		Err(e) => assert_eq!(e, TokenError::BelowMinimum.into()),
	};
	assert_eq!(T::total_issuance(), 0.into());
	assert_eq!(T::balance(&account), 0.into());

	// Can deposit minimum balance into non-existent account
	let amount = T::minimum_balance();
	match T::deposit(&account, amount, Precision::Exact) {
		Ok(d) => assert_eq!(d.peek(), amount),
		Err(_) => panic!("Balanced::deposit failed"),
	};
	assert_eq!(T::total_issuance(), amount);
	assert_eq!(T::balance(&account), amount);

	// Depositing amount that would overflow when Precision::Exact fails and is a noop
	let amount = T::Balance::max_value();
	let balance_before = T::balance(&account);
	let total_issuance_before = T::total_issuance();
	match T::deposit(&account, amount, Precision::Exact) {
		Ok(_) => panic!("Balanced::deposit should have failed"),
		Err(e) => assert_eq!(e, ArithmeticError::Overflow.into()),
	};
	assert_eq!(T::total_issuance(), total_issuance_before);
	assert_eq!(T::balance(&account), balance_before);

	// Depositing amount that would overflow when Precision::BestEffort saturates
	match T::deposit(&account, amount, Precision::BestEffort) {
		Ok(d) => assert_eq!(d.peek(), T::Balance::max_value() - balance_before),
		Err(_) => panic!("Balanced::deposit failed"),
	};
	assert_eq!(T::total_issuance(), T::Balance::max_value());
	assert_eq!(T::balance(&account), T::Balance::max_value());
}

/// Tests [`Balanced::withdraw`].
pub fn withdraw<T, AccountId>()
where
	T: Balanced<AccountId>,
	<T as Inspect<AccountId>>::Balance: AtLeast8BitUnsigned + Debug,
	AccountId: AtLeast8BitUnsigned,
{
	let account = AccountId::from(0);

	// Init an account with some balance
	let initial_balance = T::minimum_balance() + 10.into();
	match T::deposit(&account, initial_balance, Precision::Exact) {
		Ok(_) => {},
		Err(_) => panic!("Balanced::deposit failed"),
	};
	assert_eq!(T::total_issuance(), initial_balance);
	assert_eq!(T::balance(&account), initial_balance);

	// Withdrawing an amount smaller than the balance works when Precision::Exact
	let amount = 1.into();
	match T::withdraw(
		&account,
		amount,
		Precision::Exact,
		Preservation::Expendable,
		Fortitude::Polite,
	) {
		Ok(c) => assert_eq!(c.peek(), amount),
		Err(_) => panic!("withdraw failed"),
	};
	assert_eq!(T::total_issuance(), initial_balance - amount);
	assert_eq!(T::balance(&account), initial_balance - amount);

	// Withdrawing an amount greater than the balance fails when Precision::Exact
	let balance_before = T::balance(&account);
	let amount = balance_before + 1.into();
	match T::withdraw(
		&account,
		amount,
		Precision::Exact,
		Preservation::Expendable,
		Fortitude::Polite,
	) {
		Ok(_) => panic!("should have failed"),
		Err(e) => assert_eq!(e, TokenError::FundsUnavailable.into()),
	};
	assert_eq!(T::total_issuance(), balance_before);
	assert_eq!(T::balance(&account), balance_before);

	// Withdrawing an amount greater than the balance works when Precision::BestEffort
	let balance_before = T::balance(&account);
	let amount = balance_before + 1.into();
	match T::withdraw(
		&account,
		amount,
		Precision::BestEffort,
		Preservation::Expendable,
		Fortitude::Polite,
	) {
		Ok(c) => assert_eq!(c.peek(), balance_before),
		Err(_) => panic!("withdraw failed"),
	};
	assert_eq!(T::total_issuance(), 0.into());
	assert_eq!(T::balance(&account), 0.into());
}

/// Tests [`Balanced::pair`].
pub fn pair<T, AccountId>()
where
	T: Balanced<AccountId>,
	<T as Inspect<AccountId>>::Balance: AtLeast8BitUnsigned + Debug,
	AccountId: AtLeast8BitUnsigned,
{
	T::set_total_issuance(50.into());

	// Pair zero balance works
	let (credit, debt) = T::pair(0.into()).unwrap();
	assert_eq!(debt.peek(), 0.into());
	assert_eq!(credit.peek(), 0.into());

	// Pair with non-zero balance: the credit and debt cancel each other out
	let balance = 10.into();
	let (credit, debt) = T::pair(balance).unwrap();
	assert_eq!(credit.peek(), balance);
	assert_eq!(debt.peek(), balance);

	// Creating a pair that could increase total_issuance beyond the max value returns an error
	let max_value = T::Balance::max_value();
	let distance_from_max_value = 5.into();
	T::set_total_issuance(max_value - distance_from_max_value);
	T::pair(distance_from_max_value + 5.into()).unwrap_err();
}
