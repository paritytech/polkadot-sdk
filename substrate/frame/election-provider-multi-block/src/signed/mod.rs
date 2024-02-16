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

//! The signed phase of the multi-block election system.
//!
//! The main goal of the signed sub-pallet is to keep and manage a list of sorted score commitments
//! submitted by any account when the election system is in [`crate::Phase::Signed`].
//!
//! Accounts may submit up to [`T::MaxSubmissions`] score commitments per election round and this
//! pallet ensures that the scores are stored under the map [`SortedScores`] are sorted and keyed
//! by the correct round number.
//!
//! Each submitter must hold a deposit per submission that is calculated based on the number of
//! pages required for a full submission and the number of submissions in the queue. The deposit is
//! returned in case the verified score was not incorrect.
//!
//! When the time to evaluate the signed submission comes, the solutions are checked from best to
//! worse, which may result in one of three scenarios:
//!
//! 1. If the committed score and page submissions are correct, the submitter is rewarded.
//! 2. Any queued score that was not evaluated, the hold deposit is returned.
//! 3. Any invalid solution results in a 100% slash of the hold submission deposit.
//!
//! Once the [`crate::Phase::Validation`] phase starts, the async verifier is notified to start
//! verifying the best queued solution.

#[cfg(test)]
mod tests;

