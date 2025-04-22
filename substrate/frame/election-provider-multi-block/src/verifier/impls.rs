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

//! The implementation of the verifier pallet, and an implementation of [`crate::Verifier`] and
//! [`crate::AsynchronousVerifier`] for [`Pallet`].

use super::*;
use crate::{
	helpers,
	types::VoterOf,
	unsigned::miner::{MinerConfig, PageSupportsOfMiner},
	verifier::Verifier,
	SolutionOf,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_election_provider_support::{
	ExtendedBalance, NposSolution, PageIndex, TryFromOtherBounds,
};
use frame_support::{
	ensure,
	pallet_prelude::{ValueQuery, *},
	traits::{defensive_prelude::*, Defensive, Get},
};
use frame_system::pallet_prelude::*;
use pallet::*;
use sp_npos_elections::{evaluate_support, ElectionScore};
use sp_runtime::Perbill;
use sp_std::{collections::btree_map::BTreeMap, prelude::*};

pub(crate) type SupportsOfVerifier<V> = frame_election_provider_support::BoundedSupports<
	<V as Verifier>::AccountId,
	<V as Verifier>::MaxWinnersPerPage,
	<V as Verifier>::MaxBackersPerWinner,
>;

pub(crate) type VerifierWeightsOf<T> = <T as Config>::WeightInfo;

/// The status of this pallet.
#[derive(
	Encode, Decode, scale_info::TypeInfo, Clone, Copy, MaxEncodedLen, Debug, PartialEq, Eq,
)]
pub enum Status {
	/// A verification is ongoing, and the next page that will be verified is indicated with the
	/// inner value.
	Ongoing(PageIndex),
	/// Nothing is happening.
	Nothing,
}

impl Default for Status {
	fn default() -> Self {
		Self::Nothing
	}
}

/// Enum to point to the valid variant of the [`QueuedSolution`].
#[derive(Encode, Decode, scale_info::TypeInfo, Clone, Copy, MaxEncodedLen)]
enum ValidSolution {
	X,
	Y,
}

impl Default for ValidSolution {
	fn default() -> Self {
		ValidSolution::Y
	}
}

impl ValidSolution {
	fn other(&self) -> Self {
		match *self {
			ValidSolution::X => ValidSolution::Y,
			ValidSolution::Y => ValidSolution::X,
		}
	}
}

/// A simple newtype that represents the partial backing of a winner. It only stores the total
/// backing, and the sum of backings, as opposed to a [`sp_npos_elections::Support`] that also
/// stores all of the backers' individual contribution.
///
/// This is mainly here to allow us to implement `Backings` for it.
#[derive(Default, Encode, Decode, MaxEncodedLen, scale_info::TypeInfo)]
pub struct PartialBackings {
	/// The total backing of this particular winner.
	pub total: ExtendedBalance,
	/// The number of backers.
	pub backers: u32,
}

impl sp_npos_elections::Backings for PartialBackings {
	fn total(&self) -> ExtendedBalance {
		self.total
	}
}

