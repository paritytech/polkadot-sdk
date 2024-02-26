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
	fn transferable_balance(who: &Self::AccountId) -> Self::Balance;

	/// Total balance of the account held for staking.
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
	fn release_delegation(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;

	/// Returns true if the `delegate` has pending slash to be applied.
	fn has_pending_slash(delegate: &Self::AccountId) -> bool;

	/// Apply a slash to the `delegator`.
	///
	/// This is called when the corresponding `delegate` has pending slash to be applied.
	fn delegator_slash(
		delegate: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
		maybe_reporter: Option<Self::AccountId>,
	) -> DispatchResult;
}
