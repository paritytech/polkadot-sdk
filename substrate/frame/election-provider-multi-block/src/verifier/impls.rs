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

use super::*;
use crate::{unsigned::miner, verifier::weights::WeightInfo, MinerSupportsOf, SolutionOf};
use pallet::*;

use frame_election_provider_support::PageIndex;
use frame_support::{
	ensure,
	pallet_prelude::Weight,
	traits::{Defensive, DefensiveSaturating, TryCollect},
	BoundedVec,
};
use sp_runtime::Perbill;
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData, vec::Vec};

#[frame_support::pallet]
pub(crate) mod pallet {
	use super::*;
	use frame_support::pallet_prelude::{ValueQuery, *};
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: crate::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Origin that can control this pallet. This must be a *trusted origin* since the
		/// actions taken by this origin are not checked (e.g. `set_emergency_solution`).
		type ForceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Minimum improvement to a solution that defines a new solution as "better".
		type SolutionImprovementThreshold: Get<Perbill>;

		/// Something that can provide the solution data to the verifier.
		type SolutionDataProvider: crate::verifier::SolutionDataProvider<
			Solution = SolutionOf<Self::MinerConfig>,
		>;

		/// The weight information of this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Started a signed submission verification with `claimed_score`.
		VerificationStarted { claimed_score: ElectionScore },
		/// A verificaction failed at the given page.
		VerificationFailed { page: PageIndex, error: FeasibilityError },
		/// The final verifications of the `finalize_verification` failed. If this error happened,
		/// all the single pages passed the feasibility checks.
		FinalizeVerificationFailed { error: FeasibilityError },
		/// The given page has been correctly verified, with the number of backers that are part of
		/// the page.
		Verified { page: PageIndex, backers: u32 },
		/// A new solution with the given score has replaced the previous best solution, if any.
		Queued { score: ElectionScore, old_score: Option<ElectionScore> },
		/// The solution data was not available for a specific page.
		SolutionDataUnavailable { page: PageIndex },
	}

	/// Supports of the solution of the variant X.
	///
	/// A potential valid or invalid solution may be stored in this variant during the round.
	#[pallet::storage]
	pub type QueuedSolutionX<T: Config> =
		StorageMap<_, Twox64Concat, PageIndex, MinerSupportsOf<T::MinerConfig>>;

	/// Supports of the solution of the variant Y.
	///
	/// A potential valid or invalid solution may be stored in this variant during the round.
	#[pallet::storage]
	pub type QueuedSolutionY<T: Config> =
		StorageMap<_, Twox64Concat, PageIndex, MinerSupportsOf<T::MinerConfig>>;

	/// The `(amount, count)` of backings, keyed by page.
	///
	/// This is stored to facilitate the `MaxBackersPerWinner` check at the end of an async
	/// verification. Once the solution is valid (i.e. verified), the solution backings are not
	/// useful anymore and can be cleared.
	#[pallet::storage]
	pub(crate) type QueuedSolutionBackings<T: Config> = StorageMap<
		_,
		Twox64Concat,
		PageIndex,
		BoundedVec<(T::AccountId, PartialBackings), T::MaxWinnersPerPage>,
	>;

	/// The score of the current valid solution.
	#[pallet::storage]
	type QueuedSolutionScore<T: Config> = StorageValue<_, ElectionScore>;

	/// Pointer for the storage variant (X or Y) that stores the current valid variant.
	#[pallet::storage]
	type QueuedValidVariant<T: Config> = StorageValue<_, SolutionPointer, ValueQuery>;

	/// The minimum score that each solution must have to be considered feasible.
	#[pallet::storage]
	pub(crate) type MinimumScore<T: Config> = StorageValue<_, ElectionScore>;

	/// Current status of the verification process.
	#[pallet::storage]
	pub(crate) type VerificationStatus<T: Config> = StorageValue<_, Status, ValueQuery>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub minimum_score: Option<ElectionScore>,
		pub _phantom: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			if let Some(min_score) = self.minimum_score {
				Pallet::<T>::set_minimum_score(min_score);
			}
		}
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
			match VerificationStatus::<T>::get() {
				// all submission pages have been verified. finalize.
				Status::Ongoing(current_page) if current_page.is_zero() =>
					Self::finalize_verification(),
				Status::Ongoing(current_page) =>
					Self::progress_verification(current_page.defensive_saturating_sub(1)),
				Status::Nothing => {
					// verifier should be (re)start if not enable during the signed validation and
					// there are still existing pending submissions to be verified.
					if CorePallet::<T>::current_phase().is_signed_validation() &&
						<T::SolutionDataProvider as SolutionDataProvider>::has_pending_submission(
							CorePallet::<T>::current_round(),
						) {
						let _ = <Pallet<T> as AsyncVerifier>::start().defensive();

						// TODO
						Default::default()
					} else {
						T::DbWeight::get().reads(1)
					}
				},
			}
		}

		fn integrity_test() {
			assert_eq!(T::MaxWinnersPerPage::get(), <Self as Verifier>::MaxWinnersPerPage::get());
			assert_eq!(
				T::MaxBackersPerWinner::get(),
				<Self as Verifier>::MaxBackersPerWinner::get()
			);
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}

	/// A wrapper type of the storage items related to the queued solution.
	///
	/// It manages the following storage types:
	///∂
	/// - [`QueuedSolutionX`]: variant X of the queued solution.
	/// - [`QueuedSolutionY`]: variant Y of the queued solution.
	/// - [`QueuedValidVariant`]: pointer to which variant is the currently valid.
	/// - [`QueuedSolutionScore`]: the solution score of the current valid variant.
	/// - [`QueuedSolutionBackings`].
	///
	/// Note that, as an async verification is progressing, the paged solution is kept in the
	/// invalid variant storage. A solution is considered valid only when all the single page and
	/// full solution checks have been performed based on the stored [`QueuedSolutionBackings`]. for
	/// the corresponding in-verification solution. After the solution verification is successful,
	/// the election score can be calculated and stored.
	///
	/// ### Invariants
	///
	/// - [`QueuedSolutionScore`] must be always the correct queued score of a variant corresponding
	/// to the [`QueuedValidVariant`].
	/// - [`QueuedSolution`] must always be [`Config::SolutionImprovementThreshold`] better than
	/// [`MininumScore`].
	/// - The [`QueuedSolutionBackings`] are always the backings corresponding to the *invalid*
	/// variant.
	pub struct QueuedSolution<T: Config>(PhantomData<T>);

	impl<T: Config> QueuedSolution<T> {
		fn mutate_checked<R>(mutate: impl FnOnce() -> R) -> R {
			let result = mutate();

			#[cfg(debug_assertions)]
			Self::sanity_check()
				.map_err(|err| {
					sublog!(debug, "verifier", "Queued solution sanity check failure {:?}", err);
				})
				.unwrap();

			result
		}

		/// Clear all relevant data of an invalid solution.
		///
		/// This should be called when a solution being verified is deemed infeasible.
		pub(crate) fn clear_invalid_and_backings() {
			Self::mutate_checked(|| {
				let _ = match Self::invalid() {
					SolutionPointer::X => QueuedSolutionX::<T>::clear(u32::MAX, None),
					SolutionPointer::Y => QueuedSolutionY::<T>::clear(u32::MAX, None),
				};
				let _ = QueuedSolutionBackings::<T>::clear(u32::MAX, None);
			});
		}

		/// Clear all verifier storage items.
		pub(crate) fn kill() {
			Self::mutate_checked(|| {
				let _ = QueuedSolutionX::<T>::clear(u32::MAX, None);
				let _ = QueuedSolutionY::<T>::clear(u32::MAX, None);
				QueuedValidVariant::<T>::kill();
				let _ = QueuedSolutionBackings::<T>::clear(u32::MAX, None);
				QueuedSolutionScore::<T>::kill();
			})
		}

		/// Finalize a correct solution.
		///
		/// It should be called at the end of the verification process of a valid solution to update
		/// the queued solution score and flip the invalid variant.
		pub(crate) fn finalize_solution(score: ElectionScore) {
			sublog!(
				debug,
				"verifier",
				"finalizing verification of a correct solution, replacing old score {:?} with {:?}",
				QueuedSolutionScore::<T>::get(),
				score
			);

			Self::mutate_checked(|| {
				QueuedValidVariant::<T>::mutate(|v| *v = v.other());
				QueuedSolutionScore::<T>::put(score);
			})
		}

		/// Write a single page of a valid solution into the `invalid` variant of the storage.
		///
		/// It should be called only once the page has been verified to be 100% correct.
		pub(crate) fn set_page(page: PageIndex, supports: MinerSupportsOf<T::MinerConfig>) {
			Self::mutate_checked(|| {
				let backings: BoundedVec<_, _> = supports
                    .iter()
                    .map(|(x, s)| (x.clone(), PartialBackings {total: s.total, backers: s.voters.len() as u32}))
                    .try_collect()
                    .expect("`SupportsOf` is bounded by Verifier::MaxWinnersPerPage which is ensured by an integrity test; qed.");

				QueuedSolutionBackings::<T>::insert(page, backings);

				// store the new page into the invalid variant storage type.
				match Self::invalid() {
					SolutionPointer::X => QueuedSolutionX::<T>::insert(page, supports),
					SolutionPointer::Y => QueuedSolutionY::<T>::insert(page, supports),
				}
			})
		}

		/// Computes the score and the winner count of a stored variant solution.
		///
		/// At this point we expect all the single `T::Pages` to be verified and in storage.
		pub(crate) fn compute_current_score() -> Result<(ElectionScore, u32), FeasibilityError> {
			// ensures that all the pages are complete;
			if QueuedSolutionBackings::<T>::iter_keys().count() != T::Pages::get() as usize {
				return Err(FeasibilityError::Incomplete)
			}

			let mut supports: BTreeMap<T::AccountId, PartialBackings> = Default::default();
			for (who, PartialBackings { backers, total }) in
				QueuedSolutionBackings::<T>::iter().map(|(_, backings)| backings).flatten()
			{
				let entry = supports.entry(who).or_default();
				entry.total = entry.total.saturating_add(total);
				entry.backers = entry.backers.saturating_add(backers);

				if entry.backers > T::MaxBackersPerWinner::get() {
					return Err(FeasibilityError::TooManyBackings)
				}
			}

			let winners_count = supports.len() as u32;
			let score = sp_npos_elections::evaluate_support(
				supports.into_iter().map(|(_, backings)| backings),
			);

			Ok((score, winners_count))
		}

		/// Returns the current queued score, if any.
		pub(crate) fn queued_score() -> Option<ElectionScore> {
			QueuedSolutionScore::<T>::get()
		}

		/// Returns the current *valid* paged queued solution, if any.
		pub(crate) fn get_queued_solution(
			page: PageIndex,
		) -> Option<MinerSupportsOf<T::MinerConfig>> {
			match Self::valid() {
				SolutionPointer::X => QueuedSolutionX::<T>::get(page),
				SolutionPointer::Y => QueuedSolutionY::<T>::get(page),
			}
		}

		/// Returns the pointer for the valid solution storage.
		pub(crate) fn valid() -> SolutionPointer {
			QueuedValidVariant::<T>::get()
		}

		/// Returns the pointer for the invalid solution storage.
		pub(crate) fn invalid() -> SolutionPointer {
			Self::valid().other()
		}

		/// Returns the next *valid* missing solution page while in signed and unsigned submissions
		/// are ongoing.
		pub(crate) fn next_missing_solution_page() -> Option<PageIndex> {
			match CorePallet::<T>::current_phase() {
				crate::Phase::Unsigned(_) | crate::Phase::Signed => {
					let expected_pages: BoundedVec<PageIndex, T::Pages> = BoundedVec::truncate_from(
						(CorePallet::<T>::lsp()..=CorePallet::<T>::msp()).collect::<Vec<_>>(),
					);
					let stored_pages = QueuedSolutionBackings::<T>::iter_keys().collect::<Vec<_>>();

					expected_pages
						.into_iter()
						.filter(|e| !stored_pages.contains(e))
						.collect::<Vec<_>>()
						.pop()
				},
				_ => None,
			}
		}
	}

	#[cfg(any(test, debug_assertions, feature = "runtime-benchmarks"))]
	impl<T: Config> QueuedSolution<T> {
		pub(crate) fn sanity_check() -> Result<(), &'static str> {
			Self::check_variants()?;
			Self::check_out_of_phase()
		}

		/// Invariants:
		///
		/// 1. The valid and invalid solution pointers are always different.
		/// 2. If the queued solution is in storage, the valid variant solution should be set with
		///    the expected T::Pages number of pages (unless it is unsigned solution),
		/// 3. If the queued solution is in storage, the backings storage is empty (not needed
		///    anymore as the score has been calculated and stored)
		fn check_variants() -> Result<(), &'static str> {
			ensure!(
				QueuedSolution::<T>::valid() != QueuedSolution::<T>::invalid(),
				"valid and invalid solution pointers are the same"
			);

			if QueuedSolution::<T>::queued_score().is_some() &&
				!CorePallet::<T>::current_phase().is_unsigned()
			{
				match Self::valid() {
					SolutionPointer::X => {
						ensure!(
                            QueuedSolutionX::<T>::iter_keys().count() as u32 == T::Pages::get(),
                            "solution does not exist/incomplete in valid variant with a queued score."
                        )
					},
					SolutionPointer::Y => {
						ensure!(
                            QueuedSolutionY::<T>::iter_keys().count() as u32 == T::Pages::get(),
                            "solution does not exist/incomplete in valid variant with a queued score."
                        )
					},
				}
			}

			Ok(())
		}

		/// Invariants:
		///
		/// TODO: finish invariants
		fn check_out_of_phase() -> Result<(), &'static str> {
			let async_verifier_ongoing = <Pallet<T> as AsyncVerifier>::status().is_ongoing();
			let current_phase = CorePallet::<T>::current_phase();

			// queued backings and queued solution may only exist during signed validation and
			// unsigned phases.
			if !(current_phase.is_unsigned() || current_phase.is_signed_validation()) {
				ensure!(
					QueuedSolutionBackings::<T>::iter_keys().count() == 0,
					"backings may be stored only during unsigned and signed validation phases"
				);
				ensure!(
					QueuedSolutionX::<T>::iter_keys().count() as u32 == 0,
					"nothing happening, queued solution should be empty."
				);
				ensure!(
					QueuedSolutionY::<T>::iter_keys().count() as u32 == 0,
					"nothing happening, queued solution should be empty."
				);
			}

			if !current_phase.is_signed_validation() {
				ensure!(
					!async_verifier_ongoing,
					"async verifier status should *not* be ongoing out of signed validation phase."
				);
			}

			Ok(())
		}
	}
}