#[frame_support::pallet]
pub(crate) mod pallet {
	use super::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: crate::Config {
		/// The minimum amount of improvement to the solution score that defines a solution as
		/// "better".
		#[pallet::constant]
		type SolutionImprovementThreshold: Get<Perbill>;

		/// Maximum number of backers, per winner, among all pages of an election.
		///
		/// This can only be checked at the very final step of verification.
		///
		/// NOTE: at the moment, we don't check this, and it is in place for future compatibility.
		#[pallet::constant]
		type MaxBackersPerWinnerFinal: Get<u32>;

		/// Maximum number of backers, per winner, per page.
		#[pallet::constant]
		type MaxBackersPerWinner: Get<u32>;

		/// Maximum number of supports (aka. winners/validators/targets) that can be represented in
		/// a page of results.
		#[pallet::constant]
		type MaxWinnersPerPage: Get<u32>;

		/// Something that can provide the solution data to the verifier.
		///
		/// In reality, this will be fulfilled by the signed phase.
		type SolutionDataProvider: crate::verifier::SolutionDataProvider<
			Solution = SolutionOf<Self::MinerConfig>,
		>;

		/// The weight information of this pallet.
		type WeightInfo: super::WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T> {
		/// The verification data was unavailable and it could not continue.
		VerificationDataUnavailable,
		/// A verification failed at the given page.
		///
		/// NOTE: if the index is 0, then this could mean either the feasibility of the last page
		/// was wrong, or the final checks of `finalize_verification` failed.
		VerificationFailed(PageIndex, FeasibilityError),
		/// The given page of a solution has been verified, with the given number of winners being
		/// found in it.
		Verified(PageIndex, u32),
		/// A solution with the given score has replaced our current best solution.
		Queued(ElectionScore, Option<ElectionScore>),
	}

	/// A wrapper interface for the storage items related to the queued solution.
	///
	/// It wraps the following:
	///
	/// - `QueuedSolutionX`
	/// - `QueuedSolutionY`
	/// - `QueuedValidVariant`
	/// - `QueuedSolutionScore`
	/// - `QueuedSolutionBackings`
	///
	/// As the name suggests, `QueuedValidVariant` points to the correct variant between
	/// `QueuedSolutionX` and `QueuedSolutionY`. In the context of this pallet, by VALID and
	/// INVALID variant we mean either of these two storage items, based on the value of
	/// `QueuedValidVariant`.
	///
	/// ### Invariants
	///
	/// The following conditions must be met at all times for this group of storage items to be
	/// sane.
	///
	/// - `QueuedSolutionScore` must always be correct. In other words, it should correctly be the
	///   score of `QueuedValidVariant`.
	/// - `QueuedSolutionScore` must always be [`Config::SolutionImprovementThreshold`] better than
	///   `MinimumScore`.
	/// - The number of existing keys in `QueuedSolutionBackings` must always match that of the
	///   INVALID variant.
	///
	/// Moreover, the following conditions must be met when this pallet is in [`Status::Nothing`],
	/// meaning that no ongoing asynchronous verification is ongoing.
	///
	/// - No keys should exist in the INVALID variant.
	/// 	- This implies that no data should exist in `QueuedSolutionBackings`.
	///
	/// > Note that some keys *might* exist in the queued variant, but since partial solutions
	/// > (having less than `T::Pages` pages) are in principle correct, we cannot assert anything on
	/// > the number of keys in the VALID variant. In fact, an empty solution with score of [0, 0,
	/// > 0] can also be correct.
	///
	/// No additional conditions must be met when the pallet is in [`Status::Ongoing`]. The number
	/// of pages in
	pub struct QueuedSolution<T: Config>(sp_std::marker::PhantomData<T>);
	impl<T: Config> QueuedSolution<T> {
		/// Private helper for mutating the storage group.
		fn mutate_checked<R>(mutate: impl FnOnce() -> R) -> R {
			let r = mutate();
			#[cfg(debug_assertions)]
			assert!(Self::sanity_check().is_ok());
			r
		}

		/// Finalize a correct solution.
		///
		/// Should be called at the end of a verification process, once we are sure that a certain
		/// solution is 100% correct.
		///
		/// It stores its score, flips the pointer to it being the current best one, and clears all
		/// the backings and the invalid variant. (note: in principle, we can skip clearing the
		/// backings here)
		pub(crate) fn finalize_correct(score: ElectionScore) {
			sublog!(
				info,
				"verifier",
				"finalizing verification a correct solution, replacing old score {:?} with {:?}",
				QueuedSolutionScore::<T>::get(),
				score
			);

			Self::mutate_checked(|| {
				QueuedValidVariant::<T>::mutate(|v| *v = v.other());
				QueuedSolutionScore::<T>::put(score);

				// Clear what was previously the valid variant. Also clears the partial backings.
				Self::clear_invalid_and_backings_unchecked();
			});
		}

		/// Clear all relevant information of an invalid solution.
		///
		/// Should be called at any step, if we encounter an issue which makes the solution
		/// infeasible.
		pub(crate) fn clear_invalid_and_backings() {
			Self::mutate_checked(Self::clear_invalid_and_backings_unchecked)
		}

		/// Same as [`clear_invalid_and_backings`], but without any checks for the integrity of the
		/// storage item group.
		pub(crate) fn clear_invalid_and_backings_unchecked() {
			// clear is safe as we delete at most `Pages` entries, and `Pages` is bounded.
			match Self::invalid() {
				ValidSolution::X => clear_paged_map!(QueuedSolutionX::<T>),
				ValidSolution::Y => clear_paged_map!(QueuedSolutionY::<T>),
			};
			clear_paged_map!(QueuedSolutionBackings::<T>);
		}

		/// Write a single page of a valid solution into the `invalid` variant of the storage.
		///
		/// This should only be called once we are sure that this particular page is 100% correct.
		///
		/// This is called after *a page* has been validated, but the entire solution is not yet
		/// known to be valid. At this stage, we write to the invalid variant. Once all pages are
		/// verified, a call to [`finalize_correct`] will seal the correct pages and flip the
		/// invalid/valid variants.
		pub(crate) fn set_invalid_page(page: PageIndex, supports: SupportsOfVerifier<Pallet<T>>) {
			use frame_support::traits::TryCollect;
			Self::mutate_checked(|| {
				let backings: BoundedVec<_, _> = supports
					.iter()
					.map(|(x, s)| (x.clone(), PartialBackings { total: s.total, backers: s.voters.len() as u32 } ))
					.try_collect()
					.expect("`SupportsOfVerifier` is bounded by <Pallet<T> as Verifier>::MaxWinnersPerPage, which is assured to be the same as `T::MaxWinnersPerPage` in an integrity test");
				QueuedSolutionBackings::<T>::insert(page, backings);

				match Self::invalid() {
					ValidSolution::X => QueuedSolutionX::<T>::insert(page, supports),
					ValidSolution::Y => QueuedSolutionY::<T>::insert(page, supports),
				}
			})
		}

		/// Write a single page to the valid variant directly.
		///
		/// This is not the normal flow of writing, and the solution is not checked.
		///
		/// This is only useful to override the valid solution with a single (likely backup)
		/// solution.
		pub(crate) fn force_set_single_page_valid(
			page: PageIndex,
			supports: SupportsOfVerifier<Pallet<T>>,
			score: ElectionScore,
		) {
			Self::mutate_checked(|| {
				// clear everything about valid solutions.
				match Self::valid() {
					ValidSolution::X => clear_paged_map!(QueuedSolutionX::<T>),
					ValidSolution::Y => clear_paged_map!(QueuedSolutionY::<T>),
				};
				QueuedSolutionScore::<T>::kill();

				// write a single new page.
				match Self::valid() {
					ValidSolution::X => QueuedSolutionX::<T>::insert(page, supports),
					ValidSolution::Y => QueuedSolutionY::<T>::insert(page, supports),
				}

				// write the score.
				QueuedSolutionScore::<T>::put(score);
			})
		}

		pub(crate) fn force_set_multi_page_valid(
			pages: Vec<PageIndex>,
			supports: Vec<SupportsOfVerifier<Pallet<T>>>,
			score: ElectionScore,
		) {
			debug_assert_eq!(pages.len(), supports.len());
			// queue it in our valid queue
			Self::mutate_checked(|| {
				// clear everything about valid solutions.
				match Self::valid() {
					ValidSolution::X => clear_paged_map!(QueuedSolutionX::<T>),
					ValidSolution::Y => clear_paged_map!(QueuedSolutionY::<T>),
				};
				QueuedSolutionScore::<T>::kill();

				// store the valid pages
				for (support, page) in supports.into_iter().zip(pages.iter()) {
					match Self::valid() {
						ValidSolution::X => QueuedSolutionX::<T>::insert(page, support),
						ValidSolution::Y => QueuedSolutionY::<T>::insert(page, support),
					}
				}
				QueuedSolutionScore::<T>::put(score);
			});
		}

		/// Clear all storage items.
		///
		/// Should only be called once everything is done.
		pub(crate) fn kill() {
			Self::mutate_checked(|| {
				clear_paged_map!(QueuedSolutionX::<T>);
				clear_paged_map!(QueuedSolutionY::<T>);
				QueuedValidVariant::<T>::kill();
				clear_paged_map!(QueuedSolutionBackings::<T>);
				QueuedSolutionScore::<T>::kill();
			})
		}

		// -- non-mutating methods.

		/// Return the `score` and `winner_count` of verifying solution.
		///
		/// Assumes that all the corresponding pages of `QueuedSolutionBackings` exist, then it
		/// computes the final score of the solution that is currently at the end of its
		/// verification process.
		///
		/// This solution corresponds to whatever is stored in the INVALID variant of
		/// `QueuedSolution`. Recall that the score of this solution is not yet verified, so it
		/// should never become `valid`.
		pub(crate) fn compute_invalid_score() -> Result<(ElectionScore, u32), FeasibilityError> {
			// ensure that this is only called when all pages are verified individually.
			if QueuedSolutionBackings::<T>::iter_keys().count() != T::Pages::get() as usize {
				return Err(FeasibilityError::Incomplete)
			}

			let mut total_supports: BTreeMap<T::AccountId, PartialBackings> = Default::default();
			for (who, PartialBackings { backers, total }) in
				QueuedSolutionBackings::<T>::iter().flat_map(|(_, pb)| pb)
			{
				let entry = total_supports.entry(who).or_default();
				entry.total = entry.total.saturating_add(total);
				entry.backers = entry.backers.saturating_add(backers);

				if entry.backers > T::MaxBackersPerWinnerFinal::get() {
					return Err(FeasibilityError::FailedToBoundSupport)
				}
			}

			let winner_count = total_supports.len() as u32;
			let score = evaluate_support(total_supports.into_values());

			Ok((score, winner_count))
		}

		/// The score of the current best solution, if any.
		pub(crate) fn queued_score() -> Option<ElectionScore> {
			QueuedSolutionScore::<T>::get()
		}

		/// Get a page of the current queued (aka valid) solution.
		pub(crate) fn get_queued_solution_page(
			page: PageIndex,
		) -> Option<SupportsOfVerifier<Pallet<T>>> {
			match Self::valid() {
				ValidSolution::X => QueuedSolutionX::<T>::get(page),
				ValidSolution::Y => QueuedSolutionY::<T>::get(page),
			}
		}

		fn valid() -> ValidSolution {
			QueuedValidVariant::<T>::get()
		}

		fn invalid() -> ValidSolution {
			Self::valid().other()
		}
	}

	#[allow(unused)]
	#[cfg(any(test, feature = "runtime-benchmarks", feature = "try-runtime", debug_assertions))]
	impl<T: Config> QueuedSolution<T> {
		pub(crate) fn valid_iter(
		) -> impl Iterator<Item = (PageIndex, SupportsOfVerifier<Pallet<T>>)> {
			match Self::valid() {
				ValidSolution::X => QueuedSolutionX::<T>::iter(),
				ValidSolution::Y => QueuedSolutionY::<T>::iter(),
			}
		}

		pub(crate) fn invalid_iter(
		) -> impl Iterator<Item = (PageIndex, SupportsOfVerifier<Pallet<T>>)> {
			match Self::invalid() {
				ValidSolution::X => QueuedSolutionX::<T>::iter(),
				ValidSolution::Y => QueuedSolutionY::<T>::iter(),
			}
		}

		pub(crate) fn get_valid_page(page: PageIndex) -> Option<SupportsOfVerifier<Pallet<T>>> {
			match Self::valid() {
				ValidSolution::X => QueuedSolutionX::<T>::get(page),
				ValidSolution::Y => QueuedSolutionY::<T>::get(page),
			}
		}

		pub(crate) fn backing_iter() -> impl Iterator<
			Item = (PageIndex, BoundedVec<(T::AccountId, PartialBackings), T::MaxWinnersPerPage>),
		> {
			QueuedSolutionBackings::<T>::iter()
		}

		/// Ensure that all the storage items managed by this struct are in `kill` state, meaning
		/// that in the expect state after an election is OVER.
		pub(crate) fn assert_killed() {
			use frame_support::assert_storage_noop;
			assert_storage_noop!(Self::kill());
		}

		/// Ensure this storage item group is in correct state.
		pub(crate) fn sanity_check() -> Result<(), sp_runtime::DispatchError> {
			// score is correct and better than min-score.
			ensure!(
				Pallet::<T>::minimum_score()
					.zip(Self::queued_score())
					.map_or(true, |(min_score, score)| score
						.strict_threshold_better(min_score, Perbill::zero())),
				"queued solution has weak score (min-score)"
			);

			if let Some(queued_score) = Self::queued_score() {
				let mut backing_map: BTreeMap<T::AccountId, PartialBackings> = BTreeMap::new();
				Self::valid_iter()
					.flat_map(|(_, supports)| supports)
					.for_each(|(who, support)| {
						let entry = backing_map.entry(who).or_default();
						entry.total = entry.total.saturating_add(support.total);
					});
				let real_score = evaluate_support(backing_map.into_values());
				ensure!(real_score == queued_score, "queued solution has wrong score");
			} else {
				assert!(Self::valid_iter().count() == 0, "nothing should be stored if no score");
			}

			// The number of existing keys in `QueuedSolutionBackings` must always match that of
			// the INVALID variant.
			ensure!(
				QueuedSolutionBackings::<T>::iter().count() == Self::invalid_iter().count(),
				"incorrect number of backings pages",
			);

			if let Status::Nothing = StatusStorage::<T>::get() {
				ensure!(Self::invalid_iter().count() == 0, "dangling data in invalid variant");
			}

			Ok(())
		}
	}

	// -- private storage items, managed by `QueuedSolution`.

	/// The `X` variant of the current queued solution. Might be the valid one or not.
	///
	/// The two variants of this storage item is to avoid the need of copying. Recall that once a
	/// `VerifyingSolution` is being processed, it needs to write its partial supports *somewhere*.
	/// Writing theses supports on top of a *good* queued supports is wrong, since we might bail.
	/// Writing them to a bugger and copying at the ned is slightly better, but expensive. This flag
	/// system is best of both worlds.
	#[pallet::storage]
	type QueuedSolutionX<T: Config> =
		StorageMap<_, Twox64Concat, PageIndex, SupportsOfVerifier<Pallet<T>>>;

	/// The `Y` variant of the current queued solution. Might be the valid one or not.
	#[pallet::storage]
	type QueuedSolutionY<T: Config> =
		StorageMap<_, Twox64Concat, PageIndex, SupportsOfVerifier<Pallet<T>>>;
	/// Pointer to the variant of [`QueuedSolutionX`] or [`QueuedSolutionY`] that is currently
	/// valid.

	#[pallet::storage]
	type QueuedValidVariant<T: Config> = StorageValue<_, ValidSolution, ValueQuery>;

	/// The `(amount, count)` of backings, divided per page.
	///
	/// This is stored because in the last block of verification we need them to compute the score,
	/// and check `MaxBackersPerWinnerFinal`.
	///
	/// This can only ever live for the invalid variant of the solution. Once it is valid, we don't
	/// need this information anymore; the score is already computed once in
	/// [`QueuedSolutionScore`], and the backing counts are checked.
	#[pallet::storage]
	type QueuedSolutionBackings<T: Config> = StorageMap<
		_,
		Twox64Concat,
		PageIndex,
		BoundedVec<(T::AccountId, PartialBackings), T::MaxWinnersPerPage>,
	>;

	/// The score of the valid variant of [`QueuedSolution`].
	///
	/// This only ever lives for the `valid` variant.
	#[pallet::storage]
	type QueuedSolutionScore<T: Config> = StorageValue<_, ElectionScore>;

	// -- ^^ private storage items, managed by `QueuedSolution`.

	/// The minimum score that each solution must attain in order to be considered feasible.
	#[pallet::storage]
	#[pallet::getter(fn minimum_score)]
	pub(crate) type MinimumScore<T: Config> = StorageValue<_, ElectionScore>;

	/// Storage item for [`Status`].
	#[pallet::storage]
	#[pallet::getter(fn status_storage)]
	pub(crate) type StatusStorage<T: Config> = StorageValue<_, Status, ValueQuery>;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			// ensure that we have funneled some of our type parameters EXACTLY as-is to the
			// verifier trait interface we implement.
			assert_eq!(T::MaxWinnersPerPage::get(), <Self as Verifier>::MaxWinnersPerPage::get());
			assert_eq!(
				T::MaxBackersPerWinner::get(),
				<Self as Verifier>::MaxBackersPerWinner::get()
			);
			assert!(T::MaxBackersPerWinner::get() <= T::MaxBackersPerWinnerFinal::get());
		}

		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			Self::do_on_initialize()
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_now: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state(_now)
		}
	}
}

