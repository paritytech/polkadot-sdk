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

//! # Verifier sub-pallet
//!
//! This pallet implements the NPoS solution verification logic. It supports both synchronous and
//! asynchronous verification of paged solutions. Moreover, it manages and ultimately stores
//! the best correct solution in a round, which can be requested by the election provider at the
//! time of the election.
//!
//! The paged solution data to be verified async is retrieved through the
//! [`Config::SolutionDataProvider`] implementor which most likely is the signed pallet.
//!
//! ## Feasibility check
//!
//! The goal of the feasibility of a solution is to ensure that a provided
//! [`crate::Config::Solution`] is correct based on the voter and target snapshots of a given round
//! kept by the parent pallet. The correctness of a solution is defined as:
//!
//! - The edges of a solution (voter -> targets) match the expected by the current snapshot. This
//! check can be performed at the page level.
//! - The total number of winners in the solution is sufficient. This check can only be performed
//!   when the full paged solution is available;;
//! - The election score is higher than the expected minimum score. This check can only be performed
//!   when the full paged solution is available;;
//! - All of the bounds of the election are respected, namely:
//!  * [`Verifier::MaxBackersPerWinner`] - which set the total max of voters are backing a target,
//!  per election. This check can only be performed when the full paged solution is available;
//!  * [`Verifier::MaxWinnersPerPage`] - which ensure that a paged solution has a bound on the
//!  number of targets. This check can be performed at the page level.
//!
//! Some checks can be performed at the page level (e.g. correct edges check) while others can only
//! be performed when the full solution is available.
//!
//! ## Sync and Async verification modes
//!
//! 1. Single-page, synchronous verification. This mode is used when a single page needs to be
//!    verified on the fly, e.g. unsigned submission.
//! 2. Multi-page, asynchronous verification. This mode is used in the context of the multi-paged
//!    signed solutions.
//!
//! The [`crate::verifier::Verifier`] and [`crate::verifier::AsyncVerifier`] traits define the
//! interface of each of the verification modes and this pallet implements both traits.
//!
//! ## Queued solution
//!
//! Once a solution has been succefully verified, it is stored in a queue. This pallet implements
//! the [`SolutionDataProvider`] trait which allows the parent pallet to request a correct
//! solution for the current round.

mod impls;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(test)]
mod tests;

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::traits::Get;
use sp_npos_elections::{ElectionScore, ExtendedBalance};
use sp_runtime::RuntimeDebug;

// public re-exports.
pub use impls::pallet::{
	Call, Config, Event, Pallet, __substrate_call_check, __substrate_event_check, tt_default_parts,
	tt_default_parts_v2, tt_error_token,
};

use crate::{MinerSupportsOf, PageIndex, SupportsOf};

/// Errors related to the solution feasibility checks.
#[derive(Debug, Eq, PartialEq, codec::Encode, codec::Decode, scale_info::TypeInfo, Clone)]
pub enum FeasibilityError {
	/// Election score is too low to be accepted.
	ScoreTooLow,
	/// Ongoing verification was not completed.
	Incomplete,
	/// Solution exceeds the number of backers per winner for at least one winner.
	TooManyBackings,
	/// Solution exceeds the number of winners.
	WrongWinnerCount,
	/// Snapshot is not available.
	SnapshotUnavailable,
	/// A voter is invalid.
	InvalidVoter,
	/// A vote is invalid.
	InvalidVote,
	/// Solution with an invalid score.
	InvalidScore,
	/// Internal election error.
	#[codec(skip)]
	NposElection(sp_npos_elections::Error),
}

impl From<sp_npos_elections::Error> for FeasibilityError {
	fn from(err: sp_npos_elections::Error) -> Self {
		FeasibilityError::NposElection(err)
	}
}

/// The status of this pallet.
///
/// This pallet is either processing an async verification or doing nothing. A single page
/// verification can only be done while the pallet has status [`Status::Nothing`].
#[derive(Encode, Decode, scale_info::TypeInfo, Clone, Copy, MaxEncodedLen, RuntimeDebug)]
pub enum Status {
	/// A paged solution is ongoing and the next page to be verified is indicated in the inner
	/// value.
	Ongoing(PageIndex),
	/// Nothing is happening.
	Nothing,
}

impl Default for Status {
	fn default() -> Self {
		Status::Nothing
	}
}

/// Pointer to the current valid solution of `QueuedSolution`.
#[derive(Encode, Decode, scale_info::TypeInfo, Clone, Copy, MaxEncodedLen, Debug, PartialEq)]
pub enum SolutionPointer {
	X,
	Y,
}

impl Default for SolutionPointer {
	fn default() -> Self {
		SolutionPointer::X
	}
}