impl<T: impls::pallet::Config> Pallet<T> {
	// TODO
	fn progress_verification(current_page: PageIndex) -> Weight {
		if let Some(page_solution) =
			<T::SolutionDataProvider as SolutionDataProvider>::get_paged_solution(current_page)
		{
			Self::progress_verification_inner(page_solution, current_page)
		} else {
			Self::verification_failed(VerificationResult::DataUnavailable, Some(current_page));
			Self::deposit_event(Event::<T>::SolutionDataUnavailable { page: current_page });

			<T as Config>::WeightInfo::on_initialize_ongoing_failed(
				T::MaxBackersPerWinner::get(),
				T::MaxWinnersPerPage::get(),
			)
		}
	}

	// TODO
	fn progress_verification_inner(
		page_solution: SolutionOf<T::MinerConfig>,
		current_page: PageIndex,
	) -> Weight {
		match Self::feasibility_check(page_solution, current_page) {
			Ok(supports) => {
				let backers = supports.len() as u32;
				QueuedSolution::<T>::set_page(current_page, supports);
				Self::deposit_event(Event::<T>::Verified { page: current_page, backers });

				VerificationStatus::<T>::put(Status::Ongoing(current_page));

				<T as Config>::WeightInfo::on_initialize_ongoing(
					T::MaxBackersPerWinner::get(),
					T::MaxWinnersPerPage::get(),
				)
			},

			Err(error) => {
				Self::verification_failed(VerificationResult::Rejected, Some(current_page));
				Self::deposit_event(Event::<T>::VerificationFailed { page: current_page, error });

				<T as Config>::WeightInfo::on_initialize_ongoing_finalize_failed(
					T::MaxBackersPerWinner::get(),
					T::MaxWinnersPerPage::get(),
				)
			},
		}
	}

