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
//! unsigned transaction. Crucially, it supports paginated, multi-block elections. The goal of
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
//! Each sub-pallet has a specific set of tasks and implements one or more interfaces to expose
//! their functionality to the core pallet:
//! - The [`verifier`] pallet provides an implementation of the [`verifier::Verifier`] trait, which
//!   exposes the functionality to verify NPoS solutions in a multi-block context. In addition, it
//!   implements [`verifier::AsyncVerifier`] which verifies multi-paged solutions asynchronously.
//! - The [`signed`] pallet implements the signed phase, where off-chain entities commit to and
//!   submit their election solutions. This pallet implements the
//!   [`verifier::SolutionDataProvider`], which is used by the [`verifier`] pallet to fetch solution
//!   data to perform the solution verification.
//! - The [`unsigned`] pallet implements the unsigned phase, where block authors can compute and
//!   submit through inherent paged solutions. This pallet also implements the
//!   [`verifier::SolutionDataProvider`] interface.
//!
//! ### Pallet ordering
//!
//! Due to the assumptions of when the `on_initialize` hook must be called by the executor for the
//! core pallet and each sub-pallets, it is crucial that the pallets are ordered correctly in the
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
//!
//! // TODO(gpestana): use a diagram instead of text diagram.
//! ```text
//! //   -----------     -----------  --------------  ------------  --------
//! //  |            |  |            |               |             |        |
//! // Off    Snapshot Signed  SignedValidation   Unsigned       elect()   Export
//! ```
//!
//! Each phase duration depends on the estimate block number election, which can be fetched from
//! [`pallet::Config::DataProvider`].
//!
//! TODO(gpestana): finish, add all info related to EPM-MB

#![cfg_attr(not(feature = "std"), no_std)]

use frame_election_provider_support::{
	bounds::ElectionBoundsBuilder, BoundedSupportsOf, ElectionDataProvider, ElectionProvider,
	PageIndex, Weight,
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

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(test)]
mod mock;

pub use pallet::*;
pub use types::*;

pub use crate::{
	unsigned::miner,
	verifier::{AsyncVerifier, Verifier},
	weights::WeightInfo,
};

/// Log target for this the core EPM-MB pallet.
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

#[frame_support::pallet]
pub mod pallet {

	use super::*;
	use frame_support::{
		pallet_prelude::{ValueQuery, *},
		sp_runtime::Saturating,
		Twox64Concat,
	};

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>
			+ TryInto<Event<Self>>;

		/// Duration of the signed phase;
		///
		/// During the signed phase, staking miners may register their solutions and submit
		/// paginated solutions.
		#[pallet::constant]
		type SignedPhase: Get<BlockNumberFor<Self>>;

		/// Duration of the unsigned phase;
		///
		/// During the unsigned phase, offchain workers of block producing validators compute and
		/// submit paginated solutions.
		#[pallet::constant]
		type UnsignedPhase: Get<BlockNumberFor<Self>>;

		/// Duration of the signed validation phase.
		///
		/// During the signed validation phase, the async verifier verifies one or all the queued
		/// solution submitions during the signed phase. Once one solution is accepted, this phase
		/// terminates.
		///
		/// The duration of this phase **SHOULD NOT** be less than `T::Pages` and there is no point
		/// in it being more than the maximum number of pages per submission.
		#[pallet::constant]
		type SignedValidationPhase: Get<BlockNumberFor<Self>>;

		/// The limit number of blocks that the `Phase::Export` will be open for.
		///
		/// During the export phase, this pallet is open to return paginated, verified solution
		/// pages if at least one solution has been verified and accepted in the current era.
		///
		/// The export phase will terminate if it has been open for `T::ExportPhaseLimit` blocks or
		/// the `EPM::call(0)` is called.
		type ExportPhaseLimit: Get<BlockNumberFor<Self>>;

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

		/// Maximum number of voters that can support a single target, across **ALL(()) the solution
		/// pages. Thus, this can only be verified when processing the last solution page.
		///
		/// This limit must be set so that the memory limits of the rest of the system are
		/// respected.
		type MaxBackersPerWinner: Get<u32>;

		/// The number of pages.
		///
		/// A solution may contain at **MOST** this many pages.
		#[pallet::constant]
		type Pages: Get<PageIndex>;

		/// Something that will provide the election data.
		type DataProvider: ElectionDataProvider<
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
	///
	/// This strategy defines the actions of this pallet once an election fails.
	#[pallet::storage]
	pub(crate) type ElectionFailure<T: Config> =
		StorageValue<_, ElectionFailureStrategy, ValueQuery>;

	/// Current phase.
	#[pallet::storage]
	pub(crate) type CurrentPhase<T: Config> = StorageValue<_, Phase<BlockNumberFor<T>>, ValueQuery>;

