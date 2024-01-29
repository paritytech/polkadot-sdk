// This file is part of Substrate.

// Copyright (C) 2022 Parity Technologies (UK) Ltd.
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
//! asynchronous verification of paged solutions. Moreover, it also manages and ultimately stores
//! the best correct solution in a round, which can be requested by the election provider at the
//! time of the election.
//!
//! The paged solution data to be verified is retrieved through [`T::SolutionDataProvider`].
//!
//! ## Per-page and per-solution checks
//!
//! > TODO
//!
//! ## Queued solutions
//!
//! > TODO

// TODO(gpestana): clean up imports.
use frame_election_provider_support::{
	ElectionProvider, NposSolution, PageIndex, TryIntoBoundedSupports,
};
use frame_support::{
	ensure,
	pallet_prelude::Weight,
	traits::{Defensive, TryCollect},
};
use sp_runtime::{traits::Zero, Perbill};
use sp_std::collections::btree_map::BTreeMap;

use super::*;
use pallet::*;

use crate::{helpers, SolutionOf};

#[frame_support::pallet(dev_mode)]
pub(crate) mod pallet {
	use crate::SupportsOf;

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

		/// Maximum number of supports (i.e. winners/validators/targets) that can be represented
		/// in one page of a solution.
		type MaxWinnersPerPage: Get<u32>;

		/// Maximum number of voters that can support a single target, across ALL the solution
		/// pages. Thus, this can only be verified when processing the last solution page.
		///
		/// This limit must be set so that the memory limits of the rest of the system are
		/// respected.
		type MaxBackersPerWinner: Get<u32>;

		/// Something that can provide the solution data to the verifier.
		type SolutionDataProvider: crate::verifier::SolutionDataProvider<Solution = Self::Solution>;

		/// The weight information of this pallet.
		type WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T> {
		/// A verificaction failed at the given page.
		VerificationFailed(PageIndex, FeasibilityError),
		/// The final verifications of the `finalize_verification` failed. If this error happened,
		/// all the single pages passed the feasibility checks.
		FinalVerificationFailed(FeasibilityError),
		/// The given page has been correctly verified, with the number of backers that are part of
		/// the page.
		Verified(PageIndex, u32),
		/// A new solution with the given score has replaced the previous best solution, if any.
		Queued(ElectionScore, Option<ElectionScore>),
		/// The solution data was not available for a specific page.
		SolutionDataUnavailable(PageIndex),
	}

	/// A wrapper type of the storage items related to the queued solution.
	pub struct QueuedSolution<T: Config>(sp_std::marker::PhantomData<T>);

	impl<T: Config> QueuedSolution<T> {
		fn mutate_checked<R>(mutate: impl FnOnce() -> R) -> R {
			let r = mutate();
			#[cfg(debug_assertions)]
			assert!(Self::sanity_check().is_ok());
			r
		}

		/// Clear all relevant data of an invalid solution.
		pub(crate) fn clear_invalid_and_backings() {
			let _ = match Self::invalid() {
				SolutionPointer::X => QueuedSolutionX::<T>::clear(u32::MAX, None),
				SolutionPointer::Y => QueuedSolutionY::<T>::clear(u32::MAX, None),
			};
			let _ = QueuedSolutionBackings::<T>::clear(u32::MAX, None);
		}

		/// Clear all relevant storage items.
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
		/// It should be called at the end of the verification process of a valid solution.
		pub(crate) fn finalize_solution(score: ElectionScore) {
			sublog!(
				info,
				"verifier",
				"finalizing verification of a correct solution, replacing old score {:?} with {:?}",
				QueuedSolutionScore::<T>::get(),
				score
			);

			Self::mutate_checked(|| {
				QueuedValidVariant::<T>::mutate(|v| *v = v.other());
				QueuedSolutionScore::<T>::put(score);
				// TODO(gpestana): needs to clear the invalid variant or can we save those writes?
			})
		}

