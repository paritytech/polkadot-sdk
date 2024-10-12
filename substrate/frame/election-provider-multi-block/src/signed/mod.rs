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
//! The main goal of the signed sub-pallet is to keep and manage a list of sorted score commitments
//! and correponding paged solutions during the [`crate::Phase::Signed`].
//!
//! Accounts may submit up to [`Config::MaxSubmissions`] score commitments per election round and
//! this pallet ensures that the scores are stored under the map `SortedScores` are sorted and keyed
//! by the correct round number.
//!
//! Each submitter must hold a deposit per submission that is calculated based on the number of
//! pages required for a full submission and the number of submissions in the queue. The deposit is
//! returned in case the claimed score is correct after the solution verification. Note that if a
//! commitment and corresponding solution are not verified during the verification phase, the
//! submitter is not slashed and the deposits returned.
//!
//! When the time to evaluate the signed submission comes, the solutions are checked from best to
//! worse, which may result in one of three scenarios:
//!
//! 1. If the committed score and page submissions are correct, the submitter is rewarded.
//! 2. Any queued score that was not evaluated, the hold deposit is returned.
//! 3. Any invalid solution results in a 100% slash of the hold submission deposit.
//!
//! Once the [`crate::Phase::SignedValidation`] phase starts, the async verifier is notified to
//! start verifying the best queued solution.
//!
//! TODO:
//! - Be more efficient with cleaning up the submission storage by e.g. expose an extrinsic that
//! allows anyone to clean up the submissions storage with a small reward from the submission
//! deposit (clean up storage submissions and all corresponding metadata).

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(test)]
mod tests;

