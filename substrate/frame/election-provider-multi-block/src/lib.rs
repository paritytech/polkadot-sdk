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

//! # Multi-phase, multi-block, election provider pallet.
//!
//! > This pallet is sometimes abbreviated as `EPMB`, and `pallet_election_provider_multi_phase` as
//! > `EPM`.
//!
//! ## Overall idea
//!
//! `pallet_election_provider_multi_phase` provides the basic ability for NPoS solutions to be
//! computed offchain (essentially anywhere) and submitted back to the chain as signed or unsigned
//! transaction, with sensible configurations and fail-safe mechanisms to ensure system safety.
//! Nonetheless, it has a limited capacity in terms of number of voters it can process in a **single
//! block**.
//!
//! This pallet takes `EPM` system, keeps most of its ideas and core premises, and extends it to
//! support paginated, multi-block operations. The final goal of this pallet is to scale linearly
//! with the number of blocks allocated to the elections. Moreover, the amount of work that it does
//! in one block should be bounded and measurable, making it suitable for a parachain. In principle,
//! with large enough blocks (in a dedicated parachain), the number of voters included in the NPoS
//! system can grow significantly (yet, obviously not indefinitely).
//!
//! Note that this pallet does not consider how the recipient is processing the results. To ensure
//! scalability, the recipient of this pallet's data (i.e. `pallet-staking`) must also be capable of
//! pagination and multi-block processing.
//!
//! ## Companion pallets
//!
//! This pallet will only function in a sensible way if it is peered with its companion pallets.
//!
//! - The [`verifier`] pallet provides a standard implementation of the [`verifier::Verifier`]. This
//!   pallet is mandatory.
//! - The [`unsigned`] module provides the implementation of unsigned submission by validators. If
//!   this pallet is included, then [`Config::UnsignedPhase`] will determine its duration.
//! - The [`signed`] module provides the implementation of the signed submission by any account. If
//!   this pallet is included, the combined [`Config::SignedPhase`] and
//!   [`Config::SignedValidationPhase`] will determine its duration
//!
//! These pallets are in fact hierarchical. This particular one is the top level one. It contains
//! the shared information that all child pallets use. All child pallets depend on the top level
//! pallet ONLY, but not the other way around. For those cases, traits are used.
//!
//! As in, notice that [`crate::verifier::Config`] relies on [`crate::Config`], but for the
//! reverse, we rely on [`crate::verifier::Verifier`] trait, which is indeed part of
//! [`crate::Config`]. This is merely an implementation opinion.
//!
//! ### Pallet Ordering:
//!
//! The ordering of these pallets in a runtime should be:
//! 1. parent
//! 2. verifier
//! 3. signed
//! 4. unsigned
//!
//! This is critical for the phase transition to work.
//!
//! > This should be manually checked, there is not automated way to test it.
//!
//! ## Pagination
//!
//! Most of the external APIs of this pallet are paginated. All pagination follow a pattern where if
//! `N` pages exist, the first paginated call is `function(N-1)` and the last one is `function(0)`.
//! For example, with 3 pages, the `elect` of [`ElectionProvider`] is expected to be called as
//! `elect(2) -> elect(1) -> elect(0)`. In essence, calling a paginated function with index 0 is
//! always a signal of termination, meaning that no further calls will follow.
//!
//! The snapshot creation for voters (Nominators in staking), submission of signed pages, validation
//! of signed solutions and exporting of pages are all paginated. Note that this pallet is yet to
//! support paginated target (Validators in staking) snapshotting.
//!
//! ### Terminology Note: `msp` and `lsp`
//!
//! Stand for _most significant page_ (n-1) and _least significant page_ (0).
//!
//! See [`ElectionProvider::msp`] and [`ElectionProvider::lsp`], and their usage.
//!
//! ## Phases
//!
//! The operations in this pallet are divided intor rounds, a `u32` number stored in [`Round`].
//! This value helps this pallet organize itself, and leaves the door open for lazy deletion of any
//! stale data. A round, under the happy path, starts by receiving the call to
//! [`ElectionProvider::start`], and is terminated by receiving a call to
//! [`ElectionProvider::elect`] with value 0.
//!
//! The timeline of pallet is overall as follows:
//!
//! ```ignore
//!  <  Off  >
//! 0 ------------ 12 13 14 15 ----------- 20 ---------25 ------- 30
//! 	           |       |              |            |          |
//! 	     Snapshot      Signed   SignedValidation  Unsigned   Elect
//! ```
//!
//! * Duration of `Snapshot` is determined by [`Config::Pages`] + 1.
//! 	* Whereby in the first page we take the "Targets" snapshot, and in the subsequent pages we take
//!    the voter snapshot.
//! 	* For example, with `Pages = 4`:
//! 		* `Snapshot(4)` -> `Targets(all)`
//! 		* `Snapshot(3)` -> `Voters(3)`
//! 		* `Snapshot(2)` -> `Voters(2)`
//! 		* `Snapshot(1)` -> `Voters(1)`
//! 		* `Snapshot(0)` -> `Voters(0)`
//! * Duration of `Signed`, `SignedValidation` and `Unsigned` are determined by
//!   [`Config::SignedPhase`], [`Config::SignedValidationPhase`] and [`Config::UnsignedPhase`]
//!   respectively.
//! * [`Config::Pages`] calls to elect are expected, but all in all the pallet will close a round
//!   once `elect(0)` is called.
//!
//! > Given this, it is rather important for the user of this pallet to ensure it always terminates
//! > election via `elect` before requesting a new one.
//!
//! ## Feasible Solution (correct solution)
//!
//! All submissions must undergo a feasibility check. Signed solutions are checked one by one at the
//! end of the signed phase, and the unsigned solutions are checked on the spot. A feasible solution
//! is as follows:
//!
//! 0. **all** of the used indices must be correct.
//! 1. present *exactly* correct number of winners.
//! 2. any assignment is checked to match with `PagedVoterSnapshot`.
//! 3. the claimed score is valid, based on the fixed point arithmetic accuracy.
//!
//! More about this in [`verifier`], who is responsible for doing all of the above.
//!
//! ### Fallback and Emergency
//!
//! If at any page, [`ElectionProvider::elect`] fails, a call with the same page-index is dispatched
//! to [`Config::Fallback`]. [`Config::Fallback`] is itself (yet) another implementation of
//! [`ElectionProvider`], and can decide to do anything, but a few reasonable options are provided
//! here:
//!
//! 1. Do nothing: [`Continue`]
//! 2. Force us into the emergency phase: [`crate::InitiateEmergencyPhase`]. This initiates
//!    [`Phase::Emergency`], which will halt almost all operations of this pallet, and it can only
//!    be recovered by [`AdminOperation`], dispatched via [`Call::manage`].
//! 3. compute an onchain from the give page of snapshot.
//!
//! Note that configuring the fallback to be onchain computation is not recommended, unless for
//! test-nets for a number of reasons:
//!
//! 1. The solution score of fallback is never checked to match the "minimum" score. That being
//!    said, the computation happens onchain so we can trust it.
//! 2. The onchain fallback runs on the same number of voters and targets that reside on a single
//!    page of a snapshot, which will very likely be too much for actual onchain computation. Yet,
//!    we don't have another choice as we cannot request another smaller snapshot from the data
//!    provider mid-election without more bookkeeping on the staking side.
//!
//! If onchain solution is to be seriously considered, an improvement to this pallet should
//! re-request a smaller set of voters from `T::DataProvider` in a stateless manner.
//!
//! ### Signed Phase
//!
//! Signed phase is when an offchain miner, aka, `polkadot-staking-miner` should operate upon. See
//! [`signed`] for more information.
//!
//! ## Unsigned Phase
//!
//! Unsigned phase is a built-in fallback in which validators may submit a single page election,
//! taking into account only the [`ElectionProvider::msp`] (_most significant page_). See
//! [`crate::unsigned`] for more information.

// Implementation notes:
//
// - Naming convention is: `${singular}_page` for singular, e.g. `voter_page` for `Vec<Voter>`.
//   `paged_${plural}` for plural, e.g. `paged_voters` for `Vec<Vec<Voter>>`.
//
// - Since this crate has multiple `Pallet` and `Configs`, in each sub-pallet, we only reference the
//   local `Pallet` without a prefix and allow it to be imported via `use`. Avoid `super::Pallet`
//   except for the case of a modules that want to reference their local `Pallet` . The
//   `crate::Pallet` is always reserved for the parent pallet. Other sibling pallets must be
//   referenced with full path, e.g. `crate::Verifier::Pallet`. Do NOT write something like `use
//   unsigned::Pallet as UnsignedPallet`.
//
// - Respecting private storage items with wrapper We move all implementations out of the `mod
//   pallet` as much as possible to ensure we NEVER access the internal storage items directly. All
//   operations should happen with the wrapper types.

#![cfg_attr(not(feature = "std"), no_std)]

use crate::types::*;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_election_provider_support::{
	onchain, BoundedSupportsOf, DataProviderBounds, ElectionDataProvider, ElectionProvider,
	InstantElectionProvider,
};
use frame_support::{
	pallet_prelude::*,
	traits::{Defensive, EnsureOrigin},
	DebugNoBound, Twox64Concat,
};
use frame_system::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_arithmetic::{
	traits::{CheckedAdd, Zero},
	PerThing, UpperOf,
};
use sp_npos_elections::VoteWeight;
use sp_runtime::{
	traits::{Hash, Saturating},
	SaturatedConversion,
};
use sp_std::{borrow::ToOwned, boxed::Box, prelude::*};
use verifier::Verifier;

#[cfg(test)]
mod mock;
#[macro_use]
pub mod helpers;
#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

/// The common logginv prefix of all pallets in this crate.
pub const LOG_PREFIX: &'static str = "runtime::multiblock-election";

macro_rules! clear_paged_map {
	($map: ty) => {{
		let __r = <$map>::clear(u32::MAX, None);
		debug_assert!(__r.unique <= T::Pages::get(), "clearing map caused too many removals")
	}};
}

/// The signed pallet
pub mod signed;
/// Common types of the pallet
pub mod types;
/// The unsigned pallet
pub mod unsigned;
/// The verifier pallet
pub mod verifier;
/// The weight module
pub mod weights;

pub use pallet::*;
pub use types::*;
pub use weights::measured::pallet_election_provider_multi_block::WeightInfo;

/// A fallback implementation that transitions the pallet to the emergency phase.
pub struct InitiateEmergencyPhase<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> ElectionProvider for InitiateEmergencyPhase<T> {
	type AccountId = T::AccountId;
	type BlockNumber = BlockNumberFor<T>;
	type DataProvider = T::DataProvider;
	type Error = &'static str;
	type Pages = T::Pages;
	type MaxBackersPerWinner = <T::Verifier as Verifier>::MaxBackersPerWinner;
	type MaxWinnersPerPage = <T::Verifier as Verifier>::MaxWinnersPerPage;

	fn elect(_page: PageIndex) -> Result<BoundedSupportsOf<Self>, Self::Error> {
		Pallet::<T>::phase_transition(Phase::Emergency);
		Err("Emergency phase started.")
	}

	fn status() -> Result<bool, ()> {
		Ok(true)
	}

	fn start() -> Result<(), Self::Error> {
		Ok(())
	}

	fn duration() -> Self::BlockNumber {
		Zero::zero()
	}
}

impl<T: Config> InstantElectionProvider for InitiateEmergencyPhase<T> {
	fn instant_elect(
		_voters: Vec<VoterOf<T::MinerConfig>>,
		_targets: Vec<Self::AccountId>,
		_desired_targets: u32,
	) -> Result<BoundedSupportsOf<Self>, Self::Error> {
		Self::elect(0)
	}

	fn bother() -> bool {
		false
	}
}

