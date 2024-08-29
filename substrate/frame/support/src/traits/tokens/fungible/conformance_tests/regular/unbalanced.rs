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
	fungible::{Inspect, Unbalanced},
	tokens::{Fortitude, Precision, Preservation},
};
use core::fmt::Debug;
use sp_arithmetic::{traits::AtLeast8BitUnsigned, ArithmeticError};
use sp_runtime::{traits::Bounded, TokenError};

/// Tests [`Unbalanced::write_balance`].
///
/// We don't need to test the Error case for this function, because the trait makes no
/// assumptions about the ways it can fail. That is completely an implementation detail.
pub fn write_balance<T, AccountId>()
where
	T: Unbalanced<AccountId>,
	<T as Inspect<AccountId>>::Balance: AtLeast8BitUnsigned + Debug,
	AccountId: AtLeast8BitUnsigned,
{
	// Setup some accounts to test varying initial balances
	let account_0_ed = AccountId::from(0);
	let account_1_gt_ed = AccountId::from(1);
	let account_2_empty = AccountId::from(2);
	T::increase_balance(&account_0_ed, T::minimum_balance(), Precision::Exact).unwrap();
	T::increase_balance(&account_1_gt_ed, T::minimum_balance() + 5.into(), Precision::Exact)
		.unwrap();

	// Test setting the balances of each account by gt the minimum balance succeeds with no
	// dust.
	let amount = T::minimum_balance() + 10.into();
	assert_eq!(T::write_balance(&account_0_ed, amount), Ok(None));
	assert_eq!(T::write_balance(&account_1_gt_ed, amount), Ok(None));
	assert_eq!(T::write_balance(&account_2_empty, amount), Ok(None));
	assert_eq!(T::balance(&account_0_ed), amount);
	assert_eq!(T::balance(&account_1_gt_ed), amount);
	assert_eq!(T::balance(&account_2_empty), amount);

	// Test setting the balances of each account to below the minimum balance succeeds with
	// the expected dust.
	// If the minimum balance is 1, then the dust is 0, represented as None.
	// If the minimum balance is >1, then the dust is the remaining balance that will be wiped
	// as the account is reaped.
	let amount = T::minimum_balance() - 1.into();
	if T::minimum_balance() == 1.into() {
		assert_eq!(T::write_balance(&account_0_ed, amount), Ok(None));
		assert_eq!(T::write_balance(&account_1_gt_ed, amount), Ok(None));
		assert_eq!(T::write_balance(&account_2_empty, amount), Ok(None));
	} else if T::minimum_balance() > 1.into() {
		assert_eq!(T::write_balance(&account_0_ed, amount), Ok(Some(amount)));
		assert_eq!(T::write_balance(&account_1_gt_ed, amount), Ok(Some(amount)));
		assert_eq!(T::write_balance(&account_2_empty, amount), Ok(Some(amount)));
	}
}

/// Tests [`Unbalanced::decrease_balance`] called with [`Preservation::Expendable`].
pub fn decrease_balance_expendable<T, AccountId>()
where
	T: Unbalanced<AccountId>,
	<T as Inspect<AccountId>>::Balance: AtLeast8BitUnsigned + Debug,
	AccountId: AtLeast8BitUnsigned,
{
	// Setup account with some balance
	let account_0 = AccountId::from(0);
	let account_0_initial_balance = T::minimum_balance() + 10.into();
	T::increase_balance(&account_0, account_0_initial_balance, Precision::Exact).unwrap();

	// Decreasing the balance still above the minimum balance should not reap the account.
	let amount = 1.into();
	assert_eq!(
		T::decrease_balance(
			&account_0,
			amount,
			Precision::Exact,
			Preservation::Expendable,
			Fortitude::Polite,
		),
		Ok(amount),
	);
	assert_eq!(T::balance(&account_0), account_0_initial_balance - amount);

	// Decreasing the balance below funds available should fail when Precision::Exact
	let balance_before = T::balance(&account_0);
	assert_eq!(
		T::decrease_balance(
			&account_0,
			account_0_initial_balance,
			Precision::Exact,
			Preservation::Expendable,
			Fortitude::Polite,
		),
		Err(TokenError::FundsUnavailable.into())
	);
	// Balance unchanged
	assert_eq!(T::balance(&account_0), balance_before);

	// And reap the account when Precision::BestEffort
	assert_eq!(
		T::decrease_balance(
			&account_0,
			account_0_initial_balance,
			Precision::BestEffort,
			Preservation::Expendable,
			Fortitude::Polite,
		),
		Ok(balance_before),
	);
	// Account reaped
	assert_eq!(T::balance(&account_0), 0.into());
}