impl<T: Config> Pallet<T> {
	fn do_on_initialize() -> Weight {
		if let Status::Ongoing(current_page) = Self::status_storage() {
			let maybe_page_solution =
				<T::SolutionDataProvider as SolutionDataProvider>::get_page(current_page);

			if maybe_page_solution.as_ref().is_none() {
				// the data provider has zilch, revert to a clean state, waiting for a new `start`.
				sublog!(
					error,
					"verifier",
					"T::SolutionDataProvider failed to deliver page {}. This is an unexpected error.",
					current_page,
				);

				QueuedSolution::<T>::clear_invalid_and_backings();
				StatusStorage::<T>::put(Status::Nothing);
				T::SolutionDataProvider::report_result(VerificationResult::DataUnavailable);

				Self::deposit_event(Event::<T>::VerificationDataUnavailable);
				// weight is a bit overestimate.
				let wasted_pages = T::Pages::get().saturating_sub(current_page);
				return VerifierWeightsOf::<T>::on_initialize_invalid_non_terminal(wasted_pages);
			}

			let page_solution = maybe_page_solution.expect("Option checked to not be None; qed");
			let maybe_supports = Self::feasibility_check_page_inner(page_solution, current_page);

			sublog!(
				debug,
				"verifier",
				"verified page {} of a solution, outcome = {:?}",
				current_page,
				maybe_supports.as_ref().map(|s| s.len())
			);

			match maybe_supports {
				Ok(supports) => {
					Self::deposit_event(Event::<T>::Verified(current_page, supports.len() as u32));
					QueuedSolution::<T>::set_invalid_page(current_page, supports);

					if current_page > crate::Pallet::<T>::lsp() {
						// not last page, just tick forward.
						StatusStorage::<T>::put(Status::Ongoing(current_page.saturating_sub(1)));
						VerifierWeightsOf::<T>::on_initialize_valid_non_terminal()
					} else {
						// last page, finalize everything. Solution data provider must always have a
						// score for us at this point. Not much point in reporting a result, we just
						// assume default score, which will almost certainly fail and cause a proper
						// cleanup of the pallet, which is what we want anyways.
						let claimed_score =
							T::SolutionDataProvider::get_score().defensive_unwrap_or_default();

						// in both cases of the following match, we are not back to the nothing
						// state.
						StatusStorage::<T>::put(Status::Nothing);

						match Self::finalize_async_verification(claimed_score) {
							Ok(_) => {
								T::SolutionDataProvider::report_result(VerificationResult::Queued);
								VerifierWeightsOf::<T>::on_initialize_valid_terminal()
							},
							Err(_) => {
								T::SolutionDataProvider::report_result(
									VerificationResult::Rejected,
								);
								// In case of any of the errors, kill the solution.
								QueuedSolution::<T>::clear_invalid_and_backings();
								VerifierWeightsOf::<T>::on_initialize_invalid_terminal()
							},
						}
					}
				},
				Err(err) => {
					// the page solution was invalid.
					Self::deposit_event(Event::<T>::VerificationFailed(current_page, err));
					StatusStorage::<T>::put(Status::Nothing);
					QueuedSolution::<T>::clear_invalid_and_backings();
					T::SolutionDataProvider::report_result(VerificationResult::Rejected);
					let wasted_pages = T::Pages::get().saturating_sub(current_page);
					VerifierWeightsOf::<T>::on_initialize_invalid_non_terminal(wasted_pages)
				},
			}
		} else {
			T::DbWeight::get().reads(1)
		}
	}