/// A fallback implementation that silently continues into the next page.
///
/// This is suitable for onchain usage.
pub struct Continue<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> ElectionProvider for Continue<T> {
	type AccountId = T::AccountId;
	type BlockNumber = BlockNumberFor<T>;
	type DataProvider = T::DataProvider;
	type Error = &'static str;
	type Pages = T::Pages;
	type MaxBackersPerWinner = <T::Verifier as Verifier>::MaxBackersPerWinner;
	type MaxWinnersPerPage = <T::Verifier as Verifier>::MaxWinnersPerPage;

	fn elect(_page: PageIndex) -> Result<BoundedSupportsOf<Self>, Self::Error> {
		log!(warn, "'Continue' fallback will do nothing");
		Err("'Continue' fallback will do nothing")
	}

	fn start() -> Result<(), Self::Error> {
		Ok(())
	}

	fn duration() -> Self::BlockNumber {
		Zero::zero()
	}

	fn status() -> Result<bool, ()> {
		Ok(true)
	}
}

impl<T: Config> InstantElectionProvider for Continue<T> {
	fn instant_elect(
		_voters: Vec<VoterOf<T::MinerConfig>>,
		_targets: Vec<Self::AccountId>,
		_desired_targets: u32,
	) -> Result<BoundedSupportsOf<Self>, Self::Error> {
		Self::elect(0)
	}

	fn bother() -> bool {
		false
	}
}

/// A easy means to configure [`Config::AreWeDone`].
///
/// With this, you can say what to do if a solution is queued, or what to do if not.
///
/// Two common shorthands of this are provided:
/// * [`ProceedRegardlessOf`]
/// * [`RevertToSignedIfNotQueuedOf`]
pub struct IfSolutionQueuedElse<T, Queued, NotQueued>(
	sp_std::marker::PhantomData<(T, Queued, NotQueued)>,
);

/// A `Get` impl for `Phase::Done`
pub struct GetDone<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> Get<Phase<T>> for GetDone<T> {
	fn get() -> Phase<T> {
		Phase::Done
	}
}

/// A `Get` impl for `Phase::Signed(T::SignedPhase::get())`
pub struct GetSigned<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> Get<Phase<T>> for GetSigned<T> {
	fn get() -> Phase<T> {
		Phase::Signed(T::SignedPhase::get().saturating_sub(1u32.into()))
	}
}

/// A shorthand for [`IfSolutionQueuedElse`] that proceeds regardless of the solution being queued.
pub type ProceedRegardlessOf<T> = IfSolutionQueuedElse<T, GetDone<T>, GetDone<T>>;

/// A shorthand for [`IfSolutionQueuedElse`] that proceeds to `Phase::Done` if the solution is
/// queued. Otherwise, it proceeds to `Phase::Signed`.
pub type RevertToSignedIfNotQueuedOf<T> = IfSolutionQueuedElse<T, GetDone<T>, GetSigned<T>>;

impl<T: Config, Queued, NotQueued> IfSolutionQueuedElse<T, Queued, NotQueued> {
	fn something_queued() -> bool {
		let queued_score = <T::Verifier as verifier::Verifier>::queued_score().is_some();
		#[cfg(debug_assertions)]
		{
			let any_pages_queued = (Pallet::<T>::lsp()..=Pallet::<T>::msp()).any(|p| {
				<T::Verifier as verifier::Verifier>::get_queued_solution_page(p).is_some()
			});
			assert_eq!(
				queued_score, any_pages_queued,
				"queued score ({}) and queued pages ({}) must match",
				queued_score, any_pages_queued
			);
		}
		queued_score
	}
}

impl<T: Config, Queued: Get<Phase<T>>, NotQueued: Get<Phase<T>>> Get<Phase<T>>
	for IfSolutionQueuedElse<T, Queued, NotQueued>
{
	fn get() -> Phase<T> {
		if Self::something_queued() {
			Queued::get()
		} else {
			NotQueued::get()
		}
	}
}

/// Internal errors of the pallet. This is used in the implementation of [`ElectionProvider`].
///
/// Note that this is different from [`pallet::Error`].
#[derive(
	frame_support::DebugNoBound, frame_support::PartialEqNoBound, frame_support::EqNoBound,
)]
pub enum ElectionError<T: Config> {
	/// An error happened in the feasibility check sub-system.
	Feasibility(verifier::FeasibilityError),
	/// An error in the fallback.
	Fallback(FallbackErrorOf<T>),
	/// An error in the onchain seq-phragmen implementation
	OnChain(onchain::Error),
	/// An error happened in the data provider.
	DataProvider(&'static str),
	/// the corresponding page in the queued supports is not available.
	SupportPageNotAvailable,
	/// The election is not ongoing and therefore no results may be queried.
	NotOngoing,
	/// The election is currently ongoing, and therefore we cannot start again.
	Ongoing,
	/// Other misc error
	Other(&'static str),
}

impl<T: Config> From<onchain::Error> for ElectionError<T> {
	fn from(e: onchain::Error) -> Self {
		ElectionError::OnChain(e)
	}
}

impl<T: Config> From<verifier::FeasibilityError> for ElectionError<T> {
	fn from(e: verifier::FeasibilityError) -> Self {
		ElectionError::Feasibility(e)
	}
}

/// Different operations that the [`Config::AdminOrigin`] can perform on the pallet.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	DebugNoBound,
	CloneNoBound,
	PartialEqNoBound,
	EqNoBound,
)]
#[codec(mel_bound(T: Config))]
#[scale_info(skip_type_params(T))]
pub enum AdminOperation<T: Config> {
	/// Forcefully go to the next round, starting from the Off Phase.
	ForceRotateRound,
	/// Force-set the phase to the given phase.
	///
	/// This can have many many combinations, use only with care and sufficient testing.
	ForceSetPhase(Phase<T>),
	/// Set the given (single page) emergency solution.
	///
	/// Can only be called in emergency phase.
	EmergencySetSolution(Box<BoundedSupportsOf<Pallet<T>>>, ElectionScore),
	/// Trigger the (single page) fallback in `instant` mode, with the given parameters, and
	/// queue it if correct.
	///
	/// Can only be called in emergency phase.
	EmergencyFallback,
	/// Set the minimum untrusted score. This is directly communicated to the verifier component to
	/// be taken into account.
	///
	/// This is useful in preventing any serious issue where due to a bug we accept a very bad
	/// solution.
	SetMinUntrustedScore(ElectionScore),
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Duration of the unsigned phase.
		#[pallet::constant]
		type UnsignedPhase: Get<BlockNumberFor<Self>>;
		/// Duration of the signed phase.
		#[pallet::constant]
		type SignedPhase: Get<BlockNumberFor<Self>>;
		/// Duration of the singed validation phase.
		///
		/// The duration of this should not be less than `T::Pages`, and there is no point in it
		/// being more than `SignedPhase::MaxSubmission::get() * T::Pages`. TODO: integrity test for
		/// it.
		#[pallet::constant]
		type SignedValidationPhase: Get<BlockNumberFor<Self>>;

		/// The number of snapshot voters to fetch per block.
		#[pallet::constant]
		type VoterSnapshotPerBlock: Get<u32>;

		/// The number of snapshot targets to fetch per block.
		#[pallet::constant]
		type TargetSnapshotPerBlock: Get<u32>;

		/// The number of pages.
		///
		/// The snapshot is created with this many keys in the storage map.
		///
		/// The solutions may contain at MOST this many pages, but less pages are acceptable as
		/// well.
		#[pallet::constant]
		type Pages: Get<PageIndex>;

		/// Something that will provide the election data.
		type DataProvider: ElectionDataProvider<
			AccountId = Self::AccountId,
			BlockNumber = BlockNumberFor<Self>,
		>;

		/// The miner configuration.
		///
		/// These configurations are passed to [`crate::unsigned::miner::BaseMiner`]. An external
		/// miner implementation should implement this trait, and use the said `BaseMiner`.
		type MinerConfig: crate::unsigned::miner::MinerConfig<
			Pages = Self::Pages,
			AccountId = <Self as frame_system::Config>::AccountId,
			MaxVotesPerVoter = <Self::DataProvider as ElectionDataProvider>::MaxVotesPerVoter,
			VoterSnapshotPerBlock = Self::VoterSnapshotPerBlock,
			TargetSnapshotPerBlock = Self::TargetSnapshotPerBlock,
			MaxBackersPerWinner = <Self::Verifier as verifier::Verifier>::MaxBackersPerWinner,
			MaxWinnersPerPage = <Self::Verifier as verifier::Verifier>::MaxWinnersPerPage,
		>;

		/// The fallback type used for the election.
		type Fallback: InstantElectionProvider<
			AccountId = Self::AccountId,
			BlockNumber = BlockNumberFor<Self>,
			DataProvider = Self::DataProvider,
			MaxBackersPerWinner = <Self::Verifier as verifier::Verifier>::MaxBackersPerWinner,
			MaxWinnersPerPage = <Self::Verifier as verifier::Verifier>::MaxWinnersPerPage,
		>;

		/// The verifier pallet's interface.
		type Verifier: verifier::Verifier<
				Solution = SolutionOf<Self::MinerConfig>,
				AccountId = Self::AccountId,
			> + verifier::AsynchronousVerifier;

		/// The origin that can perform administration operations on this pallet.
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// An indicator of whether we should move to do the [`crate::types::Phase::Done`] or not?
		/// This is called at the end of the election process.
		///
		/// Common implementation is [`ProceedRegardlessOf`] or [`RevertToSignedIfNotQueuedOf`].
		type AreWeDone: Get<Phase<Self>>;

		/// The weight of the pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Manage this pallet.
		///
		/// The origin of this call must be [`Config::AdminOrigin`].
		///
		/// See [`AdminOperation`] for various operations that are possible.
		#[pallet::weight(T::WeightInfo::manage())]
		#[pallet::call_index(0)]
		pub fn manage(origin: OriginFor<T>, op: AdminOperation<T>) -> DispatchResultWithPostInfo {
			use crate::verifier::Verifier;
			use sp_npos_elections::EvaluateSupport;

			let _ = T::AdminOrigin::ensure_origin(origin);
			match op {
				AdminOperation::EmergencyFallback => {
					ensure!(Self::current_phase() == Phase::Emergency, Error::<T>::UnexpectedPhase);
					// note: for now we run this on the msp, but we can make it configurable if need
					// be.
					let voters = Snapshot::<T>::voters(Self::msp()).ok_or(Error::<T>::Snapshot)?;
					let targets = Snapshot::<T>::targets().ok_or(Error::<T>::Snapshot)?;
					let desired_targets =
						Snapshot::<T>::desired_targets().ok_or(Error::<T>::Snapshot)?;
					let fallback = T::Fallback::instant_elect(
						voters.into_inner(),
						targets.into_inner(),
						desired_targets,
					)
					.map_err(|e| {
						log!(warn, "Fallback failed: {:?}", e);
						Error::<T>::Fallback
					})?;
					let score = fallback.evaluate();
					T::Verifier::force_set_single_page_valid(fallback, 0, score);
					Ok(().into())
				},
				AdminOperation::EmergencySetSolution(supports, score) => {
					ensure!(Self::current_phase() == Phase::Emergency, Error::<T>::UnexpectedPhase);
					// TODO: hardcoding zero here doesn't make a lot of sense
					T::Verifier::force_set_single_page_valid(*supports, 0, score);
					Ok(().into())
				},
				AdminOperation::ForceSetPhase(phase) => {
					Self::phase_transition(phase);
					Ok(().into())
				},
				AdminOperation::ForceRotateRound => {
					Self::rotate_round();
					Ok(().into())
				},
				AdminOperation::SetMinUntrustedScore(score) => {
					T::Verifier::set_minimum_score(score);
					Ok(().into())
				},
			}
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_now: BlockNumberFor<T>) -> Weight {
			let current_phase = CurrentPhase::<T>::get();
			let weight1 = match current_phase {
				Phase::Snapshot(x) if x == T::Pages::get() => {
					// create the target snapshot
					Self::create_targets_snapshot().defensive_unwrap_or_default();
					T::WeightInfo::on_initialize_into_snapshot_msp()
				},
				Phase::Snapshot(x) => {
					// create voter snapshot
					Self::create_voters_snapshot_paged(x).unwrap();
					T::WeightInfo::on_initialize_into_snapshot_rest()
				},
				_ => T::WeightInfo::on_initialize_nothing(),
			};

			// in all cases, go to next phase
			let next_phase = current_phase.next();

			let weight2 = match next_phase {
				Phase::Signed(_) => T::WeightInfo::on_initialize_into_signed(),
				Phase::SignedValidation(_) => T::WeightInfo::on_initialize_into_signed_validation(),
				Phase::Unsigned(_) => T::WeightInfo::on_initialize_into_unsigned(),
				_ => T::WeightInfo::on_initialize_nothing(),
			};

			Self::phase_transition(next_phase);

			// bit messy, but for now this works best.
			#[cfg(test)]
			{
				let test_election_start: BlockNumberFor<T> =
					(crate::mock::ElectionStart::get() as u32).into();
				if _now == test_election_start {
					crate::log!(info, "TESTING: Starting election at block {}", _now);
					crate::mock::MultiBlock::start().unwrap();
				}
			}

			weight1 + weight2
		}

