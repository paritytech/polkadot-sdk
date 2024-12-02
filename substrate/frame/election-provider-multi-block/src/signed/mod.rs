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

//! # Signed sub-pallet
//!
//! The main goal of the signed sub-pallet is to manage a solution submissions from list of sorted
//! score commitments and correponding paged solutions during the [`crate::Phase::Signed`] and to
//! implement the [`SolutionDataProvider`] trait which exposes an interface for external entities to
//! fetch data related to signed submissions for the active round.
//!
//! ## Overview
//!
//!	The core logic of this pallet is only active during [`Phase::Signed`]. During the signed phase,
//!	accounts can register a solution for the current round and submit the solution's pages, one per
//!	extrindic call. The main flow is the following:
//!
//! 1. [`Phase::Signed`] is enacted in the parent EPM pallet;
//! 2. Submitters call [`Call::register`] to register a solution with a given claimed score. This
//!    pallet ensures that accepted submission registrations (encapsulated as
//!    [`SubmissionMetadata`]) are kept sorted by claimed score in the [`SubmissionMetadata`]
//!    storage map. This pallet accepts up to [`Config::MaxSubmissions`] active registrations per
//!    round.
//! 3. Submitters that have successfully registered, may submit the solution pages through
//!    [`Call::submit_page`], one page per call.
//! 4. Submitters may bail from a registered solution by calling [`Call::bail`]. Bailing from a
//!    solution registration will result in a partial slash.
//! 5. This pallet implements the trait [`SolutionDataProvider`] which exposes methods for external
//!    entities (e.g. verifier pallet) to query the data and metadata of the current best submitted
//!    solution.
//! 6. Upon solution verification (performed by an external entity e.g. the verifier pallet),
//!    [`SolutionDataProvider::report_result`] can be called to report the verification result of
//!    the current best solution. Depending on the result, the corresponding submitter's deposit may
//!    be fully slashed or the submitter may be rewarded with [`Config::Reward`].
//!
//! Accounts may submit up to [`Config::MaxSubmissions`] score commitments per election round and
//! this pallet ensures that the scores are stored under the map `SortedScores` are sorted and keyed
//! by the correct round number.
//!
//! ## Reporting the verification result
//!
//! When the time to evaluate the signed submission comes, the solutions are checked from best to
//! worse. The [`SolutionDataProvider`] trait exposes the submission data and metadata to an
//! external entity that verifies the queued solutions until it accepts one solution (or none). The
//! verifier entity reports the result of the solution verification which may result in one of three
//! scenarios:
//!
//! 1. If the *best* committed score and page submissions are correct, the submitter is rewarded.
//! 2. Any queued score that was not evaluated, the held deposit is fully returned.
//! 3. Any invalid solution results in a 100% slash of the held deposit.
//!
//! ## Submission deposit
//!
//! Each submitter must hold a "base deposit" per submission that is calculated based on the number
//! of the number of submissions in the queue. In addition, for each solution page submitted there
//! is a fixed [`Config::PageDeposit`] deposit held. The held deposit may be returned or slashed at
//! by the end of the round, depending on the following:
//!
//! 1. If a submission is verified and accepted, the deposit is returned.
//! 2. If a submission is verified and not accepted, the whole deposit is slashed.
//! 3. If a submission is not verified, the deposit is returned.
//! 4. Bailing a registration will return the page deposit and burn the base balance.
//!
//! The deposit is burned when all the data from the submitter is cleared through the
//! [`Call::force_clear_submission`].
//!
//! ## Submission reward
//!
//! Exposing [`SolutionDataProvider::report_result`] allows an external verifier to signal whether
//! the current best solution is correct or not. If the solution is correct, the submitter is
//! rewarded and the pallet can start clearing up the state of the current round.
//!
//! ## Storage management
//!
//! ### Storage mutations
//!
//! The [`Submissions`] wraps all the mutation and getters related to the sorted scores, metadata
//! and submissions storage types. All the mutations to those storage items *MUST* be performed
//! through [`Submissions`] to leverage the mutate checks and ensure the data consistency of the
//! submissions data.
//!
//! ### Clearing up the storage
//!
//! The [`SortedScores`] of the *active* submissions in a
//! given round. Each of the registered submissions may have one or more associated paged solution
//! stored in [`SubmissionsStorage`] and its corresponding [`SubmissionMetadata`].
//!
//! This pallet never implicitly clears either the metadata or the paged submissions storage data.
//! The data is kept in storage until [`Call::force_clear_submission`] extrinsic is called. At that
//! time, the hold deposit may be slashed depending on the state of the `release_strategy`
//! associated with the metadata.

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(test)]
mod tests;