	// TODO
	fn finalize_verification() -> Weight {
		let max_backers_winner = T::MaxBackersPerWinner::get();
		let max_winners_page = T::MaxWinnersPerPage::get();

		// last page, finalize everything. At this point, the solution data
		// provider should have a score ready for us. Otherwise, a default score
		// will be rejected and reset the ongoing verification which is the desired
		// behaviour.
		debug_assert!(<VerificationStatus<T>>::get() == Status::Ongoing(CorePallet::<T>::lsp()));
		let claimed_score = T::SolutionDataProvider::get_score().defensive_unwrap_or_default();

		match Self::finalize_verification_inner(claimed_score) {
			Ok(_) => {
				VerificationStatus::<T>::put(Status::Nothing);
				T::SolutionDataProvider::report_result(VerificationResult::Queued);

				<T as Config>::WeightInfo::on_initialize_ongoing_finalize(
					max_backers_winner,
					max_winners_page,
				)
			},
			Err(error) => {
				Self::verification_failed(VerificationResult::DataUnavailable, None);
				Self::deposit_event(Event::<T>::FinalizeVerificationFailed { error });

				<T as Config>::WeightInfo::on_initialize_ongoing_finalize_failed(
					max_backers_winner,
					max_winners_page,
				)
			},
		}
	}

