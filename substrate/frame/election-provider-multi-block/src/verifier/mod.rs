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

//! # The Verifier Pallet
//!
//! This pallet has no end-user functionality, and is only used internally by other pallets in the
//! EPMB machinery to verify solutions.
//!
//! ### *Feasibility* Check
//!
//! Before explaining the pallet itself, it should be explained what a *verification* even means.
//! Verification of a solution page ([`crate::unsigned::miner::MinerConfig::Solution`]) includes the
//! process of checking all of its edges against a snapshot to be correct. For instance, all voters
//! that are presented in a solution page must have actually voted for the winner that they are
//! backing, based on the snapshot kept in the parent pallet.
//!
//! Such checks are bound to each page of the solution, and happen per-page. After checking all of
//! the edges in each page, a handful of other checks are performed. These checks cannot happen
//! per-page, and in order to do them we need to have the entire solution checked and verified.
//!
//! 1. Check that the total number of winners is sufficient (`DesiredTargets`).
//! 2. Check that the claimed score ([`sp_npos_elections::ElectionScore`]) is correct,
//!   * and more than the minimum score that can be specified via [`Verifier::set_minimum_score`].
//! 3. Check that all of the bounds of the solution are respected, namely
//!    [`Verifier::MaxBackersPerWinner`], [`Verifier::MaxWinnersPerPage`] and
//!    [`Verifier::MaxBackersPerWinnerFinal`].
//!
//! Note that the common factor of all of the above checks is that they can ONLY be checked after
//! all pages are already verified. So, in the case of a multi-page verification, these checks are
//! performed at the last page.
//!
//! The errors that can arise while performing the feasibility check are encapsulated in
//! [`verifier::FeasibilityError`].
//!
//! ## Modes of Verification
//!
//! The verifier pallet provide two modes of functionality:
//!
//! 1. Single or multi-page, synchronous verification. This is useful in the context of single-page,
//!    emergency, or unsigned solutions that need to be verified on the fly. This is similar to how
//!    the old school `multi-phase` pallet works. See [`Verifier::verify_synchronous`] and
//!    [`Verifier::verify_synchronous_multi`].
//! 2. Multi-page, asynchronous verification. This is useful in the context of multi-page, signed
//!    solutions. See [`verifier::AsynchronousVerifier`] and [`verifier::SolutionDataProvider`].
//!
//! Both of this, plus some helper functions, is exposed via the [`verifier::Verifier`] trait.
//!
//! ## Queued Solution
//!
//! once a solution has been verified, it is called a *queued solution*. It is sitting in a queue,
//! waiting for either of:
//!
//! 1. being challenged and potentially replaced by better solution, if any.
//! 2. being exported as the final outcome of the election.

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
mod impls;
#[cfg(test)]
mod tests;

// internal imports
pub use crate::weights::measured::pallet_election_provider_multi_block_verifier::*;

use frame_election_provider_support::PageIndex;
use impls::SupportsOfVerifier;
pub use impls::{feasibility_check_page_inner_with_snapshot, pallet::*, Status};
use sp_core::Get;
use sp_npos_elections::ElectionScore;
use sp_std::{fmt::Debug, prelude::*};

/// Errors that can happen in the feasibility check.
#[derive(
	Debug,
	Eq,
	PartialEq,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	scale_info::TypeInfo,
	Clone,
)]
pub enum FeasibilityError {
	/// Wrong number of winners presented.
	WrongWinnerCount,
	/// The snapshot is not available.
	///
	/// Kinda defensive: The pallet should technically never attempt to do a feasibility check
	/// when no snapshot is present.
	SnapshotUnavailable,
	/// A vote is invalid.
	InvalidVote,
	/// A voter is invalid.
	InvalidVoter,
	/// A winner is invalid.
	InvalidWinner,
	/// The given score was invalid.
	InvalidScore,
	/// The provided round is incorrect.
	InvalidRound,
	/// Solution does not have a good enough score.
	ScoreTooLow,
	/// The support type failed to be bounded.
	///
	/// Relates to [`Config::MaxWinnersPerPage`], [`Config::MaxBackersPerWinner`] or
	/// `MaxBackersPerWinnerFinal`
	FailedToBoundSupport,
	/// Internal error from the election crate.
	NposElection(sp_npos_elections::Error),
	/// The solution is incomplete, it has too few pages.
	///
	/// This is (somewhat) synonym to `WrongPageCount` in other places.
	Incomplete,
}

impl From<sp_npos_elections::Error> for FeasibilityError {
	fn from(e: sp_npos_elections::Error) -> Self {
		FeasibilityError::NposElection(e)
	}
}

/// The interface of something that can verify solutions for other sub-pallets in the multi-block
/// election pallet-network.
pub trait Verifier {
	/// The solution type.
	type Solution;
	/// The account if type.
	type AccountId;

