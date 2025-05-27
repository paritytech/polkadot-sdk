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

use crate::EraIndex;
pub struct LedgerState<Account, Balance: HasCompact, UnlockChunksBound: Get<u32>> {
    /// The account whose balance is held and at stake.
    pub owner: T::AccountId,

    /// The total amount of the stash's balance that is currently held at stake.
    /// It's just `active` plus all the `unlocking` balances.
    #[codec(compact)]
    pub total: BalanceOf<T>,

    /// The total amount of the stash's balance that will be at stake in any forthcoming
    /// rounds.
    #[codec(compact)]
    pub active: BalanceOf<T>,

    /// Any balance that is becoming free, which may eventually be transferred out of the stash
    /// (assuming it doesn't get slashed first). It is assumed that this will be treated as a first
    /// in, first out queue where the new (higher value) eras get pushed on the back.
    pub unlocking: BoundedVec<UnlockChunkState<BalanceOf<T>>, UnlockChunksBound>,
}

pub struct UnlockChunkState<Balance: HasCompact> {
    /// The amount of the balance that is requested to be unlocked.
    #[codec(compact)]
    pub value: Balance,

    /// The era in which the unlocking was requested.
    pub request_era: EraIndex,
}