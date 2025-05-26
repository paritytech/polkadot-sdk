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
	/// The referendum is not started.
	None,
	/// The referendum is ongoing.
	Ongoing,
	/// The referendum is finished.
	Completed,
}

/// Trait for voting hooks that are called during various stages of the voting process.
///
/// # Important Note
/// These hooks are called BEFORE the actual vote is recorded in storage. This means:
/// - If `on_vote` returns an error, the entire voting operation will be reverted
/// - If `on_vote` succeeds but the voting operation fails later, any storage modifications made by
///   `on_vote` will still persist
///
/// # Hook Methods
/// - `on_vote`: Called before a vote is recorded. Returns `Err` to prevent the vote from being
///   recorded. Storage modifications made by this hook will persist even if the vote fails later.
///
/// - `on_remove_vote`: Called before a vote is removed. Cannot fail.
/// - `lock_balance_on_unsuccessful_vote`: Called when a vote fails to be recorded (e.g. due to
///   insufficient balance). Returns optionally locked balance amount.
///
/// # Benchmarking Hooks
/// The following methods are only used during runtime benchmarking:
/// - `on_vote_worst_case`: Sets up worst-case state for `on_vote` benchmarking
/// - `on_remove_vote_worst_case`: Sets up worst-case state for `on_remove_vote` benchmarking
///
/// # Generic Parameters
/// - `AccountId`: The type used to identify accounts in the system
/// - `Index`: The type used for referendum indices
/// - `Balance`: The type used for balance values
pub trait VotingHooks<AccountId, Index, Balance> {
	// Called before a vote is recorded.
	// Returns `Err` to prevent the vote from being recorded.
	fn on_before_vote(
		who: &AccountId,
		ref_index: Index,
		vote: AccountVote<Balance>,
	) -> DispatchResult;

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
	fn on_before_vote(_who: &A, _ref_index: I, _vote: AccountVote<B>) -> DispatchResult {
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
