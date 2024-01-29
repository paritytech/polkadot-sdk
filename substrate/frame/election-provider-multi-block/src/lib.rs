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

//! # Multi-phase, multi-block election provider pallet.
//!
//! This pallet manages the NPoS election across its different phases, with the ability to accept
//! both on-chain and off-chain solutions. The off-chain solutions may be submitted as a signed or
//! unsigned transaction. Crucially, supports paginated, multi-block elections. The goal of
//! supporting paged elections is to scale the elections linearly with the number of blocks
//! allocated to the election.
//!
//! **PoV-friendly**: The work performed in a block by this pallet must be measurable and the time
//! and computation required per block must be deterministic with regards to the number of voters,
//! targets and any other configurations that are set apriori. These assumptions make this pallet
//! suitable to run on a parachain.
//!
//! ## Pallet organization and sub-pallets
//!
//! This pallet consists of a `core` pallet and multiple sub-pallets. Conceptually, the pallets have
//! a hierarquical relationship, where the `core` pallet sets and manages shared state between all
//! pallets. Its "child" pallets can not depend on the `core` pallet and iteract with it through
//! trait interfaces:
//!
//!```ignore
//! 	pallet-core
//! 		|
//! 		|
//! 		- pallet-verifier
//! 		|
//! 		- pallet-signed
//! 		|
//! 		- pallet-unsigned
//! ```
//!
//! Each sub-pallet has a specific set of tasks and implement one or more interfaces to expose their
//! functionality to the core pallet:
//! - The [`verifier`] pallet provides an implementation of the [`verifier::Verifier`] trait, which
//!   exposes the functionality to verify NPoS solutions in a multi-block context. In addition, it
//!   implements [`verifier::VerifierAsync`] which verifies multi-paged solution asynchronously.
//! - The [`signed`] pallet implements the signed phase, where off-chain entities commit to and
//!   submit their election solutions. This pallet implements the
//!   [`verifier::SolutionDataProvider`], which is used by the [`verifier`] pallet to fetch solution
//!   data.
//! - The [`unsigned`] pallet implements the unsigned phase, where block authors can calculate and
//!   submit through inherent paged solutions. This pallet also implements the
//!   [`verifier::SolutionDataProvider`] interface.
//!
//! ### Pallet ordering
//!
//! Due to the assumptions of when the `on_initialize` hook must be called by the executor for the
//! core pallet and each sub-pallets, it is crucial the the pallets are ordered correctly in the
//! construct runtime. The ordering must be the following:
//!
//! ```ignore
//! 1. pallet-core
//! 2. pallet-verifier
//! 3. pallet-signed
//! 4. pallet-unsigned
//! ```
//!
//! ## Election Phases
//!
//! ```ignore
//! // ----------      ----------   --------------   -----------
//! //            |  |            |                |             |
//! //    Snapshot Signed  SignedValidation    Unsigned       elect()
//! ```
//!
//! > to-finish

#![cfg_attr(not(feature = "std"), no_std)]

// TODO(gpestana): clean imports
use codec::MaxEncodedLen;
use scale_info::TypeInfo;

use frame_election_provider_support::{
	bounds::ElectionBoundsBuilder, BoundedSupportsOf, ElectionDataProvider, ElectionProvider,
	NposSolution, PageIndex, VoterOf,
};
use frame_support::{
	defensive, ensure,
	traits::{Defensive, DefensiveSaturating, Get},
	BoundedVec,
};
use sp_runtime::traits::Zero;

use frame_system::pallet_prelude::BlockNumberFor;

#[macro_use]
pub mod helpers;
#[cfg(test)]
mod mock;

const LOG_PREFIX: &'static str = "runtime::multiblock-election";

pub mod signed;
pub mod types;
pub mod unsigned;
pub mod verifier;

pub use pallet::*;
pub use types::*;