	fn do_verify_synchronous_multi(
		partial_solutions: Vec<SolutionOf<T::MinerConfig>>,
		solution_pages: Vec<PageIndex>,
		claimed_score: ElectionScore,
	) -> Result<(), (PageIndex, FeasibilityError)> {
		let first_page = solution_pages.first().cloned().unwrap_or_default();
		let last_page = solution_pages.last().cloned().unwrap_or_default();
		// first, ensure this score will be good enough, even if valid..
		let _ = Self::ensure_score_quality(claimed_score).map_err(|fe| (first_page, fe))?;
		ensure!(
			partial_solutions.len() == solution_pages.len(),
			(first_page, FeasibilityError::Incomplete)
		);

		// verify each page, and amalgamate into a final support.
		let mut backings =
			sp_std::collections::btree_map::BTreeMap::<T::AccountId, PartialBackings>::new();
		let mut linked_supports = Vec::with_capacity(partial_solutions.len());

		for (solution_page, page) in partial_solutions.into_iter().zip(solution_pages.iter()) {
			let page_supports = Self::feasibility_check_page_inner(solution_page, *page)
				.map_err(|fe| (*page, fe))?;

			linked_supports.push(page_supports.clone());
			let support_len = page_supports.len() as u32;
			for (who, support) in page_supports.into_iter() {
				let entry = backings.entry(who).or_default();
				entry.total = entry.total.saturating_add(support.total);
				// Note we assume snapshots are always disjoint, and therefore we can easily extend
				// here.
				entry.backers = entry.backers.saturating_add(support.voters.len() as u32);
				if entry.backers > T::MaxBackersPerWinnerFinal::get() {
					return Err((*page, FeasibilityError::FailedToBoundSupport))
				}
			}

			Self::deposit_event(Event::<T>::Verified(*page, support_len));
		}

		// then check that the number of winners was exactly enough..
		let desired_targets = crate::Snapshot::<T>::desired_targets()
			.ok_or(FeasibilityError::SnapshotUnavailable)
			.map_err(|fe| (last_page, fe))?;
		ensure!(
			backings.len() as u32 == desired_targets,
			(last_page, FeasibilityError::WrongWinnerCount)
		);

		// then check the score was truth..
		let truth_score = evaluate_support(backings.into_values());
		ensure!(truth_score == claimed_score, (last_page, FeasibilityError::InvalidScore));

		let maybe_current_score = QueuedSolution::<T>::queued_score();

		// then store it.
		sublog!(
			info,
			"verifier",
			"queued sync solution with score {:?} for pages {:?}",
			truth_score,
			solution_pages
		);
		QueuedSolution::<T>::force_set_multi_page_valid(
			solution_pages,
			linked_supports,
			truth_score,
		);
		Self::deposit_event(Event::<T>::Queued(truth_score, maybe_current_score));

		Ok(())
	}

