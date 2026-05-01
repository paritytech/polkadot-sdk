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

//! Contains all the interactions with [`Config::Currency`] to manipulate the underlying staking
//! asset.

use crate::{BalanceOf, Config, HoldReason, NegativeImbalanceOf, PositiveImbalanceOf};
use frame_support::traits::{
	fungible::{
		hold::{Balanced as FunHoldBalanced, Inspect as FunHoldInspect, Mutate as FunHoldMutate},
		Balanced, Inspect as FunInspect,
	},
	tokens::{Fortitude, Precision, Preservation},
};
use sp_runtime::{DispatchResult, Saturating};

/// Existential deposit for the chain.
pub fn existential_deposit<T: Config>() -> BalanceOf<T> {
	T::Currency::minimum_balance()
}

/// Total issuance of the chain.
pub fn total_issuance<T: Config>() -> BalanceOf<T> {
	T::Currency::total_issuance()
}

/// Total balance of `who`. Includes both free and staked.
pub fn total_balance<T: Config>(who: &T::AccountId) -> BalanceOf<T> {
	T::Currency::total_balance(who)
}

/// Stakeable balance of `who`.
///
/// This includes balance free to stake along with any balance that is already staked.
pub fn stakeable_balance<T: Config>(who: &T::AccountId) -> BalanceOf<T> {
	free_to_stake::<T>(who).saturating_add(staked::<T>(who))
}

/// Balance of `who` that is currently at stake.
///
/// The staked amount is on hold and cannot be transferred out of `who`s account.
pub fn staked<T: Config>(who: &T::AccountId) -> BalanceOf<T> {
	T::Currency::balance_on_hold(&HoldReason::Staking.into(), who)
}

/// Balance of who that can be staked additionally.
///
/// Does not include the current stake.
pub fn free_to_stake<T: Config>(who: &T::AccountId) -> BalanceOf<T> {
	// since we want to be able to use frozen funds for staking, we force the reduction.
	T::Currency::reducible_balance(who, Preservation::Preserve, Fortitude::Force)
}

/// Update `amount` at stake for `who`.
///
/// Overwrites the existing stake amount. If passed amount is lower than the existing stake, the
/// difference is unlocked.
pub fn update_stake<T: Config>(who: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
	T::Currency::set_on_hold(&HoldReason::Staking.into(), who, amount)
}

/// Release all staked amount to `who`.
///
/// Fails if there are consumers left on `who` that restricts it from being reaped.
pub fn kill_stake<T: Config>(who: &T::AccountId) -> DispatchResult {
	T::Currency::release_all(&HoldReason::Staking.into(), who, Precision::BestEffort).map(|_| ())
}

/// Slash the value from `who`.
///
/// A negative imbalance is returned which can be resolved to deposit the slashed value.
pub fn slash<T: Config>(
	who: &T::AccountId,
	value: BalanceOf<T>,
) -> (NegativeImbalanceOf<T>, BalanceOf<T>) {
	T::Currency::slash(&HoldReason::Staking.into(), who, value)
}

/// Mint `value` into an existing account `who`.
///
/// This does not increase the total issuance.
pub fn mint_into_existing<T: Config>(
	who: &T::AccountId,
	value: BalanceOf<T>,
) -> Option<PositiveImbalanceOf<T>> {
	// since the account already exists, we mint exact value even if value is below ED.
	T::Currency::deposit(who, value, Precision::Exact).ok()
}

/// Mint `value` and create account for `who` if it does not exist.
///
/// If value is below existential deposit, the account is not created.
///
/// Note: This does not increase the total issuance.
pub fn mint_creating<T: Config>(who: &T::AccountId, value: BalanceOf<T>) -> PositiveImbalanceOf<T> {
	T::Currency::deposit(who, value, Precision::BestEffort).unwrap_or_default()
}

/// Deposit newly issued or slashed `value` into `who`.
pub fn deposit_slashed<T: Config>(who: &T::AccountId, value: NegativeImbalanceOf<T>) {
	let _ = T::Currency::resolve(who, value);
}

/// Issue `value` increasing total issuance.
///
/// Creates a negative imbalance.
pub fn issue<T: Config>(value: BalanceOf<T>) -> NegativeImbalanceOf<T> {
	T::Currency::issue(value)
}

/// Burn the amount from the total issuance.
#[cfg(feature = "runtime-benchmarks")]
pub fn burn<T: Config>(amount: BalanceOf<T>) -> PositiveImbalanceOf<T> {
	T::Currency::rescind(amount)
}

/// Set balance that can be staked for `who`.
///
/// If `Value` is lower than the current staked balance, the difference is unlocked.
///
/// Should only be used with test.
#[cfg(any(test, feature = "runtime-benchmarks"))]
pub fn set_stakeable_balance<T: Config>(who: &T::AccountId, value: BalanceOf<T>) {
	use frame_support::traits::fungible::Mutate;

	// minimum free balance (non-staked) required to keep the account alive.
	let ed = existential_deposit::<T>();
	// currently on stake
	let staked_balance = staked::<T>(who);

	// if new value is greater than staked balance, mint some free balance.
	if value > staked_balance {
		let _ = T::Currency::set_balance(who, value - staked_balance + ed);
	} else {
		// else reduce the staked balance.
		update_stake::<T>(who, value).expect("can remove from what is staked");
		// burn all free, only leaving ED.
		let _ = T::Currency::set_balance(who, ed);
	}

	// ensure new stakeable balance same as desired `value`.
	assert_eq!(stakeable_balance::<T>(who), value);
}

/// Return the amount staked and available to stake in one tuple.
#[cfg(test)]
pub fn staked_and_not<T: Config>(who: &T::AccountId) -> (BalanceOf<T>, BalanceOf<T>) {
	(staked::<T>(who), free_to_stake::<T>(who))
}
