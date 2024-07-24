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

//! The traits for putting freezes within a single fungible token class.
//!
//! See the [`crate::traits::fungible`] doc for more information about fungible traits
//! including the place of the Freezes in FRAME.

use scale_info::TypeInfo;
use sp_arithmetic::{
	traits::{CheckedAdd, CheckedSub},
	ArithmeticError,
};
use sp_runtime::{DispatchResult, TokenError};

use crate::{ensure, traits::tokens::Fortitude};

/// Trait for inspecting a fungible asset which can be frozen. Freezing is essentially setting a
/// minimum balance bellow which the total balance (inclusive of any funds placed on hold) may not
/// be normally allowed to drop. Generally, freezers will provide an "update" function such that
/// if the total balance does drop below the limit, then the freezer can update their housekeeping
/// accordingly.
pub trait Inspect<AccountId>: super::Inspect<AccountId> {
	/// An identifier for a freeze.
	type Id: codec::Encode + TypeInfo + 'static;

	/// Amount of funds frozen in reserve by `who` for the given `id`.
	fn balance_frozen(id: &Self::Id, who: &AccountId) -> Self::Balance;

	/// The amount of the balance which can become frozen. Defaults to `total_balance()`.
	fn balance_freezable(who: &AccountId) -> Self::Balance {
		Self::total_balance(who)
	}

	/// Returns `true` if it's possible to introduce a freeze for the given `id` onto the
	/// account of `who`. This will be true as long as the implementor supports as many
	/// concurrent freezes as there are possible values of `id`.
	fn can_freeze(id: &Self::Id, who: &AccountId) -> bool;
}

/// Trait for introducing, altering and removing freezes for an account for its funds never
/// go below a set minimum.
pub trait Mutate<AccountId>: Inspect<AccountId> {
	/// Prevent actions which would reduce the balance of the account of `who` below the given
	/// `amount` and identify this restriction though the given `id`. Unlike `extend_freeze`, any
	/// outstanding freeze in place for `who` under the `id` are dropped.
	///
	/// If `amount` is zero, it is equivalent to using `thaw`.
	///
	/// Note that `amount` can be greater than the total balance, if desired.
	fn set_freeze(id: &Self::Id, who: &AccountId, amount: Self::Balance) -> DispatchResult;

	/// Prevent the balance of the account of `who` from being reduced below the given `amount` and
	/// identify this restriction though the given `id`. Unlike `set_freeze`, this does not
	/// counteract any pre-existing freezes in place for `who` under the `id`. Also unlike
	/// `set_freeze`, in the case that `amount` is zero, this is no-op and never fails.
	///
	/// Note that more funds can be frozen than the total balance, if desired.
	fn extend_freeze(id: &Self::Id, who: &AccountId, amount: Self::Balance) -> DispatchResult;

	/// Remove an existing freeze.
	fn thaw(id: &Self::Id, who: &AccountId) -> DispatchResult;

	/// Attempt to alter the amount frozen under the given `id` to `amount`.
	///
	/// Fail if the account of `who` has fewer freezable funds than `amount`, unless `fortitude` is
	/// [`Fortitude::Force`].
	fn set_frozen(
		id: &Self::Id,
		who: &AccountId,
		amount: Self::Balance,
		fortitude: Fortitude,
	) -> DispatchResult {
		let force = fortitude == Fortitude::Force;
		ensure!(force || Self::balance_freezable(who) >= amount, TokenError::FundsUnavailable);
		Self::set_freeze(id, who, amount)
	}

	/// Attempt to set the amount frozen under the given `id` to `amount`, iff this would increase
	/// the amount frozen under `id`. Do nothing otherwise.
	///
	/// Fail if the account of `who` has fewer freezable funds than `amount`, unless `fortitude` is
	/// [`Fortitude::Force`].
	fn ensure_frozen(
		id: &Self::Id,
		who: &AccountId,
		amount: Self::Balance,
		fortitude: Fortitude,
	) -> DispatchResult {
		let force = fortitude == Fortitude::Force;
		ensure!(force || Self::balance_freezable(who) >= amount, TokenError::FundsUnavailable);
		Self::extend_freeze(id, who, amount)
	}

	/// Decrease the amount which is being frozen for a particular freeze, failing in the case of
	/// underflow.
	fn decrease_frozen(id: &Self::Id, who: &AccountId, amount: Self::Balance) -> DispatchResult {
		let a = Self::balance_frozen(id, who)
			.checked_sub(&amount)
			.ok_or(ArithmeticError::Underflow)?;
		Self::set_freeze(id, who, a)
	}

	/// Increase the amount which is being frozen for a particular freeze, failing in the case that
	/// too little balance is available for being frozen.
	fn increase_frozen(id: &Self::Id, who: &AccountId, amount: Self::Balance) -> DispatchResult {
		let a = Self::balance_frozen(id, who)
			.checked_add(&amount)
			.ok_or(ArithmeticError::Overflow)?;
		Self::set_frozen(id, who, a, Fortitude::Polite)
	}
}