use crate::{
	signed::pallet::Submissions,
	types::AccountIdOf,
	verifier::{AsyncVerifier, SolutionDataProvider, VerificationResult},
	PageIndex, PagesOf, Pallet as CorePallet, SolutionOf,
};

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	defensive,
	traits::{
		fungible::{
			hold::Balanced as FnBalanced, Credit, Inspect as FnInspect,
			InspectHold as FnInspectHold, Mutate as FnMutate, MutateHold as FnMutateHold,
		},
		tokens::Precision,
		Defensive, DefensiveSaturating, Get,
	},
	RuntimeDebugNoBound,
};
use scale_info::TypeInfo;
use sp_npos_elections::ElectionScore;
use sp_runtime::BoundedVec;
use sp_std::vec::Vec;

// public re-exports.
pub use pallet::{
	Call, Config, Error, Event, HoldReason, Pallet, __substrate_call_check,
	__substrate_event_check, tt_default_parts, tt_default_parts_v2, tt_error_token,
};

/// Alias for the pallet's balance type.
type BalanceOf<T> = <<T as Config>::Currency as FnInspect<AccountIdOf<T>>>::Balance;
/// Alias for the pallet's hold credit type.
pub type CreditOf<T> = Credit<AccountIdOf<T>, <T as Config>::Currency>;

/// Release strategy for currency held by this pallet.
#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebugNoBound, PartialEq, Clone)]
pub(crate) enum ReleaseStrategy {
	/// Releases all held deposit.
	All,
	/// Releases only the base deposit,
	BaseDeposit,
	/// Releases only the pages deposit.
	PageDeposit,
	/// Burn all held deposit.
	BurnAll,
}

impl Default for ReleaseStrategy {
	fn default() -> Self {
		Self::All
	}
}

/// Metadata of a registered submission.
#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Default, RuntimeDebugNoBound, Clone)]
#[cfg_attr(test, derive(frame_support::PartialEqNoBound, frame_support::EqNoBound))]
#[codec(mel_bound(T: Config))]
#[scale_info(skip_type_params(T))]
pub struct SubmissionMetadata<T: Config> {
	/// The score that this submission is proposing.
	claimed_score: ElectionScore,
	/// A bit-wise bounded vec representing the submitted pages thus far.
	pages: BoundedVec<bool, PagesOf<T>>,
	/// The amount held for this submission.
	deposit: BalanceOf<T>,
	/// Current release strategy for this metadata entry.
	release_strategy: ReleaseStrategy,
}

#[frame_support::pallet]
pub mod pallet {
	use core::marker::PhantomData;

	use crate::verifier::{AsyncVerifier, Verifier};

	use super::*;
	use frame_support::{
		pallet_prelude::{ValueQuery, *},
		traits::{tokens::Fortitude, Defensive, EstimateCallFee, OnUnbalanced},
		Twox64Concat,
	};
	use frame_system::{
		ensure_signed,
		pallet_prelude::{BlockNumberFor, OriginFor},
		WeightInfo,
	};
	use sp_npos_elections::ElectionScore;
	use sp_runtime::traits::Convert;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: crate::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The currency type.
		type Currency: FnMutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>
			+ FnBalanced<Self::AccountId>
			+ FnInspectHold<Self::AccountId>
			+ FnMutate<Self::AccountId>;

		/// Something that can predict the fee of a call. Used to sensibly distribute rewards.
		type EstimateCallFee: EstimateCallFee<Call<Self>, BalanceOf<Self>>;

		/// Handler for the unbalanced reduction that happens when submitters are slashed.
		type OnSlash: OnUnbalanced<CreditOf<Self>>;

		/// Something that calculates the signed base deposit based on the size of the current
		/// queued solution proposals.
		type DepositBase: Convert<usize, BalanceOf<Self>>;

		/// Per-page deposit for a signed solution.
		#[pallet::constant]
		type DepositPerPage: Get<BalanceOf<Self>>;