		fn integrity_test() {
			use sp_std::mem::size_of;
			// The index type of both voters and targets need to be smaller than that of usize (very
			// unlikely to be the case, but anyhow).
			assert!(size_of::<SolutionVoterIndexOf<T::MinerConfig>>() <= size_of::<usize>());
			assert!(size_of::<SolutionTargetIndexOf<T::MinerConfig>>() <= size_of::<usize>());

			// also, because `VoterSnapshotPerBlock` and `TargetSnapshotPerBlock` are in u32, we
			// assert that both of these types are smaller than u32 as well.
			assert!(size_of::<SolutionVoterIndexOf<T::MinerConfig>>() <= size_of::<u32>());
			assert!(size_of::<SolutionTargetIndexOf<T::MinerConfig>>() <= size_of::<u32>());

			// pages must be at least 1.
			assert!(T::Pages::get() > 0);

			// Based on the requirements of [`sp_npos_elections::Assignment::try_normalize`].
			let max_vote: usize = <SolutionOf<T::MinerConfig> as NposSolution>::LIMIT;

			// 2. Maximum sum of [SolutionAccuracy; 16] must fit into `UpperOf<OffchainAccuracy>`.
			let maximum_chain_accuracy: Vec<UpperOf<SolutionAccuracyOf<T::MinerConfig>>> = (0..
				max_vote)
				.map(|_| {
					<UpperOf<SolutionAccuracyOf<T::MinerConfig>>>::from(
						<SolutionAccuracyOf<T::MinerConfig>>::one().deconstruct(),
					)
				})
				.collect();
			let _: UpperOf<SolutionAccuracyOf<T::MinerConfig>> = maximum_chain_accuracy
				.iter()
				.fold(Zero::zero(), |acc, x| acc.checked_add(x).unwrap());

			// We only accept data provider who's maximum votes per voter matches our
			// `T::Solution`'s `LIMIT`.
			//
			// NOTE that this pallet does not really need to enforce this in runtime. The
			// solution cannot represent any voters more than `LIMIT` anyhow.
			assert_eq!(
				<T::DataProvider as ElectionDataProvider>::MaxVotesPerVoter::get(),
				<SolutionOf<T::MinerConfig> as NposSolution>::LIMIT as u32,
			);

			// Either (signed + signed validation) is non-zero, or unsigned is non-zero
			let has_signed = !T::SignedPhase::get().is_zero();
			let signed_validation = T::SignedValidationPhase::get();
			let has_signed_validation = !signed_validation.is_zero();
			let has_unsigned = !T::UnsignedPhase::get().is_zero();
			assert!(
				has_signed == has_signed_validation,
				"Signed phase not set correct -- both should be set or unset"
			);
			assert!(
				signed_validation.is_zero() || signed_validation >= T::Pages::get().into(),
				"signed validation phase should be at least as long as the number of pages."
			);

			assert!(has_signed || has_unsigned, "either signed or unsigned phase must be set");
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(now: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state(now).map_err(Into::into)
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A phase transition happened. Only checks major changes in the variants, not minor inner
		/// values.
		PhaseTransitioned {
			/// the source phase
			from: Phase<T>,
			/// The target phase
			to: Phase<T>,
		},
	}

	/// Error of the pallet that can be returned in response to dispatches.
	#[pallet::error]
	pub enum Error<T> {
		/// Triggering the `Fallback` failed.
		Fallback,
		/// Unexpected phase
		UnexpectedPhase,
		/// Snapshot was unavailable.
		Snapshot,
	}

	/// Common errors in all sub-pallets and miner.
	#[derive(PartialEq, Eq, Clone, Encode, Decode, Debug)]
	pub enum CommonError {
		/// Submission is too early (or too late, depending on your point of reference).
		EarlySubmission,
		/// The round counter is wrong.
		WrongRound,
		/// Submission is too weak to be considered an improvement.
		WeakSubmission,
		/// Wrong number of pages in the solution.
		WrongPageCount,
		/// Wrong number of winners presented.
		WrongWinnerCount,
		/// The snapshot fingerprint is not a match. The solution is likely outdated.
		WrongFingerprint,
		/// Snapshot was not available.
		Snapshot,
	}

	/// Internal counter for the number of rounds.
	///
	/// This is useful for de-duplication of transactions submitted to the pool, and general
	/// diagnostics of the pallet.
	///
	/// This is merely incremented once per every time that an upstream `elect` is called.
	#[pallet::storage]
	#[pallet::getter(fn round)]
	pub type Round<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Current phase.
	#[pallet::storage]
	#[pallet::getter(fn current_phase)]
	pub type CurrentPhase<T: Config> = StorageValue<_, Phase<T>, ValueQuery>;

	/// Wrapper struct for working with snapshots.
	///
	/// It manages the following storage items:
	///
	/// - `DesiredTargets`: The number of targets that we wish to collect.
	/// - `PagedVoterSnapshot`: Paginated map of voters.
	/// - `PagedVoterSnapshotHash`: Hash of the aforementioned.
	/// - `PagedTargetSnapshot`: Paginated map of targets.
	/// - `PagedTargetSnapshotHash`: Hash of the aforementioned.
	///
	/// ### Invariants
	///
	/// The following invariants must be met at **all times** for this storage item to be "correct".
	///
	/// - `PagedVoterSnapshotHash` must always contain the correct the same number of keys, and the
	///   corresponding hash of the `PagedVoterSnapshot`.
	/// - `PagedTargetSnapshotHash` must always contain the correct the same number of keys, and the
	///   corresponding hash of the `PagedTargetSnapshot`.
	///
	/// - If any page from the paged voters/targets exists, then the aforementioned (desired
	///   targets) must also exist.
	///
	/// The following invariants might need to hold based on the current phase.
	///
	///   - If `Phase` IS `Snapshot(_)`, then partial voter/target pages must exist from `msp` to
	///     `lsp` based on the inner value.
	///   - If `Phase` IS `Off`, then, no snapshot must exist.
	///   - In all other phases, the snapshot must FULLY exist.
	pub(crate) struct Snapshot<T>(sp_std::marker::PhantomData<T>);
	impl<T: Config> Snapshot<T> {
		// ----------- mutable methods
		pub(crate) fn set_desired_targets(d: u32) {
			DesiredTargets::<T>::put(d);
		}

		pub(crate) fn set_targets(targets: BoundedVec<T::AccountId, T::TargetSnapshotPerBlock>) {
			let hash = Self::write_storage_with_pre_allocate(
				&PagedTargetSnapshot::<T>::hashed_key_for(Pallet::<T>::msp()),
				targets,
			);
			PagedTargetSnapshotHash::<T>::insert(Pallet::<T>::msp(), hash);
		}

		pub(crate) fn set_voters(page: PageIndex, voters: VoterPageOf<T::MinerConfig>) {
			let hash = Self::write_storage_with_pre_allocate(
				&PagedVoterSnapshot::<T>::hashed_key_for(page),
				voters,
			);
			PagedVoterSnapshotHash::<T>::insert(page, hash);
		}

		/// Destroy the entire snapshot.
		///
		/// Should be called only once we transition to [`Phase::Off`].
		pub(crate) fn kill() {
			DesiredTargets::<T>::kill();
			clear_paged_map!(PagedVoterSnapshot::<T>);
			clear_paged_map!(PagedVoterSnapshotHash::<T>);
			clear_paged_map!(PagedTargetSnapshot::<T>);
			clear_paged_map!(PagedTargetSnapshotHash::<T>);
		}

		// ----------- non-mutables
		pub(crate) fn desired_targets() -> Option<u32> {
			DesiredTargets::<T>::get()
		}

		pub(crate) fn voters(page: PageIndex) -> Option<VoterPageOf<T::MinerConfig>> {
			PagedVoterSnapshot::<T>::get(page)
		}

		pub(crate) fn targets() -> Option<BoundedVec<T::AccountId, T::TargetSnapshotPerBlock>> {
			// NOTE: targets always have one index, which is 0, aka lsp.
			PagedTargetSnapshot::<T>::get(Pallet::<T>::msp())
		}

		/// Get a fingerprint of the snapshot, from all the hashes that are stored for each page of
		/// the snapshot.
		///
		/// This is computed as: `(target_hash, voter_hash_n, voter_hash_(n-1), ..., voter_hash_0)`
		/// where `n` is `T::Pages - 1`. In other words, it is the concatenated hash of targets, and
		/// voters, from `msp` to `lsp`.
		pub fn fingerprint() -> T::Hash {
			let mut hashed_target_and_voters =
				Self::targets_hash().unwrap_or_default().as_ref().to_vec();
			let hashed_voters = (Pallet::<T>::msp()..=Pallet::<T>::lsp())
				.map(|i| PagedVoterSnapshotHash::<T>::get(i).unwrap_or_default())
				.flat_map(|hash| <T::Hash as AsRef<[u8]>>::as_ref(&hash).to_owned())
				.collect::<Vec<u8>>();
			hashed_target_and_voters.extend(hashed_voters);
			T::Hashing::hash(&hashed_target_and_voters)
		}

		fn write_storage_with_pre_allocate<E: Encode>(key: &[u8], data: E) -> T::Hash {
			let size = data.encoded_size();
			let mut buffer = Vec::with_capacity(size);
			data.encode_to(&mut buffer);

			let hash = T::Hashing::hash(&buffer);

			// do some checks.
			debug_assert_eq!(buffer, data.encode());
			// buffer should have not re-allocated since.
			debug_assert!(buffer.len() == size && size == buffer.capacity());
			sp_io::storage::set(key, &buffer);

			hash
		}

		pub(crate) fn targets_hash() -> Option<T::Hash> {
			PagedTargetSnapshotHash::<T>::get(Pallet::<T>::msp())
		}
	}

	#[allow(unused)]
	#[cfg(any(test, feature = "runtime-benchmarks", feature = "try-runtime"))]
	impl<T: Config> Snapshot<T> {
		///Ensure target snapshot exists.
		pub(crate) fn ensure_target_snapshot(exists: bool) -> Result<(), &'static str> {
			ensure!(exists ^ Self::desired_targets().is_none(), "desired target mismatch");
			ensure!(exists ^ Self::targets().is_none(), "targets mismatch");
			ensure!(exists ^ Self::targets_hash().is_none(), "targets hash mismatch");

			// and the hash is correct.
			if let Some(targets) = Self::targets() {
				let hash = Self::targets_hash().expect("must exist; qed");
				ensure!(hash == T::Hashing::hash(&targets.encode()), "targets hash mismatch");
			}
			Ok(())
		}

		/// Ensure voters exists, from page `T::Pages::get()` for `up_to_page` subsequent pages.
		pub(crate) fn ensure_voter_snapshot(
			exists: bool,
			mut up_to_page: PageIndex,
		) -> Result<(), &'static str> {
			up_to_page = up_to_page.min(T::Pages::get());
			// ensure that voter pages that should exist, indeed to exist..
			let mut sum_existing_voters: usize = 0;
			for p in (crate::Pallet::<T>::lsp()..=crate::Pallet::<T>::msp())
				.rev()
				.take(up_to_page as usize)
			{
				ensure!(
					(exists ^ Self::voters(p).is_none()) &&
						(exists ^ Self::voters_hash(p).is_none()),
					"voter page existence mismatch"
				);

				if let Some(voters_page) = Self::voters(p) {
					sum_existing_voters = sum_existing_voters.saturating_add(voters_page.len());
					let hash = Self::voters_hash(p).expect("must exist; qed");
					ensure!(hash == T::Hashing::hash(&voters_page.encode()), "voter hash mismatch");
				}
			}

			// ..and those that should not exist, indeed DON'T.
			for p in (crate::Pallet::<T>::lsp()..=crate::Pallet::<T>::msp())
				.take((T::Pages::get() - up_to_page) as usize)
			{
				ensure!(
					(exists ^ Self::voters(p).is_some()) &&
						(exists ^ Self::voters_hash(p).is_some()),
					"voter page non-existence mismatch"
				);
			}
			Ok(())
		}

