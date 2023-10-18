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
use sp_runtime::{DispatchError, DispatchResult, Saturating};
use sp_std::{ops::Sub};

pub struct StakeableBalance<B: Copy> {
	pub free: B,
	// balance that is delegated to this account but not staked.
	pub delegated_unstaked: B,
}

impl<B: Saturating + Copy> StakeableBalance<B> {
	pub fn stakeable_balance(&self) -> B {
		self.free.saturating_add(self.delegated_unstaked)
	}
}

pub struct StakeResult<B: Copy> {
	pub free: B,
	// balance that is delegated to this account but not staked.
	pub delegated_unstaked: B,
}

#[derive(Clone, Debug)]
pub enum StakeType {
	// staked from free balance
	Direct,
	// staked from delegated balance
	Delegated,
}

/// A generic representation of a delegation based staking apis that other runtime pallets can use.
///
/// Compared to StakingInterface that allows an account to be a direct nominator,
/// `DelegateStakingInterface` allows an account (called delegator) to delegate its stake to another
/// account (delegatee). In delegation based staking, the funds are locked in the delegator's
/// account and gives the delegatee the right to use the funds for staking as if it is a direct
/// nominator.
pub trait DelegatedStakeInterface {
	/// AccountId type used by the runtime.
	type AccountId: Clone + sp_std::fmt::Debug;

	/// Balance type used by the runtime.
	type Balance: Sub<Output = Self::Balance>
		+ Ord
		+ PartialEq
		+ Default
		+ Copy
		+ MaxEncodedLen
		+ FullCodec
		+ TypeInfo
		+ Saturating;

	/// Delegate some funds to a new staker.
	///
	/// Similar to [`StakingInterface::bond`].
	fn delegated_bond_new(
		delegator: Self::AccountId,
		delegatee: Self::AccountId,
		value: Self::Balance,
		payee: Self::AccountId,
	) -> DispatchResult;

	/// Delegate some funds or add to an existing staker.
	///
	/// Similar to [`StakingInterface::bond_extra`].
	fn delegated_bond_extra(
		delegator: Self::AccountId,
		delegatee: Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult;

	/// Migrate a direct stake to a delegation based stake.
	///
	/// Takes a new delegatee account as input. The required funds are moved from the delegatee
	/// account (who is an active staker) to the delegator account and restaked.
	///
	/// This is useful to move active funds in a non-delegation based pool account and migrate it
	/// into a delegation based staking.
	fn delegated_bond_migrate(
		delegator: Self::AccountId,
		delegatee: Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult;

	/// Unbond some funds from a delegator.
	///
	/// Similar to [`StakingInterface::unbond`].
	fn unbond(delegatee: Self::AccountId, value: Self::Balance) -> DispatchResult;

	/// Remove delegation of some or all funds available for unlock at the current era.
	///
	/// Returns whether the stash was killed because of this withdraw or not.
	///
	/// Similar to [`StakingInterface::withdraw_unbonded`].
	fn withdraw_unbonded(
		delegatee: Self::AccountId,
		delegator: Self::AccountId,
		value: Self::Balance,
	) -> Result<bool, DispatchError>;
}
