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

//! # Unsigned sub-pallet
//!
//! The main goal of this sub-pallet is to manage the unsigned phase submissions by an off-chain
//! worker. It implements the `offchain_worker` hook which will compute and store
//! in the off-chain cache a paged solution and try to submit it if:
//!
//! - Current phase is [`crate::Phase::Unsigned`];
//! - The score of the computed solution is better than the minimum score defined by the verifier
//! pallet and the current election score stored by the [`crate::signed::Pallet`].
//!
//! ## Sync off-chain worker

pub mod miner;
#[cfg(test)]
mod tests;

use crate::{
	unsigned::miner::{OffchainMinerError, OffchainWorkerMiner},
	verifier, Pallet as EPM, Phase, SolutionAccuracyOf, SolutionOf, Verifier,
};
use frame_election_provider_support::PageIndex;
use frame_support::{
	ensure,
	pallet_prelude::{TransactionValidity, ValidTransaction},
	traits::Get,
};
use frame_system::{offchain::SendTransactionTypes, pallet_prelude::BlockNumberFor};
use sp_npos_elections::ElectionScore;
use sp_runtime::SaturatedConversion;

// public re-exports.
pub use pallet::{
	Call, Config, Event, Pallet, __substrate_call_check, __substrate_event_check,
	__substrate_validate_unsigned_check, tt_default_parts, tt_error_token,
};

#[frame_support::pallet(dev_mode)]
pub(crate) mod pallet {

	use super::*;
	use frame_support::pallet_prelude::*;
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
	}

	#[pallet::storage]
	pub type Something<T: Config> = StorageMap<_, Twox64Concat, u32, u32>;

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if let Call::submit_page_unsigned { page, partial_score, .. } = call {
				sublog!(info, "unsigned", "validate_unsigned OK");

				ValidTransaction::with_tag_prefix("ElectionOffchainWorker")
					// priority increases propotional to the `solution.minimal_stake`.
					.priority(
						T::MinerTxPriority::get()
							.saturating_add(partial_score.minimal_stake.saturated_into()),
					)
					// deduplicates unsigned solutions since each validator should calculate at most
					// one paged solution per block.
					.and_provides(page)
					// transaction stays in the pool as long as the unsigned phase.
					.longevity(T::UnsignedPhase::get().saturated_into::<u64>())
					.propagate(false)
					.build()
			} else {
				sublog!(info, "unsigned", "validate_unsigned ERROR");
				InvalidTransaction::Call.into()
			}
		}
	}

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
			claimed_full_score: ElectionScore,
		) -> DispatchResult {
			ensure_none(origin)?;
			let error_message = "Invalid unsigned submission must produce invalid block and \
				 deprive validator from their authoring reward.";

			// Check if score is an improvement, the current phase, page index and other paged
			// solution metadata checks.
			Self::pre_dispatch_checks(page, &claimed_full_score).expect(error_message);

			// The verifier will store the paged solution, if valid.
			let _ = <T::Verifier as verifier::Verifier>::verify_synchronous(
				solution,
				partial_score,
				page,
			)
			.expect(error_message);

			// if this is the last page, request an async verification finalization which will work
			// on the queued paged solutions.
			if page == EPM::<T>::lsp() {
				<T::Verifier as verifier::AsyncVerifier>::force_finalize_async_verification(
					claimed_full_score,
				)
				.expect(error_message);
			}

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
			// get lock for the unsigned phase.
			let mut lock =
				StorageLock::<BlockAndTime<frame_system::Pallet<T>>>::with_block_deadline(
					miner::OffchainWorkerMiner::<T>::OFFCHAIN_LOCK,
					T::UnsignedPhase::get().saturated_into(),
				);

			if crate::Pallet::<T>::current_phase().is_unsigned() {
				match lock.try_lock() {
					Ok(_guard) => {
						let _ = Self::do_sync_offchain_worker(now).map_err(|e| {
							sublog!(debug, "unsigned", "offchain worker error.");
							e
						});
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
	pub fn do_sync_offchain_worker(_now: BlockNumberFor<T>) -> Result<(), OffchainMinerError> {
		let missing_solution_page = <T::Verifier as Verifier>::next_missing_solution_page();

		match (crate::Pallet::<T>::current_phase(), missing_solution_page) {
			(Phase::Unsigned(_), Some(page)) => {
				let (full_score, partial_score, partial_solution) =
					//OffchainWorkerMiner::<T>::fetch_or_mine(page).map_err(|e| {
					OffchainWorkerMiner::<T>::mine(page)?;

				// submit page only if full score improves the current queued score.
				if <T::Verifier as Verifier>::ensure_score_improves(full_score) {
					OffchainWorkerMiner::<T>::submit_paged_call(
						page,
						partial_solution,
						partial_score,
						full_score,
					)?;
				} else {
					sublog!(
						debug,
						"unsigned",
						"unsigned solution with score {:?} does not improve current queued solution; skip it.",
						full_score
					);
				}
			},
			(Phase::Export(_), _) | (Phase::Unsigned(_), None) => {
				// Unsigned phase is over or unsigned solution is no more required, clear the
				// cache.
				OffchainWorkerMiner::<T>::clear_cache();
			},
			_ => (), // nothing to do here.
		}

		Ok(())
	}

	pub(crate) fn pre_dispatch_checks(
		page: PageIndex,
		claimed_full_score: &ElectionScore,
	) -> Result<(), ()> {
		// timing and metadata checks.
		ensure!(crate::Pallet::<T>::current_phase().is_unsigned(), ());
		ensure!(page <= crate::Pallet::<T>::msp(), ());

		// full solution check.
		ensure!(<T::Verifier as Verifier>::ensure_score_improves(*claimed_full_score), ());

		Ok(())
	}
}