		pub(crate) fn ensure_snapshot(
			exists: bool,
			mut up_to_page: PageIndex,
		) -> Result<(), &'static str> {
			Self::ensure_target_snapshot(exists)
				.and_then(|_| Self::ensure_voter_snapshot(exists, up_to_page))
		}

		pub(crate) fn ensure_full_snapshot() -> Result<(), &'static str> {
			// if any number of pages supposed to exist, these must also exist.
			ensure!(Self::desired_targets().is_some(), "desired target mismatch");
			ensure!(Self::targets_hash().is_some(), "targets hash mismatch");
			ensure!(
				Self::targets_decode_len().unwrap_or_default() as u32 ==
					T::TargetSnapshotPerBlock::get(),
				"targets decode length mismatch"
			);

			// ensure that voter pages that should exist, indeed to exist..
			for p in crate::Pallet::<T>::lsp()..=crate::Pallet::<T>::msp() {
				ensure!(
					Self::voters_hash(p).is_some() &&
						Self::voters_decode_len(p).unwrap_or_default() as u32 ==
							T::VoterSnapshotPerBlock::get(),
					"voter page existence mismatch"
				);
			}

			Ok(())
		}

		pub(crate) fn voters_decode_len(page: PageIndex) -> Option<usize> {
			PagedVoterSnapshot::<T>::decode_len(page)
		}

		pub(crate) fn targets_decode_len() -> Option<usize> {
			PagedTargetSnapshot::<T>::decode_len(Pallet::<T>::msp())
		}

		pub(crate) fn voters_hash(page: PageIndex) -> Option<T::Hash> {
			PagedVoterSnapshotHash::<T>::get(page)
		}

		pub(crate) fn sanity_check() -> Result<(), &'static str> {
			// check the snapshot existence based on the phase. This checks all of the needed
			// conditions except for the metadata values.
			let phase = Pallet::<T>::current_phase();
			let _ = match phase {
				// no page should exist in this phase.
				Phase::Off => Self::ensure_snapshot(false, T::Pages::get()),

				// we will star the snapshot in the next phase.
				Phase::Snapshot(p) if p == T::Pages::get() =>
					Self::ensure_snapshot(false, T::Pages::get()),
				// we are mid voter snapshot.
				Phase::Snapshot(p) if p < T::Pages::get() && p > 0 =>
					Self::ensure_snapshot(true, T::Pages::get() - p - 1),
				// we cannot check anything in this block -- we take the last page of the snapshot.
				Phase::Snapshot(_) => Ok(()),

				// full snapshot must exist in these phases.
				Phase::Emergency |
				Phase::Signed(_) |
				Phase::SignedValidation(_) |
				Phase::Export(_) |
				Phase::Done |
				Phase::Unsigned(_) => Self::ensure_snapshot(true, T::Pages::get()),
			}?;

			Ok(())
		}
	}

	#[cfg(test)]
	impl<T: Config> Snapshot<T> {
		pub(crate) fn voter_pages() -> PageIndex {
			use sp_runtime::SaturatedConversion;
			PagedVoterSnapshot::<T>::iter().count().saturated_into::<PageIndex>()
		}

		pub(crate) fn target_pages() -> PageIndex {
			use sp_runtime::SaturatedConversion;
			PagedTargetSnapshot::<T>::iter().count().saturated_into::<PageIndex>()
		}

		pub(crate) fn voters_iter_flattened() -> impl Iterator<Item = VoterOf<T::MinerConfig>> {
			let key_range =
				(crate::Pallet::<T>::lsp()..=crate::Pallet::<T>::msp()).collect::<Vec<_>>();
			key_range
				.into_iter()
				.flat_map(|k| PagedVoterSnapshot::<T>::get(k).unwrap_or_default())
		}

		pub(crate) fn remove_voter_page(page: PageIndex) {
			PagedVoterSnapshot::<T>::remove(page);
		}

		pub(crate) fn kill_desired_targets() {
			DesiredTargets::<T>::kill();
		}

		pub(crate) fn remove_target_page() {
			PagedTargetSnapshot::<T>::remove(Pallet::<T>::msp());
		}

		pub(crate) fn remove_target(at: usize) {
			PagedTargetSnapshot::<T>::mutate(crate::Pallet::<T>::msp(), |maybe_targets| {
				if let Some(targets) = maybe_targets {
					targets.remove(at);
					// and update the hash.
					PagedTargetSnapshotHash::<T>::insert(
						crate::Pallet::<T>::msp(),
						T::Hashing::hash(&targets.encode()),
					)
				} else {
					unreachable!();
				}
			})
		}
	}

	/// Desired number of targets to elect for this round.
	#[pallet::storage]
	type DesiredTargets<T> = StorageValue<_, u32>;
	/// Paginated voter snapshot. At most [`T::Pages`] keys will exist.
	#[pallet::storage]
	type PagedVoterSnapshot<T: Config> =
		StorageMap<_, Twox64Concat, PageIndex, VoterPageOf<T::MinerConfig>>;
	/// Same as [`PagedVoterSnapshot`], but it will store the hash of the snapshot.
	///
	/// The hash is generated using [`frame_system::Config::Hashing`].
	#[pallet::storage]
	type PagedVoterSnapshotHash<T: Config> = StorageMap<_, Twox64Concat, PageIndex, T::Hash>;
	/// Paginated target snapshot.
	///
	/// For the time being, since we assume one pages of targets, at most ONE key will exist.
	#[pallet::storage]
	type PagedTargetSnapshot<T: Config> =
		StorageMap<_, Twox64Concat, PageIndex, BoundedVec<T::AccountId, T::TargetSnapshotPerBlock>>;
	/// Same as [`PagedTargetSnapshot`], but it will store the hash of the snapshot.
	///
	/// The hash is generated using [`frame_system::Config::Hashing`].
	#[pallet::storage]
	type PagedTargetSnapshotHash<T: Config> = StorageMap<_, Twox64Concat, PageIndex, T::Hash>;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);
}

impl<T: Config> Pallet<T> {
	/// Returns the most significant page of the snapshot.
	///
	/// Based on the contract of `ElectionDataProvider`, this is the first page that is filled.
	fn msp() -> PageIndex {
		T::Pages::get().checked_sub(1).defensive_unwrap_or_default()
	}

	/// Returns the least significant page of the snapshot.
	///
	/// Based on the contract of `ElectionDataProvider`, this is the last page that is filled.
	fn lsp() -> PageIndex {
		Zero::zero()
	}

	/// Return the `length` most significant pages.
	///
	/// For example, if `Pages = 4`, and `length = 2`, our full snapshot range would be [0,
	/// 1, 2, 3], with 3 being msp. But, in this case, then this returns `[2, 3]` two most
	/// significant pages, in the old order.
	pub fn msp_range_for(length: usize) -> Vec<PageIndex> {
		(Self::lsp()..Self::msp() + 1).rev().take(length).rev().collect::<Vec<_>>()
	}

	pub(crate) fn phase_transition(to: Phase<T>) {
		let from = Self::current_phase();
		use sp_std::mem::discriminant;
		if discriminant(&from) != discriminant(&to) {
			log!(debug, "transitioning phase from {:?} to {:?}", from, to);
			Self::deposit_event(Event::PhaseTransitioned { from, to });
		} else {
			log!(trace, "transitioning phase from {:?} to {:?}", from, to);
		}
		<CurrentPhase<T>>::put(to);
	}

	/// Perform all the basic checks that are independent of the snapshot. To be more specific,
	/// these are all the checks that you can do without the need to read the massive blob of the
	/// actual snapshot. This function only contains a handful of storage reads, with bounded size.
	///
	/// A sneaky detail is that this does check the `DesiredTargets` aspect of the snapshot, but
	/// neither of the large storage items.
	///
	/// Moreover, we do optionally check the fingerprint of the snapshot, if provided.
	///
	/// These complement a feasibility-check, which is exactly the opposite: snapshot-dependent
	/// checks.
	pub(crate) fn snapshot_independent_checks(
		paged_solution: &PagedRawSolution<T::MinerConfig>,
		maybe_snapshot_fingerprint: Option<T::Hash>,
	) -> Result<(), CommonError> {
		// Note that the order of these checks are critical for the correctness and performance of
		// `restore_or_compute_then_maybe_submit`. We want to make sure that we always check round
		// first, so that if it has a wrong round, we can detect and delete it from the cache right
		// from the get go.

		// ensure round is current
		ensure!(Self::round() == paged_solution.round, CommonError::WrongRound);

		// ensure score is being improved, if the claim is even correct.
		ensure!(
			<T::Verifier as Verifier>::ensure_claimed_score_improves(paged_solution.score),
			CommonError::WeakSubmission,
		);

		// ensure solution pages are no more than the snapshot
		ensure!(
			paged_solution.solution_pages.len().saturated_into::<PageIndex>() <= T::Pages::get(),
			CommonError::WrongPageCount
		);

		// finally, check the winner count being correct.
		if let Some(desired_targets) = Snapshot::<T>::desired_targets() {
			ensure!(
				desired_targets == paged_solution.winner_count_single_page_target_snapshot() as u32,
				CommonError::WrongWinnerCount
			)
		}

		// check the snapshot fingerprint, if asked for.
		ensure!(
			maybe_snapshot_fingerprint
				.map_or(true, |snapshot_fingerprint| Snapshot::<T>::fingerprint() ==
					snapshot_fingerprint),
			CommonError::WrongFingerprint
		);

		Ok(())
	}

	/// Creates the target snapshot.
	pub(crate) fn create_targets_snapshot() -> Result<(), ElectionError<T>> {
		// if requested, get the targets as well.
		Snapshot::<T>::set_desired_targets(
			T::DataProvider::desired_targets().map_err(ElectionError::DataProvider)?,
		);

		let count = T::TargetSnapshotPerBlock::get();
		let bounds = DataProviderBounds { count: Some(count.into()), size: None };
		let targets: BoundedVec<_, T::TargetSnapshotPerBlock> =
			T::DataProvider::electable_targets(bounds, 0)
				.and_then(|v| v.try_into().map_err(|_| "try-into failed"))
				.map_err(ElectionError::DataProvider)?;

		let count = targets.len() as u32;
		log!(debug, "created target snapshot with {} targets.", count);
		Snapshot::<T>::set_targets(targets);

		Ok(())
	}

