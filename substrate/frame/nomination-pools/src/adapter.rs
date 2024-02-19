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
use frame_support::traits::tokens::Balance;

/// Staking adapter trait that can support multiple modes of staking: i.e. Delegated or Direct.
pub trait StakingAdapter {
	type Balance: Balance;
	type AccountId: Clone + Debug;

	/// Similar to [Inspect::balance].
	fn balance(who: &Self::AccountId) -> Self::Balance;

	/// Similar to [Inspect::total_balance].
	fn total_balance(who: &Self::AccountId) -> Self::Balance;

	/// Start delegation to the pool account.
	///
	/// Similar to [Mutate::transfer] for Direct Stake.
	fn delegate(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult;

	/// Add more delegation to the pool account.
	///
	/// Similar to [Mutate::transfer] for Direct Stake.
	///
	/// We need this along with [Self::delegate] as NominationPool has a slight different behaviour
	/// for the first delegation and the subsequent ones.
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

/// Basic pool adapter that only supports Direct Staking.
///
/// When delegating, tokens are moved between the delegator and pool account as opposed to holding
/// tokens in delegator's accounts.
pub struct NoDelegation<T: Config>(PhantomData<T>);

impl<T: Config> StakingAdapter for NoDelegation<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	fn balance(who: &Self::AccountId) -> Self::Balance {
		T::Currency::balance(who)
	}

	fn total_balance(who: &Self::AccountId) -> Self::Balance {
		T::Currency::total_balance(who)
	}

	fn delegate(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		T::Currency::transfer(who, &pool_account, amount, Preservation::Expendable)?;

		Ok(())
	}

	fn delegate_extra(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		T::Currency::transfer(who, &pool_account, amount, Preservation::Preserve)?;

		Ok(())
	}

	fn release_delegation(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		T::Currency::transfer(&pool_account, &who, amount, Preservation::Expendable)?;

		Ok(())
	}
}

impl<T: Config> StakingAdapter for Pallet<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	fn balance(who: &Self::AccountId) -> Self::Balance {
		todo!()
	}

	fn total_balance(who: &Self::AccountId) -> Self::Balance {
		todo!()
	}

	fn delegate(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		todo!()
	}

	fn delegate_extra(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		todo!()
	}

	fn release_delegation(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		todo!()
	}
}
