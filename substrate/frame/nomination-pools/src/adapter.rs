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

/// Pool adapter trait that can support multiple modes of staking: i.e. Delegated or Direct.
pub trait StakeAdapter {
	type Balance: frame_support::traits::tokens::Balance;
	type AccountId: Clone + sp_std::fmt::Debug;

	/// Balance that is free and can be released to delegator.
	fn transferable_balance(who: &Self::AccountId) -> Self::Balance;

	/// Total balance of the account held for staking.
	fn total_balance(who: &Self::AccountId) -> Self::Balance;

	/// Initiate delegation to the pool account.
	fn bond(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		reward_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;

	/// Add more delegation to the pool account.
	fn bond_extra(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;

	/// Revoke delegation to pool account.
	fn claim_withdraw(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;
}


/// Basic pool adapter that only supports Direct Staking.
///
/// When delegating, tokens are moved between the delegator and pool account as opposed to holding
/// tokens in delegator's accounts.
pub struct TransferStake<T: Config>(PhantomData<T>);

/// TODO(ankan) Call it FundManager/CurrencyAdapter/DelegationManager
impl<T: Config> StakeAdapter for TransferStake<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	fn transferable_balance(who: &Self::AccountId) -> Self::Balance {
		// Note on why we can't use `Currency::reducible_balance`: Since pooled account has a
		// provider (staking pallet), the account can not be set expendable by
		// `pallet-nomination-pool`. This means reducible balance always returns balance preserving
		// ED in the account. What we want though is transferable balance given the account can be
		// dusted.
		T::Currency::balance(who).saturating_sub(T::Staking::active_stake(who).unwrap_or_default())
	}

	fn total_balance(who: &Self::AccountId) -> Self::Balance {
		T::Currency::total_balance(who)
	}

	fn bond(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		reward_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		T::Currency::transfer(who, &pool_account, amount, Preservation::Expendable)?;
		T::Staking::bond(pool_account, amount, reward_account)
	}

	fn bond_extra(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		T::Currency::transfer(who, pool_account, amount, Preservation::Preserve)?;
		T::Staking::bond_extra(pool_account, amount)
	}

	fn claim_withdraw(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		T::Currency::transfer(&pool_account, &who, amount, Preservation::Expendable)?;

		Ok(())
	}
}

pub struct DelegationStake<T: Config>(PhantomData<T>);
impl<T: Config> StakeAdapter for DelegationStake<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	/// Return balance of the `Delegate` (pool account) that is not bonded.
	///
	/// Equivalent to [FunInspect::balance] for non delegate accounts.
	fn transferable_balance(who: &Self::AccountId) -> Self::Balance {
		T::Staking::delegatee_balance(who).saturating_sub(T::Staking::active_stake(who).unwrap_or_default())
	}

	/// Returns balance of account that is held.
	///
	/// - For `delegate` accounts, this is their total delegation amount.
	/// - For `delegator` accounts, this is their delegation amount.
	fn total_balance(who: &Self::AccountId) -> Self::Balance {
		if T::Staking::is_delegatee(who) {
			return T::Staking::delegatee_balance(who)
		}

		// for delegators we return their held balance as well.
		T::Currency::total_balance(who)
	}

	/// Add initial delegation to the pool account.
	///
	/// Equivalent to [FunMutate::transfer] for Direct Staking.
	fn bond(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		reward_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		// This is the first delegation so we needs to register the pool account as a `delegate`.
		T::Staking::delegate(who, pool_account, reward_account, amount)
	}

	fn bond_extra(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		T::Staking::delegate_extra(who, pool_account, amount)
	}

	fn claim_withdraw(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		// fixme(ank4n): This should not require slashing spans.
		T::Staking::withdraw_delegation(who, pool_account, amount)
	}
}