	/// Creates the voter snapshot.
	pub(crate) fn create_voters_snapshot_paged(
		remaining: PageIndex,
	) -> Result<(), ElectionError<T>> {
		let count = T::VoterSnapshotPerBlock::get();
		let bounds = DataProviderBounds { count: Some(count.into()), size: None };
		let voters: BoundedVec<_, T::VoterSnapshotPerBlock> =
			T::DataProvider::electing_voters(bounds, remaining)
				.and_then(|v| v.try_into().map_err(|_| "try-into failed"))
				.map_err(ElectionError::DataProvider)?;

		let count = voters.len() as u32;
		Snapshot::<T>::set_voters(remaining, voters);
		log!(debug, "created voter snapshot with {} voters, {} remaining.", count, remaining);

		Ok(())
	}

	/// Perform the tasks to be done after a new `elect` has been triggered:
	///
	/// 1. Increment round.
	/// 2. Change phase to [`Phase::Off`]
	/// 3. Clear all snapshot data.
	pub(crate) fn rotate_round() {
		// Inc round.
		<Round<T>>::mutate(|r| *r += 1);

		// Phase is off now.
		Self::phase_transition(Phase::Off);

		// Kill everything in the verifier.
		T::Verifier::kill();

		// Kill the snapshot.
		Snapshot::<T>::kill();
	}

	/// Call fallback for the given page.
	///
	/// This uses the [`ElectionProvider::bother`] to check if the fallback is actually going to do
	/// anything. If so, it will re-collect the associated snapshot page and do the fallback. Else,
	/// it will early return without touching the snapshot.
	fn fallback_for_page(page: PageIndex) -> Result<BoundedSupportsOf<Self>, ElectionError<T>> {
		use frame_election_provider_support::InstantElectionProvider;
		let (voters, targets, desired_targets) = if T::Fallback::bother() {
			(
				Snapshot::<T>::voters(page).ok_or(ElectionError::Other("snapshot!"))?,
				Snapshot::<T>::targets().ok_or(ElectionError::Other("snapshot!"))?,
				Snapshot::<T>::desired_targets().ok_or(ElectionError::Other("snapshot!"))?,
			)
		} else {
			(Default::default(), Default::default(), Default::default())
		};
		T::Fallback::instant_elect(voters.into_inner(), targets.into_inner(), desired_targets)
			.map_err(|fe| ElectionError::Fallback(fe))
	}

	/// A reasonable next election block number.
	pub fn average_election_duration() -> u32 {
		let signed: u32 = T::SignedPhase::get().saturated_into();
		let unsigned: u32 = T::UnsignedPhase::get().saturated_into();
		let signed_validation: u32 = T::SignedValidationPhase::get().saturated_into();
		let snapshot = T::Pages::get();

		// we don't count the export.
		let _export = T::Pages::get();

		snapshot + signed + signed_validation + unsigned
	}

	#[cfg(any(test, feature = "runtime-benchmarks", feature = "try-runtime"))]
	pub(crate) fn do_try_state(_: BlockNumberFor<T>) -> Result<(), &'static str> {
		Snapshot::<T>::sanity_check()
	}
}

#[allow(unused)]
#[cfg(any(feature = "runtime-benchmarks", test))]
// helper code for testing and benchmarking
impl<T> Pallet<T>
where
	T: Config + crate::signed::Config + crate::unsigned::Config + crate::verifier::Config,
	BlockNumberFor<T>: From<u32>,
{
	/// Progress blocks until the criteria is met.
	pub(crate) fn roll_until_matches(criteria: impl FnOnce() -> bool + Copy) {
		loop {
			Self::roll_next(true, false);
			if criteria() {
				break
			}
		}
	}

	/// Progress blocks until one block before the criteria is met.
	pub(crate) fn roll_until_before_matches(criteria: impl FnOnce() -> bool + Copy) {
		use frame_support::storage::TransactionOutcome;
		loop {
			let should_break = frame_support::storage::with_transaction(
				|| -> TransactionOutcome<Result<_, DispatchError>> {
					Pallet::<T>::roll_next(true, false);
					if criteria() {
						TransactionOutcome::Rollback(Ok(true))
					} else {
						TransactionOutcome::Commit(Ok(false))
					}
				},
			)
			.unwrap();

			if should_break {
				break
			}
		}
	}

	pub(crate) fn roll_to_signed_and_mine_full_solution() -> PagedRawSolution<T::MinerConfig> {
		use unsigned::miner::OffchainWorkerMiner;
		Self::roll_until_matches(|| Self::current_phase().is_signed());
		// ensure snapshot is full.
		crate::Snapshot::<T>::ensure_full_snapshot().expect("Snapshot is not full");
		OffchainWorkerMiner::<T>::mine_solution(T::Pages::get(), false)
			.expect("mine_solution failed")
	}

	pub(crate) fn submit_full_solution(
		PagedRawSolution { score, solution_pages, .. }: PagedRawSolution<T::MinerConfig>,
	) -> DispatchResultWithPostInfo {
		use frame_system::RawOrigin;
		use sp_std::boxed::Box;
		use types::Pagify;

		// register alice
		let alice = crate::Pallet::<T>::funded_account("alice", 0);
		signed::Pallet::<T>::register(RawOrigin::Signed(alice.clone()).into(), score)?;

		// submit pages
		for (index, page) in solution_pages.pagify(T::Pages::get()) {
			signed::Pallet::<T>::submit_page(
				RawOrigin::Signed(alice.clone()).into(),
				index,
				Some(Box::new(page.clone())),
			)
			.inspect_err(|&e| {
				log!(error, "submit_page {:?} failed: {:?}", page, e);
			})?;
		}

		Ok(().into())
	}

	pub(crate) fn roll_to_signed_and_submit_full_solution() -> DispatchResultWithPostInfo {
		Self::submit_full_solution(Self::roll_to_signed_and_mine_full_solution())
	}

	fn funded_account(seed: &'static str, index: u32) -> T::AccountId {
		use frame_benchmarking::whitelist;
		use frame_support::traits::fungible::{Inspect, Mutate};
		let who: T::AccountId = frame_benchmarking::account(seed, index, 777);
		whitelist!(who);
		let balance = T::Currency::minimum_balance() * 1_0000_0000u32.into();
		T::Currency::mint_into(&who, balance).unwrap();
		who
	}

	/// Roll all pallets forward, for the given number of blocks.
	pub(crate) fn roll_to(n: BlockNumberFor<T>, with_signed: bool, try_state: bool) {
		let now = frame_system::Pallet::<T>::block_number();
		assert!(n > now, "cannot roll to current or past block");
		let one: BlockNumberFor<T> = 1u32.into();
		let mut i = now + one;
		while i <= n {
			frame_system::Pallet::<T>::set_block_number(i);

			Pallet::<T>::on_initialize(i);
			verifier::Pallet::<T>::on_initialize(i);
			unsigned::Pallet::<T>::on_initialize(i);

			if with_signed {
				signed::Pallet::<T>::on_initialize(i);
			}

			// invariants must hold at the end of each block.
			if try_state {
				Pallet::<T>::do_try_state(i).unwrap();
				verifier::Pallet::<T>::do_try_state(i).unwrap();
				unsigned::Pallet::<T>::do_try_state(i).unwrap();
				signed::Pallet::<T>::do_try_state(i).unwrap();
			}

			i += one;
		}
	}

	/// Roll to next block.
	pub(crate) fn roll_next(with_signed: bool, try_state: bool) {
		Self::roll_to(
			frame_system::Pallet::<T>::block_number() + 1u32.into(),
			with_signed,
			try_state,
		);
	}
}

impl<T: Config> ElectionProvider for Pallet<T> {
	type AccountId = T::AccountId;
	type BlockNumber = BlockNumberFor<T>;
	type Error = ElectionError<T>;
	type DataProvider = T::DataProvider;
	type Pages = T::Pages;
	type MaxWinnersPerPage = <T::Verifier as Verifier>::MaxWinnersPerPage;
	type MaxBackersPerWinner = <T::Verifier as Verifier>::MaxBackersPerWinner;

	fn elect(remaining: PageIndex) -> Result<BoundedSupportsOf<Self>, Self::Error> {
		match Self::status() {
			// we allow `elect` to be called as long as we have received a start signal.
			Ok(_) => (),
			Err(_) => return Err(ElectionError::NotOngoing),
		}

		let result = T::Verifier::get_queued_solution_page(remaining)
			.ok_or(ElectionError::SupportPageNotAvailable)
			.or_else(|err: ElectionError<T>| {
				log!(
					warn,
					"primary election for page {} failed due to: {:?}, trying fallback",
					remaining,
					err,
				);
				Self::fallback_for_page(remaining)
			})
			.map_err(|err| {
				// if any pages returns an error, we go into the emergency phase and don't do
				// anything else anymore. This will prevent any new submissions to signed and
				// unsigned pallet, and thus the verifier will also be almost stuck, except for the
				// submission of emergency solutions.
				log!(warn, "primary and fallback ({:?}) failed for page {:?}", err, remaining);
				err
			})
			.map(|supports| {
				// convert to bounded
				supports.into()
			});

		// if fallback has possibly put us into the emergency phase, don't do anything else.
		if CurrentPhase::<T>::get().is_emergency() && result.is_err() {
			log!(error, "Emergency phase triggered, halting the election.");
		} else {
			if remaining.is_zero() {
				log!(info, "receiving last call to elect(0), rotating round");
				Self::rotate_round()
			} else {
				Self::phase_transition(Phase::Export(remaining))
			}
		}

		result
	}

	fn start() -> Result<(), Self::Error> {
		match Self::status() {
			Err(()) => (),
			Ok(_) => return Err(ElectionError::Ongoing),
		}

		Self::phase_transition(Phase::<T>::start_phase());
		Ok(())
	}

	fn duration() -> Self::BlockNumber {
		Self::average_election_duration().into()
	}