/// Tests [`Unbalanced::decrease_balance`] called with [`Preservation::Preserve`].
pub fn decrease_balance_preserve<T, AccountId>()
where
	T: Unbalanced<AccountId>,
	<T as Inspect<AccountId>>::Balance: AtLeast8BitUnsigned + Debug,
	AccountId: AtLeast8BitUnsigned,
{
	// Setup account with some balance
	let account_0 = AccountId::from(0);
	let account_0_initial_balance = T::minimum_balance() + 10.into();
	T::increase_balance(&account_0, account_0_initial_balance, Precision::Exact).unwrap();

	// Decreasing the balance below the minimum when Precision::Exact should fail.
	let amount = 11.into();
	assert_eq!(
		T::decrease_balance(
			&account_0,
			amount,
			Precision::Exact,
			Preservation::Preserve,
			Fortitude::Polite,
		),
		Err(TokenError::FundsUnavailable.into()),
	);
	// Balance should not have changed.
	assert_eq!(T::balance(&account_0), account_0_initial_balance);

	// Decreasing the balance below the minimum when Precision::BestEffort should reduce to
	// minimum balance.
	let amount = 11.into();
	assert_eq!(
		T::decrease_balance(
			&account_0,
			amount,
			Precision::BestEffort,
			Preservation::Preserve,
			Fortitude::Polite,
		),
		Ok(account_0_initial_balance - T::minimum_balance()),
	);
	assert_eq!(T::balance(&account_0), T::minimum_balance());
}

/// Tests [`Unbalanced::increase_balance`].
pub fn increase_balance<T, AccountId>()
where
	T: Unbalanced<AccountId>,
	<T as Inspect<AccountId>>::Balance: AtLeast8BitUnsigned + Debug,
	AccountId: AtLeast8BitUnsigned,
{
	let account_0 = AccountId::from(0);
	assert_eq!(T::balance(&account_0), 0.into());

	// Increasing the bal below the ED errors when precision is Exact
	if T::minimum_balance() > 0.into() {
		assert_eq!(
			T::increase_balance(&account_0, T::minimum_balance() - 1.into(), Precision::Exact),
			Err(TokenError::BelowMinimum.into()),
		);
	}
	assert_eq!(T::balance(&account_0), 0.into());

	// Increasing the bal below the ED leaves the balance at zero when precision is BestEffort
	if T::minimum_balance() > 0.into() {
		assert_eq!(
			T::increase_balance(&account_0, T::minimum_balance() - 1.into(), Precision::BestEffort),
			Ok(0.into()),
		);
	}
	assert_eq!(T::balance(&account_0), 0.into());

	// Can increase if new bal is >= ED
	assert_eq!(
		T::increase_balance(&account_0, T::minimum_balance(), Precision::Exact),
		Ok(T::minimum_balance()),
	);
	assert_eq!(T::balance(&account_0), T::minimum_balance());
	assert_eq!(T::increase_balance(&account_0, 5.into(), Precision::Exact), Ok(5.into()),);
	assert_eq!(T::balance(&account_0), T::minimum_balance() + 5.into());

	// Increasing by amount that would overflow fails when precision is Exact
	assert_eq!(
		T::increase_balance(&account_0, T::Balance::max_value(), Precision::Exact),
		Err(ArithmeticError::Overflow.into()),
	);

	// Increasing by amount that would overflow saturates when precision is BestEffort
	let balance_before = T::balance(&account_0);
	assert_eq!(
		T::increase_balance(&account_0, T::Balance::max_value(), Precision::BestEffort),
		Ok(T::Balance::max_value() - balance_before),
	);
	assert_eq!(T::balance(&account_0), T::Balance::max_value());
}

/// Tests [`Unbalanced::set_total_issuance`].
pub fn set_total_issuance<T, AccountId>()
where
	T: Unbalanced<AccountId>,
	<T as Inspect<AccountId>>::Balance: AtLeast8BitUnsigned + Debug,
	AccountId: AtLeast8BitUnsigned,
{
	T::set_total_issuance(1.into());
	assert_eq!(T::total_issuance(), 1.into());

	T::set_total_issuance(0.into());
	assert_eq!(T::total_issuance(), 0.into());

	T::set_total_issuance(T::minimum_balance());
	assert_eq!(T::total_issuance(), T::minimum_balance());

	T::set_total_issuance(T::minimum_balance() + 5.into());
	assert_eq!(T::total_issuance(), T::minimum_balance() + 5.into());

	if T::minimum_balance() > 0.into() {
		T::set_total_issuance(T::minimum_balance() - 1.into());
		assert_eq!(T::total_issuance(), T::minimum_balance() - 1.into());
	}
}

/// Tests [`Unbalanced::deactivate`] and [`Unbalanced::reactivate`].
pub fn deactivate_and_reactivate<T, AccountId>()
where
	T: Unbalanced<AccountId>,
	<T as Inspect<AccountId>>::Balance: AtLeast8BitUnsigned + Debug,
	AccountId: AtLeast8BitUnsigned,
{
	T::set_total_issuance(10.into());
	assert_eq!(T::total_issuance(), 10.into());
	assert_eq!(T::active_issuance(), 10.into());

	T::deactivate(2.into());
	assert_eq!(T::total_issuance(), 10.into());
	assert_eq!(T::active_issuance(), 8.into());

	// Saturates at total_issuance
	T::reactivate(4.into());
	assert_eq!(T::total_issuance(), 10.into());
	assert_eq!(T::active_issuance(), 10.into());

	// Decrements correctly after saturating at total_issuance
	T::deactivate(1.into());
	assert_eq!(T::total_issuance(), 10.into());
	assert_eq!(T::active_issuance(), 9.into());

	// Saturates at zero
	T::deactivate(15.into());
	assert_eq!(T::total_issuance(), 10.into());
	assert_eq!(T::active_issuance(), 0.into());

	// Increments correctly after saturating at zero
	T::reactivate(1.into());
	assert_eq!(T::total_issuance(), 10.into());
	assert_eq!(T::active_issuance(), 1.into());
}
