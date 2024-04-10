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

#![allow(unused)] // TODO(remove)

pub mod miner;

use crate::{
	unsigned::miner::{MinerError, OffchainWorkerMiner},
	PageSize, Phase, SolutionAccuracyOf, Verifier,
};
use frame_election_provider_support::PageIndex;
use frame_support::traits::Get;
use frame_system::{
	offchain::{SendTransactionTypes, SubmitTransaction},
	pallet_prelude::BlockNumberFor,
};
use sp_npos_elections::ElectionScore;
use sp_runtime::{SaturatedConversion, Saturating};
use sp_std::boxed::Box;

// public re-exports.
pub use pallet::{
	Call, Config, Event, Pallet, __substrate_call_check, __substrate_event_check, tt_default_parts,
	tt_error_token,
};

use self::miner::OffchainMinerError;

#[frame_support::pallet(dev_mode)]
pub(crate) mod pallet {

	use crate::{verifier, PagedRawSolution, SolutionOf};

	use super::*;
	use frame_support::pallet_prelude::{ValueQuery, *};
	use frame_system::{
		ensure_none,
		pallet_prelude::{BlockNumberFor, OriginFor},
		weights::WeightInfo,
	};

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: crate::Config + SendTransactionTypes<Call<Self>> {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The off-chain worker interval between retrying to submit a solution.
		type OffchainRepeatInterval: Get<BlockNumberFor<Self>>;

		/// The priority of the unsigned tx submitted.
		type MinerTxPriority: Get<TransactionPriority>;

		/// The solver used by the offchain worker miner.
		type OffchainSolver: frame_election_provider_support::NposSolver<
			AccountId = Self::AccountId,
			Accuracy = SolutionAccuracyOf<Self>,
		>;

		/// Maximum length of the solution that the miner is allowed to generate.
		///
		/// Solutions are trimmed to respect this.
		type MaxLength: Get<u32>;

		/// Maximum weight of the solution that the miner is allowed to generate.
		///
		/// Solutions are trimmed to respect this.
		///
		/// The weight is computed using `solution_weight`.
		type MaxWeight: Get<Weight>;

		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Unsigned solution submitted successfully.
		UnsignedSolutionSubmitted { at: BlockNumberFor<T>, page: PageIndex },
		UnsignedTrace,
	}

	#[pallet::storage]
	pub type Something<T: Config> = StorageMap<_, Twox64Concat, u32, u32>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit a paged unsigned solution.
		///
		/// The dispatch origin fo this call must be __none__.
		///
		/// This submission is checked on the fly. Moreover, this unsigned solution is only
		/// validated when submitted to the pool from the **local** node. Effectively, this means
		/// that only active validators can submit this transaction when authoring a block (similar
		/// to an inherent).
		///
		/// To prevent any incorrect solution (and thus wasted time/weight), this transaction will
		/// panic if the solution submitted by the validator is invalid in any way, effectively
		/// putting their authoring reward at risk.
		///
		/// No deposit or reward is associated with this submission.
		#[pallet::call_index(1)]
		#[pallet::weight(0)]
		pub fn submit_page_unsigned(
			origin: OriginFor<T>,
			page: PageIndex,
			solution: SolutionOf<T>,
			partial_score: ElectionScore,
		) -> DispatchResult {
			ensure_none(origin)?;
			let error_message = "Invalid unsigned submission must produce invalid block and \
				 deprive validator from their authoring reward.";

			// Check if score is an improvement, the current phase, page index and other paged
			// solution metadata checks.
			//Self::pre_dispatch_checks(&raw_solution).expect(error_message);

			// Ensure the weight witness matches the paged solution provided.
			// TODO: how to do it partially?
			// let SolutionOrSnapshotSize { voters, targets } =
			//  Self::snapshot_metadata().expect(error_message);

			// TODO: block author -> slash validator.

			// The verifier will store the paged solution, if valid.
			let _ = <T::Verifier as Verifier>::verify_synchronous(solution, partial_score, page)
				.expect(error_message);

			Self::deposit_event(Event::UnsignedSolutionSubmitted {
				at: <frame_system::Pallet<T>>::block_number(),
				page,
			});

			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn offchain_worker(now: BlockNumberFor<T>) {
			use sp_runtime::offchain::storage_lock::{BlockAndTime, StorageLock};

			Self::deposit_event(Event::UnsignedTrace);

			// get lock for the unsigned phase.
			let mut lock =
				StorageLock::<BlockAndTime<frame_system::Pallet<T>>>::with_block_deadline(
					miner::OffchainWorkerMiner::<T>::OFFCHAIN_LOCK,
					T::UnsignedPhase::get().saturated_into(),
				);

			if crate::Pallet::<T>::current_phase().is_unsigned() {
				match lock.try_lock() {
					Ok(_guard) => {
						Self::do_synchronized_offchain_worker(now);
					},
					Err(deadline) => {
						sublog!(
							debug,
							"unsigned",
							"offchain worker lock not released, deadline is {:?}",
							deadline
						);
					},
				};
			}
		}

		fn integrity_test() {
			// TODO(gpestana)
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			todo!()
		}
	}
}

impl<T: Config> Pallet<T> {
	pub fn do_synchronized_offchain_worker(
		now: BlockNumberFor<T>,
	) -> Result<(), OffchainMinerError> {
		// TODO: signed phase has submitted a solution.
		let missing_solution_page = <T::Verifier as Verifier>::next_missing_solution_page();

		Self::deposit_event(Event::UnsignedTrace);

		match (crate::Pallet::<T>::current_phase(), missing_solution_page) {
			(Phase::Unsigned(started_at), Some(page)) => {
				OffchainWorkerMiner::<T>::maybe_mine_and_submit(page)?;
			},
			_ => (), // nothing to do here.
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;
	use frame_election_provider_support::ElectionProvider;
	use frame_support::assert_ok;

	#[test]
	fn unsigned_submission_works() {
		let (mut ext, pool) = ExtBuilder::default().build_offchainify(0);
		ext.execute_with(|| {
			// election predicted at 30.
			assert_eq!(election_prediction(), 30);

			roll_to_with_ocw(25, Some(pool.clone()));

			// no solution available until the unsigned phase.
			assert!(<VerifierPallet as Verifier>::queued_score().is_none());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(2).is_none());

			// progress through unsigned phase just before the election.
			roll_to_with_ocw(29, Some(pool.clone()));

			// successful submission events for all 3 pages, as expected.
			assert_eq!(
				unsigned_events(),
				[
					Event::UnsignedSolutionSubmitted { at: 25, page: 2 },
					Event::UnsignedSolutionSubmitted { at: 26, page: 1 },
					Event::UnsignedSolutionSubmitted { at: 27, page: 0 }
				]
			);
			// now, solution exists.
			assert!(<VerifierPallet as Verifier>::queued_score().is_some());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(2).is_some());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(1).is_some());
			assert!(<VerifierPallet as Verifier>::get_queued_solution(0).is_some());

			// roll to election prediction bn.
			roll_to_with_ocw(election_prediction(), Some(pool.clone()));

			// still in unsigned phase (after unsigned submissions have been submitted and before
			// the election happened).
			assert!(current_phase().is_unsigned());

			// elect() works as expected.
			assert_ok!(<MultiPhase as ElectionProvider>::elect(2));
			assert_ok!(<MultiPhase as ElectionProvider>::elect(1));
			assert_ok!(<MultiPhase as ElectionProvider>::elect(0));

			assert_eq!(current_phase(), Phase::Off);

			// 2nd round election predicted at 60.
			assert_eq!(election_prediction(), 60);
		})
	}
}