	/// Current round.
	#[pallet::storage]
	pub(crate) type Round<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Target snapshot.
	///
	/// Note: The target snapshot is single-paged.
	#[pallet::storage]
	pub(crate) type TargetSnapshot<T: Config> = StorageValue<_, TargetPageOf<T>, OptionQuery>;

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

			let export_deadline = T::ExportPhaseLimit::get().saturating_add(T::Lookhaead::get());
			let unsigned_deadline = export_deadline.saturating_add(T::UnsignedPhase::get());
			let signed_validation_deadline =
				unsigned_deadline.saturating_add(T::SignedValidationPhase::get());
			let signed_deadline = signed_validation_deadline.saturating_add(T::SignedPhase::get());
			let snapshot_deadline = signed_deadline.saturating_add(T::Pages::get().into());

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
					match Self::try_start_snapshot() {
						Ok(weight) => weight,
						Err(weight) => {
							Self::handle_election_failure();
							weight
						},
					},

				// continue snapshot.
				Phase::Snapshot(page) => {
					let weight = match Self::try_progress_snapshot() {
						Ok(weight) => weight,
						Err(weight) => {
							Self::handle_election_failure();
							return weight
						},
					};

					match page.is_zero() {
						true if !T::SignedPhase::get().is_zero() =>
							Self::phase_transition(Phase::Signed),
						// start unsigned phase if snapshot is ready and signed phase is disabled.
						true if T::SignedPhase::get().is_zero() =>
							Self::phase_transition(Phase::Unsigned(now)),
						_ => Self::phase_transition(Phase::Snapshot(page.saturating_sub(1))),
					}

					weight.saturating_add(T::WeightInfo::on_phase_transition())
				},

				// start signed validation. The `signed` sub-pallet will take further actions now.
				Phase::Signed
					if remaining_blocks <= signed_validation_deadline &&
						remaining_blocks > unsigned_deadline =>
				{
					Self::phase_transition(Phase::SignedValidation(now));

					T::WeightInfo::on_phase_transition()
				},

				// force start unsigned phase. The `unsigned` sub-pallet will take further actions
				// now.
				Phase::Signed | Phase::SignedValidation(_)
					if remaining_blocks <= unsigned_deadline && remaining_blocks > Zero::zero() =>
				{
					// force stop any async verification that may be happening.
					<T::Verifier as AsyncVerifier>::stop();

					Self::phase_transition(Phase::Unsigned(now));
					T::WeightInfo::on_phase_transition()
				},

				// signed validation phase has found a valid solution, progress to unsigned phase.
				Phase::SignedValidation(_)
					if <T::Verifier as verifier::Verifier>::queued_score().is_some() =>
				{
					debug_assert!(
						<T::Verifier as AsyncVerifier>::status() == verifier::Status::Nothing
					);

					Self::phase_transition(Phase::Unsigned(now));
					T::WeightInfo::on_phase_transition()
				},

				// start export phase.
				Phase::Unsigned(_) if now == next_election.saturating_sub(export_deadline) => {
					Self::phase_transition(Phase::Export(now));
					T::WeightInfo::on_phase_transition()
				},

				// election solution **MAY** be ready, start export phase to allow external pallets
				// to request paged election solutions.
				Phase::Export(started_at) => Self::do_export_phase(now, started_at),

				_ => T::WeightInfo::on_initialize_do_nothing(),
			}
		}

		fn integrity_test() {
			assert!(
				T::SignedValidationPhase::get() >= T::Pages::get().into(),
				"signed validaton phase ({:?}) should not be less than the number of pages per submission ({:?})",
				T::SignedValidationPhase::get(),
				T::Pages::get(),
			);
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state(n)
		}
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);
}

#[cfg(any(test, feature = "try-runtime"))]
impl<T: Config> Pallet<T> {
	pub(crate) fn do_try_state(_now: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
		Self::check_snapshot()?;
		Self::check_election_provider_ongoing()
	}

	/// Invariants:
	/// * If Phase::Off, no snapshot should exist.
	/// * If Phase::Snapshot phase is on, we expect the target snapshot to exist and `N` pages of
	/// voter snapshot should exist, where `N` is the number of blocks that the snapshot phase has
	/// been open for.
	fn check_snapshot() -> Result<(), sp_runtime::TryRuntimeError> {
		Snapshot::<T>::ensure()?;
		Ok(())
	}