	pub(crate) fn finalize_verification_inner(
		claimed_score: ElectionScore,
	) -> Result<(), FeasibilityError> {
		let outcome =
			QueuedSolution::<T>::compute_current_score().and_then(|(final_score, winner_count)| {
				let desired_targets = crate::Snapshot::<T>::desired_targets().unwrap_or_default();

				match (final_score == claimed_score, winner_count <= desired_targets) {
					(true, true) => {
						let old_score = QueuedSolution::<T>::queued_score();
						QueuedSolution::<T>::finalize_solution(final_score);
						Self::deposit_event(Event::<T>::Queued { score: final_score, old_score });

						Ok(())
					},
					(false, true) => Err(FeasibilityError::InvalidScore),
					(true, false) => Err(FeasibilityError::WrongWinnerCount),
					(false, false) => Err(FeasibilityError::InvalidScore),
				}
			});

		sublog!(debug, "verifier", "finalize verification outcome: {:?}", outcome);
		outcome
	}

	fn verification_failed(reason: VerificationResult, maybe_page: Option<PageIndex>) {
		// TODO: simplify.
		if let Some(page) = maybe_page {
			sublog!(
				debug,
				"verifier",
				"Page {} verification failed due to {:?} at {:?}.",
				page,
				reason,
				CorePallet::<T>::current_phase(),
			);
		} else {
			sublog!(
				debug,
				"verifier",
				"Verification failed due to {:?} at finalization stage of round {:?}.",
				reason,
				CorePallet::<T>::current_phase(),
			);
		}

		// restart verification in the next msp.
		//VerificationStatus::<T>::put(Status::Ongoing(CorePallet::<T>::msp()));

		VerificationStatus::<T>::put(Status::Nothing);
		QueuedSolution::<T>::clear_invalid_and_backings();
		T::SolutionDataProvider::report_result(reason);
	}