impl SolutionPointer {
	pub fn other(&self) -> SolutionPointer {
		match *self {
			SolutionPointer::X => SolutionPointer::Y,
			SolutionPointer::Y => SolutionPointer::X,
		}
	}
}

/// A type that represents a *partial* backing of a winner. It does not contain the
/// supports normally associated with a list of backings.
#[derive(Debug, Default, Encode, Decode, MaxEncodedLen, scale_info::TypeInfo)]
pub struct PartialBackings {
	/// Total backing of a particular winner.
	total: ExtendedBalance,
	/// Number of backers.
	backers: u32,
}

impl sp_npos_elections::Backings for PartialBackings {
	fn total(&self) -> ExtendedBalance {
		self.total
	}
}

/// The interface of something that can verify solutions for election in a multi-block context.
pub trait Verifier {
	/// The account ID type.
	type AccountId;

	/// The solution type;
	type Solution;

	/// Maximum number of winners that a page supports.
	///
	/// Note: This must always be greater or equal to `T::DataProvider::desired_targets()`.
	type MaxWinnersPerPage: Get<u32>;

	/// Maximum number of backers that each winner can have.
	type MaxBackersPerWinner: Get<u32>;

	/// Sets the minimum score that an election must have from now on.
	fn set_minimum_score(score: ElectionScore);

	/// Fetches the current queued election score, if any.
	///
	/// Returns `None` if not score is queued.
	fn queued_score() -> Option<ElectionScore>;

	/// Check if a claimed score improves the current queued score.
	fn ensure_score_improves(claimed_score: ElectionScore) -> bool;

	/// Returns the next missing solution page.
	fn next_missing_solution_page() -> Option<PageIndex>;

	/// Clears all the storage items related to the verifier pallet.
	fn kill();

	/// Get a single page of the best verified solutions, if any.
	fn get_queued_solution(page_index: PageIndex) -> Option<SupportsOf<Self>>;

	/// Perform the feasibility check on a given single-page solution.
	///
	/// This will perform:
	/// 1. feasibility-check
	/// 2. claimed score is correct and it is an improvements
	/// 3. check if bounds are correct
	/// 4. store the solution if all checks pass
	fn verify_synchronous(
		partial_solution: Self::Solution,
		claimed_score: ElectionScore,
		page: PageIndex,
	) -> Result<SupportsOf<Self>, FeasibilityError>;

	/// Just perform a single-page feasibility-check, based on the standards of this pallet.
	///
	/// No score check is part of this.
	fn feasibility_check(
		partial_solution: Self::Solution,
		page: PageIndex,
	) -> Result<SupportsOf<Self>, FeasibilityError>;
}

/// Something that can verify a solution asynchronously.
pub trait AsyncVerifier: Verifier {
	/// The data provider that can provide the candidate solution to verify. The result of the
	/// verification is returned back to this entity.
	type SolutionDataProvider: SolutionDataProvider;

	/// Forces finalizing the async verification.
	fn force_finalize_verification(claimed_score: ElectionScore) -> Result<(), FeasibilityError>;

	/// Returns the status of the current verification.
	fn status() -> Status;

	/// Start a verification process.
	fn start() -> Result<(), &'static str>; // new error type?

	/// Stop the verification.
	///
	/// An implementation must ensure that all related state and storage items are cleaned.
	fn stop();

	/// Sets current status. Only used for benchmarks and tests.
	#[cfg(any(test, feature = "runtime-benchmarks"))]
	fn set_status(status: Status);
}

/// Encapsulates the result of the verification of a candidate solution.
#[derive(Clone, Copy, RuntimeDebug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub enum VerificationResult {
	/// Solution is valid and is queued.
	Queued,
	/// Solution is rejected, for whichever of the multiple reasons that it could be.
	Rejected,
	/// The data needed (solution pages or the score) was unavailable. This should rarely happen.
	DataUnavailable,
}

/// Something that provides paged solution data for the verifier.
///
/// This can be implemented by [`crate::signed::Pallet`] where signed solutions are queued and
/// sorted based on the solution's score.
pub trait SolutionDataProvider {
	// The solution type.
	type Solution;

	/// Returns the `page`th page of the current best solution that the data provider has in store,
	/// if it exists. Otherwise it returns `None`.
	fn get_paged_solution(page: PageIndex) -> Option<Self::Solution>;

	/// Get the claimed score of the current best solution.
	fn get_score() -> Option<ElectionScore>;

	/// Hook to report back the results of the verification of the current candidate solution that
	/// is being exposed via [`Self::get_paged_solution`] and [`Self::get_score`].
	fn report_result(result: VerificationResult);
}
