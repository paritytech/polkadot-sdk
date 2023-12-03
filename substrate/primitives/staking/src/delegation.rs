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

use codec::{FullCodec, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::{DispatchResult, Saturating};
use sp_std::ops::Sub;

/// Allows an account to accept stake delegations and manage its operations.
pub trait Delegatee {
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

	/// Total delegated balance to this account.
	fn delegate_balance(who: Self::AccountId) -> Self::Balance;

	/// Set intention to accept delegations.
	fn accept_delegations(
		delegatee: &Self::AccountId,
		reward_destination: &Self::AccountId,
	) -> DispatchResult;

	/// Migrate an nominator account into a delegatee.
	///
	/// # Arguments
	///
	/// * `new_delegatee`: This is the current nominator account. Funds will be moved from this
	///   account to `proxy_delegator` and delegated back to `new_delegatee`.
	/// * `proxy_delegator`: All existing staked funds will be moved to this account. Future
	///   migration of funds from `proxy_delegator` to `delegator` is possible via calling
	///   [`Self::migrate_delegator`].
	///  * `payee`: Delegatees need to set where they want their rewards to be paid out.
	///
	/// This is similar to [`Self::accept_delegations`] but allows a current nominator to migrate to
	/// a delegatee.
	fn migrate_accept_delegations(
		new_delegatee: &Self::AccountId,
		proxy_delegator: &Self::AccountId,
		payee: &Self::AccountId,
	) -> DispatchResult;

	/// Stop accepting new delegations on this account.
	fn block_delegations(delegatee: &Self::AccountId) -> DispatchResult;

	/// Remove oneself as Delegatee.
	///
	/// This will only succeed if all delegations to this delegatee are withdrawn.
	fn kill_delegatee(delegatee: &Self::AccountId) -> DispatchResult;

	/// Update bond whenever there is a new delegate funds that are not staked.
	fn update_bond(delegatee: &Self::AccountId) -> DispatchResult;

	/// Request removal of delegated stake.
	fn withdraw(
		delegatee: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult;

	/// Applies a pending slash on delegatee by passing a delegator account who should be slashed
	/// and the value to be slashed. Optionally also takes a reporter account who will be rewarded
	/// from part of the slash imbalance.
	fn apply_slash(
		delegatee: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
		reporter: Option<Self::AccountId>,
	) -> DispatchResult;

	/// Swap a delegated `value` from `delegator_from` to `delegator_to`, with delegatee remaining
	/// the same.
	///
	/// This is useful for migrating old pool accounts using direct staking to lazily move
	/// delegators to the new delegated pool account.
	///
	/// FIXME(ank4n): delegator_from should be removed and be always `proxy_delegator` that was
	/// registered while calling [`Self::migrate_accept_delegations`].
	fn migrate_delegator(
		delegatee: &Self::AccountId,
		delegator_from: &Self::AccountId,
		delegator_to: &Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult;
}

/// Allows an account to delegate their stakes to a delegatee.
pub trait Delegator {
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

	/// Delegate some funds to a Delegatee
	fn delegate(
		delegator: &Self::AccountId,
		delegatee: &Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult;

	/// Request removal of delegated stake.
	fn request_undelegate(
		delegator: &Self::AccountId,
		delegatee: &Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult;
}