		/// Write a single page of a valid solution into the `invalid` variant of the storage.
		///
		/// It should be called only once the page has been verified to be 100% correct.
		pub(crate) fn set_page(page: PageIndex, supports: SupportsOf<Pallet<T>>) {
			Self::mutate_checked(|| {
				let backings: BoundedVec<_, _> = supports
                    .iter()
                    .map(|(x, s)| (x.clone(), PartialBackings {total: s.total, backers: s.voters.len() as u32}))
                    .try_collect()
                    .expect("`SupportsOf` is bounded by <Pallet<T> as Verifier>::MaxWinnersPerPage which is ensured by an integrity test; qed.");

				QueuedSolutionBackings::<T>::insert(page, backings);

				// store the new page into the invalid variant storage type.
				match Self::invalid() {
					SolutionPointer::X => QueuedSolutionX::<T>::insert(page, supports),
					SolutionPointer::Y => QueuedSolutionY::<T>::insert(page, supports),
				}
			})
		}

		/// Write a single page directly into the valid variant.
		pub(crate) fn force_set_single_page_valid(
			page: PageIndex,
			supports: SupportsOf<Pallet<T>>,
			score: ElectionScore,
		) {
			Self::mutate_checked(|| {
				// clear all the data of the current valid solution.
				let _ = match Self::valid() {
					SolutionPointer::X => QueuedSolutionX::<T>::clear(u32::MAX, None),
					SolutionPointer::Y => QueuedSolutionY::<T>::clear(u32::MAX, None),
				};
				QueuedSolutionScore::<T>::kill();

				// write the new single page and new score.
				let _ = match Self::valid() {
					SolutionPointer::X => QueuedSolutionX::<T>::insert(page, supports),
					SolutionPointer::Y => QueuedSolutionY::<T>::insert(page, supports),
				};
				QueuedSolutionScore::<T>::put(score);
			})
		}

		/// Computes the score and the winner count of a stored variant solution.
		/// TODO(gpestana): comments
		pub(crate) fn compute_current_score() -> Result<(ElectionScore, u32), FeasibilityError> {
			// ensures that all the pages are complete;
			// TODO(gpestana): maybe keep track of the number of pages of the variant that has been
			// stored to make this check cheaper.
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

		pub(crate) fn queued_score() -> Option<ElectionScore> {
			QueuedSolutionScore::<T>::get()
		}

		pub(crate) fn get_queued_solution(page: PageIndex) -> Option<SupportsOf<Pallet<T>>> {
			match Self::valid() {
				SolutionPointer::X => QueuedSolutionX::<T>::get(page),
				SolutionPointer::Y => QueuedSolutionY::<T>::get(page),
			}
		}

		pub(crate) fn valid() -> SolutionPointer {
			QueuedValidVariant::<T>::get()
		}

		pub(crate) fn invalid() -> SolutionPointer {
			Self::valid().other()
		}

		pub(crate) fn sanity_check() -> Result<(), &'static str> {
			// TODO(gpestana)
			Ok(())
		}
	}

	/// Supports of the solution of the variant X.
	///
	/// A potential valid or invalid solution may be stored in this variant during the round.
	#[pallet::storage]
	pub type QueuedSolutionX<T: Config> =
		StorageMap<_, Twox64Concat, PageIndex, SupportsOf<Pallet<T>>>;

	/// Supports of the solution of the variant Y.
	///
	/// A potential valid or invalid solution may be stored in this variant during the round.
	#[pallet::storage]
	pub type QueuedSolutionY<T: Config> =
		StorageMap<_, Twox64Concat, PageIndex, SupportsOf<Pallet<T>>>;

	// TODO
	#[pallet::storage]
	type QueuedSolutionBackings<T: Config> = StorageMap<
		_,
		Twox64Concat,
		PageIndex,
		BoundedVec<(T::AccountId, PartialBackings), T::MaxWinnersPerPage>,
	>;

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

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			Self::do_on_initialize()
		}

		fn integrity_test() {
			// TODO(gpestana): add more integrity tests related to queued solution et al.
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
}

