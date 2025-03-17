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
// limitations under the License.use crate::AccountVote;

//! Traits for conviction voting.

use crate::AccountVote;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::dispatch::DispatchResult;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;


/// Represents the differents states of a referendum.
/// None: The referendum is not started.
/// Ongoing: The referendum is ongoing.
/// Completed: The referendum is finished.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Copy,
	Clone,
	Eq,
	PartialEq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum Status {
	None,
	Ongoing,
	Completed,
}
pub trait VotingHooks<AccountId, Index, Balance> {
	// Called when vote is executed.
	fn on_vote(who: &AccountId, ref_index: Index, vote: AccountVote<Balance>) -> DispatchResult;

	// Called when removed vote is executed.
	fn on_remove_vote(who: &AccountId, ref_index: Index, status: Status);

	// Called when a vote is unsuccessful.
	// Returns the amount of locked balance, which is `None` in the default implementation.
	fn lock_balance_on_unsuccessful_vote(who: &AccountId, ref_index: Index) -> Option<Balance>;

	/// Will be called by benchmarking before calling `on_vote` in a benchmark.
	///
	/// Should setup the state in such a way that when `on_vote` is called it will
	/// take the worst case path performance wise.
	#[cfg(feature = "runtime-benchmarks")]
	fn on_vote_worst_case(who: &AccountId);

	/// Will be called by benchmarking before calling `on_remove_vote` in a benchmark.  
    ///  
    /// Should setup the state in such a way that when `on_remove_vote` is called it will  
    /// take the worst case path performance wise.  
	#[cfg(feature = "runtime-benchmarks")]
	fn on_remove_vote_worst_case(who: &AccountId);
}

// Default implementation for VotingHooks
impl<A, I, B> VotingHooks<A, I, B> for () {
	fn on_vote(_who: &A, _ref_index: I, _vote: AccountVote<B>) -> DispatchResult {
		Ok(())
	}

	fn on_remove_vote(_who: &A, _ref_index: I, _status: Status) {}

	fn lock_balance_on_unsuccessful_vote(_who: &A, _ref_index: I) -> Option<B> {
		None
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn on_vote_worst_case(_who: &A) {}

	#[cfg(feature = "runtime-benchmarks")]
	fn on_remove_vote_worst_case(_who: &A) {}
}
