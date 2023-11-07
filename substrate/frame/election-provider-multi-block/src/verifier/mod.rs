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

mod impls;
#[test]
mod tests;

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::traits::Get;
use sp_npos_elections::ElectionScore;
use sp_runtime::RuntimeDebug;

use crate::{Config, PageIndex, SupportsOf};

/// Errors related to the solution feasibility checks.
#[derive(Debug, Eq, PartialEq, codec::Encode, codec::Decode, scale_info::TypeInfo, Clone)]
pub enum FeasibilityError {
	// TODO(gpestana)
	/// Election score is too low to be accepted.
	ScoreTooLow,
}

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

#[derive(Encode, Decode, scale_info::TypeInfo, Clone, Copy, MaxEncodedLen)]
pub enum ValidSolutionPointer {
	X,
	Y,
}

impl Default for ValidSolutionPointer {
	fn default() -> Self {
		ValidSolutionPointer::X
	}
}

impl ValidSolutionPointer {
	pub fn other(&self) -> ValidSolutionPointer {
		match *self {
			ValidSolutionPointer::X => ValidSolutionPointer::Y,
			ValidSolutionPointer::Y => ValidSolutionPointer::X,
		}
	}
}

/// The interface of something that can verify solutions for election in a multi-block context.
pub trait Verifier {
	/// The account ID type.
	type AccountId;

	/// The solution type;
	type Solution;

	/// Maximum number of winners that a page supports.
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
	///
	/// It always returns `true` if there is no score queued.
	fn ensure_score_improves(claimed_score: ElectionScore) -> bool;

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
}

/// Something that can verify a solution asynchronously.
pub trait AsyncVerifier: Verifier {
	/// The data provider that can provide the candidate solution to verify. The result of the
	/// verification is returned back to this entity.
	type SolutionDataProvider: SolutionDataProvider;

	/// Returns the status of the current verification.
	fn status() -> Status;

	/// Start a verification process.
	///
	/// From the coming block onwards, the verifier will start and fetch the relevant information
	/// and solution pages from [`SolutionDataProvider`]. It is expected that the
	/// [`SolutionDataProvider`] is ready before calling [`start`].
	///
	/// Pages of the solution are fetched sequentially and in order from [`SolutionDataProvider`],
	/// from `msp` to `lsp`.
	///
	/// This ends in either of the two:
	///
	/// 1. All pages, including the final checks (like score and other facts that can only be
	///    derived from a full solution) are valid and the solution is verified. The solution is
	///    queued and is ready for further export.
	/// 2. The solution checks verification at one of the steps. Nothing is stored inside the
	///    verifier pallet and all intermediary data is removed.
	///
	/// In both cases, the [`SolutionDataProvider`] is informed via
	/// [`SolutionDataProvider::report_result`]. It is sensible for the data provide to call `start`
	/// again if the verification has failed, and nothing otherwise. Indeed, the
	/// [`SolutionDataProvider`] must adjust its internal state such that it returns a new candidate
	/// solution after each failure.
	fn start() -> Result<(), &'static str>; // new error type?

	/// Stop the verification.
	///
	/// An implementation must ensure that all related state and storage items are cleaned.
	fn stop();
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
	/// is being exposed via [`get_page`] and [`get_score`].
	fn report_result(result: VerificationResult);
}