use crate::{
	signed::pallet::Submissions,
	types::AccountIdOf,
	verifier::{AsyncVerifier, SolutionDataProvider, VerificationResult},
	PageIndex, PagesOf,
};

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	traits::{
		fungible::{
			hold::Balanced as FnBalanced, Credit, Inspect as FnInspect, MutateHold as FnMutateHold,
		},
		tokens::Precision,
		Defensive,
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

/// Metadata of a registered submission.
#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Default, RuntimeDebugNoBound)]
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
}

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use core::marker::PhantomData;

	use crate::verifier::{AsyncVerifier, Verifier};

	use super::*;
	use frame_support::{
		pallet_prelude::{ValueQuery, *},
		traits::{Defensive, EstimateCallFee, OnUnbalanced},
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
			+ FnBalanced<Self::AccountId>;

		/// Something that can predict the fee of a call. Used to sensibly distribute rewards.
		type EstimateCallFee: EstimateCallFee<Call<Self>, BalanceOf<Self>>;

		/// Handler for the unbalanced reduction that happens when submitters are slashed.
		type OnSlash: OnUnbalanced<CreditOf<Self>>;

		/// Something that calculates the signed base deposit based on the size of the current
		/// queued solution proposals.
		/// TODO: rename to `Deposit` or other?
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

	/// A triple-map from (round, account, page) to a submitted solution.
	#[pallet::storage]
	type SubmissionStorage<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Twox64Concat, u32>,
			NMapKey<Twox64Concat, T::AccountId>,
			NMapKey<Twox64Concat, PageIndex>,
		),
		T::Solution,
		OptionQuery,
	>;

	/// A double-map from (`round`, `account_id`) to a submission metadata of a registered
	/// solution commitment.
	#[pallet::storage]
	type SubmissionMetadataStorage<T: Config> =
		StorageDoubleMap<_, Twox64Concat, u32, Twox64Concat, T::AccountId, SubmissionMetadata<T>>;

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
		/// Bad timing for force clearing a stored submission.
		CannotClear,
	}

	/// Wrapper for signed submissions.
	///
	/// It handle 3 storage items:
	///
	/// 1. [`SortedScores`]: A flat, striclty sorted, vector with all the submission's scores. The
	///    vector contains a tuple of `submitter_id` and `claimed_score`.
	/// 2. [`SubmissionStorage`]: Paginated map with all submissions, keyed by round, submitter and
	///    page index.
	/// 3. [`SubmissionMetadataStorage`]: Double map with submissions metadata, keyed by submitter
	///    ID and round.
	///
	/// Invariants:
	/// - TODO
	pub(crate) struct Submissions<T: Config>(core::marker::PhantomData<T>);
	impl<T: Config> Submissions<T> {
		/// Generic mutation helper with checks.
		///
		/// All the mutation functions must be done through this function.
		fn mutate_checked<R, F: FnOnce() -> R>(_round: u32, mutate: F) -> R {
			let result = mutate();

			#[cfg(debug_assertions)]
			assert!(Self::sanity_check_round(crate::Pallet::<T>::current_round()).is_ok());

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
			let mut scores = SortedScores::<T>::get(round);
			scores.iter().try_for_each(|(account, _)| -> DispatchResult {
				ensure!(account != who, Error::<T>::DuplicateRegister);
				Ok(())
			})?;

			// most likely checked before, but double-checking.
			debug_assert!(!SubmissionMetadataStorage::<T>::contains_key(round, who));

			// the submission score must be higher than the minimum trusted score. Note that since
			// there is no queued solution yet, the check is performed against the minimum score
			// only. TODO: consider rename `ensure_score_improves`.
			ensure!(
				<T::Verifier as Verifier>::ensure_score_improves(metadata.claimed_score),
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
					let _ =
						SubmissionStorage::<T>::clear_prefix((round, &discarded), u32::MAX, None);
					// unreserve deposit
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
			maybe_solution: Option<T::Solution>,
		) -> DispatchResult {
			Self::mutate_checked(round, || {
				Self::try_mutate_page_inner(who, round, page, maybe_solution)
			})
		}

		fn try_mutate_page_inner(
			who: &T::AccountId,
			round: u32,
			page: PageIndex,
			maybe_solution: Option<T::Solution>,
		) -> DispatchResult {
			ensure!(
				crate::Pallet::<T>::current_phase().is_signed(),
				Error::<T>::NotAcceptingSubmissions
			);
			ensure!(page < T::Pages::get(), Error::<T>::BadPageIndex);

			ensure!(
				SubmissionMetadataStorage::<T>::contains_key(round, who),
				Error::<T>::SubmissionNotRegistered
			);

			// TODO: update the held deposit to account for the paged submission deposit.

			SubmissionStorage::<T>::mutate_exists((round, who, page), |maybe_old_solution| {
				*maybe_old_solution = maybe_solution
			});

			Ok(())
		}

		/// Clears all the stored data from the leader.
		///
		/// Returns the submission metadata of the cleared submission, if any.
		pub(crate) fn take_leader_data(
			round: u32,
		) -> Option<(T::AccountId, SubmissionMetadata<T>)> {
			Self::mutate_checked(round, || {
				SortedScores::<T>::mutate(round, |scores| scores.pop()).and_then(
					|(submitter, _score)| {
						let _ = SubmissionStorage::<T>::clear_prefix(
							(round, &submitter),
							u32::MAX,
							None,
						); // TODO: handle error.

						SubmissionMetadataStorage::<T>::take(round, &submitter)
							.map(|metadata| (submitter, metadata))
					},
				)
			})
		}

		/// Returns the leader submitter for the current round and corresponding claimed score.
		pub(crate) fn leader(round: u32) -> Option<(T::AccountId, ElectionScore)> {
			SortedScores::<T>::get(round).last().cloned()
		}

		/// Returns a submission page for a given round, submitter and page index.
		pub(crate) fn get_page(
			who: &T::AccountId,
			round: u32,
			page: PageIndex,
		) -> Option<T::Solution> {
			SubmissionStorage::<T>::get((round, who, page))
		}
	}

	#[allow(dead_code)]
	impl<T: Config> Submissions<T> {
		/// Returns the metadata of a submitter for a given account.
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

		/// Returns the submission of a submitter for a given round and page.
		pub(crate) fn submission_for(
			who: T::AccountId,
			round: u32,
			page: PageIndex,
		) -> Option<T::Solution> {
			SubmissionStorage::<T>::get((round, who, page))
		}
	}

	#[cfg(debug_assertions)]
	impl<T: Config> Submissions<T> {
		fn sanity_check_round(_round: u32) -> Result<(), &'static str> {
			// TODO
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub(crate) fn do_register(
			who: &T::AccountId,
			claimed_score: ElectionScore,
			round: u32,
		) -> DispatchResult {
			let deposit = T::DepositBase::convert(
				SubmissionMetadataStorage::<T>::iter_key_prefix(round).count(),
			);

			T::Currency::hold(&HoldReason::ElectionSolutionSubmission.into(), &who, deposit)?;

			let pages: BoundedVec<_, T::Pages> = (0..T::Pages::get())
				.map(|_| false)
				.collect::<Vec<_>>()
				.try_into()
				.expect("bounded vec constructed from bound; qed.");

			let metadata = SubmissionMetadata { pages, claimed_score, deposit };

			let _ = Submissions::<T>::try_register(&who, round, metadata)?;
			Ok(())
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit a score commitment for a solution in the current round.
		///
		/// The scores must be kept sorted in the `SortedScores` storage map.
		#[pallet::call_index(1)]
		pub fn register(origin: OriginFor<T>, claimed_score: ElectionScore) -> DispatchResult {
			let who = ensure_signed(origin)?;

			ensure!(
				crate::Pallet::<T>::current_phase().is_signed(),
				Error::<T>::NotAcceptingSubmissions
			);

			let round = crate::Pallet::<T>::current_round();
			ensure!(
				!SubmissionMetadataStorage::<T>::contains_key(round, who.clone()),
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
		///
		/// TODO: for security reasons, we have to ensure that ALL submitters "space" to
		/// submit their pages and be verified.
		#[pallet::call_index(2)]
		pub fn submit_page(
			origin: OriginFor<T>,
			page: PageIndex,
			maybe_solution: Option<T::Solution>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			ensure!(
				crate::Pallet::<T>::current_phase().is_signed(),
				Error::<T>::NotAcceptingSubmissions
			);

			let round = crate::Pallet::<T>::current_round();
			Submissions::<T>::try_mutate_page(&who, round, page, maybe_solution)?;

			Self::deposit_event(Event::<T>::PageStored {
				round: crate::Pallet::<T>::current_round(),
				who,
				page,
			});

			Ok(())
		}

		/// Unregister a submission.
		///
		/// This will fully remove the solution and corresponding metadata from storage and refund
		/// the submission deposit.
		///
		/// NOTE: should we refund the deposit? there's an attack vector where an attacker can
		/// register with a set of very high elections core and then retract all submission just
		/// before the signed phase ends. This may end up depriving other honest miners from
		/// registering their solution.
		#[pallet::call_index(3)]
		pub fn bail(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			ensure!(
				crate::Pallet::<T>::current_phase().is_signed(),
				Error::<T>::NotAcceptingSubmissions
			);

			// TODO
			// 1. clear all storage items related to `who`
			// 2. return deposit

			Self::deposit_event(Event::<T>::Bailed {
				round: crate::Pallet::<T>::current_round(),
				who,
			});

			Ok(())
		}

		/// Force clean submissions storage.
		///
		/// Allows any account to receive a reward for requesting the submission storage and
		/// corresponding metadata to be cleaned. This extrinsic will fail if the signed or signed
		/// validated phases are active to prevent disruption in the election progress.
		///
		/// A successfull call will result in a reward that is taken from the cleared submission
		/// deposit and the return of the call fees.
		#[pallet::call_index(4)]
		pub fn force_clear_submission(
			origin: OriginFor<T>,
			submitter: T::AccountId,
		) -> DispatchResult {
			let _who = ensure_signed(origin);

			// prevent cleaning up submissions storage during the signed and signed validation
			// phase.
			ensure!(
				!crate::Pallet::<T>::current_phase().is_signed() &&
					!crate::Pallet::<T>::current_phase().is_signed_validation_open_at(None),
				Error::<T>::CannotClear,
			);

			// TODO:
			// 1. clear the submission, if it exists
			// 2. clear the submission metadata
			// 3. reward caller as a portions of the submittion's deposit
			let reward = Default::default();
			// 4. return fees.

			Self::deposit_event(Event::<T>::SubmissionCleared {
				round: crate::Pallet::<T>::current_round(),
				submitter,
				reward,
			});

			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// The `on_initialize` signals the [`AsyncVerifier`] whenever it should start or stop the
		/// asynchronous verification of the stored submissions.
		///
		/// - Start async verification at the beginning of the [`crate::Phase::SignedValidation`].
		/// - Stopns async verification at the beginning of the [`crate::Phase::Unsigned`].
		fn on_initialize(now: BlockNumberFor<T>) -> Weight {
			// TODO: match
			if crate::Pallet::<T>::current_phase().is_signed_validation_open_at(Some(now)) {
				let _ = <T::Verifier as AsyncVerifier>::start().defensive();
			};

			if crate::Pallet::<T>::current_phase().is_unsigned_open_at(now) {
				sublog!(info, "signed", "signed validation phase ended, signaling the verifier.");
				<T::Verifier as AsyncVerifier>::stop();
			}

			if crate::Pallet::<T>::current_phase() == crate::Phase::Off {
				sublog!(info, "signed", "clear up storage for pallets.");

				// TODO: optimize.
				let _ = SubmissionMetadataStorage::<T>::clear(u32::MAX, None);
				let _ = SubmissionStorage::<T>::clear(u32::MAX, None);
				let _ = SortedScores::<T>::clear(u32::MAX, None);
			}

			Weight::default()
		}
	}
}

impl<T: Config> SolutionDataProvider for Pallet<T> {
	type Solution = T::Solution;

	fn get_paged_solution(page: PageIndex) -> Option<Self::Solution> {
		let round = crate::Pallet::<T>::current_round();

		Submissions::<T>::leader(round).map(|(who, _score)| {
			sublog!(info, "signed", "returning page {} of leader's {:?} solution", page, who);
			Submissions::<T>::get_page(&who, round, page).unwrap_or_default()
		})
	}

	fn get_score() -> Option<ElectionScore> {
		let round = crate::Pallet::<T>::current_round();
		Submissions::<T>::leader(round).map(|(_who, score)| score)
	}

	fn report_result(result: VerificationResult) {
		let round = crate::Pallet::<T>::current_round();
		match result {
			VerificationResult::Queued => {},
			VerificationResult::Rejected => {
				if let Some((_offender, _metadata)) = Submissions::<T>::take_leader_data(round) {
					// TODO: slash offender
				} else {
					// no signed submission in storage, signal async verifier to stop and move on.
					let _ = <T::Verifier as AsyncVerifier>::stop();
				};

				if crate::Pallet::<T>::current_phase().is_signed_validation_open_at(None) &&
					Submissions::<T>::leader(round).is_some()
				{
					let _ = <T::Verifier as AsyncVerifier>::start().defensive();
				}
			},
			VerificationResult::DataUnavailable => {
				// signed pallet did not have the required data.
			},
		}
	}
}