	fn status() -> Result<bool, ()> {
		match <CurrentPhase<T>>::get() {
			// we're not doing anything.
			Phase::Off => Err(()),

			// we're doing sth but not read.
			Phase::Signed(_) |
			Phase::SignedValidation(_) |
			Phase::Unsigned(_) |
			Phase::Snapshot(_) |
			Phase::Emergency => Ok(false),

			// we're ready
			Phase::Done | Phase::Export(_) => Ok(true),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn asap() {
		// prepare our snapshot so we can "hopefully" run a fallback.
		Self::create_targets_snapshot().unwrap();
		for p in (Self::lsp()..=Self::msp()).rev() {
			Self::create_voters_snapshot_paged(p).unwrap()
		}
	}
}

#[cfg(test)]
mod phase_rotation {
	use super::{Event, *};
	use crate::{mock::*, Phase};
	use frame_election_provider_support::ElectionProvider;
	use frame_support::traits::Hooks;

	#[test]
	fn single_page() {
		ExtBuilder::full()
			.pages(1)
			.election_start(13)
			.fallback_mode(FallbackModes::Onchain)
			.build_and_execute(|| {
				// 0 -------- 14 15 --------- 20 ------------- 25 ---------- 30
				//            |  |            |                |             |
				//    Snapshot Signed  SignedValidation    Unsigned       elect()

				assert_eq!(System::block_number(), 0);
				assert_eq!(MultiBlock::current_phase(), Phase::Off);
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(false, 1));
				assert_eq!(MultiBlock::round(), 0);

				roll_to(4);
				assert_eq!(MultiBlock::current_phase(), Phase::Off);
				assert_eq!(MultiBlock::round(), 0);

				roll_to(13);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(1));
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(false, 3));

				roll_to(14);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(0));
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 0));

				roll_to(15);
				assert_eq!(MultiBlock::current_phase(), Phase::Signed(SignedPhase::get() - 1));
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 1));
				assert_eq!(MultiBlock::round(), 0);

				assert_eq!(
					multi_block_events_since_last_call(),
					vec![
						Event::PhaseTransitioned { from: Phase::Off, to: Phase::Snapshot(1) },
						Event::PhaseTransitioned {
							from: Phase::Snapshot(0),
							to: Phase::Signed(SignedPhase::get() - 1)
						}
					]
				);

				roll_to(19);
				assert_eq!(MultiBlock::current_phase(), Phase::Signed(0));
				assert_eq!(MultiBlock::round(), 0);

				roll_to(20);
				assert_eq!(
					MultiBlock::current_phase(),
					Phase::SignedValidation(SignedValidationPhase::get() - 1)
				);
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 1));
				assert_eq!(MultiBlock::round(), 0);

				assert_eq!(
					multi_block_events_since_last_call(),
					vec![Event::PhaseTransitioned {
						from: Phase::Signed(0),
						to: Phase::SignedValidation(SignedValidationPhase::get() - 1)
					}],
				);

				roll_to(24);
				assert_eq!(MultiBlock::current_phase(), Phase::SignedValidation(0));
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 1));
				assert_eq!(MultiBlock::round(), 0);

				roll_to(25);
				assert_eq!(MultiBlock::current_phase(), Phase::Unsigned(UnsignedPhase::get() - 1));
				assert_eq!(
					multi_block_events_since_last_call(),
					vec![Event::PhaseTransitioned {
						from: Phase::SignedValidation(0),
						to: Phase::Unsigned(UnsignedPhase::get() - 1)
					}],
				);

				roll_to(29);
				assert_eq!(MultiBlock::current_phase(), Phase::Unsigned(0));

				// We stay in done otherwise
				roll_to(30);
				assert!(MultiBlock::current_phase().is_done());

				// We stay in done otherwise
				roll_to(31);
				assert!(MultiBlock::current_phase().is_done());

				// We close when upstream tells us to elect.
				roll_to(32);
				assert_eq!(MultiBlock::current_phase(), Phase::Done);
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 1));

				MultiBlock::elect(0).unwrap();

				assert!(MultiBlock::current_phase().is_off());
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(false, 1));
				assert_eq!(MultiBlock::round(), 1);

				roll_to(42);
				assert_eq!(MultiBlock::current_phase(), Phase::Off);
			})
	}

	#[test]
	fn multi_page_2() {
		ExtBuilder::full()
			.pages(2)
			.fallback_mode(FallbackModes::Onchain)
			.election_start(12)
			.build_and_execute(|| {
				// 0 -------13 14 15 ------- 20 ---- 25 ------- 30
				//           |     |         |       |          |
				//    Snapshot    Signed SigValid  Unsigned   Elect

				assert_eq!(System::block_number(), 0);
				assert_eq!(MultiBlock::current_phase(), Phase::Off);
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(false, 2));
				assert_eq!(MultiBlock::round(), 0);

				roll_to(4);
				assert_eq!(MultiBlock::current_phase(), Phase::Off);
				assert_eq!(MultiBlock::round(), 0);

				roll_to(11);
				assert_eq!(MultiBlock::current_phase(), Phase::Off);
				assert_eq!(MultiBlock::round(), 0);

				roll_to(12);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(2));
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(false, 2));

				roll_to(13);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(1));
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 0));

				roll_to(14);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(0));
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 1));

				roll_to(15);
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 2));
				assert_eq!(MultiBlock::round(), 0);
				assert_eq!(MultiBlock::current_phase(), Phase::Signed(SignedPhase::get() - 1));

				assert_eq!(
					multi_block_events_since_last_call(),
					vec![
						Event::PhaseTransitioned { from: Phase::Off, to: Phase::Snapshot(2) },
						Event::PhaseTransitioned {
							from: Phase::Snapshot(0),
							to: Phase::Signed(SignedPhase::get() - 1)
						}
					]
				);

				roll_to(19);
				assert_eq!(MultiBlock::current_phase(), Phase::Signed(0));
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 2));
				assert_eq!(MultiBlock::round(), 0);

				roll_to(20);
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 2));
				assert_eq!(MultiBlock::round(), 0);
				assert_eq!(
					MultiBlock::current_phase(),
					Phase::SignedValidation(SignedValidationPhase::get() - 1)
				);

				assert_eq!(
					multi_block_events_since_last_call(),
					vec![Event::PhaseTransitioned {
						from: Phase::Signed(0),
						to: Phase::SignedValidation(SignedValidationPhase::get() - 1)
					}],
				);

				roll_to(24);
				assert_eq!(MultiBlock::current_phase(), Phase::SignedValidation(0));
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 2));
				assert_eq!(MultiBlock::round(), 0);

				roll_to(25);
				assert_eq!(MultiBlock::current_phase(), Phase::Unsigned(UnsignedPhase::get() - 1));
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 2));
				assert_eq!(MultiBlock::round(), 0);

				assert_eq!(
					multi_block_events_since_last_call(),
					vec![Event::PhaseTransitioned {
						from: Phase::SignedValidation(0),
						to: Phase::Unsigned(UnsignedPhase::get() - 1)
					}],
				);

				roll_to(29);
				assert_eq!(MultiBlock::current_phase(), Phase::Unsigned(0));
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 2));

				roll_to(30);
				assert_eq!(MultiBlock::current_phase(), Phase::Done);
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 2));

				// We close when upstream tells us to elect.
				roll_to(32);
				assert_eq!(MultiBlock::current_phase(), Phase::Done);

				// and even this one's coming from the fallback.
				MultiBlock::elect(0).unwrap();
				assert!(MultiBlock::current_phase().is_off());

				// all snapshots are gone.
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(false, 2));
				assert_eq!(MultiBlock::round(), 1);
			})
	}

	#[test]
	fn multi_page_3() {
		ExtBuilder::full()
			.pages(3)
			.fallback_mode(FallbackModes::Onchain)
			.build_and_execute(|| {
				// 0 ------- 12 13 14 15 ----------- 20 ---------25 ------- 30
				//            |       |              |            |          |
				//     Snapshot      Signed   SignedValidation  Unsigned   Elect

				assert_eq!(System::block_number(), 0);
				assert!(MultiBlock::current_phase().is_off());
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(false, 3));
				assert_eq!(MultiBlock::round(), 0);

				roll_to(10);
				assert!(MultiBlock::current_phase().is_off());
				assert_eq!(MultiBlock::round(), 0);

				roll_to(11);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(3));
				// no snapshot is take yet, we start at the next block
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(false, 3));

				roll_to(12);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(2));
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 0));

				roll_to(13);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(1));
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 1));

				roll_to(14);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(0));
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, 2));

				roll_to(15);
				assert_ok!(Snapshot::<Runtime>::ensure_snapshot(true, Pages::get()));
				assert_eq!(MultiBlock::current_phase(), Phase::Signed(4));
				assert_eq!(
					multi_block_events_since_last_call(),
					vec![
						Event::PhaseTransitioned { from: Phase::Off, to: Phase::Snapshot(3) },
						Event::PhaseTransitioned {
							from: Phase::Snapshot(0),
							to: Phase::Signed(SignedPhase::get() - 1)
						}
					]
				);
				assert_eq!(MultiBlock::round(), 0);

				roll_to(19);
				assert_eq!(MultiBlock::current_phase(), Phase::Signed(0));
				assert_eq!(MultiBlock::round(), 0);

				roll_to(20);
				assert_eq!(
					MultiBlock::current_phase(),
					Phase::SignedValidation(SignedValidationPhase::get() - 1)
				);
				assert_eq!(
					multi_block_events_since_last_call(),
					vec![Event::PhaseTransitioned {
						from: Phase::Signed(0),
						to: Phase::SignedValidation(SignedValidationPhase::get() - 1)
					}]
				);

				roll_to(24);
				assert_eq!(MultiBlock::current_phase(), Phase::SignedValidation(0));
				assert_eq!(MultiBlock::round(), 0);

				roll_to(25);
				assert_eq!(MultiBlock::current_phase(), Phase::Unsigned(UnsignedPhase::get() - 1));
				assert_eq!(
					multi_block_events_since_last_call(),
					vec![Event::PhaseTransitioned {
						from: Phase::SignedValidation(0),
						to: Phase::Unsigned(UnsignedPhase::get() - 1)
					}]
				);

				roll_to(29);
				assert_eq!(MultiBlock::current_phase(), Phase::Unsigned(0));

				roll_to(30);
				assert_eq!(MultiBlock::current_phase(), Phase::Done);

				// We close when upstream tells us to elect.
				roll_to(32);
				assert_eq!(MultiBlock::current_phase(), Phase::Done);

				MultiBlock::elect(0).unwrap();
				assert!(MultiBlock::current_phase().is_off());

				// all snapshots are gone.
				assert_none_snapshot();
				assert_eq!(MultiBlock::round(), 1);
			})
	}

	#[test]
	fn no_unsigned_phase() {
		ExtBuilder::full()
			.pages(3)
			.unsigned_phase(0)
			.election_start(16)
			.fallback_mode(FallbackModes::Onchain)
			.build_and_execute(|| {
				// 0 --------------------- 17 ------ 20 ---------25 ------- 30
				//            |            |         |            |          |
				//                     Snapshot    Signed  SignedValidation   Elect

				assert_eq!(System::block_number(), 0);
				assert_eq!(MultiBlock::current_phase(), Phase::Off);
				assert_none_snapshot();
				assert_eq!(MultiBlock::round(), 0);

				roll_to(4);
				assert_eq!(MultiBlock::current_phase(), Phase::Off);
				assert_eq!(MultiBlock::round(), 0);

				roll_to(16);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(3));

				roll_to(17);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(2));

				roll_to(18);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(1));

				roll_to(19);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(0));

				roll_to(20);
				assert_eq!(MultiBlock::current_phase(), Phase::Signed(SignedPhase::get() - 1));

				assert_full_snapshot();
				assert_eq!(MultiBlock::round(), 0);

				roll_to(25);
				assert_eq!(
					MultiBlock::current_phase(),
					Phase::SignedValidation(SignedValidationPhase::get() - 1)
				);

				assert_eq!(
					multi_block_events_since_last_call(),
					vec![
						Event::PhaseTransitioned { from: Phase::Off, to: Phase::Snapshot(3) },
						Event::PhaseTransitioned {
							from: Phase::Snapshot(0),
							to: Phase::Signed(SignedPhase::get() - 1)
						},
						Event::PhaseTransitioned {
							from: Phase::Signed(0),
							to: Phase::SignedValidation(SignedValidationPhase::get() - 1)
						},
					]
				);

				// last block of signed validation
				roll_to(29);
				assert_eq!(MultiBlock::current_phase(), Phase::SignedValidation(0));

				// we are done now
				roll_to(30);
				assert_eq!(MultiBlock::current_phase(), Phase::Done);

				roll_to(31);
				assert_eq!(MultiBlock::current_phase(), Phase::Done);

				MultiBlock::elect(0).unwrap();
				assert!(MultiBlock::current_phase().is_off());

				// all snapshots are gone.
				assert_none_snapshot();
				assert_eq!(MultiBlock::round(), 1);
				assert_ok!(signed::Submissions::<Runtime>::ensure_killed(0));
				verifier::QueuedSolution::<Runtime>::assert_killed();
			})
	}

	#[test]
	fn no_signed_phase() {
		ExtBuilder::full()
			.pages(3)
			.signed_phase(0, 0)
			.election_start(21)
			.fallback_mode(FallbackModes::Onchain)
			.build_and_execute(|| {
				// 0 ------------------------- 22 ------ 25 ------- 30
				//                             |         |          |
				//                         Snapshot   Unsigned   Elect

				assert_eq!(System::block_number(), 0);
				assert_eq!(MultiBlock::current_phase(), Phase::Off);
				assert_none_snapshot();
				assert_eq!(MultiBlock::round(), 0);

				roll_to(20);
				assert_eq!(MultiBlock::current_phase(), Phase::Off);
				assert_eq!(MultiBlock::round(), 0);

				roll_to(21);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(3));
				roll_to(22);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(2));
				roll_to(23);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(1));
				roll_to(24);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(0));

				roll_to(25);
				assert_eq!(MultiBlock::current_phase(), Phase::Unsigned(UnsignedPhase::get() - 1));
				assert_full_snapshot();
				assert_eq!(MultiBlock::round(), 0);

				assert_eq!(
					multi_block_events(),
					vec![
						Event::PhaseTransitioned { from: Phase::Off, to: Phase::Snapshot(3) },
						Event::PhaseTransitioned {
							from: Phase::Snapshot(0),
							to: Phase::Unsigned(UnsignedPhase::get() - 1)
						},
					]
				);

				roll_to(29);
				assert_eq!(MultiBlock::current_phase(), Phase::Unsigned(0));

				roll_to(30);
				assert_eq!(MultiBlock::current_phase(), Phase::Done);
				roll_to(31);
				assert_eq!(MultiBlock::current_phase(), Phase::Done);

				// eventually the call to elect comes, and we exit done phase.
				MultiBlock::elect(0).unwrap();
				assert!(MultiBlock::current_phase().is_off());

				// all snapshots are gone.
				assert_none_snapshot();
				assert_eq!(MultiBlock::round(), 1);
				assert_ok!(signed::Submissions::<Runtime>::ensure_killed(0));
				verifier::QueuedSolution::<Runtime>::assert_killed();
			})
	}

	#[test]
	fn no_signed_and_unsigned_phase() {
		ExtBuilder::full()
			.pages(3)
			.signed_phase(0, 0)
			.unsigned_phase(0)
			.election_start(10)
			.fallback_mode(FallbackModes::Onchain)
			.build_and_execute(|| {
				assert_eq!(System::block_number(), 0);
				assert_eq!(MultiBlock::current_phase(), Phase::Off);
				assert_none_snapshot();
				assert_eq!(MultiBlock::round(), 0);

				roll_to(10);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(3));
				assert_eq!(MultiBlock::round(), 0);

				roll_to(11);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(2));
				roll_to(12);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(1));
				roll_to(13);
				assert_eq!(MultiBlock::current_phase(), Phase::Snapshot(0));

				// And we are done already
				roll_to(14);
				assert_eq!(MultiBlock::current_phase(), Phase::Done);
			});
	}

	#[test]
	#[should_panic(
		expected = "signed validation phase should be at least as long as the number of pages"
	)]
	fn incorrect_signed_validation_phase() {
		ExtBuilder::full()
			.pages(3)
			.signed_validation_phase(2)
			.build_and_execute(|| <MultiBlock as Hooks<BlockNumber>>::integrity_test())
	}

	#[test]
	fn are_we_done_back_to_signed() {
		ExtBuilder::full()
			.are_we_done(AreWeDoneModes::BackToSigned)
			.build_and_execute(|| {
				// roll to unsigned
				roll_to_last_unsigned();

				assert_eq!(MultiBlock::round(), 0);
				assert_eq!(MultiBlock::current_phase(), Phase::Unsigned(0));
				assert_eq!(
					multi_block_events_since_last_call(),
					vec![
						Event::PhaseTransitioned { from: Phase::Off, to: Phase::Snapshot(3) },
						Event::PhaseTransitioned { from: Phase::Snapshot(0), to: Phase::Signed(4) },
						Event::PhaseTransitioned {
							from: Phase::Signed(0),
							to: Phase::SignedValidation(4)
						},
						Event::PhaseTransitioned {
							from: Phase::SignedValidation(0),
							to: Phase::Unsigned(4)
						}
					]
				);

				roll_next();
				// we are back to signed phase
				assert_eq!(MultiBlock::current_phase(), Phase::Signed(SignedPhase::get() - 1));
				// round is still the same
				assert_eq!(MultiBlock::round(), 0);

				// we proceed to normally again:
				roll_next();
				assert_eq!(MultiBlock::current_phase(), Phase::Signed(SignedPhase::get() - 2));

				roll_next();
				assert_eq!(MultiBlock::current_phase(), Phase::Signed(SignedPhase::get() - 3));
			});
	}
}

