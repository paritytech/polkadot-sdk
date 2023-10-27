// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! The parachain bulk assignment module.
//!
//! Handles scheduling of bulk core time.

mod mock_helpers;
#[cfg(test)]
mod tests;

use crate::{
	assigner_on_demand, configuration, paras,
	scheduler::common::{
		Assignment, AssignmentProvider, AssignmentProviderConfig, AssignmentVersion,
	},
};

use frame_support::{
	pallet_prelude::*,
	traits::{
		Currency,
		ExistenceRequirement::{self, AllowDeath, KeepAlive},
		WithdrawReasons,
	},
};
use frame_system::pallet_prelude::*;
use primitives::{CoreIndex, Id as ParaId};
use sp_runtime::{
	traits::{One, SaturatedConversion},
	FixedPointNumber, FixedPointOperand, FixedU128, Perbill, Saturating,
};
use pallet_broker::CoreAssignment;

use sp_std::{collections::vec_deque::VecDeque, prelude::*};

const LOG_TARGET: &str = "runtime::parachains::assigner-bulk";

pub use pallet::*;

pub trait WeightInfo {}

/// A weight info that is only suitable for testing.
pub struct TestWeightInfo;

/// Fraction expressed as a nominator with an assumed denominator of 57,600.
pub type PartsOf57600 = u16;

impl WeightInfo for TestWeightInfo {}

/// AssignmentSets as they are scheduled by block number
///
/// for a particular core.
#[derive(Encode, Decode, TypeInfo)]
struct Schedule<N> {
	// Original assignments
	assignments: Vec<(CoreAssignment, PartsOf57600)>,
	/// When do our assignments become invalid, if at all?
	///
	/// If this is `Some`, then this `CoreState` will be dropped at that block number. If this is
	/// `None`, then we will keep serving our core assignments in a circle until a new set of
	/// assignments is scheduled.
	///
	/// Note: This is used to chain schedules: If there is any more schedules for this core you
	/// will find the next at `end_hint` + 1 block number in `Workplan`.
	end_hint: Option<N>,
}

/// An instantiated `Schedule`.
///
/// This is the state of assignments currently being served via the `AssignmentProvider` interface,
/// as opposed to `Schedule` which is upcoming not yet served assignments.
#[derive(Encode, Decode, TypeInfo)]
struct CoreState<N> {
	/// Assignments with current state.
	///
	/// Assignments and book keeping on how much has been served already. We keep track of serviced
	/// assignments in order to adhere to the specified ratios.
	assignments: Vec<(CoreAssignment, AssignmentState)>,
	/// When do our assignments become invalid if at all?
	///
	/// If this is `Some`, then this `CoreState` will be dropped at that block number. If this is
	/// `None`, then we will keep serving our core assignments in a circle until a new set of
	/// assignments is scheduled.
	///
	/// Note: This is used to chain schedules: If there is any future schedule for this core you
	/// will find it at `end_hint` + 1 block number in `Workplan`.
	end_hint: Option<N>,
	/// Position in the assignments we are currently in.
	///
	/// Aka which core assignment will be popped next on
	/// `AssignmentProvider::pop_assignment_for_core`.
	pos: u16,
	/// Step width
	///
	/// How much we subtract from `AssignmentState::remaining` for a core served.
	step: PartsOf57600,
}

#[derive(Encode, Decode, TypeInfo)]
struct AssignmentState {
	/// Ratio of the core this assignment has.
	///
	/// As initially received via `assign_core`.
	ratio: PartsOf57600,
	/// How many parts are remaining in this round?
	///
	/// At the beginning of each round this will be set to ratio + credit, then everytime we get
	/// scheduled we subtract a core worth of points. Once we reach 0 or a number lower than what a
	/// core is worth, we move on to the next item in the vec.
	remaining: PartsOf57600,
}

impl<N> From<Schedule<N>> for CoreState<N> {
	fn from(schedule: Schedule<N>) -> Self {
		let Schedule { assignments, end_hint } = schedule;
		let step = if let Some(min_step_assignment) = assignments.iter().min_by(|a, b| a.1.cmp(&b.1)) {
			min_step_assignment.1
		} else {
			// Assignments empty, should not exist. In any case step size does not matter here:
			log::debug!("assignments of a `Schedule` should never be empty.");
			1
		};
		let assignments = assignments
			.into_iter()
			.map(|(a, ratio)| (a, AssignmentState { ratio, remaining: ratio }))
			.collect();

		Self { assignments, end_hint, pos: 0, step }
	}
}

#[frame_support::pallet]
pub mod pallet {

	use super::*;

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + configuration::Config + paras::Config + assigner_on_demand::Config {
		/// Something that provides the weight of this pallet.
		type WeightInfo: WeightInfo;
	}

	/// Scheduled assignment sets.
	///
	/// Assignments as of the given block number. They will go into state once the block number is
	/// reached (and replace whatever was in there before).
	///
	/// Invariants: We assume that this is append only and consumed. In other words new schedules
	/// inserted for a core must have a higher block number than all of the already existing
	/// schedules.
	#[pallet::storage]
	pub(super) type Workplan<T: Config> = StorageMap<
		_,
		Twox256,
		(BlockNumberFor<T>, CoreIndex),
		Schedule<BlockNumberFor<T>>,
		OptionQuery,
	>;

	/// Latest schedule for the given core.
	///
	/// Used for updating `end_hint` of that latest schedule based on newly appended schedules.
	#[pallet::storage]
	pub(super) type LatestCoreSchedule<T: Config> =
		StorageMap<_, Twox256, CoreIndex, BlockNumberFor<T>, OptionQuery>;