impl<T: impls::pallet::Config> Verifier for Pallet<T> {
	type AccountId = T::AccountId;
	type Solution = SolutionOf<T>;
	type MaxWinnersPerPage = T::MaxWinnersPerPage;
	type MaxBackersPerWinner = T::MaxBackersPerWinner;

	fn set_minimum_score(score: ElectionScore) {
		MinimumScore::<T>::put(score);
	}

	fn queued_score() -> Option<ElectionScore> {
		QueuedSolution::<T>::queued_score()
	}

	fn ensure_score_improves(claimed_score: ElectionScore) -> bool {
		Self::ensure_score_quality(claimed_score).is_ok()
	}

	fn get_queued_solution(page_index: PageIndex) -> Option<SupportsOf<Self>> {
		QueuedSolution::<T>::get_queued_solution(page_index)
	}

	fn kill() {
		QueuedSolution::<T>::kill();
		<VerificationStatus<T>>::put(Status::Nothing);
	}

	fn verify_synchronous(
		partial_solution: Self::Solution,
		claimed_score: ElectionScore,
		page: PageIndex,
	) -> Result<SupportsOf<Self>, FeasibilityError> {
		let maybe_current_score = Self::queued_score();
		match Self::do_verify_sync(partial_solution, claimed_score, page) {
			Ok(supports) => {
				sublog!(info, "verifier", "queued sync solution with score {:?}", claimed_score);
				Self::deposit_event(Event::<T>::Verified(page, supports.len() as u32));
				Self::deposit_event(Event::<T>::Queued(claimed_score, maybe_current_score));
				Ok(supports)
			},
			Err(err) => {
				sublog!(info, "verifier", "sync verification failed with {:?}", err);
				Self::deposit_event(Event::<T>::VerificationFailed(page, err.clone()));
				Err(err)
			},
		}
	}

	fn feasibility_check(
		partial_solution: Self::Solution,
		page: PageIndex,
	) -> Result<SupportsOf<Self>, FeasibilityError> {
		Self::feasibility_check(partial_solution, page)
	}
}

impl<T: impls::pallet::Config> AsyncVerifier for Pallet<T> {
	type SolutionDataProvider = T::SolutionDataProvider;

	fn status() -> Status {
		VerificationStatus::<T>::get()
	}

	fn start() -> Result<(), &'static str> {
		if let Status::Nothing = Self::status() {
			let claimed_score = Self::SolutionDataProvider::get_score().unwrap_or_default();

			if Self::ensure_score_quality(claimed_score).is_err() {
				Self::deposit_event(Event::<T>::VerificationFailed(
					crate::Pallet::<T>::msp(),
					FeasibilityError::ScoreTooLow,
				));
				// report to the solution data provider that the page verification failed.
				T::SolutionDataProvider::report_result(VerificationResult::Rejected);
				// despite the verification failed, this was a successful `start` operation.
				Ok(())
			} else {
				VerificationStatus::<T>::put(Status::Ongoing(crate::Pallet::<T>::msp()));
				Ok(())
			}
		} else {
			sublog!(warn, "verifier", "tries to start election while ongoing, ignored.");
			Err("verification ongoing")
		}
	}

	fn stop() {
		sublog!(warn, "verifier", "stop signal received. clearing everything.");
		// TODO(gpestana): debug asserts

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
}

