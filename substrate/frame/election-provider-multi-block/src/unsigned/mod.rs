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
//! During the unsigned phase, multiple block builders will collaborate to submit the full
//! solution, one page per block.
//!
//! ## Sync/Async off-chain worker
//!
//! The unsigned phase relies on a mix of sync and async checks to ensure that the paged unsigned
//! submissions (and final solution) are correct, namely:
//!
//! - Synchronous checks: each block builder will compute the *full* election solution. However,
//!   only one page
//! is verified through the [Verifier::verify_synchronous] and submitted through the
//! [`Call::submit_page_unsigned`] callable as an inherent.
//! - Asynchronous checks: once all pages are submitted, the [`Call::submit_page_unsigned`] will
//!   call [`verifier::AsyncVerifier::force_finalize_verification`] to ensure that the full solution
//! submitted by all the block builders is good.
//!
//! In sum, each submitted page is verified using the synchronous verification implemented by the
//! verifier pallet (i.e. [`verifier::Verifier::verify_synchronous`]). The pages are submitted by
//! order from [`crate::Pallet::msp`] down to [`crate::Pallet::lsp`]. After successfully submitting
//! the last page, the [`verifier::AsyncVerifier::force_finalize_verification`], which will perform
//! the last feasibility checks over the full stored solution.
//!
//! At each block of the unsigned phase, the block builder running the node with the off-chain
//! worker enabled will compute a solution based on the round's snapshot. The solution is pagified
//! and stored in the local cache.
//!
//! The off-chain miner will *always* compute a new solution regardless of whether there
//! is a queued solution for the current era. The solution will be added to the storage through the
//! inherent [`Call::submit_page_unsigned`] only if the computed (total) solution score is strictly
//! better than the current queued solution.

pub mod miner;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(test)]
mod tests;

use crate::{
	unsigned::{
		miner::{OffchainMinerError, OffchainWorkerMiner},
		weights::WeightInfo,
	},
	verifier, Phase, SolutionOf, Verifier,
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
	__substrate_validate_unsigned_check, tt_default_parts, tt_default_parts_v2, tt_error_token,
};

#[frame_support::pallet(dev_mode)]
pub(crate) mod pallet {

	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::{
		ensure_none,
		pallet_prelude::{BlockNumberFor, OriginFor},
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

		/// The weights for this pallet.
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

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if let Call::submit_page_unsigned { page, partial_score, .. } = call {
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
		#[pallet::weight(<T as Config>::WeightInfo::submit_page_unsigned(
			<T::Verifier as verifier::Verifier>::MaxBackersPerWinner::get(),
			<T::Verifier as verifier::Verifier>::MaxWinnersPerPage::get(),
		))]
		pub fn submit_page_unsigned(
			origin: OriginFor<T>,
			page: PageIndex,
			solution: SolutionOf<T::MinerConfig>,
			partial_score: ElectionScore,
			claimed_full_score: ElectionScore,
		) -> DispatchResult {
			ensure_none(origin)?;
			let error_message = "Invalid unsigned submission must produce invalid block and \
				 deprive validator from their authoring reward.";

			sublog!(
				info,
				"unsigned",
				"submitting page {:?} with partial score {:?}",
				page,
				partial_score
			);

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

			// if all pages have been submitted, request an async verification finalization which
			// will work on the queued paged solutions.
			if <T::Verifier as Verifier>::next_missing_solution_page().is_none() {
				<T::Verifier as verifier::AsyncVerifier>::force_finalize_verification(
					claimed_full_score,
				)
				.expect(error_message);
				sublog!(info, "unsigned", "validate_unsigned last page verify OK");
			} else {
				sublog!(info, "unsigned", "submit_page_unsigned: page {:?} submitted", page);
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
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			if crate::Pallet::<T>::current_phase() == Phase::Off {
				T::DbWeight::get().reads_writes(1, 1)
			} else {
				Default::default()
			}
		}

		/// The off-chain worker implementation
		///
		/// The off-chain worker for this pallet will run IFF:
		///
		/// - It can obtain the off-chain worker lock;
		/// - The current block is part of the unsigned phase;
		fn offchain_worker(now: BlockNumberFor<T>) {
			use sp_runtime::offchain::storage_lock::{BlockAndTime, StorageLock};

			let mut lock =
				StorageLock::<BlockAndTime<frame_system::Pallet<T>>>::with_block_deadline(
					miner::OffchainWorkerMiner::<T>::OFFCHAIN_LOCK,
					T::UnsignedPhase::get().saturated_into(),
				);

			if crate::Pallet::<T>::current_phase().is_unsigned() {
				match lock.try_lock() {
					Ok(_guard) => {
						sublog!(info, "unsigned", "obtained offchain lock at {:?}", now);
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
			// TODO
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			todo!()
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Perform the off-chain worker workload.
	///
	/// If the current block is part of the unsigned phase and there are missing solution pages:
	///
	/// 1. Compute or restore a mined solution;
	/// 2. Pagify the solution;
	/// 3. Calculate the partial score for the page to submit;
	/// 4. Verify if the *total* solution is strictly better than the current queued solution or
	///    better than the minimum score, of no queued solution exists.
	/// 5. Submits the paged solution as an inherent through the [`Call::submit_page_unsigned`]
	///    callable.
	pub fn do_sync_offchain_worker(_now: BlockNumberFor<T>) -> Result<(), OffchainMinerError> {
		let missing_solution_page = <T::Verifier as Verifier>::next_missing_solution_page();

		match (crate::Pallet::<T>::current_phase(), missing_solution_page) {
			(Phase::Unsigned(_), Some(page)) => {
				let (full_score, partial_score, partial_solution) =
					OffchainWorkerMiner::<T>::fetch_or_mine(page).map_err(|err| {
						sublog!(error, "unsigned", "OCW mine error: {:?}", err);
						err
					})?;

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

	/// Ihnerent pre-dispatch checks.
	pub(crate) fn pre_dispatch_checks(
		page: PageIndex,
		claimed_full_score: &ElectionScore,
	) -> Result<(), ()> {
		// timing and metadata checks.
		ensure!(crate::Pallet::<T>::current_phase().is_unsigned(), ());
		ensure!(page <= crate::Pallet::<T>::msp(), ());

		// full solution score check.
		ensure!(<T::Verifier as Verifier>::ensure_score_improves(*claimed_full_score), ());

		Ok(())
	}
}
