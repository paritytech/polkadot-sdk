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

//! # Multi-phase, multi-block election provider pallet
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
//!```text
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
//!   implements [`verifier::AsyncVerifier`] which verifies multi-paged solution asynchronously.
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
//! ```text
//! 1. pallet-core
//! 2. pallet-verifier
//! 3. pallet-signed
//! 4. pallet-unsigned
//! ```
//!
//! ## Election Phases
//!
//! This pallet manages the election phases which signal to the other sub-pallets which actions to
//! take at a given block. The election phases are the following:
//!
//! ```text
//! //   -----------     -----------  --------------  ------------  --------
//! //  |            |  |            |               |             |        |
//! // Off    Snapshot Signed  SignedValidation   Unsigned       elect()   Export
//! ```
//!
//! Each phase duration depends on the estimate block number election, which can be fetched from
//! [`pallet::Config::DataProvider`].
//!
//! > to-finish

#![cfg_attr(not(feature = "std"), no_std)]
// TODO: remove
#![allow(dead_code)]

use frame_election_provider_support::{
	bounds::ElectionBoundsBuilder, BoundedSupportsOf, ElectionDataProvider, ElectionProvider,
	LockableElectionDataProvider, PageIndex, VoterOf, Weight,
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

pub mod signed;
pub mod types;
pub mod unsigned;
pub mod verifier;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(test)]
mod mock;

pub use pallet::*;
pub use types::*;

pub use crate::{unsigned::miner, verifier::Verifier, weights::WeightInfo};

/// Internal crate re-exports to use across benchmarking and tests.
#[cfg(any(test, feature = "runtime-benchmarks"))]
use crate::verifier::Pallet as PalletVerifier;

const LOG_TARGET: &'static str = "runtime::multiblock-election";

/// Page configured for the election.
pub type PagesOf<T> = <T as crate::Config>::Pages;

/// Trait defining the benchmarking configs.
pub trait BenchmarkingConfig {
	/// Range of voters registerd in the system.
	const VOTERS: u32;
	/// Range of targets registered in the system.
	const TARGETS: u32;
	/// Range of voters per snapshot page.
	const VOTERS_PER_PAGE: [u32; 2];
	/// Range of targets per snapshot page.
	const TARGETS_PER_PAGE: [u32; 2];
}

#[frame_support::pallet(dev_mode)]
pub mod pallet {

	use super::*;
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

		/// Maximum number of supports (i.e. winners/validators/targets) that can be represented
		/// in one page of a solution.
		type MaxWinnersPerPage: Get<u32>;

		/// Maximum number of voters that can support a single target, across ALL the solution
		/// pages. Thus, this can only be verified when processing the last solution page.
		///
		/// This limit must be set so that the memory limits of the rest of the system are
		/// respected.
		type MaxBackersPerWinner: Get<u32>;

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

		/// Something that will provide the election data.
		type DataProvider: LockableElectionDataProvider<
			AccountId = Self::AccountId,
			BlockNumber = BlockNumberFor<Self>,
		>;

		// The miner configuration.
		type MinerConfig: miner::Config<
			AccountId = AccountIdOf<Self>,
			Pages = Self::Pages,
			MaxVotesPerVoter = <Self::DataProvider as frame_election_provider_support::ElectionDataProvider>::MaxVotesPerVoter,
			TargetSnapshotPerBlock = Self::TargetSnapshotPerBlock,
			VoterSnapshotPerBlock = Self::VoterSnapshotPerBlock,
			MaxWinnersPerPage = Self::MaxWinnersPerPage,
			MaxBackersPerWinner = Self::MaxBackersPerWinner,
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
		type Verifier: verifier::Verifier<
				AccountId = Self::AccountId,
				Solution = SolutionOf<Self::MinerConfig>,
			> + verifier::AsyncVerifier;

		/// Benchmarking configurations for this and sub-pallets.
		type BenchmarkingConfig: BenchmarkingConfig;

