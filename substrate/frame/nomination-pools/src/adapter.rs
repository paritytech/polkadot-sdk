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

/// Basic pool adapter that only supports Direct Staking.
///
/// When delegating, tokens are moved between the delegator and pool account as opposed to holding
/// tokens in delegator's accounts.
pub struct NoDelegation<T: Config>(PhantomData<T>);

impl<T: Config> PoolAdapter for NoDelegation<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	fn releasable_balance(who: &Self::AccountId) -> Self::Balance {
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

	fn delegate(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		reward_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		T::Currency::transfer(who, &pool_account, amount, Preservation::Expendable)?;
		T::Staking::bond(pool_account, amount, reward_account)
	}

	fn delegate_extra(
		who: &Self::AccountId,
		pool_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		T::Currency::transfer(who, pool_account, amount, Preservation::Preserve)?;
		T::Staking::bond_extra(pool_account, amount)
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
