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

use crate::*;

/// An adapter trait that can support multiple staking strategies: e.g. `Transfer and stake` or
/// `delegation and stake`.
///
/// This trait is very specific to nomination pool and meant to make switching between different
/// staking implementations easier.
pub trait StakeAdapter {
	type Balance: frame_support::traits::tokens::Balance;
	type AccountId: Clone + sp_std::fmt::Debug;

	/// Balance of the account.
	fn balance(who: &Self::AccountId) -> Self::Balance;

	/// Total balance of the account.
	fn total_balance(who: &Self::AccountId) -> Self::Balance;

	/// Bond delegator via the pool account.
	fn delegator_bond(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		reward_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;

	/// Add more bond for delegator via the pool account.
	fn delegator_bond_extra(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;

	/// Withdrawn amount from pool to delegator.
	fn delegator_withdraw(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;
}

/// An adapter implementation that supports transfer based staking
///
/// The funds are transferred to the pool account and then staked via the pool account.
pub struct TransferStake<T: Config>(PhantomData<T>);

impl<T: Config> StakeAdapter for TransferStake<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	fn balance(who: &Self::AccountId) -> Self::Balance {
		// Note on why we can't use `Currency::reducible_balance`: Since pooled account has a
		// provider (staking pallet), the account can not be set expendable by
		// `pallet-nomination-pool`. This means reducible balance always returns balance preserving
		// ED in the account. What we want though is transferable balance given the account can be
		// dusted.
		T::Currency::balance(who)
	}

	fn total_balance(who: &Self::AccountId) -> Self::Balance {
		T::Currency::total_balance(who)
	}

	fn delegator_bond(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		reward_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		T::Currency::transfer(who, &pool_account, amount, Preservation::Expendable)?;
		T::Staking::bond(pool_account, amount, reward_account)
	}

	fn delegator_bond_extra(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		T::Currency::transfer(who, pool_account, amount, Preservation::Preserve)?;
		T::Staking::bond_extra(pool_account, amount)
	}

	fn delegator_withdraw(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		T::Currency::transfer(&pool_account, &who, amount, Preservation::Expendable)?;

		Ok(())
	}
}

/// An adapter implementation that supports delegation based staking
///
/// The funds are delegated from pool account to a delegatee and then staked. The advantage of this
/// approach is that the funds are held in the delegator account and not in the pool account.
pub struct DelegationStake<T: Config>(PhantomData<T>);

impl<T: Config> StakeAdapter for DelegationStake<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	fn balance(who: &Self::AccountId) -> Self::Balance {
		// Pool account is a delegatee, and its balance is the sum of all member delegations towards
		// it.
		if T::Staking::is_delegatee(who) {
			return T::Staking::delegatee_balance(who);
		}

		T::Currency::balance(who)
	}

	fn total_balance(who: &Self::AccountId) -> Self::Balance {
		if T::Staking::is_delegatee(who) {
			return T::Staking::delegatee_balance(who);
		}

		T::Currency::total_balance(who)
	}

	fn delegator_bond(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		reward_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		// For delegation staking, we just delegate the funds to pool account.
		T::Staking::delegate(who, pool_account, reward_account, amount)
	}

	fn delegator_bond_extra(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		T::Staking::delegate_extra(who, pool_account, amount)
	}

	fn delegator_withdraw(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		T::Staking::withdraw_delegation(who, pool_account, amount)
	}
}
