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
// FIXME(ank4n): Remove this and add a new trait (in delegation pallet) for NP adapter.
pub trait DelegationInterface {
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
	fn delegated_balance(who: &Self::AccountId) -> Self::Balance;

	/// Total delegated balance to this account that is not yet bonded to staking.
	fn unbonded_balance(who: &Self::AccountId) -> Self::Balance;

	/// Set intention to accept delegations.
	fn accept_delegations(
		delegate: &Self::AccountId,
		reward_destination: &Self::AccountId,
	) -> DispatchResult;

	/// Migrate a nominator account into a `delegate`.
	///
	/// # Arguments
	///
	/// * `new_delegate`: This is the current nominator account. Funds will be moved from this
	///   account to `proxy_delegator` and delegated back to `new_delegate`.
	/// * `proxy_delegator`: All existing staked funds will be moved to this account. Future
	///   migration of funds from `proxy_delegator` to `delegator` is possible via calling
	///   [`Self::migrate_delegator`].
	/// * `payee`: `Delegate` needs to set where they want their rewards to be paid out. This can be
	///   anything other than the `delegate` account itself.
	///
	/// This is similar to [`Self::accept_delegations`] but allows a current nominator to migrate to
	/// a `delegate`.
	fn migrate_accept_delegations(
		new_delegate: &Self::AccountId,
		proxy_delegator: &Self::AccountId,
		payee: &Self::AccountId,
	) -> DispatchResult;

	/// Stop accepting new delegations to this account.
	fn block_delegations(delegate: &Self::AccountId) -> DispatchResult;

	/// Unblock delegations to this account.
	fn unblock_delegations(delegate: &Self::AccountId) -> DispatchResult;

	/// Remove oneself as a `delegate`.
	///
	/// This will only succeed if all delegations to this `delegate` are withdrawn.
	fn kill_delegate(delegate: &Self::AccountId) -> DispatchResult;

	/// Bond all fund that is delegated but not staked.
	/// FIXME(ank4n): Should not be allowed as withdrawn funds would get restaked.
	fn bond_all(delegate: &Self::AccountId) -> DispatchResult;

	/// Request withdrawal of unbonded stake of `delegate` belonging to the provided `delegator`.
	///
	/// Important: It is upto `delegate` to enforce which `delegator` can withdraw `value`. The
	/// withdrawn value is released in `delegator`'s account.
	fn delegate_withdraw(
		delegate: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
		num_slashing_spans: u32,
	) -> DispatchResult;

	/// Applies a pending slash on `delegate` by passing a delegator account who should be slashed
	/// and the value to be slashed. Optionally also takes a reporter account who will be rewarded
	/// from part of the slash imbalance.
	fn apply_slash(
		delegate: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
		reporter: Option<Self::AccountId>,
	) -> DispatchResult;

	/// Move a delegated amount from `proxy_delegator` to `new_delegator`.
	///
	/// `Delegate` must have used [`Self::migrate_accept_delegations`] to setup a `proxy_delegator`.
	/// This is useful for migrating old pool accounts using direct staking to lazily move
	/// delegators to the new delegated pool account.
	fn migrate_delegator(
		delegate: &Self::AccountId,
		new_delegator: &Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult;

	/// Delegate some funds to a `delegate` account.
	fn delegate(
		delegator: &Self::AccountId,
		delegate: &Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult;
}

/// Something that provides delegation support to core staking.
pub trait StakingDelegationSupport {
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

	/// Balance of who which is available for stake.
	fn stakeable_balance(who: &Self::AccountId) -> Self::Balance;

	/// Returns true if provided reward destination is not allowed.
	fn restrict_reward_destination(
		_who: &Self::AccountId,
		_reward_destination: Option<Self::AccountId>,
	) -> bool {
		// never restrict by default
		false
	}

	/// Returns true if `who` accepts delegations for stake.
	fn is_delegate(who: &Self::AccountId) -> bool;

	/// Reports an ongoing slash to the `delegate` account that would be applied lazily.
	fn report_slash(who: &Self::AccountId, slash: Self::Balance);
}

/// Pool adapter trait that can support multiple modes of staking: i.e. Delegated or Direct.
pub trait PoolAdapter {
	type Balance: Sub<Output = Self::Balance>
		+ Ord
		+ PartialEq
		+ Default
		+ Copy
		+ MaxEncodedLen
		+ FullCodec
		+ TypeInfo
		+ Saturating;
	type AccountId: Clone + sp_std::fmt::Debug;

	/// Balance that is free and can be released to delegator.
	fn releasable_balance(who: &Self::AccountId) -> Self::Balance;

	/// Similar to [Inspect::total_balance].
	fn total_balance(who: &Self::AccountId) -> Self::Balance;

	/// Initiate delegation to the pool account.
	fn delegate(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		reward_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;

	/// Add more delegation to the pool account.
	fn delegate_extra(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;

	/// Revoke delegation to pool account.
	///
	/// Similar to [Mutate::transfer] for Direct Stake but in reverse direction to [Self::delegate].
	fn release_delegation(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;
}