// TODO(gpestana): clean imports.
use crate::{
	signed::pallet::Submissions,
	types::AccountIdOf,
	verifier::{AsyncVerifier, SolutionDataProvider, VerificationResult},
	PageIndex,
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

// public re-exports.
pub use pallet::{
	Call, Config, Error, Event, HoldReason, Pallet, __substrate_call_check,
	__substrate_event_check, tt_default_parts, tt_error_token,
};

// Alias for the pallet's balance type.
type BalanceOf<T> = <<T as Config>::Currency as FnInspect<AccountIdOf<T>>>::Balance;
// Alias for the pallet's hold credit type.
pub type CreditOf<T> = Credit<AccountIdOf<T>, <T as Config>::Currency>;

/// Metadata of a registered submission.
#[derive(PartialEq, Encode, Decode, MaxEncodedLen, TypeInfo, Default, RuntimeDebugNoBound)]
pub struct SubmissionMetadata {
	/// The score that this submission is proposing.
	claimed_score: ElectionScore,
	/// A counter of the pages submitted thus far.
	pages: PageIndex,
}

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use core::marker::PhantomData;

	use crate::verifier::AsyncVerifier;

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
	use sp_runtime::{traits::Convert, BoundedVec};

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

	/// A sorted list of the current submissions scores corresponding to pre-solutions submitted in
	/// the signed phase, keyed by round.
	///
	/// The implementor *MUST* ensure the bounded vec of scores is always sorted after mutation. //
	/// TODO: add try state check.
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

	/// A double-map from (round, account_id) to a submission metadata of a registered solution.
	#[pallet::storage]
	type SubmissionMetadataStorage<T: Config> =
		StorageDoubleMap<_, Twox64Concat, u32, Twox64Concat, T::AccountId, SubmissionMetadata>;

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
		/// A a page submission was attempted for a submission that was not previously registered.
		SubmissionNotRegistered,
	}

	/// TODO: docs
	pub(crate) struct Submissions<T: Config>(core::marker::PhantomData<T>);
	impl<T: Config> Submissions<T> {
		/// Generic mutation helper with checks.
		///
		/// All the mutation functions must be done through this function.
		fn mutate_checked<R, F: FnOnce() -> R>(_round: u32, mutate: F) -> R {
			let result = mutate();

			#[cfg(debug_assertions)]
			{
				//assert!(Self::sanity_check_round(round).is_ok()); // TODO
			}

			result
		}

		/// TODO: docs
		fn try_register(
			who: &T::AccountId,
			round: u32,
			metadata: SubmissionMetadata,
		) -> DispatchResult {
			Self::mutate_checked(round, || Self::try_register_inner(who, round, metadata))
		}

		fn try_register_inner(
			who: &T::AccountId,
			round: u32,
			metadata: SubmissionMetadata,
		) -> DispatchResult {
			let mut scores = SortedScores::<T>::get(round);

			scores.iter().try_for_each(|(account, _)| -> DispatchResult {
				ensure!(account != who, Error::<T>::DuplicateRegister);
				Ok(())
			})?;

			// most likely checked before, but double-checking.
			debug_assert!(!SubmissionMetadataStorage::<T>::contains_key(round, who));

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

		// TODO: docs
		// Note: if `maybe_solution` is None, it will delete the given page from the submission
		// store. Successive calls to this with the same page index will replace the existing page
		// submission.
		fn try_mutate_page(
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

		pub(crate) fn take_leader_data(round: u32) -> Option<(T::AccountId, SubmissionMetadata)> {
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

		pub(crate) fn leader(round: u32) -> Option<(T::AccountId, ElectionScore)> {
			SortedScores::<T>::get(round).last().cloned()
		}

		pub(crate) fn get_page(
			who: &T::AccountId,
			round: u32,
			page: PageIndex,
		) -> Option<T::Solution> {
			SubmissionStorage::<T>::get((round, who, page))
		}
	}

	#[cfg(any(test, debug_assertions))]
	impl<T: Config> Submissions<T> {
		#[allow(dead_code)]
		pub(crate) fn metadata(round: u32, who: &T::AccountId) -> Option<SubmissionMetadata> {
			SubmissionMetadataStorage::<T>::get(round, who)
		}

		#[allow(dead_code)]
		pub(crate) fn scores_for(
			round: u32,
		) -> BoundedVec<(T::AccountId, ElectionScore), T::MaxSubmissions> {
			SortedScores::<T>::get(round)
		}

		#[allow(dead_code)]
		pub(crate) fn submission_for(
			who: T::AccountId,
			round: u32,
			page: PageIndex,
		) -> Option<T::Solution> {
			SubmissionStorage::<T>::get((round, who, page))
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit a score commitment for a solution in the current round.
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

			let deposit_base = T::DepositBase::convert(
				SubmissionMetadataStorage::<T>::iter_key_prefix(round).count(),
			);

			T::Currency::hold(&HoldReason::ElectionSolutionSubmission.into(), &who, deposit_base)?;

			let metadata = SubmissionMetadata { pages: 0, claimed_score };

			let _ = Submissions::<T>::try_register(&who, round, metadata)?;

			Self::deposit_event(Event::<T>::Registered { round, who, claimed_score });
			Ok(())
		}

		/// Submit a page for a solution.
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

			// Note/TODO: for security reasons, we have to ensure that ALL submitters "space" to
			// submit their pages and be verified.

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
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(now: BlockNumberFor<T>) -> Weight {
			if crate::Pallet::<T>::current_phase().is_signed_validation_open_at(Some(now)) {
				// TODO(gpestana): check if there is currently a leader score submission.
				let _ = <T::Verifier as AsyncVerifier>::start().defensive();
			};

			if crate::Pallet::<T>::current_phase().is_unsigned_open_at(now) {
				sublog!(info, "signed", "signed validation phase ended, signaling the verifier.");
				<T::Verifier as AsyncVerifier>::stop();
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

	// TODO: finish
	fn report_result(result: VerificationResult) {
		let round = crate::Pallet::<T>::current_round();
		match result {
			VerificationResult::Queued => {},
			VerificationResult::Rejected => {
				if let Some((_offender, _metadata)) =
					Submissions::<T>::take_leader_data(round).defensive()
				{
					// TODO: slash offender
				} else {
					// should never happen, defensive
					// note: should the AsyncVerifier be notified to proceed with next leader in
					// this case?
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

#[cfg(test)]
mod signed_tests {
	use super::*;
	use crate::{mock::*, Phase};
	use frame_support::{testing_prelude::*, BoundedVec};

	type MaxSubmissions = <Runtime as Config>::MaxSubmissions;

	mod submissions {
		use super::*;

		#[test]
		fn submit_solution_happy_path_works() {
			ExtBuilder::default().build_and_execute(|| {
				// TODO: check events

				roll_to_phase(Phase::Signed);

				let current_round = MultiPhase::current_round();

				assert!(Submissions::<Runtime>::metadata(current_round, &10).is_none());

				let claimed_score = ElectionScore::default();

				// register submission
				assert_ok!(SignedPallet::register(RuntimeOrigin::signed(10), claimed_score,));

				// metadata and claimed scores have been stored as expected.
				assert_eq!(
					Submissions::<Runtime>::metadata(current_round, &10),
					Some(SubmissionMetadata { pages: 0, claimed_score })
				);
				let expected_scores: BoundedVec<(AccountId, ElectionScore), MaxSubmissions> =
					bounded_vec![(10, claimed_score)];
				assert_eq!(Submissions::<Runtime>::scores_for(current_round), expected_scores);

				// submit all pages of a noop solution;
				let solution = TestNposSolution::default();
				for page in (0..=MultiPhase::msp()).into_iter().rev() {
					assert_ok!(SignedPallet::submit_page(
						RuntimeOrigin::signed(10),
						page,
						Some(solution.clone())
					));

					assert_eq!(
						Submissions::<Runtime>::submission_for(10, current_round, page),
						Some(solution.clone())
					);
				}
			})
		}
	}
}