	/// Finalize an asynchronous verification. Checks the final score for correctness, and ensures
	/// that it matches all of the criteria.
	///
	/// This should only be called when all pages of an async verification are done.
	///
	/// Returns:
	/// - `Ok()` if everything is okay, at which point the valid variant of the queued solution will
	/// be updated. Returns
	/// - `Err(Feasibility)` if any of the last verification steps fail.
	fn finalize_async_verification(claimed_score: ElectionScore) -> Result<(), FeasibilityError> {
		let outcome = QueuedSolution::<T>::compute_invalid_score()
			.and_then(|(final_score, winner_count)| {
				let desired_targets = crate::Snapshot::<T>::desired_targets().unwrap();
				// claimed_score checked prior in seal_unverified_solution
				match (final_score == claimed_score, winner_count == desired_targets) {
					(true, true) => {
						// all good, finalize this solution
						// NOTE: must be before the call to `finalize_correct`.
						Self::deposit_event(Event::<T>::Queued(
							final_score,
							QueuedSolution::<T>::queued_score(), /* the previous score, now
							                                      * ejected. */
						));
						QueuedSolution::<T>::finalize_correct(final_score);
						Ok(())
					},
					(false, true) => Err(FeasibilityError::InvalidScore),
					(true, false) => Err(FeasibilityError::WrongWinnerCount),
					(false, false) => Err(FeasibilityError::InvalidScore),
				}
			})
			.map_err(|err| {
				sublog!(warn, "verifier", "Finalizing solution was invalid due to {:?}.", err);
				// and deposit an event about it.
				Self::deposit_event(Event::<T>::VerificationFailed(0, err.clone()));
				err
			});
		sublog!(debug, "verifier", "finalize verification outcome: {:?}", outcome);
		outcome
	}

