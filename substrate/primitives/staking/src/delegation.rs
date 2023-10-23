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
use sp_runtime::{DispatchError, DispatchResult};

/// A generic representation of a delegation based staking apis that other runtime pallets can use.
///
/// Compared to StakingInterface that allows an account to be a direct nominator,
/// `DelegateStakingInterface` allows an account (called delegator) to delegate its stake to another
/// account (delegatee). In delegation based staking, the funds are locked in the delegator's
/// account and gives the delegatee the right to use the funds for staking as if it is a direct
/// nominator.
pub trait DelegatedStakeInterface: StakingInterface {
	/// Set intention to accept delegations.
	///
	/// The caller would be Delegatee. Also takes input where the reward should be paid out.
	fn accept_delegations(delegatee: &Self::AccountId, payee: &Self::AccountId) -> DispatchResult;

	/// Stop accepting new delegations.
	fn block_delegations(delegatee: &Self::AccountId) -> DispatchResult;

	/// Remove yourself as Delegatee.
	///
	/// This will only succeed if all delegations to this delegatee are removed.
	fn kill_delegatee(delegatee: &Self::AccountId) -> DispatchResult;

	/// Delegate some funds to a Delegatee
	///
	/// Delegated funds are locked in delegator's account and added to Delegatee's stakeable
	/// balance.
	fn delegate(
		delegator: &Self::AccountId,
		delegatee: &Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult;

	fn update_bond(delegatee: &Self::AccountId, value: Self::Balance) -> DispatchResult;

	/// Unbond some funds from a delegatee.
	///
	/// Similar to [`StakingInterface::unbond`].
	fn unbond(delegatee: &Self::AccountId, value: Self::Balance) -> DispatchResult;

	/// Remove delegation of some or all funds available for unlock at the current era.
	///
	/// Returns whether the stash was killed because of this withdraw or not.
	///
	/// Similar to [`StakingInterface::withdraw_unbonded`].
	fn withdraw_unbonded(
		delegatee: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
	) -> Result<bool, DispatchError>;

	/// Applies a pending slash on delegatee by passing a delegator account who should be slashed
	/// and the value to be slashed. Optionally also takes a reporter account who will be rewarded
	/// from part of the slash imbalance.
	fn apply_slash(
		delegatee: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
		reporter: Option<Self::AccountId>,
	) -> DispatchResult;

	/// Migrate a staker account into a delegatee by providing another delegator account where all
	/// bonded funds will be moved and delegated from.
	///
	/// This is useful for migrating old pool accounts to use delegation by providing a pool
	/// delegator account. This pool delegator account funds can then lazily move to actual
	/// delegators using [`Self::delegation_swap`].
	fn migrate(staker: &Self::AccountId, delegator: &Self::AccountId) -> DispatchResult;

	/// Swap a delegated `value` from `delegator_from` to `delegator_to`, with delegatee remaining
	/// the same.
	///
	/// This is useful for migrating old pool accounts using direct staking to lazily move
	/// delegators to the new delegated pool account.
	///
	/// This is useful to move active funds in a non-delegation based pool account and migrate it
	/// into a delegation based staking.
	fn delegation_swap(
		delegatee: &Self::AccountId,
		delegator_from: &Self::AccountId,
		delegator_to: &Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult;
}