	/// Assignments which are currently active.
	///
	/// They will be picked from `PendingAssignments` once we reach the scheduled block number in
	/// `PendingAssignments`.
	#[pallet::storage]
	pub(super) type Workload<T: Config> =
		StorageMap<_, Twox256, CoreIndex, CoreState<BlockNumberFor<T>>, OptionQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_now: BlockNumberFor<T>) -> Weight {
			unimplemented!("TODO: Implement")
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}
}

/// Assignments as provided by our `AssignmentProvider` implementation.
#[derive(Encode, Decode, TypeInfo, Debug)]
pub enum BulkAssignment<OnDemand> {
	/// Assignment was an instantaneous core time assignment.
	Instantaneous(OnDemand),
	/// Assignment was served directly from a core managed directly by bulk.
	Bulk(ParaId),
}

type BulkAssignmentType<T> = BulkAssignment<
	<assigner_on_demand::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::AssignmentType,
>;

impl<OnDemand: Assignment> Assignment for BulkAssignment<OnDemand> {
	fn para_id(&self) -> ParaId {
		match self {
			Self::Instantaneous(on_demand) => on_demand.para_id(),
			Self::Bulk(para_id) => *para_id,
		}
	}
}

impl<T: Config> AssignmentProvider<BlockNumberFor<T>> for Pallet<T> {
	type AssignmentType = BulkAssignmentType<T>;

	type OldAssignmentType = BulkAssignmentType<T>;

	const ASSIGNMENT_STORAGE_VERSION: AssignmentVersion = AssignmentVersion::new(0);

	fn migrate_old_to_current(
		old: Self::OldAssignmentType,
		core: CoreIndex,
	) -> Self::AssignmentType {
		old
	}

	fn session_core_count() -> u32 {
		panic!("TODO");
	}

	fn pop_assignment_for_core(core_idx: CoreIndex) -> Option<Self::AssignmentType> {
		let now = <frame_system::Pallet<T>>::block_number();

		Workload::<T>::mutate(core_idx, |core_state| {
			Self::ensure_workload(now, core_idx, core_state);
			match core_state {
				Some(core_state) => {
					core_state.pos = core_state.pos % core_state.assignments.len() as u16;
					let (a_type, a_state) = &mut core_state
						.assignments
						.get_mut(core_state.pos as usize)
						.expect("We limited pos to the size of the vec one line above. qed");

					// advance:
					a_state.remaining -= core_state.step;
					if a_state.remaining < core_state.step {
						// Assignment exhausted, need to move to the next and credit remaining for next
						// round.
						core_state.pos += 1;
						// Reset to ratio + still remaining "credits":
						a_state.remaining += a_state.ratio;
					}

					match a_type {
						CoreAssignment::Idle => return None,
						CoreAssignment::Pool =>
							return <assigner_on_demand::Pallet<T> as AssignmentProvider<
								BlockNumberFor<T>,
							>>::pop_assignment_for_core(core_idx)
							.map(|assignment| BulkAssignment::Instantaneous(assignment)),
						CoreAssignment::Task(para_id) => return Some(BulkAssignment::Bulk((*para_id).into())),
					}
				},
				None => return None,
			}
		})
	}

	fn report_processed(assignment: Self::AssignmentType) {
		match assignment {
			BulkAssignment::Instantaneous(on_demand) => <assigner_on_demand::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::report_processed(on_demand),
			BulkAssignment::Bulk(_) => {}
		}
	}

	/// Push an assignment back to the front of the queue.
	///
	/// The assignment has not been processed yet. Typically used on session boundaries.
	/// Parameters:
	/// - `assignment`: The on demand assignment.
	fn push_back_assignment(assignment: Self::AssignmentType) {
		match assignment {
			BulkAssignment::Instantaneous(on_demand) =>
				<assigner_on_demand::Pallet<T> as AssignmentProvider<BlockNumberFor<T>>>::push_back_assignment(
				on_demand,
			),
			BulkAssignment::Bulk(_) => {
				// Session changes are rough. We just drop assignments that did not make it on a session boundary.
				// This seems sensible as bulk is region based. Meaning, even if we made the effort catching up on
				// those dropped assignments, this would very likely lead to other assignments not getting served at the
				// "end" (when our assignment set gets replaced).
			},
		}
	}

	fn get_provider_config(_core_idx: CoreIndex) -> AssignmentProviderConfig<BlockNumberFor<T>> {
		panic!("TODO!");
	}
}

impl<T: Config> Pallet<T> {
	/// Ensure given workload for core is up to date.
	fn ensure_workload(
		now: BlockNumberFor<T>,
		core_idx: CoreIndex,
		workload: &mut Option<CoreState<BlockNumberFor<T>>>,
	) {
		let update = if let Some(workload) = workload {
			match workload.end_hint {
				Some(end_hint) if end_hint < now => {
					// Invariant: Always points to next item in `Workplan`, if such an item exists.
					let n = end_hint.saturating_add(BlockNumberFor::<T>::from(1u32));
					// Workload expired - update to whatever is scheduled or `None` if nothing is:
					Workplan::<T>::take((n, core_idx))
				},
				// Still alive:
				Some(_) => return,
				// No end in sight, still valid:
				None => return,
			}
		} else {
			// Invariant: If there is no workload, workplan must be empty for core.
			// Therefore nothing to do here.
			return
		};

		// Needs update:
		*workload = update.map(|schedule| schedule.into());
	}
}

// Tests/Invariant:
// - After `assign_core`, Workload is `Some`.
// - end_hint always points to next item in Workplan.
