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

/// Pool adapter that supports DirectStake.
pub struct DirectStake<T: Config>(PhantomData<T>);

impl<T: Config> sp_staking::delegation::PoolAdapter for DirectStake<T> {
    type Balance = BalanceOf<T>;
    type AccountId = T::AccountId;

    fn balance(who: &Self::AccountId) -> Self::Balance {
        T::Currency::balance(who)
    }

    fn total_balance(who: &Self::AccountId) -> Self::Balance {
        T::Currency::total_balance(who)
    }

    fn delegate(who: &Self::AccountId, pool_account: &Self::AccountId, amount: Self::Balance) -> DispatchResult {
        T::Currency::transfer(
            who,
            &pool_account,
            amount,
            Preservation::Expendable,
        )?;

        Ok(())
    }

    fn delegate_extra(who: &Self::AccountId, pool_account: &Self::AccountId, amount: Self::Balance) -> DispatchResult {
        T::Currency::transfer(
            who,
            &pool_account,
            amount,
            Preservation::Preserve,
        )?;

        Ok(())
    }

    fn release_delegation(who: &Self::AccountId, pool_account: &Self::AccountId, amount: Self::Balance) -> DispatchResult {
        T::Currency::transfer(
            &pool_account,
            &who,
            amount,
            Preservation::Expendable,
        )?;

        Ok(())
    }
}