	pub(crate) fn do_verify_sync(
		partial_solution: SolutionOf<T::MinerConfig>,
		partial_score: ElectionScore,
		page: PageIndex,
	) -> Result<MinerSupportsOf<T::MinerConfig>, FeasibilityError> {
		let _ = Self::ensure_score_quality(partial_score)?;
		let supports = Self::feasibility_check(partial_solution.clone(), page)?;

		// TODO: implement fn evaluate on `BondedSupports`; remove extra clone.
		let real_score = sp_npos_elections::evaluate_support(
			supports.clone().into_iter().map(|(_, backings)| backings),
		);
		ensure!(real_score == partial_score, FeasibilityError::InvalidScore);

		// queue valid solution of single page.
		QueuedSolution::<T>::set_page(page, supports.clone());

		Ok(supports)
	}

	/// Checks if `score` improves the current queued score by `T::SolutionImprovementThreshold` and
	/// that it is higher than `MinimumScore`.
	pub fn ensure_score_quality(score: ElectionScore) -> Result<(), FeasibilityError> {
		let is_improvement = <Self as Verifier>::queued_score().map_or(true, |best_score| {
			score.strict_threshold_better(best_score, T::SolutionImprovementThreshold::get())
		});
		ensure!(is_improvement, FeasibilityError::ScoreTooLow);

		let is_greater_than_min_trusted = MinimumScore::<T>::get()
			.map_or(true, |min_score| score.strict_threshold_better(min_score, Perbill::zero()));

		ensure!(is_greater_than_min_trusted, FeasibilityError::ScoreTooLow);

		Ok(())
	}
}