pub use crate::verifier::Verifier;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;
	use codec::EncodeLike;
	use frame_support::{
		pallet_prelude::{ValueQuery, *},
		sp_runtime::Saturating,
		Twox64Concat,
	};
	use frame_system::pallet_prelude::BlockNumberFor;

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

		/// The number of snapshot voters to fetch per block.
		#[pallet::constant]
		type VoterSnapshotPerBlock: Get<u32>;

		/// The number of snapshot targets to fetch per block.
		#[pallet::constant]
		type TargetSnapshotPerBlock: Get<u32>;

		/// The number of pages.
		///
		/// A solution may contain at MOST this many pages.
		#[pallet::constant]
		type Pages: Get<PageIndex>;

		/// The limit number of blocks that the `Phase::Export` will be open for.
		///
		/// The export phase will terminate if it has been open for `T::ExportPhaseLimit` blocks or
		/// the `EPM::call(0)` is called.
		type ExportPhaseLimit: Get<BlockNumberFor<Self>>;

		/// The solution type.
		type Solution: codec::Codec
			+ sp_std::fmt::Debug
			+ Default
			+ PartialEq
			+ Eq
			+ Clone
			+ Sized
			+ Ord
			+ NposSolution
			+ TypeInfo
			+ EncodeLike
			+ MaxEncodedLen;

		/// Something that will provide the election data.
		type DataProvider: ElectionDataProvider<
			AccountId = Self::AccountId,
			BlockNumber = BlockNumberFor<Self>,
		>;

		/// Something that implements a fallback election.
		///
		/// This provider must run the election in one block, thus it has at most 1 page.
		type Fallback: ElectionProvider<
			AccountId = Self::AccountId,
			BlockNumber = BlockNumberFor<Self>,
			DataProvider = Self::DataProvider,
			MaxWinnersPerPage = <Self::Verifier as Verifier>::MaxWinnersPerPage,
			MaxBackersPerWinner = <Self::Verifier as Verifier>::MaxBackersPerWinner,
			Pages = ConstU32<1>,
		>;

		/// Something that implements an election solution verifier.
		type Verifier: verifier::Verifier<AccountId = Self::AccountId, Solution = SolutionOf<Self>>
			+ verifier::AsyncVerifier;
	}

	/// Current phase.
	#[pallet::storage]
	pub(crate) type CurrentPhase<T: Config> = StorageValue<_, Phase<BlockNumberFor<T>>, ValueQuery>;

	/// Current round
	#[pallet::storage]
	pub(crate) type Round<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Paginated target snapshot.
	#[pallet::storage]
	pub(crate) type PagedTargetSnapshot<T: Config> =
		StorageMap<_, Twox64Concat, PageIndex, BoundedVec<T::AccountId, T::TargetSnapshotPerBlock>>;

	/// Paginated voter snapshot.
	#[pallet::storage]
	pub(crate) type PagedVoterSnapshot<T: Config> =
		StorageMap<_, Twox64Concat, PageIndex, VoterPageOf<T>>;

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
			let current_phase = Self::current_phase();

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

			// closure that tries to progress paged snapshot creation.
			// TODO(gpestana): weights
			let try_snapshot_next = |remaining_pages: PageIndex| {
				let target_snapshot_weight = if !Snapshot::<T>::targets_snapshot_exists() {
					match Self::create_targets_snapshot() {
						Ok(target_count) => {
							log!(info, "target snapshot created with {} targets", target_count);
							Weight::default()
						},
						Err(err) => {
							log!(error, "error preparing targets snapshot: {:?}", err);
							Weight::default()
						},
					}
				} else {
					Weight::default()
				};

				let voter_snapshot_weight = match Self::create_voters_snapshot(remaining_pages) {
					Ok(voter_count) => {
						log!(
							info,
							"voter snapshot progressed: page {} with {} voters",
							remaining_pages,
							voter_count,
						);

						Self::phase_transition(Phase::Snapshot(remaining_pages));
						Weight::default() // weights
					},
					Err(err) => {
						log!(error, "error preparing voter snapshot: {:?}", err);
						Weight::default() // weights
					},
				};

				target_snapshot_weight.saturating_add(voter_snapshot_weight)
			};

			match current_phase {
				// start snapshot.
				Phase::Off
					if remaining_blocks <= snapshot_deadline &&
						remaining_blocks > signed_deadline =>
					try_snapshot_next(Self::msp()),

				// continue snapshot.
				Phase::Snapshot(x) if x > 0 => try_snapshot_next(x.saturating_sub(1)),

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

				// EPM is "serving" the staking pallet with the election results.
				Phase::Export(started_at) => {
					if now > started_at + T::ExportPhaseLimit::get() {
						// TODO: test the edge case where the export phase saturates. maybe do not
						// enter in emergency phase?
						log!(
					    error,
					    "phase `Export` has been open for too long ({} blocks). entering emergency mode",
					    T::ExportPhaseLimit::get(),
				    );

						Self::phase_transition(Phase::Emergency)
					}

					Weight::default()
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

/// Wrapper struct for working with snapshots.
///
/// It manages the following storage items:
///
/// - [`PagedVoterSnapshot`]: Paginated map of voters.
/// - [`PagedTargetSnapshot`]: Paginated map of targets.
///
///	To ensure correctness and data consistency, all the reads and writes to storage items related to
///	the snapshot and "wrapped" by this struct must be performed through the methods exposed by the
///	implementation of [`Snapshot`].
///
/// ### Invariants
///
/// TODO(gpestana): finish rust docs
/// TODO(gpestana): consider moving Snapshot<T> under the `mod pallet` to keep the storage type as
/// private.
pub(crate) struct Snapshot<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> Snapshot<T> {
	/// Returns the targets snapshot.
	///
	/// TODO(gpestana): paginate targets too? (update: a lot of shenenigans on the assignments
	/// converstion and target/voter index. Hard to ensure that no more than 1 snapshot page is
	/// fetched when both voter and target snapshots are paged.)
	fn targets() -> Option<BoundedVec<T::AccountId, T::TargetSnapshotPerBlock>> {
		PagedTargetSnapshot::<T>::get(Pallet::<T>::lsp())
	}

	/// Sets a page of targets in the snapshot's storage.
	fn set_targets(page: PageIndex, targets: BoundedVec<T::AccountId, T::TargetSnapshotPerBlock>) {
		PagedTargetSnapshot::<T>::insert(page, targets);
	}

	fn targets_snapshot_exists() -> bool {
		!PagedTargetSnapshot::<T>::iter_keys().count().is_zero()
	}

	/// Return the number of desired targets, which is defined by [`T::DataProvider`].
	fn desired_targets() -> Option<u32> {
		match T::DataProvider::desired_targets() {
			Ok(desired) => Some(desired),
			Err(err) => {
				defensive!(
					"error fetching the desired targets from the election data provider {}",
					err
				);
				None
			},
		}
	}

	/// Returns the voters of a specific `page` index in the current snapshot.
	fn voters(page: PageIndex) -> Option<VoterPageOf<T>> {
		PagedVoterSnapshot::<T>::get(page)
	}

	/// Sets a apgege of voters in the snapshot's storage.
	fn set_voters(
		page: PageIndex,
		voters: BoundedVec<VoterOf<T::DataProvider>, T::VoterSnapshotPerBlock>,
	) {
		PagedVoterSnapshot::<T>::insert(page, voters);
	}

	/// Clears all data related to a snapshot.
	///
	/// At the end of a round, all the snapshot related data must be cleared and the election phase
	/// has transitioned to `Phase::Off`.
	fn kill() {
		let _ = PagedVoterSnapshot::<T>::clear(u32::MAX, None);
		let _ = PagedTargetSnapshot::<T>::clear(u32::MAX, None);

		debug_assert_eq!(<CurrentPhase<T>>::get(), Phase::Off);
	}

	#[cfg(any(test, debug_assertions))]
	fn _ensure_snapshot(_must_exist: bool) -> Result<(), &'static str> {
		// TODO(gpestana): implement debug assertions for the snapshot.
		todo!()
	}
}

impl<T: Config> Pallet<T> {
	/// Return the current election phase.
	pub fn current_phase() -> Phase<BlockNumberFor<T>> {
		<CurrentPhase<T>>::get()
	}

	/// Return the current election round.
	pub fn current_round() -> u32 {
		<Round<T>>::get()
	}

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

	/// Creates the target snapshot.
	fn create_targets_snapshot() -> Result<u32, ElectionError<T>> {
		// set target count bound as the max number of targets per page.
		let bounds = ElectionBoundsBuilder::default()
			.targets_count(T::TargetSnapshotPerBlock::get().into())
			.build()
			.targets;

		let targets: BoundedVec<_, T::TargetSnapshotPerBlock> =
			T::DataProvider::electable_targets(bounds, Zero::zero())
				.and_then(|t| {
					t.try_into().map_err(|_| "too many targets returned by the data provider.")
				})
				.map_err(|e| {
					log!(debug, "error fetching electable targets from data provider: {:?}", e);
					ElectionError::<T>::DataProvider
				})?;

		let count = targets.len() as u32;
		log!(info, "created target snapshot with {} targets.", count);

		Snapshot::<T>::set_targets(Zero::zero(), targets);

		Ok(count)
	}

	fn create_voters_snapshot(remaining_pages: u32) -> Result<u32, ElectionError<T>> {
		ensure!(remaining_pages < T::Pages::get(), ElectionError::<T>::RequestedPageExceeded);

		// set voter count bound as the max number of voterss per page.
		let bounds = ElectionBoundsBuilder::default()
			.voters_count(T::VoterSnapshotPerBlock::get().into())
			.build()
			.voters;

		let voters: BoundedVec<_, T::VoterSnapshotPerBlock> =
			T::DataProvider::electing_voters(bounds, remaining_pages)
				.and_then(|v| {
					v.try_into().map_err(|_| "too many voters returned by the data provider")
				})
				.map_err(|_| ElectionError::<T>::DataProvider)?;

		let count = voters.len() as u32;
		log!(info, "created voter snapshot with {} voters.", count);

		Snapshot::<T>::set_voters(remaining_pages, voters);

		Ok(count)
	}

	/// Performs all tasks required after a successful election:
	///
	/// 1. Increment round.
	/// 2. Change phase to [`Phase::Off`].
	/// 3. Clear all snapshot data.
	fn rotate_round() {
		<Round<T>>::mutate(|r| r.defensive_saturating_accrue(1));
		Self::phase_transition(Phase::Off);
		Snapshot::<T>::kill();
	}
}

impl<T: Config> ElectionProvider for Pallet<T> {
	type AccountId = T::AccountId;
	type BlockNumber = BlockNumberFor<T>;
	type Error = ElectionError<T>;
	type MaxWinnersPerPage = <T::Verifier as Verifier>::MaxWinnersPerPage;
	type MaxBackersPerWinner = <T::Verifier as Verifier>::MaxBackersPerWinner;
	type Pages = T::Pages;
	type DataProvider = T::DataProvider;

	/// Important note: we do exect the caller of `elect` to reach page 0.
	fn elect(remaining: PageIndex) -> Result<BoundedSupportsOf<Self>, Self::Error> {
		T::Verifier::get_queued_solution(remaining)
			.ok_or(ElectionError::<T>::SupportPageNotAvailable(remaining))
			.or_else(|err| {
				log!(error, "election provider failed due to {:?}, trying fallback.", err);
				T::Fallback::elect(remaining).map_err(|fe| ElectionError::<T>::Fallback(fe))
			})
			.map(|supports| {
				if remaining.is_zero() {
					log!(info, "provided the last supports page, rotating round.");
					Self::rotate_round();
				} else {
					// Phase::Export is on while the election is calling all pages of `elect`.
					if !Self::current_phase().is_export() {
						let now = <frame_system::Pallet<T>>::block_number();
						Self::phase_transition(Phase::Export(now));
					}
				}
				supports.into()
			})
			.map_err(|err| {
				// election failed, go to emergency phase. TODO(gpestana): rething emergency phase.
				log!(
					error,
					"fetching election page {} and fallback failed. entering emergency mode",
					remaining
				);
				Self::phase_transition(Phase::Emergency);
				err
			})
	}
}

#[cfg(test)]
mod phase_transition {
	use super::*;
	use crate::mock::*;

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

                // tests transition phase boundaries and performs snapshot sanity checks.
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

	use frame_support::{assert_noop, assert_ok};

	#[test]
	fn setters_getters_work() {
		ExtBuilder::default().build_and_execute(|| {
			let v = BoundedVec::<_, _>::try_from(vec![]).unwrap();

			assert!(Snapshot::<T>::targets().is_none());
			assert!(Snapshot::<T>::voters(0).is_none());
			assert!(Snapshot::<T>::voters(1).is_none());

			Snapshot::<T>::set_targets(0, v.clone());
			assert!(Snapshot::<T>::targets().is_some());

			Snapshot::<T>::kill();
			assert!(Snapshot::<T>::targets().is_none());
			assert!(Snapshot::<T>::voters(0).is_none());
			assert!(Snapshot::<T>::voters(1).is_none());
		})
	}

	#[test]
	fn targets_voters_snapshot_boundary_checks_works() {
		ExtBuilder::default().build_and_execute(|| {
			assert_eq!(Pages::get(), 3);
			assert_eq!(MultiPhase::msp(), 2);
			assert_eq!(MultiPhase::lsp(), 0);

			assert_ok!(MultiPhase::create_targets_snapshot());

			assert_ok!(MultiPhase::create_voters_snapshot(2));
			assert_ok!(MultiPhase::create_voters_snapshot(1));
			assert_ok!(MultiPhase::create_voters_snapshot(0));

			assert_noop!(
				MultiPhase::create_voters_snapshot(3),
				ElectionError::<T>::RequestedPageExceeded
			);
			assert_noop!(
				MultiPhase::create_voters_snapshot(10),
				ElectionError::<T>::RequestedPageExceeded
			);
		})
	}

	#[test]
	fn create_targets_snapshot_works() {
		ExtBuilder::default().build_and_execute(|| {
			assert_eq!(MultiPhase::msp(), 2);

			let no_bounds = ElectionBoundsBuilder::default().build().targets;
			let all_targets =
				<MockStaking as ElectionDataProvider>::electable_targets(no_bounds, 0);
			assert_eq!(all_targets.unwrap(), Targets::get());
			assert_eq!(Targets::get().len(), 8);

			// sets max targets per page to 2.
			TargetSnapshotPerBlock::set(2);

			let result_and_count = MultiPhase::create_targets_snapshot();
			assert_eq!(result_and_count.unwrap(), 2);
			assert_eq!(Snapshot::<T>::targets().unwrap().to_vec(), vec![10, 20]);

			// sets max targets per page to 4.
			TargetSnapshotPerBlock::set(4);

			let result_and_count = MultiPhase::create_targets_snapshot();
			assert_eq!(result_and_count.unwrap(), 4);
			assert_eq!(Snapshot::<T>::targets().unwrap().to_vec(), vec![10, 20, 30, 40]);

			Snapshot::<T>::kill();

			TargetSnapshotPerBlock::set(6);

			let result_and_count = MultiPhase::create_targets_snapshot();
			assert_eq!(result_and_count.unwrap(), 6);
			assert_eq!(Snapshot::<T>::targets().unwrap().to_vec(), vec![10, 20, 30, 40, 50, 60]);

			// reset storage.
			Snapshot::<T>::kill();
		})
	}

	#[test]
	fn voters_snapshot_works() {
		ExtBuilder::default().build_and_execute(|| {
			assert_eq!(MultiPhase::msp(), 2);

			let no_bounds = ElectionBoundsBuilder::default().build().voters;
			let all_voters = <MockStaking as ElectionDataProvider>::electing_voters(no_bounds, 0);
			assert_eq!(all_voters.unwrap(), Voters::get());
			assert_eq!(Voters::get().len(), 16);

			// sets max voters per page to 7.
			VoterSnapshotPerBlock::set(7);

			let voters_page = |page: PageIndex| {
				Snapshot::<T>::voters(page)
					.unwrap()
					.iter()
					.map(|v| v.0)
					.collect::<Vec<AccountId>>()
			};

			// page `msp`.
			let result_and_count = MultiPhase::create_voters_snapshot(MultiPhase::msp());
			assert_eq!(result_and_count.unwrap(), 7);
			assert_eq!(voters_page(MultiPhase::msp()), vec![1, 2, 3, 4, 5, 6, 7]);

			let result_and_count = MultiPhase::create_voters_snapshot(1);
			assert_eq!(result_and_count.unwrap(), 7);
			assert_eq!(voters_page(1), vec![8, 10, 20, 30, 40, 50, 60]);

			// page `lsp`.
			let result_and_count = MultiPhase::create_voters_snapshot(MultiPhase::lsp());
			assert_eq!(result_and_count.unwrap(), 2);
			assert_eq!(voters_page(MultiPhase::lsp()), vec![70, 80]);
		})
	}
}
