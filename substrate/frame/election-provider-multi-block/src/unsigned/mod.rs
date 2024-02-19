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

use crate::PageSize;
use frame_election_provider_support::PageIndex;
use sp_std::boxed::Box;

// public re-exports.
pub use pallet::{
	Call, Config, Event, Pallet, __substrate_call_check, __substrate_event_check, tt_default_parts,
	tt_error_token,
};

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
	pub trait Config: crate::Config {
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

		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T> {}

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
			raw_solution: Box<PagedRawSolution<T>>,
			weight_witness: PageSize,
		) -> DispatchResult {
			ensure_none(origin)?;
			let error_message = "Invalid unsigned submission must produce invalid block and \
				 deprive validator from their authoring reward.";

			// Check if score is an improvement, the current phase, page index and other paged
			// solution metadata.
			// Self::pre_dispatch_checks(&raw_solution).expect(error_message);

			// Ensure the weight witness matches the paged solution provided.
			// let SolutionOrSnapshotSize { voters, targets } =
			//  Self::snapshot_metadata().expect(error_message);

			// Check paged solution.
			// let ready = feasibility_check...

			// Store newly received paged solution.
			// QueuedSolution<T>::put(...)

			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			// TODO(gpestana)
			Weight::zero()
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
