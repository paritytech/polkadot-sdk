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

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::traits::Currency;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;

/// Type alias for the balance type from the configuration.
pub type BalanceOf<T, I = ()> = <<T as crate::Config<I>>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

/// Status of a vote.
#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum VoteStatus {
    /// The vote is active.
    Active,
    /// The vote has been cancelled.
    Cancelled,
}

/// A vote for a funding request.
#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct Vote<Balance> {
    /// The amount of the vote.
    pub amount: Balance,
    /// The status of the vote.
    pub status: VoteStatus,
}

/// A funding request.
#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct FundingRequest<AccountId, Balance, BlockNumber, Description> {
    /// The account that proposed the request.
    pub proposer: AccountId,
    /// The amount requested.
    pub amount: Balance,
    /// A description of the request.
    pub description: Description,
    /// The block number when the request was submitted.
    pub submitted_at: BlockNumber,
    /// The block number when the funding period ends.
    pub period_end: BlockNumber,
    /// The number of votes for this request.
    pub votes_count: u32,
    /// The total amount of votes for this request.
    pub votes_amount: Balance,
}
