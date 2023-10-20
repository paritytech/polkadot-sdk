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

//! # Multi phase, multi block, offchain election provider pallet.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_election_provider_support::{
	ElectionDataProvider, ElectionProvider, ElectionProviderBase, PageIndex,
};
use frame_support::{
	traits::{Defensive, Get},
	DebugNoBound,
};
use frame_system::pallet_prelude::BlockNumberFor;

#[macro_use]
pub mod helpers;
#[cfg(test)]
mod mock;

const LOG_PREFIX: &'static str = "runtime::multiblock-election";

pub mod types;
pub use pallet::*;
pub use types::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		pallet_prelude::{ValueQuery, *},
		sp_runtime::{traits::Zero, Saturating},
	};
	use frame_system::pallet_prelude::{BlockNumberFor, *};

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>
			+ TryInto<Event<Self>>;

		/// Duration of the signed phase;
		#[pallet::constant]
		type SignedPhase: Get<BlockNumberFor<Self>>;

		/// Duration of the unsigned phase;
		#[pallet::constant]
		type UnsignedPhase: Get<BlockNumberFor<Self>>;

		/// Duration of the signed validation phase.
		///
		/// The duration of this phase SHOULD NOT be less than `T::Pages` and there is no point in
		/// it being more than the maximum number of pages per submission.
		#[pallet::constant]
		type SignedValidationPhase: Get<BlockNumberFor<Self>>;

		/// The number of blocks that the election should be ready before the election deadline.
		#[pallet::constant]
		type Lookhaead: Get<BlockNumberFor<Self>>;

		/// The number of pages.
		///
		/// A solution may contain at MOST this many pages.
		#[pallet::constant]
		type Pages: Get<PageIndex>;

		/// Something that will provide the election data.
		type DataProvider: ElectionDataProvider<
			AccountId = Self::AccountId,
			BlockNumber = BlockNumberFor<Self>,
		>;
	}

	/// Current phase.
	#[pallet::storage]
	pub type CurrentPhase<T: Config> = StorageValue<_, Phase<BlockNumberFor<T>>, ValueQuery>;

	/// Current round
	#[pallet::storage]
	pub type Round<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// There was a phase transition in a given round.
		PhaseTransitioned {
			from: Phase<BlockNumberFor<T>>,
			to: Phase<BlockNumberFor<T>>,
			round: u32,
		},
	}

	#[pallet::error]
	pub enum Error<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(now: BlockNumberFor<T>) -> Weight {
			//  ---------- ---------- ---------- ---------- ----------
			// |         |          |          |          |          |
			// Off       Snapshot   Signed     SigValid   Unsigned   elect()

			let unsigned_deadline = T::UnsignedPhase::get();

			let signed_validation_deadline =
				T::SignedValidationPhase::get().saturating_add(unsigned_deadline);

			let signed_deadline = T::SignedPhase::get().saturating_add(signed_validation_deadline);
			let snapshot_deadline = signed_deadline.saturating_add(T::Pages::get().into());

			let next_election = T::DataProvider::next_election_prediction(now)
				.saturating_sub(T::Lookhaead::get())
				.max(now);

			let remaining_blocks = next_election - now;
			let current_phase = <CurrentPhase<T>>::get();

			log!(
				trace,
				"current phase {:?}, next election {:?}, remaining: {:?}, deadlines: [unsigned {:?} signed_validation {:?}, signed {:?}, snapshot {:?}]",
				current_phase,
				next_election,
				remaining_blocks,
				unsigned_deadline,
				signed_validation_deadline,
				signed_deadline,
				snapshot_deadline,
			);

			match current_phase {
				// start snapshot.
				Phase::Off
					if remaining_blocks <= snapshot_deadline &&
						remaining_blocks > signed_deadline =>
				{
					let remaining_pages = Self::msp();

					Self::create_targets_snapshot(remaining_pages).unwrap(); // TODO(gpestana): unwrap
					Self::create_voters_snapshot(remaining_pages).unwrap(); // TODO(gpestana): unwrap

					Self::phase_transition(Phase::Snapshot(remaining_pages));
					Weight::default() // weights
				},

				// continue snapshot.
				Phase::Snapshot(x) if x > 0 => {
					let remaining_pages = x.saturating_sub(1);
					Self::create_targets_snapshot(remaining_pages).unwrap(); // TODO(gpestana): unwrap
					Self::create_voters_snapshot(remaining_pages).unwrap(); // TODO(gpestana): unwrap

					Self::phase_transition(Phase::Snapshot(remaining_pages));
					Weight::default() // weights
				},

				// start signed phase. The `signed` pallet will take further actions now.
				Phase::Snapshot(0)
					if remaining_blocks <= signed_deadline &&
						remaining_blocks > signed_validation_deadline =>
				{
					Self::phase_transition(Phase::Signed);
					Weight::default()
				},

				// start signed validation. The `signed` pallet will take further actions now.
				Phase::Signed
					if remaining_blocks <= signed_validation_deadline &&
						remaining_blocks > unsigned_deadline =>
				{
					Self::phase_transition(Phase::SignedValidation(now));
					Weight::default()
				},

				// start unsigned phase. The `unsigned` pallet will take further actions now.
				Phase::Signed | Phase::SignedValidation(_) | Phase::Snapshot(0)
					if remaining_blocks <= unsigned_deadline && remaining_blocks > Zero::zero() =>
				{
					Self::phase_transition(Phase::Unsigned(now));
					Weight::default() // weights
				},

				_ => Weight::default(), // TODO(gpestana): T::WeightInfo::on_initialize_nothing()
			}
		}

		fn integrity_test() {
			// the signed validator phase must not be less than the number of pages of a
			// submission.
			assert!(
				T::SignedValidationPhase::get() <= T::Pages::get().into(),
				"signed validaton phase ({}) should not be less than the number of pages per submission ({})",
				T::SignedValidationPhase::get(),
				T::Pages::get(),
			);
		}
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);
}

