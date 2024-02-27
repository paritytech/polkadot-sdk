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
	fn is_delegatee(who: &Self::AccountId) -> bool;

	/// Reports an ongoing slash to the `delegatee` account that would be applied lazily.
	fn report_slash(who: &Self::AccountId, slash: Self::Balance);
}