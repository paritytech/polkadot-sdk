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
	fn accept_delegations(delegatee: &Self::AccountId, reward_destination: &Self::AccountId) -> DispatchResult;

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

	/// Migrate a nominator account into a delegatee by moving its funds to delegator account and
	/// delegating these funds back to delegatee.
	///
	/// Also takes input a payee which will be the new reward destination for the new delegatee.
	///
	/// This is useful for migrating old pool accounts to use delegation by providing a pool
	/// delegator account. This pool delegator account funds can then lazily move funds to actual
	/// delegators using [`Self::delegator_migrate`].
	///
	/// Note: Potentially unsafe and should be only called by trusted runtime code.
	fn delegatee_migrate(
		new_delegatee: &Self::AccountId,
		proxy_delegator: &Self::AccountId,
		payee: &Self::AccountId,
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

	/// Swap a delegated `value` from `delegator_from` to `delegator_to`, with delegatee remaining
	/// the same.
	///
	/// This is useful for migrating old pool accounts using direct staking to lazily move
	/// delegators to the new delegated pool account.
	///
	/// Note: Potentially unsafe and should be only called by trusted runtime code.
	fn delegator_migrate(
		delegator_from: &Self::AccountId,
		delegator_to: &Self::AccountId,
		delegatee: &Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult;
}