impl<T: Config> Pallet<T> {
	/// Phase transition helper.
	pub(crate) fn phase_transition(to: Phase<BlockNumberFor<T>>) {
		log!(info, "starting phase {:?}, round {}", to, <Round<T>>::get());
		Self::deposit_event(Event::PhaseTransitioned {
			from: <CurrentPhase<T>>::get(),
			to,
			round: <Round<T>>::get(),
		});
		<CurrentPhase<T>>::put(to);
	}

	/// Return the most significant page of the snapshot.
	///
	/// Based on the contract with `ElectionDataProvider`, tis is the first page to be filled.
	fn msp() -> PageIndex {
		T::Pages::get().checked_sub(1).defensive_unwrap_or_default()
	}

	fn create_targets_snapshot(remaining_pages: u32) -> Result<u32, ElectionError> {
		Ok(0)
	}

	fn create_voters_snapshot(remaining_pages: u32) -> Result<u32, ElectionError> {
		Ok(0)
	}
}

#[cfg(test)]
mod phase_transition {
	use super::{Event, *};
	use crate::mock::*;

	//TODO(gpestana): add snapshot verification once it's ready.

	#[test]
	fn single_page() {
		//  ----------      ----------   --------------   -----------
		//            |  |            |                |             |
		//    Snapshot Signed  SignedValidation    Unsigned       elect()
		ExtBuilder::default()
            .pages(1)
            .signed_phase(3)
            .validate_signed_phase(1)
            .unsigned_phase(5)
            .lookahead(0)
            .build_and_execute(|| {
                assert_eq!(System::block_number(), 0);
                assert_eq!(Pages::get(), 1);
                assert_eq!(<Round<T>>::get(), 0);
                assert_eq!(<CurrentPhase<T>>::get(), Phase::Off);

			    let next_election = <<Runtime as Config>::DataProvider as ElectionDataProvider>::next_election_prediction(
                    System::block_number()
                );
                assert_eq!(next_election, 30);

                // representing the blocknumber when the phase transition happens.
                let expected_unsigned = next_election - UnsignedPhase::get();
                let expected_validate = expected_unsigned - SignedValidationPhase::get();
                let expected_signed = expected_validate - SignedPhase::get();
                let expected_snapshot = expected_signed - Pages::get() as BlockNumber;

                // tests transition phase boundaries and does snapshot sanity checks.
                roll_to(expected_snapshot);
                assert_eq!(<CurrentPhase<T>>::get(), Phase::Off);

                roll_to(expected_snapshot + 1);
                assert_eq!(<CurrentPhase<T>>::get(), Phase::Snapshot(Pages::get() - 1));

                roll_to(expected_signed);
                assert_eq!(<CurrentPhase<T>>::get(), Phase::Snapshot(0));

                roll_to(expected_signed + 1);
                assert_eq!(<CurrentPhase<T>>::get(), Phase::Signed);

                roll_to(expected_validate);
                assert_eq!(<CurrentPhase<T>>::get(), Phase::Signed);

                roll_to(expected_validate + 1);
                let start_validate = System::block_number();
                assert_eq!(<CurrentPhase<T>>::get(), Phase::SignedValidation(start_validate));

                roll_to(expected_unsigned);
                assert_eq!(<CurrentPhase<T>>::get(), Phase::SignedValidation(start_validate));

                roll_to(expected_unsigned + 1);
                let start_unsigned = System::block_number();
                assert_eq!(<CurrentPhase<T>>::get(), Phase::Unsigned(start_unsigned));

                // elect() will be called at any time after `next_election`.
                roll_to(next_election + 5);
                assert_eq!(<CurrentPhase<T>>::get(), Phase::Unsigned(start_unsigned));

                //MultiPhase::elect();
		})
	}

	#[test]
	fn multi_page() {
		ExtBuilder::default().build_and_execute(|| {
			assert!(true);
		})
	}
}

#[cfg(test)]
mod snapshot {
	use super::*;
	use crate::mock::*;

	fn targets_snapshot_works() {}
	fn voters_snapshot_works() {}
}