#[cfg(test)]
mod election_provider {
	use super::*;
	use crate::{mock::*, unsigned::miner::OffchainWorkerMiner, verifier::Verifier, Phase};
	use frame_election_provider_support::{BoundedSupport, BoundedSupports, ElectionProvider};
	use frame_support::{
		assert_storage_noop, testing_prelude::bounded_vec, unsigned::ValidateUnsigned,
	};

	// This is probably the most important test of all, a basic, correct scenario. This test should
	// be studied in detail, and all of the branches of how it can go wrong or diverge from the
	// basic scenario assessed.
	#[test]
	fn multi_page_elect_simple_works() {
		ExtBuilder::full().build_and_execute(|| {
			roll_to_signed_open();
			assert!(MultiBlock::current_phase().is_signed());

			// load a solution into the verifier
			let paged = OffchainWorkerMiner::<Runtime>::mine_solution(Pages::get(), false).unwrap();
			let score = paged.score;

			// now let's submit this one by one, into the signed phase.
			load_signed_for_verification(99, paged);

			// now the solution should start being verified.
			roll_to_signed_validation_open();

			assert_eq!(
				multi_block_events(),
				vec![
					Event::PhaseTransitioned { from: Phase::Off, to: Phase::Snapshot(3) },
					Event::PhaseTransitioned {
						from: Phase::Snapshot(0),
						to: Phase::Signed(SignedPhase::get() - 1)
					},
					Event::PhaseTransitioned {
						from: Phase::Signed(0),
						to: Phase::SignedValidation(SignedValidationPhase::get() - 1)
					}
				]
			);
			assert_eq!(verifier_events(), vec![]);

			// there is no queued solution prior to the last page of the solution getting verified
			assert_eq!(<Runtime as crate::Config>::Verifier::queued_score(), None);

			// proceed until it is fully verified.
			roll_next();
			assert_eq!(verifier_events(), vec![verifier::Event::Verified(2, 2)]);

			roll_next();
			assert_eq!(
				verifier_events(),
				vec![verifier::Event::Verified(2, 2), verifier::Event::Verified(1, 2)]
			);

			roll_next();
			assert_eq!(
				verifier_events(),
				vec![
					verifier::Event::Verified(2, 2),
					verifier::Event::Verified(1, 2),
					verifier::Event::Verified(0, 2),
					verifier::Event::Queued(score, None),
				]
			);

			// there is now a queued solution.
			assert_eq!(<Runtime as crate::Config>::Verifier::queued_score(), Some(score));

			// now let's go to unsigned phase, but we don't expect anything to happen there since we
			// don't run OCWs.
			roll_to_unsigned_open();

			// pre-elect state
			assert!(MultiBlock::current_phase().is_unsigned_opened_now());
			assert_eq!(MultiBlock::round(), 0);
			assert_full_snapshot();

			// call elect for each page
			let _paged_solution = (MultiBlock::lsp()..MultiBlock::msp())
				.rev() // 2, 1, 0
				.map(|page| {
					MultiBlock::elect(page as PageIndex).unwrap();
					if page == 0 {
						assert!(MultiBlock::current_phase().is_off())
					} else {
						assert!(MultiBlock::current_phase().is_export())
					}
				})
				.collect::<Vec<_>>();

			// after the last elect, verifier is cleared,
			verifier::QueuedSolution::<Runtime>::assert_killed();
			// the phase is off,
			assert_eq!(MultiBlock::current_phase(), Phase::Off);
			// the round is incremented,
			assert_eq!(Round::<Runtime>::get(), 1);
			// and the snapshot is cleared,
			assert_storage_noop!(Snapshot::<Runtime>::kill());
			// signed pallet is clean.
			// NOTE: in the future, if and when we add lazy cleanup to the signed pallet, this
			// assertion might break.
			assert_ok!(signed::Submissions::<Runtime>::ensure_killed(0));
		});
	}

	#[test]
	fn multi_page_elect_fast_track() {
		ExtBuilder::full().build_and_execute(|| {
			roll_to_signed_open();
			let round = MultiBlock::round();
			assert!(MultiBlock::current_phase().is_signed());

			// load a solution into the verifier
			let paged = OffchainWorkerMiner::<Runtime>::mine_solution(Pages::get(), false).unwrap();
			let score = paged.score;
			load_signed_for_verification_and_start(99, paged, 0);

			// there is no queued solution prior to the last page of the solution getting verified
			assert_eq!(<Runtime as crate::Config>::Verifier::queued_score(), None);

			// roll to the block it is finalized
			roll_next();
			roll_next();
			roll_next();
			assert_eq!(
				verifier_events(),
				vec![
					verifier::Event::Verified(2, 2),
					verifier::Event::Verified(1, 2),
					verifier::Event::Verified(0, 2),
					verifier::Event::Queued(score, None),
				]
			);

			// there is now a queued solution.
			assert_eq!(<Runtime as crate::Config>::Verifier::queued_score(), Some(score));

			// not much impact, just for the sane-ness of the test.
			roll_to_unsigned_open();

			// pre-elect state:
			assert!(MultiBlock::current_phase().is_unsigned_opened_now());
			assert_eq!(Round::<Runtime>::get(), 0);
			assert_full_snapshot();

			// there are 3 pages (indexes 2..=0), but we short circuit by just calling 0.
			let _solution = crate::Pallet::<Runtime>::elect(0).unwrap();

			// round is incremented.
			assert_eq!(MultiBlock::round(), round + 1);
			// after elect(0) is called, verifier is cleared,
			verifier::QueuedSolution::<Runtime>::assert_killed();
			// the phase is off,
			assert_eq!(MultiBlock::current_phase(), Phase::Off);
			// the round is incremented,
			assert_eq!(Round::<Runtime>::get(), 1);
			// the snapshot is cleared,
			assert_none_snapshot();
			// and signed pallet is clean.
			assert_ok!(signed::Submissions::<Runtime>::ensure_killed(round));
		});
	}

	#[test]
	fn elect_does_not_finish_without_call_of_page_0() {
		ExtBuilder::full().build_and_execute(|| {
			roll_to_signed_open();
			assert!(MultiBlock::current_phase().is_signed());

			// load a solution into the verifier
			let paged = OffchainWorkerMiner::<Runtime>::mine_solution(Pages::get(), false).unwrap();
			let score = paged.score;
			load_signed_for_verification_and_start(99, paged, 0);

			// there is no queued solution prior to the last page of the solution getting verified
			assert_eq!(<Runtime as crate::Config>::Verifier::queued_score(), None);

			// roll to the block it is finalized
			roll_next();
			roll_next();
			roll_next();
			assert_eq!(
				verifier_events(),
				vec![
					verifier::Event::Verified(2, 2),
					verifier::Event::Verified(1, 2),
					verifier::Event::Verified(0, 2),
					verifier::Event::Queued(score, None),
				]
			);

			// there is now a queued solution
			assert_eq!(<Runtime as crate::Config>::Verifier::queued_score(), Some(score));

			// not much impact, just for the sane-ness of the test.
			roll_to_unsigned_open();

			// pre-elect state:
			assert!(MultiBlock::current_phase().is_unsigned_opened_now());
			assert_eq!(Round::<Runtime>::get(), 0);
			assert_full_snapshot();

			// call elect for page 2 and 1, but NOT 0
			let solutions = (1..=MultiBlock::msp())
				.rev() // 2, 1
				.map(|page| {
					crate::Pallet::<Runtime>::elect(page as PageIndex).unwrap();
					assert!(MultiBlock::current_phase().is_export());
				})
				.collect::<Vec<_>>();
			assert_eq!(solutions.len(), 2);

			// nothing changes from the prelect state, except phase is now export.
			assert!(MultiBlock::current_phase().is_export());
			assert_eq!(Round::<Runtime>::get(), 0);
			assert_full_snapshot();
		});
	}