impl<T: impls::pallet::Config> Pallet<T> {
	fn do_on_initialize() -> Weight {
		if let Status::Ongoing(current_page) = <VerificationStatus<T>>::get() {
			let maybe_page_solution =
				<T::SolutionDataProvider as SolutionDataProvider>::get_paged_solution(current_page);

			if maybe_page_solution.is_none() {
				sublog!(
                    error,
                    "verifier",
                    "T::SolutionDataProvider failed to deliver page {}. This is an unexpected error and should not happen. Restarting election state..",
                    current_page
                );
				// reset election data and notify the `T::SolutionDataProvider`.
				QueuedSolution::<T>::clear_invalid_and_backings();
				VerificationStatus::<T>::put(Status::Nothing);
				T::SolutionDataProvider::report_result(VerificationResult::DataUnavailable);

				Self::deposit_event(Event::<T>::SolutionDataUnavailable(current_page));

				// TODO(gpestana): benchmarks.
				return Zero::zero()
			}

			let page_solution = maybe_page_solution.expect("page solution checked to exist; qed.");
			let maybe_supports = Self::feasibility_check(page_solution, current_page);

			match maybe_supports {
				Ok(supports) => {
					Self::deposit_event(Event::<T>::Verified(current_page, supports.len() as u32));
					QueuedSolution::<T>::set_page(current_page, supports);

					if current_page > crate::Pallet::<T>::lsp() {
						// election didn't finish, tick forward.
						VerificationStatus::<T>::put(Status::Ongoing(
							current_page.saturating_sub(1),
						));
					} else {
						// last page, finalize everything. At this point, the solution data
						// provider should have a score ready for us. Otherwise, a default score
						// will reset the whole election which is the desired behaviour.
						let claimed_score =
							T::SolutionDataProvider::get_score().defensive_unwrap_or_default();

						// reset the election status.
						VerificationStatus::<T>::put(Status::Nothing);

						match Self::finalize_async_verification(claimed_score) {
							Ok(_) =>
								T::SolutionDataProvider::report_result(VerificationResult::Queued),
							Err(_) => {
								T::SolutionDataProvider::report_result(
									VerificationResult::Rejected,
								);
								// kill the solution in case of error.
								QueuedSolution::<T>::clear_invalid_and_backings();
							},
						}
					}
				},
				Err(err) => {
					// the paged solution is invalid.
					Self::deposit_event(Event::<T>::VerificationFailed(current_page, err));
					VerificationStatus::<T>::put(Status::Nothing);
					QueuedSolution::<T>::clear_invalid_and_backings();
					T::SolutionDataProvider::report_result(VerificationResult::Rejected)
				},
			};
		}

		// TODO(gpestana): benchmarks.
		Zero::zero()
	}

	fn do_verify_sync(
		partial_solution: T::Solution,
		claimed_score: ElectionScore,
		page: PageIndex,
	) -> Result<SupportsOf<Self>, FeasibilityError> {
		let _ = Self::ensure_score_quality(claimed_score);
		let supports = Self::feasibility_check(partial_solution, page)?;

		let desired_targets =
			crate::Snapshot::<T>::desired_targets().ok_or(FeasibilityError::SnapshotUnavailable)?;
		ensure!(supports.len() as u32 == desired_targets, FeasibilityError::WrongWinnerCount);

		// TODO(gpestana): this clone is unecessary, remove.
		let score = sp_npos_elections::evaluate_support(
			supports.clone().into_iter().map(|(_, backings)| backings),
		);
		ensure!(score == claimed_score, FeasibilityError::InvalidScore);

		// queue valid solution of single page.
		QueuedSolution::<T>::force_set_single_page_valid(Zero::zero(), supports.clone(), score);

		Ok(supports)
	}

	fn finalize_async_verification(claimed_score: ElectionScore) -> Result<(), FeasibilityError> {
		let outcome = QueuedSolution::<T>::compute_current_score()
			.and_then(|(final_score, winner_count)| {
				let desired_targets = crate::Snapshot::<T>::desired_targets().unwrap_or_default();

				match (final_score == claimed_score, winner_count == desired_targets) {
					(true, true) => {
						QueuedSolution::<T>::finalize_solution(final_score);

						Self::deposit_event(Event::<T>::Queued(
							final_score,
							QueuedSolution::<T>::queued_score(),
						));

						Ok(())
					},
					(false, true) => Err(FeasibilityError::InvalidScore),
					(true, false) => Err(FeasibilityError::WrongWinnerCount),
					(false, false) => Err(FeasibilityError::InvalidScore),
				}
			})
			.map_err(|err| {
				sublog!(warn, "verifier", "finalizing the solution was invalid due to {:?}", err);
				Self::deposit_event(Event::<T>::VerificationFailed(Zero::zero(), err.clone()));
				err
			});

		sublog!(debug, "verifier", "finalize verification outcome: {:?}", outcome);
		outcome
	}