		/// Reward for an accepted solution.
		#[pallet::constant]
		type Reward: Get<BalanceOf<Self>>;

		/// The maximum number of signed submissions per round.
		#[pallet::constant]
		type MaxSubmissions: Get<u32>;

		/// The pallet's hold reason.
		type RuntimeHoldReason: From<HoldReason>;

		type WeightInfo: WeightInfo;
	}

	/// A sorted list of the current submissions scores corresponding to solution commitments
	/// submitted in the signed phase, keyed by round.
	///
	/// This pallet *MUST* ensure the bounded vec of scores is always sorted after mutation.
	#[pallet::storage]
	type SortedScores<T: Config> = StorageMap<
		_,
		Twox64Concat,
		u32,
		BoundedVec<(T::AccountId, ElectionScore), T::MaxSubmissions>,
		ValueQuery,
	>;

	/// A double-map from (`round`, `account_id`) to a submission metadata of a registered
	/// solution commitment.
	#[pallet::storage]
	type SubmissionMetadataStorage<T: Config> =
		StorageDoubleMap<_, Twox64Concat, u32, Twox64Concat, T::AccountId, SubmissionMetadata<T>>;

	/// A triple-map from (round, account, page) to a submitted solution.
	#[pallet::storage]
	type SubmissionStorage<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Twox64Concat, u32>,
			NMapKey<Twox64Concat, T::AccountId>,
			NMapKey<Twox64Concat, PageIndex>,
		),
		SolutionOf<T::MinerConfig>,
		OptionQuery,
	>;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	/// A reason for this pallet placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason<I: 'static = ()> {
		/// Deposit for registering an election solution.
		ElectionSolutionSubmission,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A score commitment has been successfully registered.
		Registered { round: u32, who: AccountIdOf<T>, claimed_score: ElectionScore },
		/// A submission page was stored successfully.
		PageStored { round: u32, who: AccountIdOf<T>, page: PageIndex },
		/// Retracted a submission successfully.
		Bailed { round: u32, who: AccountIdOf<T> },
		/// A submission has been cleared by request.
		SubmissionCleared { round: u32, submitter: AccountIdOf<T>, reward: Option<BalanceOf<T>> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Requested submission does not exist.
		NoSubmission,
		/// The election system is not expecting signed submissions.
		NotAcceptingSubmissions,
		/// Duplicate registering for a given round,
		DuplicateRegister,
		/// The submissions queue is full. Reject submission.
		SubmissionsQueueFull,
		/// Submission with a page index higher than the supported.
		BadPageIndex,
		/// A page submission was attempted for a submission that was not previously registered.
		SubmissionNotRegistered,
		/// A submission score is not high enough.
		SubmissionScoreTooLow,
		/// Bail request failed.
		CannotBail,
		/// Bad timing for force clearing a stored submission.
		CannotClear,
		/// Error releasing held funds.
		CannotReleaseFunds,
	}

	/// Wrapper for signed submissions.
	///
	/// It handles 3 storage items:
	///
	/// 1. [`SortedScores`]: A flat, striclty sorted, vector with all the submission's scores. The
	///    vector contains a tuple of `submitter_id` and `claimed_score`.
	/// 2. [`SubmissionStorage`]: Paginated map with all submissions, keyed by round, submitter and
	///    page index.
	/// 3. [`SubmissionMetadataStorage`]: Double map with submissions metadata, keyed by submitter
	///    ID and round.
	///
	/// Invariants:
	/// - [`SortedScores`] must be strictly sorted or empty.
	/// - All registered scores in [`SortedScores`] must be higher than the minimum score when
	/// inserted.
	/// - An entry in [`SortedScores`] for a given round must have an associated entry in
	/// [`SubmissionMetadataStorage`].
	/// - For all registered submissions, there is a held deposit that matches that of the
	///   submission metadata and the number of submitted pages.
	pub(crate) struct Submissions<T: Config>(core::marker::PhantomData<T>);
	impl<T: Config> Submissions<T> {
		/// Generic mutation helper with checks.
		///
		/// All the mutation functions must be done through this function.
		fn mutate_checked<R, F: FnOnce() -> R>(round: u32, mutate: F) -> R {
			let result = mutate();

			#[cfg(debug_assertions)]
			Self::sanity_check_round(round)
				.map_err(|err| {
					sublog!(debug, "signed", "Submissions sanity check failure: {:?}", err);
					err
				})
				.unwrap();

			result
		}

		/// Try to register a submission commitment.
		///
		/// The submission is not accepted if one of these invariants fails:
		/// - The claimed score is not higher than the minimum expected score.
		/// - The queue is full and the election score is strictly worse than all the current
		/// queued solutions.
		///
		/// A queued solution may be discarded if the queue is full and the new submission has a
		/// better score.
		///
		/// It must ensure that the metadata queue is sorted by election score.
		fn try_register(
			who: &T::AccountId,
			round: u32,
			metadata: SubmissionMetadata<T>,
		) -> DispatchResult {
			Self::mutate_checked(round, || Self::try_register_inner(who, round, metadata))
		}

		fn try_register_inner(
			who: &T::AccountId,
			round: u32,
			metadata: SubmissionMetadata<T>,
		) -> DispatchResult {
			let mut scores = Submissions::<T>::scores_for(round);
			scores.iter().try_for_each(|(account, _)| -> DispatchResult {
				ensure!(account != who, Error::<T>::DuplicateRegister);
				Ok(())
			})?;

			// most likely checked before, but double-checking.
			debug_assert!(!SubmissionMetadataStorage::<T>::contains_key(round, who));

			// the submission score must be higher than the minimum trusted score. Note that since
			// there is no queued solution yet, the check is only performed against the minimum
			// score.
			ensure!(
				<T::Verifier as Verifier>::ensure_score_quality(metadata.claimed_score),
				Error::<T>::SubmissionScoreTooLow,
			);

			let pos =
				match scores.binary_search_by_key(&metadata.claimed_score, |(_, score)| *score) {
					// in the unlikely event that election scores already exists in the storage, we
					// store the submissions next to one other.
					Ok(pos) | Err(pos) => pos,
				};

			let submission = (who.clone(), metadata.claimed_score);

			match scores.force_insert_keep_right(pos, submission) {
				// entry inserted without discarding.
				Ok(None) => Ok(()),
				// entry inserted but queue was full, clear the discarded submission.
				Ok(Some((discarded, _s))) => {
					let _ = SubmissionStorage::<T>::clear_prefix(
						(round, &discarded),
						u32::max_value(),
						None,
					);
					// unreserve full deposit
					let _ = T::Currency::release_all(
						&HoldReason::ElectionSolutionSubmission.into(),
						&who,
						Precision::Exact,
					)
					.defensive()?;

					Ok(())
				},
				Err(_) => Err(Error::<T>::SubmissionsQueueFull),
			}?;

			// hold deposit for this submission.
			T::Currency::hold(
				&HoldReason::ElectionSolutionSubmission.into(),
				&who,
				metadata.deposit,
			)?;

			SortedScores::<T>::insert(round, scores);
			SubmissionMetadataStorage::<T>::insert(round, who, metadata);

			Ok(())
		}

		/// Store a paged solution for `who` in a given `round`.
		///
		/// If `maybe_solution` is None, it will delete the given page from the submission store.
		/// Successive calls to this with the same page index will replace the existing page
		/// submission.
		pub(crate) fn try_mutate_page(
			who: &T::AccountId,
			round: u32,
			page: PageIndex,
			maybe_solution: Option<SolutionOf<T::MinerConfig>>,
		) -> DispatchResult {
			Self::mutate_checked(round, || {
				Self::try_mutate_page_inner(who, round, page, maybe_solution)
			})
		}

		fn try_mutate_page_inner(
			who: &T::AccountId,
			round: u32,
			page: PageIndex,
			maybe_solution: Option<SolutionOf<T::MinerConfig>>,
		) -> DispatchResult {
			ensure!(page < T::Pages::get(), Error::<T>::BadPageIndex);

			ensure!(Self::metadata_for(round, &who).is_some(), Error::<T>::SubmissionNotRegistered);

			let should_hold_extra =
				SubmissionStorage::<T>::mutate_exists((round, who, page), |maybe_old_solution| {
					let exists = maybe_old_solution.is_some();
					*maybe_old_solution = maybe_solution;

					!exists
				});

			// the deposit per page is held IFF it is a new page being stored.
			if should_hold_extra {
				T::Currency::hold(
					&HoldReason::ElectionSolutionSubmission.into(),
					&who,
					T::DepositPerPage::get(),
				)?;
			};

			Ok(())
		}

		/// Set metadata for submitter.
		pub(crate) fn set_metadata(
			round: u32,
			who: &T::AccountId,
			metadata: SubmissionMetadata<T>,
		) {
			// TODO: remove comment
			//debug_assert!(SortedScores::<T>::get(round).iter().any(|(account, _)| who ==
			// account));

			Self::mutate_checked(round, || {
				SubmissionMetadataStorage::<T>::insert(round, who, metadata);
			});
		}

		/// Clears the leader's score data, effectively disabling the submittion.
		///
		/// Returns the submission metadata of the disabled.
		pub(crate) fn take_leader_score(
			round: u32,
		) -> Option<(T::AccountId, Option<SubmissionMetadata<T>>)> {
			Self::mutate_checked(round, || {
				SortedScores::<T>::mutate(round, |scores| scores.pop()).and_then(
					|(submitter, _)| {
						Some((submitter.clone(), Self::metadata_for(round, &submitter)))
					},
				)
			})
		}

		/// Clear the registed metadata of a submission and its score and release the held deposit
		/// based on the `release_strategy`.
		///
		/// Clearing a submission only clears the metadata and stored score of a solution. The
		/// paged submissions must be cleared by explicitly calling
		/// [`Call::force_clear_submission`].
		///
		/// Note: the deposit can never be released completely or burned completely since
		/// an account may have lingering held deposit from previous or subsequent rounds. Thus, the
		/// amount to release and burn must always be calculated explicitly based on the round's
		/// metadata and release strategy.
		///
		/// The held deposit that is not released is burned as a penalty.
		pub(crate) fn clear_submission_of(
			who: &T::AccountId,
			round: u32,
			release_strategy: ReleaseStrategy,
		) -> DispatchResult {
			let reason = HoldReason::ElectionSolutionSubmission;

			let base_deposit = if let Some(metadata) = Self::metadata_for(round, &who) {
				metadata.deposit
			} else {
				return Err(Error::<T>::SubmissionNotRegistered.into());
			};

			let page_submissions_count = Submissions::<T>::page_count_submission_for(round, who);

			// calculates current held page deposit for this round.
			let page_deposit =
				T::DepositPerPage::get().defensive_saturating_mul(page_submissions_count.into());

			Self::mutate_checked(round, || {
				if page_submissions_count.is_zero() {
					// no page submissions, can also clear metadata.
					SubmissionMetadataStorage::<T>::remove(round, who);
				}

				SortedScores::<T>::mutate(round, |scores| {
					scores.retain(|(submitter, _)| submitter != who);
				});
			});

			let (burn, release) = match release_strategy {
				ReleaseStrategy::All =>
					(Zero::zero(), base_deposit.defensive_saturating_add(page_deposit)),
				ReleaseStrategy::BurnAll =>
					(base_deposit.defensive_saturating_add(page_deposit), Zero::zero()),
				ReleaseStrategy::BaseDeposit => (page_deposit, base_deposit),
				ReleaseStrategy::PageDeposit => (base_deposit, page_deposit),
			};

			T::Currency::burn_held(&reason.into(), who, burn, Precision::Exact, Fortitude::Force)?;
			T::Currency::release(&reason.into(), who, release, Precision::Exact)?;

			Ok(())
		}

		/// Returns the leader submitter for the current round and corresponding claimed score.
		pub(crate) fn leader(round: u32) -> Option<(T::AccountId, ElectionScore)> {
			Submissions::<T>::scores_for(round).last().cloned()
		}

		/// Returns the metadata of a submitter for a given round.
		pub(crate) fn metadata_for(
			round: u32,
			who: &T::AccountId,
		) -> Option<SubmissionMetadata<T>> {
			SubmissionMetadataStorage::<T>::get(round, who)
		}

		/// Returns the scores for a given round.
		pub(crate) fn scores_for(
			round: u32,
		) -> BoundedVec<(T::AccountId, ElectionScore), T::MaxSubmissions> {
			SortedScores::<T>::get(round)
		}

		/// Returns the score of a (round, submitter) tuple.
		pub(crate) fn score_of_submitter(round: u32, who: &T::AccountId) -> Option<ElectionScore> {
			SortedScores::<T>::get(round)
				.iter()
				.filter(|(submitter, _)| submitter == who)
				.map(|(_, score)| score)
				.cloned()
				.reduce(|s, _| s)
		}

		/// Returns the submission of a submitter for a given round and page.
		pub(crate) fn page_submission_for(
			round: u32,
			who: T::AccountId,
			page: PageIndex,
		) -> Option<SolutionOf<T::MinerConfig>> {
			SubmissionStorage::<T>::get((round, who, page))
		}

		pub(crate) fn page_count_submission_for(round: u32, who: &T::AccountId) -> u32 {
			SubmissionStorage::<T>::iter_key_prefix((round, who)).count() as u32
		}
	}

	#[cfg(any(test, debug_assertions, feature = "runtime-benchmarks"))]
	impl<T: Config> Submissions<T> {
		/// Fetches all rounds stored in the metadata storage and runs the round sanity checks.
		pub(crate) fn ensure_all() -> Result<(), &'static str> {
			for stored_round in SubmissionMetadataStorage::<T>::iter_keys().map(|(round, _)| round)
			{
				Self::sanity_check_round(stored_round)?;
			}
			Ok(())
		}

		/// Performs a sanity check on `round`'s data and metadata.
		fn sanity_check_round(round: u32) -> Result<(), &'static str> {
			Submissions::<T>::check_scores(round)?;
			Submissions::<T>::check_submission_storage(round)?;
			Submissions::<T>::check_phase(round)
		}

		/// Invariants:
		/// * Scores in the [`SortedScores`] storage are sorted for all round, where the score at
		///   the tail is the highest score.
		/// * If `round` matches the current round, all scores must be higher than the minimum score
		///   set by the verifier.
		/// * An entry in sorted scores storage must have a corresponding submission metadata
		/// entry.
		fn check_scores(round: u32) -> Result<(), &'static str> {
			// scores are expected to be sorted from tail to head of the SortedScores vec.
			let mut entries = SortedScores::<T>::get(round).into_iter().rev();
			let mut expected_highest_score = ElectionScore::max();

			while let Some((account, score)) = entries.next() {
				ensure!(expected_highest_score >= score, "scores in storage not sorted");
				ensure!(
					SubmissionMetadataStorage::<T>::get(round, account).is_some(),
					"stored score does not have an associated metadata entry"
				);

				if round == CorePallet::<T>::current_round() {
					let minimum_score =
						<T::Verifier as Verifier>::minimum_score().unwrap_or_default();
					ensure!(
						score >= minimum_score,
						"stored score for current round is lower than minimum score"
					);
				}
				expected_highest_score = score;
			}

			Ok(())
		}

		/// Invariants:
		/// * A paged submission should always have a corresponding metadata in storage.
		fn check_submission_storage(round: u32) -> Result<(), &'static str> {
			for submission in
				SubmissionStorage::<T>::iter_keys().filter(|(r, _, _)| r.clone() == round)
			{
				ensure!(
					Submissions::<T>::metadata_for(round, &submission.1).is_some(),
					"paged submission should always have metadata"
				);
			}

			Ok(())
		}

		/// Invariants:
		/// * Data and metadata for the current round should only exist if the current phase is
		/// SIgned of any of the subsequent phases.
		fn check_phase(round: u32) -> Result<(), &'static str> {
			if round != CorePallet::<T>::current_round() {
				return Ok(())
			}

			match CorePallet::<T>::current_phase() {
				crate::Phase::Off | crate::Phase::Snapshot(_) => {
					ensure!(
						SubmissionMetadataStorage::iter()
							.filter(|s: &(u32, _, SubmissionMetadata<T>)| s.0.clone() != round)
							.count()
							.is_zero(),
						"submission metadata for round exists out of phase."
					);

					ensure!(
						SubmissionStorage::<T>::iter_keys()
							.filter(|(r, _, _)| r.clone() != round)
							.count()
							.is_zero(),
						"submission data for round exists out of phase."
					);

					ensure!(
						SortedScores::<T>::get(round).is_empty(),
						"score for round exists out of phase."
					);
				},
				_ => (),
			};

			Ok(())
		}
	}

	#[cfg(any(feature = "runtime-benchmarks", test))]
	impl<T: Config> Submissions<T> {
		pub(crate) fn submission_metadata_from(
			claimed_score: ElectionScore,
			pages: BoundedVec<bool, PagesOf<T>>,
			deposit: BalanceOf<T>,
			release_strategy: ReleaseStrategy,
		) -> SubmissionMetadata<T> {
			SubmissionMetadata { claimed_score, pages, deposit, release_strategy }
		}

		pub(crate) fn insert_score_and_metadata(
			round: u32,
			who: T::AccountId,
			maybe_score: Option<ElectionScore>,
			maybe_metadata: Option<SubmissionMetadata<T>>,
		) {
			if let Some(score) = maybe_score {
				let mut scores = Submissions::<T>::scores_for(round);
				scores.try_push((who.clone(), score)).unwrap();
				SortedScores::<T>::insert(round, scores);
			}

			if let Some(metadata) = maybe_metadata {
				SubmissionMetadataStorage::<T>::insert(round, who.clone(), metadata);
			}
		}
	}

	impl<T: Config> Pallet<T> {
		pub(crate) fn do_register(
			who: &T::AccountId,
			claimed_score: ElectionScore,
			round: u32,
		) -> DispatchResult {
			// base deposit depends on the number of submissions for the current `round`.
			let deposit = T::DepositBase::convert(
				SubmissionMetadataStorage::<T>::iter_key_prefix(round).count(),
			);

			let pages: BoundedVec<_, T::Pages> = (0..T::Pages::get())
				.map(|_| false)
				.collect::<Vec<_>>()
				.try_into()
				.expect("bounded vec constructed from bound; qed.");

			let metadata = SubmissionMetadata {
				pages,
				claimed_score,
				deposit,
				// new submissions should receive back all held deposit.
				release_strategy: ReleaseStrategy::All,
			};

			let _ = Submissions::<T>::try_register(&who, round, metadata)?;
			Ok(())
		}
	}

	#[cfg(any(test, feature = "try-runtime"))]
	impl<T: Config + crate::verifier::Config> Pallet<T> {
		pub(crate) fn do_try_state(
			_now: BlockNumberFor<T>,
		) -> Result<(), sp_runtime::TryRuntimeError> {
			Submissions::<T>::ensure_all().map_err(|e| e.into())
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit a score commitment for a solution in the current round.
		///
		/// The scores must be kept sorted in the `SortedScores` storage map.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::default())]
		pub fn register(origin: OriginFor<T>, claimed_score: ElectionScore) -> DispatchResult {
			let who = ensure_signed(origin)?;

			ensure!(
				CorePallet::<T>::current_phase().is_signed(),
				Error::<T>::NotAcceptingSubmissions
			);

			let round = CorePallet::<T>::current_round();
			ensure!(
				Submissions::<T>::metadata_for(round, &who).is_none(),
				Error::<T>::DuplicateRegister
			);

			Self::do_register(&who, claimed_score, round)?;

			Self::deposit_event(Event::<T>::Registered { round, who, claimed_score });
			Ok(())
		}

		/// Submit a page for a solution.
		///
		/// To submit a solution page successfull, the submitter must have registered the
		/// commitment before.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::default())]
		pub fn submit_page(
			origin: OriginFor<T>,
			page: PageIndex,
			maybe_solution: Option<SolutionOf<T::MinerConfig>>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			ensure!(
				CorePallet::<T>::current_phase().is_signed(),
				Error::<T>::NotAcceptingSubmissions
			);

			let round = CorePallet::<T>::current_round();
			Submissions::<T>::try_mutate_page(&who, round, page, maybe_solution)?;

			Self::deposit_event(Event::<T>::PageStored { round, who, page });

			Ok(())
		}

		/// Unregister a submission.
		///
		/// This will fully remove the solution and corresponding metadata from storage and refund
		/// the page submissions deposit only.
		///
		/// Note: the base deposit will be burned to prevent the attack where rogue submitters
		/// deprive honest submitters submitting a solution.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::default())]
		pub fn bail(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// only allow bailing submissions for the current round and in the signed phase, to
			// ensure the submission has not been verified yy.
			ensure!(CorePallet::<T>::current_phase().is_signed(), Error::<T>::CannotBail,);

			let round = CorePallet::<T>::current_round();
			Submissions::<T>::clear_submission_of(&who, round, ReleaseStrategy::PageDeposit)?;

			Self::deposit_event(Event::<T>::Bailed { round, who });

			Ok(())
		}

		/// Force clean submissions storage for a given (`submitter`, `round`) tuple.
		///
		/// This pallet expects that submitted pages for `round` may exist IFF a corresponding
		/// metadata exists.
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::default())]
		pub fn force_clear_submission(
			origin: OriginFor<T>,
			round: u32,
			submitter: T::AccountId,
		) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin);

			// force clearing submissions may happen only during phase off.
			ensure!(CorePallet::<T>::current_phase().is_off(), Error::<T>::CannotClear);

			if let Some(metadata) = Submissions::<T>::metadata_for(round, &submitter) {
				// clear submission metadata from submitter for `round`.
				let _ = Submissions::<T>::clear_submission_of(
					&submitter,
					round,
					metadata.release_strategy,
				);

				// clear all pages from submitter in `round`.
				let _ = SubmissionStorage::<T>::clear_prefix(
					(round, &submitter),
					u32::max_value(),
					None,
				);

				// clear sorted score, if it still exists in storage.
				SortedScores::<T>::mutate(round, |scores| {
					scores.retain(|(who, _)| submitter != *who);
				});
			} else {
				// if metadata does not exist, paged submission and score should not exist either.
				debug_assert!(
					Submissions::<T>::page_count_submission_for(round, &submitter).is_zero() &&
						Submissions::<T>::score_of_submitter(round, &submitter).is_none()
				);

				return Err(Error::<T>::NoSubmission.into())
			}

			Self::deposit_event(Event::<T>::SubmissionCleared {
				round: CorePallet::<T>::current_round(),
				submitter,
				reward: None,
			});

			Ok(Pays::No.into())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {}

		#[cfg(feature = "try-runtime")]
		fn try_state(n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state(n)
		}
	}
}