#[cfg(any(test, feature = "runtime-benchmarks"))]
impl<T: Config> Pallet<T> {
	/// Forces a valid solution in storage that pass the sanity checks.
	pub(crate) fn force_valid_solution(
		page: PageIndex,
		supports: MinerSupportsOf<T::MinerConfig>,
		score: ElectionScore,
	) {
		QueuedSolution::<T>::set_page(page, supports);
		QueuedSolution::<T>::finalize_solution(score);
		VerificationStatus::<T>::put(Status::Nothing);

		QueuedSolution::<T>::sanity_check().unwrap();
	}

	/// Returns the current minimum score.
	pub(crate) fn minimum_score() -> Option<ElectionScore> {
		MinimumScore::<T>::get()
	}

	/// Returns the number backings/pages verified and stored.
	#[allow(dead_code)]
	pub(crate) fn pages_backed() -> usize {
		QueuedSolutionBackings::<T>::iter_keys().count()
	}
}

#[cfg(any(test, feature = "try-runtime"))]
impl<T: Config> Pallet<T> {
	pub(crate) fn do_try_state(
		_bn: crate::BlockNumberFor<T>,
	) -> Result<(), sp_runtime::TryRuntimeError> {
		QueuedSolution::<T>::sanity_check().map_err(|e| e.into())
	}
}

impl<T: Config + impls::pallet::Config> Verifier for Pallet<T> {
	type AccountId = T::AccountId;
	type Solution = SolutionOf<T::MinerConfig>;
	type MaxWinnersPerPage = T::MaxWinnersPerPage;
	type MaxBackersPerWinner = T::MaxBackersPerWinner;

	fn set_minimum_score(score: ElectionScore) {
		MinimumScore::<T>::put(score);
	}

	fn queued_score() -> Option<ElectionScore> {
		QueuedSolution::<T>::queued_score()
	}

	fn ensure_score_quality(claimed_score: ElectionScore) -> bool {
		Self::ensure_score_quality(claimed_score).is_ok()
	}

	fn get_queued_solution(page_index: PageIndex) -> Option<MinerSupportsOf<T::MinerConfig>> {
		QueuedSolution::<T>::get_queued_solution(page_index)
	}

	fn next_missing_solution_page() -> Option<PageIndex> {
		let next_missing = QueuedSolution::<T>::next_missing_solution_page();
		sublog!(debug, "verifier", "next missing page: {:?}", next_missing);

		next_missing
	}

	fn kill() {
		QueuedSolution::<T>::kill();
		<VerificationStatus<T>>::put(Status::Nothing);
	}