	/// Invariants:
	/// * If Phase::Off or Phase::Emergency is on, election should not be ongoing.
	/// * Otherwhise, election is ongoing.
	fn check_election_provider_ongoing() -> Result<(), sp_runtime::TryRuntimeError> {
		match Self::current_phase() {
			Phase::Off | Phase::Emergency =>
				if <Pallet<T> as ElectionProvider>::ongoing() == true {
					return Err("election ongoing in wrong phase.".into());
				},
			_ =>
				if <Pallet<T> as ElectionProvider>::ongoing() != true {
					return Err("election should be ongoing.".into());
				},
		}

		Ok(())
	}
}

/// Wrapper struct for working with snapshots.
///
/// It manages the following storage items:
///
/// - [`PagedVoterSnapshot`]: Paginated map of voters.
/// - [`TargetSnapshot`]: Single page, bounded list of targets.
///
/// To ensure correctness and data consistency, all the reads and writes to storage items related
/// to the snapshot and "wrapped" by this struct must be performed through the methods exposed by
/// the implementation of [`Snapshot`].
pub(crate) struct Snapshot<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> Snapshot<T> {
	/// Returns the targets snapshot.
	///
	/// The target snapshot is single paged.
	fn targets() -> Option<TargetPageOf<T>> {
		TargetSnapshot::<T>::get()
	}

	/// Sets a page of targets in the snapshot's storage.
	///
	/// The target snapshot is single paged.
	fn set_targets(targets: TargetPageOf<T>) {
		TargetSnapshot::<T>::set(Some(targets));
	}

	/// Returns the number of desired targets, as defined by [`T::DataProvider`].
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

	/// Returns the voters of a specific `page` index of the current snapshot, if any.
	fn voters(page: PageIndex) -> Option<VoterPageOf<T>> {
		PagedVoterSnapshot::<T>::get(page)
	}

	/// Sets a single page of voters in the snapshot's storage.
	fn set_voters(page: PageIndex, voters: VoterPageOf<T>) {
		PagedVoterSnapshot::<T>::insert(page, voters);
	}

	/// Clears all data related to a snapshot.
	///
	/// At the end of a round, all the snapshot related data must be cleared. Clearing the
	/// snapshot data **MUST* only be performed only during `Phase::Off`.
	fn kill() {
		debug_assert_eq!(<CurrentPhase<T>>::get(), Phase::Off);

		let _ = PagedVoterSnapshot::<T>::clear(u32::MAX, None);
		let _ = TargetSnapshot::<T>::kill();
	}
}

#[cfg(any(test, feature = "runtime-benchmarks"))]
impl<T: Config> Snapshot<T> {
	pub(crate) fn ensure() -> Result<(), &'static str> {
		let check_voter_snapshots = |upto: u32| {
			for page in (upto..=Pallet::<T>::msp()).rev() {
				ensure!(
					Self::voters(page).is_some(),
					"at least one page of the snapshot does not exist"
				);
			}
			Ok(())
		};

		match Pallet::<T>::current_phase() {
			// if phase off, snapshot should be empty.
			Phase::Off => {
				ensure!(Self::targets().is_none(), "phase off, no target snapshot expected.");
				for page in (Pallet::<T>::lsp()..=Pallet::<T>::msp()).rev() {
					ensure!(
						Self::voters(page).is_none(),
						"phase off, no voter snapshot page expected."
					);
				}
				Ok(())
			},
			Phase::Snapshot(idx) => {
				ensure!(Self::targets().is_some(), "target snapshot does not exist.");
				if idx < T::Pages::get() {
					check_voter_snapshots(idx)
				} else {
					Ok(())
				}
			},
			Phase::Signed | Phase::Unsigned(_) => {
				// snapshot should exist during signed and unsigned phases.
				ensure!(
					Self::targets().is_some(),
					"target snapshot not available in signed/unsigned phases."
				);
				check_voter_snapshots(T::Pages::get())?;

				Ok(())
			},
			_ => Ok(()),
		}
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
	/// Based on the contract with `ElectionDataProvider`, this is the first page to be requested
	/// and filled.
	pub fn msp() -> PageIndex {
		T::Pages::get().checked_sub(1).defensive_unwrap_or_default()
	}

	/// Return the least significant page of the snapshot.
	///
	/// Based on the contract with `ElectionDataProvider`, this is the last page to be requested
	/// and filled.
	pub fn lsp() -> PageIndex {
		Zero::zero()
	}