	/// Ensure that the given score is:
	///
	/// - better than the queued solution, if one exists.
	/// - greater than the minimum untrusted score.
	pub(crate) fn ensure_score_quality(score: ElectionScore) -> Result<(), FeasibilityError> {
		let is_improvement = <Self as Verifier>::queued_score().map_or(true, |best_score| {
			score.strict_threshold_better(best_score, T::SolutionImprovementThreshold::get())
		});
		ensure!(is_improvement, FeasibilityError::ScoreTooLow);

		let is_greater_than_min_untrusted = Self::minimum_score()
			.map_or(true, |min_score| score.strict_threshold_better(min_score, Perbill::zero()));
		ensure!(is_greater_than_min_untrusted, FeasibilityError::ScoreTooLow);

		Ok(())
	}

	/// Do the full feasibility check:
	///
	/// - check all edges.
	/// - checks `MaxBackersPerWinner` to be respected IN THIS PAGE.
	/// - checks the number of winners to be less than or equal to `DesiredTargets` IN THIS PAGE
	///   ONLY.
	pub(super) fn feasibility_check_page_inner(
		partial_solution: SolutionOf<T::MinerConfig>,
		page: PageIndex,
	) -> Result<SupportsOfVerifier<Self>, FeasibilityError> {
		// Read the corresponding snapshots.
		let snapshot_targets =
			crate::Snapshot::<T>::targets().ok_or(FeasibilityError::SnapshotUnavailable)?;
		let snapshot_voters =
			crate::Snapshot::<T>::voters(page).ok_or(FeasibilityError::SnapshotUnavailable)?;
		let desired_targets =
			crate::Snapshot::<T>::desired_targets().ok_or(FeasibilityError::SnapshotUnavailable)?;

		feasibility_check_page_inner_with_snapshot::<T::MinerConfig>(
			partial_solution,
			&snapshot_voters,
			&snapshot_targets,
			desired_targets,
		)
		.and_then(|miner_supports| {
			SupportsOfVerifier::<Self>::try_from_other_bounds(miner_supports)
				.defensive_map_err(|_| FeasibilityError::FailedToBoundSupport)
		})
	}