		/// The weights for this pallet.
		type WeightInfo: WeightInfo;
	}

	// Expose miner configs over the metadata such that they can be re-implemented.
	#[pallet::extra_constants]
	impl<T: Config> Pallet<T> {
		#[pallet::constant_name(MinerMaxVotesPerVoter)]
		fn max_votes_per_voter() -> u32 {
			<T::MinerConfig as miner::Config>::MaxVotesPerVoter::get()
		}

		#[pallet::constant_name(MinerMaxBackersPerWinner)]
		fn max_backers_per_winner() -> u32 {
			<T::MinerConfig as miner::Config>::MaxBackersPerWinner::get()
		}

		#[pallet::constant_name(MinerMaxWinnersPerPage)]
		fn max_winners_per_page() -> u32 {
			<T::MinerConfig as miner::Config>::MaxWinnersPerPage::get()
		}
	}

	/// Election failure strategy.
	#[pallet::storage]
	pub(crate) type ElectionFailure<T: Config> =
		StorageValue<_, ElectionFailureStrategy, ValueQuery>;

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
			//  ---------- ---------- ---------- ----------- ---------- --------
			// |         |          |          |            |          |        |
			// Off       Snapshot   (Signed     SigValid)   Unsigned   Export  elect()

			use sp_runtime::traits::One;

			let export_deadline = T::ExportPhaseLimit::get().saturating_add(T::Lookhaead::get());
			let unsigned_deadline = export_deadline.saturating_add(T::UnsignedPhase::get());
			let signed_validation_deadline =
				unsigned_deadline.saturating_add(T::SignedValidationPhase::get());
			let signed_deadline = signed_validation_deadline.saturating_add(T::SignedPhase::get());
			let snapshot_deadline = signed_deadline
				.saturating_add(T::Pages::get().into())
				.saturating_add(One::one());

			let next_election = T::DataProvider::next_election_prediction(now)
				.saturating_sub(T::Lookhaead::get())
				.max(now);

			let remaining_blocks = next_election - now;
			let current_phase = Self::current_phase();

			log!(
				trace,
				"now {:?} - current phase {:?} | \
                    snapshot_deadline: {:?} (at #{:?}), signed_deadline: {:?} (at #{:?}), \
                    signed_validation_deadline: {:?} (at #{:?}), unsigned_deadline: {:?} (at #{:?}) \
                    export_deadline: {:?} (at #{:?}) - [next election at #{:?}, remaining: {:?}]",
				now,
				current_phase,
                snapshot_deadline,
                next_election.saturating_sub(snapshot_deadline),
                signed_deadline,
                next_election.saturating_sub(signed_deadline),
                signed_validation_deadline,
                next_election.saturating_sub(signed_validation_deadline),
                unsigned_deadline,
                next_election.saturating_sub(unsigned_deadline),
                export_deadline,
                next_election.saturating_sub(export_deadline),
				next_election,
				remaining_blocks,
			);

			match current_phase {
				// start snapshot.
				Phase::Off
					if remaining_blocks <= snapshot_deadline &&
						remaining_blocks > signed_deadline =>
				// allocate one extra block for the target snapshot.
					Self::try_progress_snapshot(T::Pages::get() + 1),

				// continue snapshot.
				Phase::Snapshot(x) if x > 0 => Self::try_progress_snapshot(x.saturating_sub(1)),

				// start unsigned phase if snapshot is ready and signed phase is disabled.
				Phase::Snapshot(0) if T::SignedPhase::get().is_zero() => {
					Self::phase_transition(Phase::Unsigned(now));
					T::WeightInfo::on_phase_transition()
				},

				// start signed phase. The `signed` pallet will take further actions now.
				Phase::Snapshot(0)
					if remaining_blocks <= signed_deadline &&
						remaining_blocks > signed_validation_deadline =>
					Self::start_signed_phase(),

				// start signed validation. The `signed` pallet will take further actions now.
				Phase::Signed
					if remaining_blocks <= signed_validation_deadline &&
						remaining_blocks > unsigned_deadline =>
				{
					Self::phase_transition(Phase::SignedValidation(now));
					T::WeightInfo::on_phase_transition()
				},

				// start unsigned phase. The `unsigned` pallet will take further actions now.
				Phase::Signed | Phase::SignedValidation(_) | Phase::Snapshot(0)
					if remaining_blocks <= unsigned_deadline && remaining_blocks > Zero::zero() =>
				{
					Self::phase_transition(Phase::Unsigned(now));
					T::WeightInfo::on_phase_transition()
				},

				// EPM is "serving" the staking pallet with the election results.
				Phase::Export(started_at) => Self::do_export_phase(now, started_at),

				_ => T::WeightInfo::on_initialize_do_nothing(),
			}
		}

		fn integrity_test() {
			// the signed validator phase must not be less than the number of pages of a
			// submission.
			assert!(
				T::SignedValidationPhase::get() <= T::Pages::get().into(),
				"signed validaton phase ({:?}) should not be less than the number of pages per submission ({:?})",
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
/// To ensure correctness and data consistency, all the reads and writes to storage items related
/// to the snapshot and "wrapped" by this struct must be performed through the methods exposed by
/// the implementation of [`Snapshot`].
pub(crate) struct Snapshot<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> Snapshot<T> {
	/// Returns the targets snapshot.
	///
	/// TODO(gpestana): consider paginating targets (update: a lot of shenenigans on the assignments
	/// converstion and target/voter index. Hard to ensure that no more than 1 snapshot page is
	/// fetched when both voter and target snapshots are paged.)
	fn targets() -> Option<BoundedVec<T::AccountId, T::TargetSnapshotPerBlock>> {
		PagedTargetSnapshot::<T>::get(Pallet::<T>::lsp())
	}

	/// Sets a page of targets in the snapshot's storage.
	fn set_targets(targets: BoundedVec<T::AccountId, T::TargetSnapshotPerBlock>) {
		PagedTargetSnapshot::<T>::insert(Pallet::<T>::lsp(), targets);
	}

	/// Returns whether the target snapshot exists in storage.
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

	/// Sets a single page of voters in the snapshot's storage.
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

	#[allow(dead_code)]
	#[cfg(any(test, debug_assertions))]
	pub(crate) fn ensure() -> Result<(), &'static str> {
		let pages = T::Pages::get();
		ensure!(pages > 0, "number pages must be higer than 0.");

		// target snapshot exists (one page only);
		ensure!(Self::targets().is_some(), "target snapshot does not exist.");

		// ensure that snapshot pages exist as expected.
		for page in (crate::Pallet::<T>::lsp()..=crate::Pallet::<T>::msp()).rev() {
			ensure!(
				Self::voters(page).is_some(),
				"at least one page of the snapshot does not exist"
			);
		}

		Ok(())
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
	pub fn msp() -> PageIndex {
		T::Pages::get().checked_sub(1).defensive_unwrap_or_default()
	}

	/// Return the least significant page of the snapshot.
	///
	/// Based on the contract with `ElectionDataProvider`, tis is the last page to be filled.
	pub fn lsp() -> PageIndex {
		Zero::zero()
	}

	/// Creates and stores the target snapshot.
	///
	/// Note: currently, the pallet uses single page target page only.
	fn create_targets_snapshot() -> Result<u32, ElectionError<T>> {
		let stored_count = Self::create_targets_snapshot_inner(T::TargetSnapshotPerBlock::get())?;
		log!(info, "created target snapshot with {} targets.", stored_count);

		Ok(stored_count)
	}

	fn create_targets_snapshot_inner(targets_per_page: u32) -> Result<u32, ElectionError<T>> {
		// set target count bound as the max number of targets per block.
		let bounds = ElectionBoundsBuilder::default()
			.targets_count(targets_per_page.into())
			.build()
			.targets;

		let targets: BoundedVec<_, T::TargetSnapshotPerBlock> =
			T::DataProvider::electable_targets(bounds, Zero::zero())
				.and_then(|t| {
					t.try_into().map_err(|e| {
						log!(error, "too many targets? err: {:?}", e);
						"too many targets returned by the data provider."
					})
				})
				.map_err(|e| {
					log!(info, "error fetching electable targets from data provider: {:?}", e);
					ElectionError::<T>::DataProvider
				})?;

		let count = targets.len() as u32;
		Snapshot::<T>::set_targets(targets);

		Ok(count)
	}

	/// Creates and store a single page of the voter snapshot.
	fn create_voters_snapshot(remaining_pages: u32) -> Result<u32, ElectionError<T>> {
		ensure!(remaining_pages < T::Pages::get(), ElectionError::<T>::RequestedPageExceeded);

		let paged_voters_count =
			Self::create_voters_snapshot_inner(remaining_pages, T::VoterSnapshotPerBlock::get())?;
		log!(info, "created voter snapshot with {} voters.", paged_voters_count);

		Ok(paged_voters_count)
	}

	fn create_voters_snapshot_inner(
		remaining_pages: u32,
		voters_per_page: u32,
	) -> Result<u32, ElectionError<T>> {
		// set voter count bound as the max number of voters per page.
		let bounds = ElectionBoundsBuilder::default()
			.voters_count(voters_per_page.into())
			.build()
			.voters;

		let paged_voters: BoundedVec<_, T::VoterSnapshotPerBlock> =
			T::DataProvider::electing_voters(bounds, remaining_pages)
				.and_then(|v| {
					v.try_into().map_err(|_| "too many voters returned by the data provider")
				})
				.map_err(|_| ElectionError::<T>::DataProvider)?;

		let count = paged_voters.len() as u32;
		Snapshot::<T>::set_voters(remaining_pages, paged_voters);

		Ok(count)
	}

	/// Tries to progress the snapshot.
	///
	/// The first (and only) target page is fetched from the [`DataProvider`] at the same block when
	/// the msp of the voter snaphot.
	fn try_progress_snapshot(remaining_pages: PageIndex) -> Weight {
		let _ = <T::DataProvider as LockableElectionDataProvider>::set_lock();

		debug_assert!(
			CurrentPhase::<T>::get().is_snapshot() ||
				!Snapshot::<T>::targets_snapshot_exists() &&
					remaining_pages == T::Pages::get() + 1,
		);

		if !Snapshot::<T>::targets_snapshot_exists() {
			// first block for single target snapshot.
			match Self::create_targets_snapshot() {
				Ok(target_count) => {
					log!(info, "target snapshot created with {} targets", target_count);
					Self::phase_transition(Phase::Snapshot(remaining_pages.saturating_sub(1)));
					T::WeightInfo::create_targets_snapshot_paged(T::TargetSnapshotPerBlock::get())
				},
				Err(err) => {
					log!(error, "error preparing targets snapshot: {:?}", err);
					// TODO: T::WeightInfo::snapshot_error();
					Weight::default()
				},
			}
		} else {
			// progress voter snapshot.
			match Self::create_voters_snapshot(remaining_pages) {
				Ok(voter_count) => {
					log!(
						info,
						"voter snapshot progressed: page {} with {} voters",
						remaining_pages,
						voter_count,
					);
					Self::phase_transition(Phase::Snapshot(remaining_pages));
					T::WeightInfo::create_voters_snapshot_paged(T::VoterSnapshotPerBlock::get())
				},
				Err(err) => {
					log!(error, "error preparing voter snapshot: {:?}", err);
					// TODO: T::WeightInfo::snapshot_error();
					Weight::default()
				},
			}
		}
	}

	pub(crate) fn start_signed_phase() -> Weight {
		// done with the snapshot, release the data provider lock.
		<T::DataProvider as LockableElectionDataProvider>::unlock();
		Self::phase_transition(Phase::Signed);

		T::WeightInfo::on_initialize_start_signed()
	}

	pub(crate) fn do_export_phase(now: BlockNumberFor<T>, started_at: BlockNumberFor<T>) -> Weight {
		if now > started_at + T::ExportPhaseLimit::get() {
			log!(
				error,
				"phase `Export` has been open for too long ({:?} blocks). election round failed.",
				T::ExportPhaseLimit::get(),
			);

			match ElectionFailure::<T>::get() {
				ElectionFailureStrategy::Restart => Self::reset_round(),
				ElectionFailureStrategy::Emergency => Self::phase_transition(Phase::Emergency),
			}
		}

		T::WeightInfo::on_initialize_start_export()
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
		<T::Verifier as Verifier>::kill();
	}

	/// Performs all tasks required after an unsuccessful election:
	///
	/// 1. Change phase to [`Phase::Off`].
	/// 2. Clear all snapshot data.
	fn reset_round() {
		Self::phase_transition(Phase::Off);
		Snapshot::<T>::kill();

		<T::Verifier as Verifier>::kill();
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
				log!(
					error,
					"elect(): (page {:?}) election provider failed due to {:?}, trying fallback.",
					remaining,
					err
				);
				T::Fallback::elect(remaining).map_err(|fe| ElectionError::<T>::Fallback(fe))
			})
			.map(|supports| {
				if remaining.is_zero() {
					log!(info, "elect(): provided the last supports page, rotating round.");
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
				log!(error, "elect(): fetching election page {} and fallback failed.", remaining);

				match ElectionFailure::<T>::get() {
					// force emergency phase for testing.
					ElectionFailureStrategy::Restart => Self::reset_round(),
					ElectionFailureStrategy::Emergency => Self::phase_transition(Phase::Emergency),
				}
				err
			})
	}
}

#[cfg(test)]
mod phase_transition {
	use super::*;
	use crate::mock::*;

	use frame_support::assert_ok;

	#[test]
	fn single_page() {
		//  ----------      ----------   --------------   -----------
		//            |  |            |                |             |
		//    Snapshot Signed  SignedValidation    Unsigned       elect()
		let (mut ext, _) = ExtBuilder::default()
			.pages(1)
			.signed_phase(3)
			.validate_signed_phase(1)
			.lookahead(0)
			.build_offchainify(1);

		ext.execute_with(|| {
            assert_eq!(System::block_number(), 0);
            assert_eq!(Pages::get(), 1);
            assert_eq!(<Round<T>>::get(), 0);
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Off);

			let next_election = <<Runtime as Config>::DataProvider as ElectionDataProvider>::next_election_prediction(
                System::block_number()
            );
            assert_eq!(next_election, 30);

            // representing the blocknumber when the phase transition happens.
			let export_deadline = next_election - (ExportPhaseLimit::get() + Lookhaead::get());
			let expected_unsigned = export_deadline - UnsignedPhase::get();
			let expected_validate = expected_unsigned - SignedValidationPhase::get();
			let expected_signed = expected_validate - SignedPhase::get();
			let expected_snapshot = expected_signed - Pages::get() as u64;

			// tests transition phase boundaries.
            roll_to(expected_snapshot);
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Snapshot(Pages::get() - 1));

            roll_to(expected_signed);
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Signed);

            roll_to(expected_validate);
            let start_validate = System::block_number();
            assert_eq!(<CurrentPhase<T>>::get(), Phase::SignedValidation(start_validate));

            roll_to(expected_unsigned);
            let start_unsigned = System::block_number();
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Unsigned(start_unsigned));

            roll_to(next_election + 1);
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Unsigned(start_unsigned));

            // unsigned phase until elect() is called.
            roll_to(next_election + 3);
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Unsigned(start_unsigned));

            assert_ok!(MultiPhase::elect(0));

            // election done, go to off phase.
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Off);
		})
	}

	#[test]
	fn multi_page() {
		let (mut ext, _) = ExtBuilder::default()
			.pages(2)
			.signed_phase(3)
			.validate_signed_phase(1)
			.lookahead(0)
			.build_offchainify(1);

		ext.execute_with(|| {
            assert_eq!(System::block_number(), 0);
            assert_eq!(Pages::get(), 2);
            assert_eq!(<Round<T>>::get(), 0);
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Off);

			let next_election = <<Runtime as Config>::DataProvider as ElectionDataProvider>::next_election_prediction(
                System::block_number()
            );
            assert_eq!(next_election, 30);

            // representing the blocknumber when the phase transition happens.
			let export_deadline = next_election - (ExportPhaseLimit::get() + Lookhaead::get());
			let expected_unsigned = export_deadline - UnsignedPhase::get();
			let expected_validate = expected_unsigned - SignedValidationPhase::get();
			let expected_signed = expected_validate - SignedPhase::get();
			let expected_snapshot = expected_signed - Pages::get() as u64;

            // two blocks for snapshot.
            roll_to(expected_snapshot);
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Snapshot(Pages::get() - 1));

            roll_to(expected_snapshot + 1);
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Snapshot(0));

            roll_to(expected_signed);
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Signed);

            roll_to(expected_signed + 1);
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Signed);

            // two blocks for validate signed.
            roll_to(expected_validate);
            let start_validate = System::block_number();
            assert_eq!(<CurrentPhase<T>>::get(), Phase::SignedValidation(start_validate));

            // now in unsigned until elect() is called.
            roll_to(expected_validate + 2);
            let start_unsigned = System::block_number();
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Unsigned(start_unsigned - 1));

		})
	}

	#[test]
	fn emergency_phase_works() {
		let (mut ext, _) = ExtBuilder::default().build_offchainify(1);
		ext.execute_with(|| {
        	let next_election = <<Runtime as Config>::DataProvider as ElectionDataProvider>::next_election_prediction(
                System::block_number()
            );

            // if election fails, enters in emergency phase.
            ElectionFailure::<T>::set(ElectionFailureStrategy::Emergency);

			compute_snapshot_checked();
            roll_to(next_election);

			// election will fail due to inexistent solution.
            assert!(MultiPhase::elect(Pallet::<T>::msp()).is_err());
			// thus entering in emergency phase.
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Emergency);
        })
	}

	#[test]
	fn restart_after_elect_fails_works() {
		let (mut ext, _) = ExtBuilder::default().build_offchainify(1);
		ext.execute_with(|| {
        	let next_election = <<Runtime as Config>::DataProvider as ElectionDataProvider>::next_election_prediction(
                System::block_number()
            );

            // if election fails, restart the election round.
            ElectionFailure::<T>::set(ElectionFailureStrategy::Restart);

			compute_snapshot_checked();
            roll_to(next_election);

			// election will fail due to inexistent solution.
            assert!(MultiPhase::elect(Pallet::<T>::msp()).is_err());
			// thus restarting from Off phase.
            assert_eq!(<CurrentPhase<T>>::get(), Phase::Off);
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

			Snapshot::<T>::set_targets(v.clone());
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

	#[test]
	fn try_progress_snapshot_works() {}
}

#[cfg(test)]
mod election_provider {
	use super::*;
	use crate::{mock::*, unsigned::miner::Miner};
	use frame_support::testing_prelude::*;

	#[test]
	fn snapshot_to_supports_conversions_work() {
		type VotersPerPage = <T as pallet::Config>::VoterSnapshotPerBlock;
		type TargetsPerPage = <T as pallet::Config>::TargetSnapshotPerBlock;
		type Pages = <T as pallet::Config>::Pages;

		ExtBuilder::default()
			.pages(2)
			.snasphot_voters_page(4)
			.snasphot_targets_page(4)
			.desired_targets(2)
			.build_and_execute(|| {
				assert_eq!(MultiPhase::msp(), 1);

				let all_targets: BoundedVec<AccountId, TargetsPerPage> =
					bounded_vec![10, 20, 30, 40];

				let all_voter_pages: BoundedVec<
					BoundedVec<VoterOf<MockStaking>, VotersPerPage>,
					Pages,
				> = bounded_vec![
					bounded_vec![
						(1, 100, bounded_vec![10, 20]),
						(2, 20, bounded_vec![30]),
						(3, 30, bounded_vec![10]),
						(10, 10, bounded_vec![10])
					],
					bounded_vec![
						(20, 20, bounded_vec![20]),
						(30, 30, bounded_vec![30]),
						(40, 40, bounded_vec![40])
					],
				];

				Snapshot::<T>::set_targets(all_targets.clone());
				Snapshot::<T>::set_voters(0, all_voter_pages[0].clone());
				Snapshot::<T>::set_voters(1, all_voter_pages[1].clone());

				let desired_targets = Snapshot::<T>::desired_targets().unwrap();
				let (results, _) = Miner::<T>::mine_paged_solution_with_snapshot(
					&all_voter_pages,
					&all_targets,
					Pages::get(),
					current_round(),
					desired_targets,
					false,
				)
				.unwrap();

				let supports_page_zero =
					PalletVerifier::<T>::feasibility_check(results.solution_pages[0].clone(), 0)
						.unwrap();
				let supports_page_one =
					PalletVerifier::<T>::feasibility_check(results.solution_pages[1].clone(), 1)
						.unwrap();

				use frame_election_provider_support::{BoundedSupports, TryIntoBoundedSupports};
				use sp_npos_elections::{Support, Supports};

				let s0: Supports<AccountId> = vec![
					(10, Support { total: 90, voters: vec![(3, 30), (10, 10), (1, 50)] }),
					(20, Support { total: 50, voters: vec![(1, 50)] }),
				];
				let bs0: BoundedSupports<_, _, _> = s0.try_into_bounded_supports().unwrap();

				let s1: Supports<AccountId> =
					vec![(20, Support { total: 20, voters: vec![(20, 20)] })];
				let bs1: BoundedSupports<_, _, _> = s1.try_into_bounded_supports().unwrap();

				assert_eq!(supports_page_zero, bs0);
				assert_eq!(supports_page_one, bs1);
			})
	}
}
