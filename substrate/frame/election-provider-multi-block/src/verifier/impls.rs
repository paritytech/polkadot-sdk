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

// TODO(gpestana): clean up imports.
use frame_election_provider_support::PageIndex;
use frame_support::{ensure, pallet_prelude::Weight};
use sp_npos_elections::ExtendedBalance;
use sp_runtime::Perbill;

use super::*;
use pallet::*;

use crate::SolutionOf;

#[frame_support::pallet]
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

		/// Maximum number of voters that a solution can support, across ALL the solution pages.
		/// Thus, this can only be verified when processing the last solution page.
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
		/// the apage.
		Verified(PageIndex, u32),
		/// A new solution with the given score has replaced the previous best solution, if any.
		Queued(ElectionScore, Option<ElectionScore>),
	}

	/// A wrapper type of the storage items related to the queued solution.
	///
	/// NOTE: All storage reads and mutations to the queued solution must be performed through this
	/// type to ensure data integrity.
	/// TODO(gpestana): finish type documentation
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
				ValidSolutionPointer::X => QueuedSolutionX::<T>::clear(u32::MAX, None),
				ValidSolutionPointer::Y => QueuedSolutionY::<T>::clear(u32::MAX, None),
			};
			let _ = QueuedSolutionBackings::<T>::clear(u32::MAX, None);
		}

        /// Clear all relevant storage items.
		pub(crate) fn kill() {
			Self::mutate_checked(|| {
				// TODO(gpestana): should we set a reasonable clear limit here or can we assume
				// that u32::MAX is safe?
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
                    .expect("`SupportsOf` is bounded by <Pallet<T> as Verifier>::MaxWinnersPerPage which is ensured by an integrity test");
                QueuedSolutionBackings::<T>::insert(page, backings);

                // store the new page into the invalid variant storage type.
                match Self::invalid() {
                    ValidSolutionPointer::X => QueuedSolutionX::<>::insert(page, supports),
                    ValidSolutionPointer::Y => QueuedSolutionY::<>::insert(page, supports),
                }
            })
        }

		pub(crate) fn queued_score() -> Option<ElectionScore> {
			QueuedSolutionScore::<T>::get()
		}

		pub(crate) fn get_queued_solution(page: PageIndex) -> Option<SupportsOf<Pallet<T>>> {
			match Self::valid() {
				ValidSolutionPointer::X => QueuedSolutionX::<T>::get(page),
				ValidSolutionPointer::Y => QueuedSolutionY::<T>::get(page),
			}
		}

		pub(crate) fn valid() -> ValidSolutionPointer {
			QueuedValidVariant::<T>::get()
		}

		pub(crate) fn invalid() -> ValidSolutionPointer {
			Self::valid().other()
		}

		pub(crate) fn sanity_check() -> Result<(), &'static str> {
			// TODO(gpestana)
			todo!()
		}
	}

	// TODO
	#[pallet::storage]
	pub type QueuedSolutionX<T: Config> =
		StorageMap<_, Twox64Concat, PageIndex, SupportsOf<Pallet<T>>>;

	// TODO
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

	// TODO
	#[pallet::storage]
	type QueuedValidVariant<T: Config> = StorageValue<_, ValidSolutionPointer, ValueQuery>;

	/// The minimum score that each solution must have to be considered feasible.
	/// TODO(gpestana): should probably be part of the params pallet
	#[pallet::storage]
	pub(crate) type MinimumScore<T: Config> = StorageValue<_, ElectionScore>;

	/// Current status of the verification process.
	#[pallet::storage]
	pub(crate) type StatusStorage<T: Config> = StorageValue<_, Status, ValueQuery>;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

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
		<StatusStorage<T>>::put(Status::Nothing);
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
}

impl<T: impls::pallet::Config> AsyncVerifier for Pallet<T> {
	type SolutionDataProvider = T::SolutionDataProvider;

	fn status() -> Status {
		StatusStorage::<T>::get()
	}

	fn start() -> Result<(), &'static str> {
		todo!()
	}

	fn stop() {
		sublog!(warn, "verifier", "stop signal received. clearing everything.");
		// TODO(gpestana): debug asserts

		QueuedSolution::<T>::clear_invalid_and_backings();

		// if a verification is ongoing, signal the solution rejection to the solution data
		// provider and reset the current status.
		StatusStorage::<T>::mutate(|status| {
			if matches!(status, Status::Ongoing(_)) {
				T::SolutionDataProvider::report_result(VerificationResult::Rejected);
			};
			*status = Status::Nothing;
		});
	}
}

/// A type that represents a partial backing of a winner. It does not contain the
/// [`sp_npos_election::Supports`].
#[derive(Default, Encode, Decode, MaxEncodedLen, scale_info::TypeInfo)]
pub struct PartialBackings {
	/// Total backing of a particular winner.
	total: ExtendedBalance,
	/// Number of backers.
	backers: u32,
}

impl PartialBackings {
	fn total(&self) -> ExtendedBalance {
		self.total
	}
}

impl<T: impls::pallet::Config> Pallet<T> {
	fn do_on_initialize() -> Weight {
		todo!()
	}

	fn do_verify_sync(
		partial_solution: T::Solution,
		claimed_score: ElectionScore,
		page: PageIndex,
	) -> Result<SupportsOf<Self>, FeasibilityError> {
		todo!()
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
}