	#[test]
	fn skip_unsigned_phase() {
		ExtBuilder::full().build_and_execute(|| {
			roll_to_signed_open();
			assert!(MultiBlock::current_phase().is_signed());
			let round = MultiBlock::round();

			// load a solution into the verifier
			let paged = OffchainWorkerMiner::<Runtime>::mine_solution(Pages::get(), false).unwrap();

			load_signed_for_verification_and_start_and_roll_to_verified(99, paged, 0);

			// and right here, in the middle of the signed verification phase, we close the round.
			// Everything should work fine.
			assert!(matches!(MultiBlock::current_phase(), Phase::SignedValidation(_)));
			assert_eq!(Round::<Runtime>::get(), 0);
			assert_full_snapshot();

			// fetch all pages.
			let _paged_solution = (MultiBlock::lsp()..MultiBlock::msp())
				.rev() // 2, 1, 0
				.map(|page| {
					MultiBlock::elect(page as PageIndex).unwrap();
					if page == 0 {
						assert!(MultiBlock::current_phase().is_off())
					} else {
						assert!(MultiBlock::current_phase().is_export())
					}
				})
				.collect::<Vec<_>>();

			// round is incremented.
			assert_eq!(MultiBlock::round(), round + 1);
			// after elect(0) is called, verifier is cleared,
			verifier::QueuedSolution::<Runtime>::assert_killed();
			// the phase is off,
			assert_eq!(MultiBlock::current_phase(), Phase::Off);
			// the snapshot is cleared,
			assert_storage_noop!(Snapshot::<Runtime>::kill());
			// and signed pallet is clean.
			assert_ok!(signed::Submissions::<Runtime>::ensure_killed(round));
		});
	}

	#[test]
	fn call_to_elect_should_prevent_any_submission() {
		ExtBuilder::full().build_and_execute(|| {
			roll_to_signed_open();
			assert!(MultiBlock::current_phase().is_signed());

			// load a solution into the verifier
			let paged = OffchainWorkerMiner::<Runtime>::mine_solution(Pages::get(), false).unwrap();
			load_signed_for_verification_and_start_and_roll_to_verified(99, paged, 0);

			assert!(matches!(MultiBlock::current_phase(), Phase::SignedValidation(_)));

			// fetch one page.
			assert!(MultiBlock::elect(MultiBlock::msp()).is_ok());

			// try submit one signed page:
			assert_noop!(
				SignedPallet::submit_page(RuntimeOrigin::signed(999), 0, Default::default()),
				crate::signed::Error::<Runtime>::PhaseNotSigned,
			);
			assert_noop!(
				SignedPallet::register(RuntimeOrigin::signed(999), Default::default()),
				crate::signed::Error::<Runtime>::PhaseNotSigned,
			);
			assert_storage_noop!(assert!(<UnsignedPallet as ValidateUnsigned>::pre_dispatch(
				&unsigned::Call::submit_unsigned { paged_solution: Default::default() }
			)
			.is_err()));
		});
	}

	#[test]
	fn multi_page_onchain_elect_fallback_works() {
		ExtBuilder::full().fallback_mode(FallbackModes::Onchain).build_and_execute(|| {
			roll_to_signed_open();

			// same targets, but voters from page 2 (1, 2, 3, 4, see `mock/staking`).
			assert_eq!(
				MultiBlock::elect(2).unwrap(),
				BoundedSupports(bounded_vec![
					(10, BoundedSupport { total: 15, voters: bounded_vec![(1, 10), (4, 5)] }),
					(
						40,
						BoundedSupport {
							total: 25,
							voters: bounded_vec![(2, 10), (3, 10), (4, 5)]
						}
					)
				])
			);
			// page 1 of voters
			assert_eq!(
				MultiBlock::elect(1).unwrap(),
				BoundedSupports(bounded_vec![
					(10, BoundedSupport { total: 15, voters: bounded_vec![(5, 5), (8, 10)] }),
					(
						30,
						BoundedSupport {
							total: 25,
							voters: bounded_vec![(5, 5), (6, 10), (7, 10)]
						}
					)
				])
			);
			// self votes
			assert_eq!(
				MultiBlock::elect(0).unwrap(),
				BoundedSupports(bounded_vec![
					(30, BoundedSupport { total: 30, voters: bounded_vec![(30, 30)] }),
					(40, BoundedSupport { total: 40, voters: bounded_vec![(40, 40)] })
				])
			);

			assert_eq!(
				multi_block_events(),
				vec![
					Event::PhaseTransitioned {
						from: Phase::Off,
						to: Phase::Snapshot(Pages::get())
					},
					Event::PhaseTransitioned {
						from: Phase::Snapshot(0),
						to: Phase::Signed(SignedPhase::get() - 1)
					},
					Event::PhaseTransitioned {
						from: Phase::Signed(SignedPhase::get() - 1),
						to: Phase::Export(2)
					},
					Event::PhaseTransitioned { from: Phase::Export(1), to: Phase::Off }
				]
			);
			assert_eq!(verifier_events(), vec![]);

			// This will set us to emergency phase, because we don't know wtf to do.
			assert_eq!(MultiBlock::current_phase(), Phase::Off);
		});
	}

	#[test]
	fn multi_page_fallback_shortcut_to_msp_works() {
		ExtBuilder::full().fallback_mode(FallbackModes::Onchain).build_and_execute(|| {
			roll_to_signed_open();

			// but then we immediately call `elect`, this will work
			assert!(MultiBlock::elect(0).is_ok());

			assert_eq!(
				multi_block_events(),
				vec![
					Event::PhaseTransitioned {
						from: Phase::Off,
						to: Phase::Snapshot(Pages::get())
					},
					Event::PhaseTransitioned {
						from: Phase::Snapshot(0),
						to: Phase::Signed(SignedPhase::get() - 1)
					},
					Event::PhaseTransitioned {
						from: Phase::Signed(SignedPhase::get() - 1),
						to: Phase::Off
					}
				]
			);

			// This will set us to the off phase, since fallback saved us.
			assert_eq!(MultiBlock::current_phase(), Phase::Off);
		});
	}

	#[test]
	#[should_panic]
	fn continue_fallback_works() {
		todo!()
	}

	#[test]
	#[should_panic]
	fn emergency_fallback_works() {
		todo!();
	}

	#[test]
	fn elect_call_when_not_ongoing() {
		ExtBuilder::full().fallback_mode(FallbackModes::Onchain).build_and_execute(|| {
			roll_to_snapshot_created();
			assert_eq!(MultiBlock::status(), Ok(false));
			assert!(MultiBlock::elect(0).is_ok());
		});
		ExtBuilder::full().fallback_mode(FallbackModes::Onchain).build_and_execute(|| {
			roll_to(10);
			assert_eq!(MultiBlock::status(), Err(()));
			assert_eq!(MultiBlock::elect(0), Err(ElectionError::NotOngoing));
		});
	}
}

#[cfg(test)]
mod admin_ops {
	use super::*;
	use crate::mock::*;
	use frame_support::assert_ok;

	#[test]
	fn set_solution_emergency_works() {
		ExtBuilder::full().build_and_execute(|| {
			roll_to_signed_open();

			// we get a call to elect(0). this will cause emergency, since no fallback is allowed.
			assert_eq!(
				MultiBlock::elect(0),
				Err(ElectionError::Fallback("Emergency phase started.".to_string()))
			);
			assert_eq!(MultiBlock::current_phase(), Phase::Emergency);

			// we can now set the solution to emergency.
			let (emergency, score) = emergency_solution();
			assert_ok!(MultiBlock::manage(
				RuntimeOrigin::root(),
				AdminOperation::EmergencySetSolution(Box::new(emergency), score)
			));

			assert_eq!(MultiBlock::current_phase(), Phase::Emergency);
			assert_ok!(MultiBlock::elect(0));
			assert_eq!(MultiBlock::current_phase(), Phase::Off);

			assert_eq!(
				multi_block_events(),
				vec![
					Event::PhaseTransitioned { from: Phase::Off, to: Phase::Snapshot(3) },
					Event::PhaseTransitioned {
						from: Phase::Snapshot(0),
						to: Phase::Signed(SignedPhase::get() - 1)
					},
					Event::PhaseTransitioned {
						from: Phase::Signed(SignedPhase::get() - 1),
						to: Phase::Emergency
					},
					Event::PhaseTransitioned { from: Phase::Emergency, to: Phase::Off }
				]
			);
			assert_eq!(
				verifier_events(),
				vec![verifier::Event::Queued(
					ElectionScore { minimal_stake: 55, sum_stake: 130, sum_stake_squared: 8650 },
					None
				)]
			);
		})
	}

	#[test]
	fn trigger_fallback_works() {
		ExtBuilder::full()
			.fallback_mode(FallbackModes::Emergency)
			.build_and_execute(|| {
				roll_to_signed_open();

				// we get a call to elect(0). this will cause emergency, since no fallback is
				// allowed.
				assert_eq!(
					MultiBlock::elect(0),
					Err(ElectionError::Fallback("Emergency phase started.".to_string()))
				);
				assert_eq!(MultiBlock::current_phase(), Phase::Emergency);

				// we can now set the solution to emergency, assuming fallback is set to onchain
				FallbackMode::set(FallbackModes::Onchain);
				assert_ok!(MultiBlock::manage(
					RuntimeOrigin::root(),
					AdminOperation::EmergencyFallback
				));

				assert_eq!(MultiBlock::current_phase(), Phase::Emergency);
				assert_ok!(MultiBlock::elect(0));
				assert_eq!(MultiBlock::current_phase(), Phase::Off);

				assert_eq!(
					multi_block_events(),
					vec![
						Event::PhaseTransitioned { from: Phase::Off, to: Phase::Snapshot(3) },
						Event::PhaseTransitioned {
							from: Phase::Snapshot(0),
							to: Phase::Signed(SignedPhase::get() - 1)
						},
						Event::PhaseTransitioned {
							from: Phase::Signed(SignedPhase::get() - 1),
							to: Phase::Emergency
						},
						Event::PhaseTransitioned { from: Phase::Emergency, to: Phase::Off }
					]
				);
				assert_eq!(
					verifier_events(),
					vec![verifier::Event::Queued(
						ElectionScore { minimal_stake: 15, sum_stake: 40, sum_stake_squared: 850 },
						None
					)]
				);
			})
	}

	#[test]
	#[should_panic]
	fn force_rotate_round() {
		// clears the snapshot and verifier data.
		// leaves the signed data as is since we bump the round.
		todo!();
	}

	#[test]
	fn set_minimum_solution_score() {
		ExtBuilder::full().build_and_execute(|| {
			assert_eq!(VerifierPallet::minimum_score(), None);
			assert_ok!(MultiBlock::manage(
				RuntimeOrigin::root(),
				AdminOperation::SetMinUntrustedScore(ElectionScore {
					minimal_stake: 100,
					..Default::default()
				})
			));
			assert_eq!(
				VerifierPallet::minimum_score().unwrap(),
				ElectionScore { minimal_stake: 100, ..Default::default() }
			);
		});
	}
}
