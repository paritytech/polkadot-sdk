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

/// Support plugin for `delegatee` accounts.
///
/// A `delegatee` account is an account that can receive delegations from other accounts. Their
/// balance is made up of multiple child delegators. This trait allows a pallet such as
/// `pallet-staking` to support these special accounts.
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

	/// Balance of `delegatee` which can be staked.
	///
	/// Similar to free balance for a normal account.
	fn stakeable_balance(delegatee: &Self::AccountId) -> Self::Balance;

	/// Returns true if `delegatee` is restricted to update which account they can receive their
	/// staking rewards.
	///
	/// For `delegatee` accounts we restrict the reward destination to be the same as the
	/// `delegatee` account itself. This is since the actual `delegatee` balances is not considered
	/// while staking. Instead, their balance is made up of multiple child delegators.
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
	/// Effective balance of the `delegatee` account.
	///
	/// This takes into account any pending slashes to `Delegatee`.
	fn delegatee_balance(delegatee: &Self::AccountId) -> Self::Balance;

	/// Returns the total amount of funds delegated by a `delegator`.
	fn delegator_balance(delegator: &Self::AccountId) -> Self::Balance;

	/// Delegate funds to `delegatee`.
	///
	/// Only used for the initial delegation. Use [`Self::delegate_extra`] to add more delegation.
	fn delegate(
		delegator: &Self::AccountId,
		delegatee: &Self::AccountId,
		reward_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;

	/// Add more delegation to the `delegatee`.
	///
	/// If this is the first delegation, use [`Self::delegate`] instead.
	fn delegate_extra(
		delegator: &Self::AccountId,
		delegatee: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;

	/// Withdraw or revoke delegation to `delegatee`.
	///
	/// If there are `delegatee` funds upto `amount` available to withdraw, then those funds would
	/// be released to the `delegator`
	fn withdraw_delegation(
		delegator: &Self::AccountId,
		delegatee: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;

	/// Returns true if there are pending slashes posted to the `delegatee` account.
	///
	/// Slashes to `delegatee` account are not immediate and are applied lazily. Since `delegatee`
	/// has an unbounded number of delegators, immediate slashing is not possible.
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
}