	fn ensure_score_quality(score: ElectionScore) -> Result<(), FeasibilityError> {
		let is_improvement = <Self as Verifier>::queued_score().map_or(true, |best_score| {
			score.strict_threshold_better(best_score, T::SolutionImprovementThreshold::get())
		});
		ensure!(is_improvement, FeasibilityError::ScoreTooLow);

		let is_greater_than_min_trusted = MinimumScore::<T>::get()
			.map_or(true, |min_score| score.strict_threshold_better(min_score, Perbill::zero()));
		ensure!(is_greater_than_min_trusted, FeasibilityError::ScoreTooLow);

		Ok(())
	}

	/// Do the full feasibility check:
	pub(crate) fn feasibility_check(
		partial_solution: SolutionOf<T>,
		page: PageIndex,
	) -> Result<SupportsOf<Self>, FeasibilityError> {
		// Read the corresponding snapshots.
		let snapshot_targets =
			crate::Snapshot::<T>::targets(page).ok_or(FeasibilityError::SnapshotUnavailable)?;
		let snapshot_voters =
			crate::Snapshot::<T>::voters(page).ok_or(FeasibilityError::SnapshotUnavailable)?;

		// ----- Start building. First, we need some closures.
		let voter_cache = helpers::generate_voter_cache::<T, _>(&snapshot_voters);
		let voter_at = helpers::voter_at_fn::<T>(&snapshot_voters);
		let voter_index = helpers::voter_index_fn_usize::<T>(&voter_cache);

		let target_cache = helpers::generate_target_cache::<T>(&snapshot_targets);
		let target_at = helpers::target_at_fn::<T>(&snapshot_targets);
		let target_index = helpers::target_index_fn_usize::<T>(&target_cache);

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
		let stake_of = helpers::stake_of_fn::<T, _>(&snapshot_voters, &voter_cache);

		// This might fail if the normalization fails. Very unlikely. See `integrity_test`.
		let staked_assignments =
			sp_npos_elections::assignment_ratio_to_staked_normalized(assignments, stake_of)
				.map_err::<FeasibilityError, _>(Into::into)?;

		let supports = sp_npos_elections::to_supports(&staked_assignments);

		// Check the maximum number of backers per winner. If this is a single-page solution, this
		// is enough to check `MaxBackersPerWinner`. Else, this is just a heuristic, and needs to be
		// checked again at the end (via `QueuedSolutionBackings`).
		ensure!(
			supports
				.iter()
				.all(|(_, s)| (s.voters.len() as u32) <= T::MaxBackersPerWinner::get()),
			FeasibilityError::TooManyBackings
		);

		// Ensure some heuristics. These conditions must hold in the **entire** support, this is
		// just a single page. But, they must hold in a single page as well.
		let desired_targets =
			crate::Snapshot::<T>::desired_targets().ok_or(FeasibilityError::SnapshotUnavailable)?;
		ensure!((supports.len() as u32) <= desired_targets, FeasibilityError::WrongWinnerCount);

		// almost-defensive-only: `MaxBackersPerWinner` is already checked. A sane value of
		// `MaxWinnersPerPage` should be more than any possible value of `desired_targets()`, which
		// is ALSO checked, so this conversion can almost never fail.
		let bounded_supports = supports
			.try_into_bounded_supports()
			.map_err(|_| FeasibilityError::WrongWinnerCount)?;
		Ok(bounded_supports)
	}
}

#[cfg(feature = "try-runtime")]
impl<T: Config> Pallet<T> {
	pub(crate) fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		Self::check_variants()
	}

	/// Invariants:
	///
	/// 1. The valid and invalid solution pointers are always different.
	fn check_variants() -> Result<(), sp_runtime::TryRuntimeError> {
		ensure!(
			QueuedSolution::<T>::valid() != QueuedSolution::<T>::invalid(),
			"valid and invalid solution pointers are the same"
		);
		Ok(())
	}
}