	fn verify_synchronous(
		partial_solution: Self::Solution,
		partial_score: ElectionScore,
		page: PageIndex,
	) -> Result<MinerSupportsOf<T::MinerConfig>, FeasibilityError> {
		match Self::do_verify_sync(partial_solution, partial_score, page) {
			Ok(supports) => {
				sublog!(
					trace,
					"verifier",
					"queued sync solution with score {:?} (page {:?})",
					partial_score,
					page
				);
				Self::deposit_event(Event::<T>::Verified { page, backers: supports.len() as u32 });
				Self::deposit_event(Event::<T>::Queued {
					score: partial_score,
					old_score: Self::queued_score(),
				});
				Ok(supports)
			},
			Err(err) => {
				sublog!(
					trace,
					"verifier",
					"sync verification failed with {:?} (page: {:?})",
					err,
					page
				);
				Self::deposit_event(Event::<T>::VerificationFailed { page, error: err.clone() });
				Err(err)
			},
		}
	}

	fn feasibility_check(
		solution: Self::Solution,
		page: PageIndex,
	) -> Result<MinerSupportsOf<T::MinerConfig>, FeasibilityError> {
		let targets =
			crate::Snapshot::<T>::targets().ok_or(FeasibilityError::SnapshotUnavailable)?;

		// prepare range to fetch all pages of the target and voter snapshot.
		let paged_range = 0..CorePallet::<T>::msp() + 1;

		// fetch all pages of the voter snapshot and collect them in a bounded vec.
		let all_voter_pages: BoundedVec<_, T::Pages> = paged_range
			.map(|page| {
				crate::Snapshot::<T>::voters(page).ok_or(FeasibilityError::SnapshotUnavailable)
			})
			.collect::<Result<Vec<_>, _>>()?
			.try_into()
			.expect("range was constructed from the bounded vec bounds; qed.");

		let desired_targets =
			crate::Snapshot::<T>::desired_targets().ok_or(FeasibilityError::SnapshotUnavailable)?;

		miner::Miner::<T::MinerConfig>::feasibility_check_partial(
			&all_voter_pages,
			&targets,
			solution,
			desired_targets,
			page,
		)
	}

	#[cfg(any(test, debug_assertions, feature = "runtime-benchmarks"))]
	fn minimum_score() -> Option<ElectionScore> {
		MinimumScore::<T>::get()
	}
}

impl<T: impls::pallet::Config> AsyncVerifier for Pallet<T> {
	type SolutionDataProvider = T::SolutionDataProvider;

	fn force_finalize_verification(claimed_score: ElectionScore) -> Result<(), FeasibilityError> {
		Self::finalize_verification_inner(claimed_score)
	}

	fn status() -> Status {
		VerificationStatus::<T>::get()
	}

	fn start() -> Result<(), &'static str> {
		if let Status::Nothing = Self::status() {
			let claimed_score = Self::SolutionDataProvider::get_score().unwrap_or_default();

			if Self::ensure_score_quality(claimed_score).is_err() {
				Self::deposit_event(Event::<T>::VerificationFailed {
					page: CorePallet::<T>::msp(),
					error: FeasibilityError::ScoreTooLow,
				});
				// report to the solution data provider that the page verification failed.
				Self::SolutionDataProvider::report_result(VerificationResult::Rejected);
				// despite the verification failed, this was a successful `start` operation.
				Ok(())
			} else {
				Self::deposit_event(Event::<T>::VerificationStarted { claimed_score });

				// start verifying first page.
				Pallet::<T>::progress_verification(CorePallet::<T>::msp());

				VerificationStatus::<T>::put(Status::Ongoing(CorePallet::<T>::msp()));
				Ok(())
			}
		} else {
			sublog!(warn, "verifier", "tries to start election while ongoing, ignored.");
			Err("verification ongoing")
		}
	}

	fn stop() {
		sublog!(warn, "verifier", "stop signal received. clearing everything.");
		QueuedSolution::<T>::clear_invalid_and_backings();

		// if a verification is ongoing, signal the solution rejection to the solution data
		// provider and reset the current status.
		VerificationStatus::<T>::mutate(|status| {
			if matches!(status, Status::Ongoing(_)) {
				T::SolutionDataProvider::report_result(VerificationResult::Rejected);
			};
			*status = Status::Nothing;
		});
	}

	// Sets current verifications status.
	#[cfg(any(test, feature = "runtime-benchmarks"))]
	fn set_status(status: Status) {
		VerificationStatus::<T>::put(status);
	}
}
