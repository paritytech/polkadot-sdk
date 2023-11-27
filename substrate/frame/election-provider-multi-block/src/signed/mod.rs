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
use frame_support::traits::fungible::{
	hold::Balanced as FnBalanced, Credit, Inspect as FnInspect, MutateHold as FnMutateHold,
};

use sp_npos_elections::ElectionScore;

use crate::{
	types::AccountIdOf,
	verifier::{SolutionDataProvider, VerificationResult},
	PageIndex,
};

// public re-exports.
pub use pallet::{
	Call, Config, Error, Event, HoldReason, Pallet, __substrate_call_check,
	__substrate_event_check, tt_default_parts, tt_error_token,
};

// Alias for the pallet's balance type.
type BalanceOf<T> = <<T as Config>::Currency as FnInspect<AccountIdOf<T>>>::Balance;
// Alias for the pallet's hold credit type.
pub type CreditOf<T> = Credit<AccountIdOf<T>, <T as Config>::Currency>;

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

	/// A reason for this pallet placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// The funds are held as deposit for submitting a signed solution.
		ElectionSubmission,
	}

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

		/// Something that calculates the signed base deposit based on the signed submissions size.
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
	/// te signed phase, keyed by round.
	#[pallet::storage]
	type SortedScores<T: Config> = StorageMap<
		_,
		Twox64Concat,
		u32,
		BoundedVec<(T::AccountId, ElectionScore), T::MaxSubmissions>,
		ValueQuery,
	>;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

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
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit a score commitment for a solution in the current round.
		#[pallet::call_index(1)]
		#[pallet::weight(0)]
		pub fn register(origin: OriginFor<T>, claimed_score: ElectionScore) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(
				crate::Pallet::<T>::current_phase().is_signed(),
				Error::<T>::NotAcceptingSubmissions
			);

			// 1. check for duplicate register
			// 2. do deposit reserve
			// 3. add submission commitment to storage (sorting implemented by the underlying ds)

			Self::deposit_event(Event::<T>::Registered {
				round: crate::Pallet::<T>::current_round(),
				who,
				claimed_score,
			});
			Ok(())
		}

		/// Submit a page for a solution.
		#[pallet::call_index(2)]
		#[pallet::weight(0)]
		pub fn submit_page(
			origin: OriginFor<T>,
			page: PageIndex,
			_maybe_solution: Option<T::Solution>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			ensure!(
				crate::Pallet::<T>::current_phase().is_signed(),
				Error::<T>::NotAcceptingSubmissions
			);

			// TODO(gpestana): allow anyone to submit a paged solution or only allow the leader to
			// do so? how about having a phase that determines which leader should add a paged
			// solution and keep going until we get a final successful solution?
			// Note: for security reasons, we have to ensure that ALL submitters "space" to submit
			// their pages and be verified.

			Self::deposit_event(Event::<T>::PageStored {
				round: crate::Pallet::<T>::current_round(),
				who,
				page,
			});

			Ok(())
		}

		/// Unregister a submission.
		#[pallet::call_index(3)]
		#[pallet::weight(0)]
		pub fn bail(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			ensure!(
				crate::Pallet::<T>::current_phase().is_signed(),
				Error::<T>::NotAcceptingSubmissions
			);

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
			if crate::Pallet::<T>::current_phase().is_signed_validation_open_at(now) {
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

	fn get_paged_solution(_page: PageIndex) -> Option<Self::Solution> {
		todo!()
	}

	fn get_score() -> Option<ElectionScore> {
		todo!()
	}

	fn report_result(_result: VerificationResult) {
		todo!()
	}
}