	/// Maximum number of winners that can be represented in each page.
	///
	/// A reasonable value for this should be the maximum number of winners that the election user
	/// (e.g. the staking pallet) could ever desire.
	type MaxWinnersPerPage: Get<u32>;
	/// Maximum number of backers, per winner, among all pages of an election.
	///
	/// This can only be checked at the very final step of verification.
	type MaxBackersPerWinnerFinal: Get<u32>;
	/// Maximum number of backers that each winner could have, per page.
	type MaxBackersPerWinner: Get<u32>;

	/// Set the minimum score that is acceptable for any solution.
	///
	/// Henceforth, all solutions must have at least this degree of quality, single-page or
	/// multi-page.
	fn set_minimum_score(score: ElectionScore);

	/// The score of the current best solution. `None` if there is none.
	fn queued_score() -> Option<ElectionScore>;

	/// Check if the claimed score is sufficient to challenge the current queued solution, if any.
	fn ensure_claimed_score_improves(claimed_score: ElectionScore) -> bool;

	/// Clear all storage items, there's nothing else to do until further notice.
	fn kill();

	/// Get a single page of the best verified solution, if any.
	///
	/// It is the responsibility of the call site to call this function with all appropriate
	/// `page` arguments.
	fn get_queued_solution_page(page: PageIndex) -> Option<SupportsOfVerifier<Self>>;

	/// Perform the feasibility check on the given single-page solution.
	///
	/// This will perform:
	///
	/// 1. feasibility-check
	/// 2. claimed score is correct and an improvement.
	/// 3. bounds are respected
	///
	/// Corresponding snapshot (represented by `page`) is assumed to be available.
	///
	/// If all checks pass, the solution is also queued.
	fn verify_synchronous(
		partial_solution: Self::Solution,
		claimed_score: ElectionScore,
		page: PageIndex,
	) -> Result<(), FeasibilityError> {
		Self::verify_synchronous_multi(vec![partial_solution], vec![page], claimed_score)
	}

	/// Perform synchronous feasibility check on the given multi-page solution.
	///
	/// Same semantics as [`Self::verify_synchronous`], but for multi-page solutions.
	fn verify_synchronous_multi(
		partial_solution: Vec<Self::Solution>,
		pages: Vec<PageIndex>,
		claimed_score: ElectionScore,
	) -> Result<(), FeasibilityError>;

	/// Force set a single page solution as the valid one.
	///
	/// Will erase any previous solution. Should only be used in case of emergency fallbacks,
	/// trusted governance solutions and so on.
	fn force_set_single_page_valid(
		partial_supports: SupportsOfVerifier<Self>,
		page: PageIndex,
		score: ElectionScore,
	);
}

/// Simple enum to encapsulate the result of the verification of a candidate solution.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub enum VerificationResult {
	/// Solution is valid and is queued.
	Queued,
	/// Solution is rejected, for whichever of the multiple reasons that it could be.
	Rejected,
	/// The data needed (solution pages or the score) was unavailable. This should rarely happen.
	DataUnavailable,
}

/// Something that can provide candidate solutions to the verifier.
///
/// In reality, this can be implemented by the [`crate::signed::Pallet`], where signed solutions are
/// queued and sorted based on claimed score, and they are put forth one by one, from best to worse.
pub trait SolutionDataProvider {
	/// The opaque solution type.
	type Solution;

	/// Return the `page`th page of the current best solution that the data provider has in store.
	///
	/// If no candidate solutions are available, then None is returned.
	fn get_page(page: PageIndex) -> Option<Self::Solution>;

	/// Get the claimed score of the current best solution.
	fn get_score() -> Option<ElectionScore>;

	/// Hook to report back the results of the verification of the current candidate solution that
	/// is being exposed via [`Self::get_page`] and [`Self::get_score`].
	///
	/// Every time that this is called, the verifier [`AsynchronousVerifier`] goes back to the
	/// [`Status::Nothing`] state, and it is the responsibility of [`Self`] to call `start` again,
	/// if desired.
	fn report_result(result: VerificationResult);
}

/// Something that can do the verification asynchronously.
pub trait AsynchronousVerifier: Verifier {
	/// The data provider that can provide the candidate solution, and to whom we report back the
	/// results.
	type SolutionDataProvider: SolutionDataProvider;

	/// Get the current stage of the verification process.
	fn status() -> Status;

	/// Start a verification process.
	///
	/// Returns `Ok(())` if verification started successfully, and `Err(..)` if a verification is
	/// already ongoing and therefore a new one cannot be started.
	///
	/// From the coming block onwards, the verifier will start and fetch the relevant information
	/// and solution pages from [`SolutionDataProvider`]. It is expected that the
	/// [`SolutionDataProvider`] is ready before calling [`Self::start`].
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
	fn start() -> Result<(), &'static str>;

	/// Stop the verification.
	///
	/// This is a force-stop operation, and should only be used in extreme cases where the
	/// [`SolutionDataProvider`] wants to suddenly bail-out.
	///
	/// An implementation should make sure that no loose ends remain state-wise, and everything is
	/// cleaned.
	fn stop();
}