	#[cfg(any(test, feature = "runtime-benchmarks", feature = "try-runtime"))]
	pub(crate) fn do_try_state(_now: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
		QueuedSolution::<T>::sanity_check()
	}
}

/// Same as `feasibility_check_page_inner`, but with a snapshot.
///
/// This is exported as a standalone function, relying on `MinerConfig` rather than `Config` so that
/// it can be used in any offchain miner.
pub fn feasibility_check_page_inner_with_snapshot<T: MinerConfig>(
	partial_solution: SolutionOf<T>,
	snapshot_voters: &BoundedVec<VoterOf<T>, T::VoterSnapshotPerBlock>,
	snapshot_targets: &BoundedVec<T::AccountId, T::TargetSnapshotPerBlock>,
	desired_targets: u32,
) -> Result<PageSupportsOfMiner<T>, FeasibilityError> {
	// ----- Start building. First, we need some closures.
	let cache = helpers::generate_voter_cache::<T, _>(snapshot_voters);
	let voter_at = helpers::voter_at_fn::<T>(snapshot_voters);
	let target_at = helpers::target_at_fn::<T>(snapshot_targets);
	let voter_index = helpers::voter_index_fn_usize::<T>(&cache);

	// Then convert solution -> assignment. This will fail if any of the indices are
	// gibberish.
	let assignments = partial_solution
		.into_assignment(voter_at, target_at)
		.map_err::<FeasibilityError, _>(Into::into)?;

	// Ensure that assignments are all correct.
	let _ = assignments
		.iter()
		.map(|ref assignment| {
			// Check that assignment.who is actually a voter (defensive-only). NOTE: while
			// using the index map from `voter_index` is better than a blind linear search,
			// this *still* has room for optimization. Note that we had the index when we
			// did `solution -> assignment` and we lost it. Ideal is to keep the index
			// around.

			// Defensive-only: must exist in the snapshot.
			let snapshot_index =
				voter_index(&assignment.who).ok_or(FeasibilityError::InvalidVoter)?;
			// Defensive-only: index comes from the snapshot, must exist.
			let (_voter, _stake, targets) =
				snapshot_voters.get(snapshot_index).ok_or(FeasibilityError::InvalidVoter)?;
			debug_assert!(*_voter == assignment.who);

			// Check that all of the targets are valid based on the snapshot.
			if assignment.distribution.iter().any(|(t, _)| !targets.contains(t)) {
				return Err(FeasibilityError::InvalidVote)
			}
			Ok(())
		})
		.collect::<Result<(), FeasibilityError>>()?;

	// ----- Start building support. First, we need one more closure.
	let stake_of = helpers::stake_of_fn::<T, _>(&snapshot_voters, &cache);

	// This might fail if the normalization fails. Very unlikely. See `integrity_test`.
	let staked_assignments =
		sp_npos_elections::assignment_ratio_to_staked_normalized(assignments, stake_of)
			.map_err::<FeasibilityError, _>(Into::into)?;

	let supports = sp_npos_elections::to_supports(&staked_assignments);

	// Ensure some heuristics. These conditions must hold in the **entire** support, this is
	// just a single page. But, they must hold in a single page as well.
	ensure!((supports.len() as u32) <= desired_targets, FeasibilityError::WrongWinnerCount);

	// almost-defensive-only: `MaxBackersPerWinner` is already checked. A sane value of
	// `MaxWinnersPerPage` should be more than any possible value of `desired_targets()`, which
	// is ALSO checked, so this conversion can almost never fail.
	let bounded_supports =
		supports.try_into().map_err(|_| FeasibilityError::FailedToBoundSupport)?;
	Ok(bounded_supports)
}

