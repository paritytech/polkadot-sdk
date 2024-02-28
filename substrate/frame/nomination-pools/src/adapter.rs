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

/// An adapter trait that can support multiple staking strategies.
///
/// Depending on which staking strategy we want to use, the staking logic can be slightly
/// different. Refer the two possible strategies currently: [`TransferStake`] and [`DelegationStake`].
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

/// An adapter implementation that supports transfer based staking.
///
/// In order to stake, this adapter transfers the funds from the delegator account to the pool
/// account and stakes directly on [Config::Staking].
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

/// An adapter implementation that supports delegation based staking.
///
/// In this approach, first the funds are delegated from delegator to the pool account and later
/// staked with [Config::Staking]. The advantage of this approach is that the funds are held in the
/// use account itself and not in the pool account.
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

/// **** New Shiny Adapters **** ///
pub trait StakeStrategy<Staking: StakingInterface> {
	fn balance(who: &Staking::AccountId) -> Staking::Balance;
	fn bond(
		who: &Staking::AccountId,
		pool_account: &Staking::AccountId,
		reward_account: &Staking::AccountId,
		amount: Staking::Balance,
	) -> DispatchResult;
}

pub struct TransferStakeStrategy<T: Config>(PhantomData<T>);

impl<T: Config, Staking: StakingInterface<Balance = BalanceOf<T>, AccountId = T::AccountId>> StakeStrategy<Staking> for TransferStakeStrategy<T> {
	fn balance(who: &T::AccountId) -> BalanceOf<T> {
		T::Currency::balance(who)
	}

	fn bond(
		who: &T::AccountId,
		pool_account: &T::AccountId,
		reward_account: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		T::Currency::transfer(who, &pool_account, amount, Preservation::Expendable)?;
		Staking::bond(pool_account, amount, reward_account)
	}

}