	/// Creates and stores the target snapshot.
	///
	/// Note: the target snapshot is single paged.
	fn create_targets_snapshot() -> Result<u32, ElectionError<T>> {
		let stored_count = Self::create_targets_snapshot_inner(T::TargetSnapshotPerBlock::get())?;
		log!(trace, "created target snapshot with {} targets.", stored_count);

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
					t.try_into().map_err(|_| "too many targets returned by the data provider.")
				})
				.map_err(|e| {
					log!(error, "error fetching electable targets from data provider: {:?}", e);
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
		log!(trace, "created voter snapshot with {} voters.", paged_voters_count);

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

	/// Tries to start the snapshot.
	///
	/// if successful, this will fetch and store the single page target snapshot.
	fn try_start_snapshot() -> Result<Weight, Weight> {
		match Self::create_targets_snapshot() {
			Ok(_) => {
				Self::phase_transition(Phase::Snapshot(T::Pages::get().saturating_sub(1)));

				Ok(T::WeightInfo::create_targets_snapshot_paged(T::TargetSnapshotPerBlock::get()))
			},
			Err(err) => {
				log!(error, "error preparing targets snapshot: {:?}", err);
				Err(Weight::default())
			},
		}
	}

	/// Tries to progress the snapshot.
	///
	/// If successful, this will fetch and store a voter snapshot page per call.
	fn try_progress_snapshot() -> Result<Weight, Weight> {
		// TODO: set data provider lock

		let remaining_pages = match Self::current_phase() {
			Phase::Snapshot(page) => page,
			_ => {
				defensive!("not in snapshot phase");
				return Err(Zero::zero()) // TODO
			},
		};
		debug_assert!(remaining_pages < T::Pages::get());

		if remaining_pages < T::Pages::get() {
			// try progress voter snapshot.
			match Self::create_voters_snapshot(remaining_pages) {
				Ok(_) => {
					Self::phase_transition(Phase::Snapshot(remaining_pages));
					Ok(T::WeightInfo::create_voters_snapshot_paged(T::VoterSnapshotPerBlock::get()))
				},
				Err(err) => {
					log!(error, "error preparing voter snapshot: {:?}", err);
					Self::handle_election_failure();

					// TODO: T::WeightInfo::snapshot_error();
					Err(Weight::default())
				},
			}

			// defensive, should never happen.
		} else {
			defensive!("unexpected page idx snapshot requested");
			Self::handle_election_failure();

			// TODO
			Err(Weight::default())
		}
	}

	/// Export phase.
	///
	/// In practice, we just need to ensure the export phase does not remain open for too long.
	/// During this phase, we expect the external entities to call [`ElectionProvider::elect`] for
	/// all the solution pages. Once the least significant page is called, the phase should
	/// transition to `Phase::Off`. Thus, if the export phase remains open for too long, it means
	/// that the election failed.
	pub(crate) fn do_export_phase(now: BlockNumberFor<T>, started_at: BlockNumberFor<T>) -> Weight {
		debug_assert!(Pallet::<T>::current_phase().is_export());

		if now > started_at + T::ExportPhaseLimit::get() {
			log!(
				error,
				"phase `Export` has been open for too long ({:?} blocks). election round failed.",
				T::ExportPhaseLimit::get(),
			);
			Self::handle_election_failure();
		}

		T::WeightInfo::on_initialize_start_export()
	}

	/// Performs all tasks required after a successful election:
	///
	/// 1. Increment round.
	/// 2. Change phase to [`Phase::Off`].
	/// 3. Clear all snapshot data.
	/// 4. Resets verifier.
	fn rotate_round() {
		<Round<T>>::mutate(|r| r.defensive_saturating_accrue(1));
		Self::phase_transition(Phase::Off);

		Snapshot::<T>::kill();
		<T::Verifier as Verifier>::kill();
	}

	/// Performs all tasks required after an unsuccessful election which should be self-healing
	/// (i.e. the election should restart without entering in emergency phase).
	///
	/// Note: the round should not restart as the previous election failed.
	///
	/// 1. Change phase to [`Phase::Off`].
	/// 2. Clear all snapshot data.
	/// 3. Resets verifier.
	fn reset_round_restart() {
		Self::phase_transition(Phase::Off);

		Snapshot::<T>::kill();
		<T::Verifier as Verifier>::kill();
	}

	/// Handles an election failure.
	fn handle_election_failure() {
		match ElectionFailure::<T>::get() {
			ElectionFailureStrategy::Restart => Self::reset_round_restart(),
			ElectionFailureStrategy::Emergency => Self::phase_transition(Phase::Emergency),
		}
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

	/// Important note: we do exect the caller of `elect` to call pages down to `lsp == 0`.
	/// Otherwise the export phase will not explicitly finish which will result in a failed
	/// election.
	fn elect(remaining: PageIndex) -> Result<BoundedSupportsOf<Self>, Self::Error> {
		ensure!(Pallet::<T>::current_phase().is_export(), ElectionError::ElectionNotReady);

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
					log!(trace, "elect(): provided the last supports page, rotating round.");
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
				Self::handle_election_failure();

				err
			})
	}

	fn ongoing() -> bool {
		match CurrentPhase::<T>::get() {
			Phase::Off | Phase::Emergency => false,
			_ => true,
		}
	}
}