impl<T: Config> Verifier for Pallet<T> {
	type AccountId = T::AccountId;
	type Solution = SolutionOf<T::MinerConfig>;
	type MaxBackersPerWinner = T::MaxBackersPerWinner;
	type MaxWinnersPerPage = T::MaxWinnersPerPage;
	type MaxBackersPerWinnerFinal = T::MaxBackersPerWinnerFinal;

	fn set_minimum_score(score: ElectionScore) {
		MinimumScore::<T>::put(score);
	}

	fn ensure_claimed_score_improves(claimed_score: ElectionScore) -> bool {
		Self::ensure_score_quality(claimed_score).is_ok()
	}

	fn queued_score() -> Option<ElectionScore> {
		QueuedSolution::<T>::queued_score()
	}

	fn kill() {
		QueuedSolution::<T>::kill();
		<StatusStorage<T>>::put(Status::Nothing);
	}

	fn get_queued_solution_page(page: PageIndex) -> Option<SupportsOfVerifier<Self>> {
		QueuedSolution::<T>::get_queued_solution_page(page)
	}

	fn verify_synchronous_multi(
		partial_solutions: Vec<Self::Solution>,
		solution_pages: Vec<PageIndex>,
		claimed_score: ElectionScore,
	) -> Result<(), FeasibilityError> {
		Self::do_verify_synchronous_multi(partial_solutions, solution_pages, claimed_score).map_err(
			|(page, fe)| {
				sublog!(
					warn,
					"verifier",
					"sync verification of page {:?} failed due to {:?}.",
					page,
					fe
				);
				Self::deposit_event(Event::<T>::VerificationFailed(page, fe.clone()));
				fe
			},
		)
	}

	fn force_set_single_page_valid(
		partial_supports: SupportsOfVerifier<Self>,
		page: PageIndex,
		score: ElectionScore,
	) {
		Self::deposit_event(Event::<T>::Queued(score, QueuedSolution::<T>::queued_score()));
		QueuedSolution::<T>::force_set_single_page_valid(page, partial_supports, score);
	}
}

impl<T: Config> AsynchronousVerifier for Pallet<T> {
	type SolutionDataProvider = T::SolutionDataProvider;

	fn status() -> Status {
		Pallet::<T>::status_storage()
	}

	fn start() -> Result<(), &'static str> {
		sublog!(info, "verifier", "start signal received.");
		if let Status::Nothing = Self::status() {
			let claimed_score = Self::SolutionDataProvider::get_score().unwrap_or_default();
			if Self::ensure_score_quality(claimed_score).is_err() {
				// don't do anything, report back that this solution was garbage.
				Self::deposit_event(Event::<T>::VerificationFailed(
					crate::Pallet::<T>::msp(),
					FeasibilityError::ScoreTooLow,
				));
				T::SolutionDataProvider::report_result(VerificationResult::Rejected);
				// Despite being an instant-reject, this was a successful `start` operation.
				Ok(())
			} else {
				// This solution is good enough to win, we start verifying it in the next block.
				StatusStorage::<T>::put(Status::Ongoing(crate::Pallet::<T>::msp()));
				Ok(())
			}
		} else {
			sublog!(warn, "verifier", "start signal received while busy. This will be ignored.");
			Err("verification ongoing")
		}
	}

	fn stop() {
		sublog!(warn, "verifier", "stop signal received. clearing everything.");

		// we clear any ongoing solution's no been verified in any case, although this should only
		// exist if we were doing something.
		#[cfg(debug_assertions)]
		assert!(
			!matches!(StatusStorage::<T>::get(), Status::Ongoing(_)) ||
				(matches!(StatusStorage::<T>::get(), Status::Ongoing(_)) &&
					QueuedSolution::<T>::invalid_iter().count() > 0)
		);
		QueuedSolution::<T>::clear_invalid_and_backings_unchecked();

		// we also mutate the status back to doing nothing.
		StatusStorage::<T>::mutate(|old| {
			if matches!(old, Status::Ongoing(_)) {
				T::SolutionDataProvider::report_result(VerificationResult::Rejected)
			}
			*old = Status::Nothing;
		});
	}
}
