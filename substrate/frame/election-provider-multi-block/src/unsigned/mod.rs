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

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Unsigned solution submitted successfully.
		UnsignedSolutionSubmitted { at: BlockNumberFor<T> },
	}

	#[pallet::storage]
	pub type Something<T: Config> = StorageMap<_, Twox64Concat, u32, u32>;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

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
			solution: SolutionOf<T>,
			claimed_score: ElectionScore,
			page: PageIndex,
			weight_witness: PageSize,
		) -> DispatchResult {
			ensure_none(origin)?;
			let error_message = "Invalid unsigned submission must produce invalid block and \
				 deprive validator from their authoring reward.";

			//let claimed_score = raw_solution.1;
			//let page = crate::Pallet::<T>::msp();

			let supports =
				<T::Verifier as Verifier>::verify_synchronous(solution, claimed_score, page)
					.expect(error_message);

			println!("{:?}", supports);

			// Check if score is an improvement, the current phase, page index and other paged
			// solution metadata.
			//Self::pre_dispatch_checks(&raw_solution).expect(error_message);

			// Ensure the weight witness matches the paged solution provided.
			// let SolutionOrSnapshotSize { voters, targets } =
			//  Self::snapshot_metadata().expect(error_message);

			// Check paged solution.
			// let ready = feasibility_check...

			// Store newly received paged solution.
			// QueuedSolution<T>::put(...)

			Self::deposit_event(Event::UnsignedSolutionSubmitted {
				at: <frame_system::Pallet<T>>::block_number(),
			});

			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn offchain_worker(now: BlockNumberFor<T>) {
			use sp_runtime::offchain::storage_lock::{BlockAndTime, StorageLock};

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
		match crate::Pallet::<T>::current_phase() {
			Phase::Unsigned(opened) if opened == now => {
				OffchainWorkerMiner::<T>::mine_check_save_submit()?;
			},
			Phase::Unsigned(opened) if opened < now => {
				OffchainWorkerMiner::<T>::restore_or_compute_then_maybe_submit();
			},
			_ => {},
		}

		//miner::OffchainWorkerMiner::<T>::submit_call(call);

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;

	#[test]
	fn unsigned_submission_works() {
		let (mut ext, pool) = ExtBuilder::default().build_offchainify(0);
		ext.execute_with(|| {
			roll_to_with_ocw(25, Some(pool.clone()));

			for p in (0..<T as crate::Config>::Pages::get()).rev() {
				println!(
					"page: {:?}, block: {:?}, events: {:?}",
					p,
					System::block_number(),
					unsigned_events()
				);
				roll_one_with_ocw(Some(pool.clone()));
			}
		})
	}
}
