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

use crate::StakingInterface;
use codec::{FullCodec, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::{DispatchResult, Saturating};
use sp_std::ops::Sub;

/// A trait that can be used as a plugin to support delegation based accounts, called `Delegatee`.
///
/// For example, `pallet-staking` which implements `StakingInterface` but does not implement
/// account delegations out of the box can be provided with a custom implementation of this trait to
/// learn how to handle these special accounts.
pub trait DelegateeSupport {
	/// Balance type used by the staking system.
	type Balance: Sub<Output = Self::Balance>
		+ Ord
		+ PartialEq
		+ Default
		+ Copy
		+ MaxEncodedLen
		+ FullCodec
		+ TypeInfo
		+ Saturating;

	/// AccountId type used by the staking system.
	type AccountId: Clone + sp_std::fmt::Debug;

	/// Balance of `delegatee` which is available for stake.
	fn stakeable_balance(delegatee: &Self::AccountId) -> Self::Balance;

	/// Returns true if `delegatee` is restricted to update which account they can receive their
	/// staking rewards.
	fn restrict_reward_destination(
		_who: &Self::AccountId,
		_reward_destination: Option<Self::AccountId>,
	) -> bool {
		// never restrict by default
		false
	}

	/// Returns true if `who` is a `delegatee` and accepts delegations from other accounts.
	fn is_delegatee(who: &Self::AccountId) -> bool;

	/// Reports an ongoing slash to the `delegatee` account that would be applied lazily.
	///
	/// Slashing a delegatee account is not immediate since the balance is made up of multiple child
	/// delegators. This function should bookkeep the slash to be applied later.
	fn report_slash(who: &Self::AccountId, slash: Self::Balance);
}

/// Trait that extends on [`StakingInterface`] to provide additional capability to delegate funds to
/// an account.
pub trait DelegatedStakeInterface: StakingInterface {
	/// Returns true if who is a `delegatee` account.
	fn is_delegatee(who: &Self::AccountId) -> bool;

	/// Effective balance of the `delegatee` account.
	fn delegatee_balance(who: &Self::AccountId) -> Self::Balance;

	/// Delegate funds to `delegatee`.
	fn delegate(
		delegator: &Self::AccountId,
		delegatee: &Self::AccountId,
		reward_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;

	/// Add more delegation to the `delegatee`.
	fn delegate_extra(
		delegator: &Self::AccountId,
		delegatee: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;

	/// Withdraw or revoke delegation to `delegatee`.
	fn withdraw_delegation(
		who: &Self::AccountId,
		delegatee: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;

	/// Returns true if there are pending slashes posted to the `delegatee` account.
	fn has_pending_slash(delegatee: &Self::AccountId) -> bool;

	/// Apply a pending slash to a `delegatee` by slashing `value` from `delegator`.
	///
	/// If a reporter is provided, the reporter will receive a fraction of the slash as reward.
	fn delegator_slash(
		delegatee: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
		maybe_reporter: Option<Self::AccountId>,
	) -> sp_runtime::DispatchResult;

	/// Returns the total amount of funds delegated by a `delegator`.
	fn delegated_balance(delegator: &Self::AccountId) -> Self::Balance;
}