impl<T: Config> SolutionDataProvider for Pallet<T> {
	type Solution = SolutionOf<T::MinerConfig>;

	/// Returns a paged solution of the *best* solution in the queue.
	fn get_paged_solution(page: PageIndex) -> Option<Self::Solution> {
		let round = CorePallet::<T>::current_round();

		Submissions::<T>::leader(round).map(|(who, _score)| {
			sublog!(info, "signed", "returning page {} of leader's {:?} solution", page, who);
			Submissions::<T>::page_submission_for(round, who, page).unwrap_or_default()
		})
	}

	/// Returns the score of the *best* solution in the queue.
	fn get_score() -> Option<ElectionScore> {
		let round = CorePallet::<T>::current_round();
		Submissions::<T>::leader(round).map(|(_who, score)| score)
	}

	/// Returns whether the submission queue has more submissions to process in a given round.
	fn has_pending_submission(era: u32) -> bool {
		!Submissions::<T>::scores_for(era).is_empty()
	}

	/// Called by an external entity to report a verification result of the current *best*
	/// solution.
	///
	/// If the verification is rejected, update the leader's metadata to be slashed (i.e. set
	/// release strategy to [`ReleaseStrategy::BurnAll`] in the leader's metadata). If successful
	/// (represented by the variant [``VerificationResult::Queued]), reward the submitter and
	/// signal the verifier to stop the async election verification.
	fn report_result(result: VerificationResult) {
		let round = CorePallet::<T>::current_round();

		let (leader, mut metadata) =
			if let Some((leader, maybe_metadata)) = Submissions::<T>::take_leader_score(round) {
				let metadata = match maybe_metadata {
					Some(m) => m,
					None => {
						defensive!("unexpected: leader with inconsistent data (no metadata).");
						return;
					},
				};
				(leader, metadata)
			} else {
				sublog!(error, "signed", "unexpected: leader called without active submissions.");
				return
			};

		match result {
			VerificationResult::Queued => {
				// solution was accepted by the verifier, reward leader and stop async
				// verification.
				let _ = T::Currency::mint_into(&leader, T::Reward::get()).defensive();
				let _ = <T::Verifier as AsyncVerifier>::stop();
			},
			VerificationResult::Rejected | VerificationResult::DataUnavailable => {
				// updates metadata release strategy so that all the deposit is burned when the
				// leader's data is cleared.
				metadata.release_strategy = ReleaseStrategy::BurnAll;
				Submissions::<T>::set_metadata(round, &leader, metadata);
			},
		}
	}
}
