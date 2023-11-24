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
//!
//! Invariant: For efficiency the schedule, starting from the current workload always form a linked
//! list.
//!
//! `CoreDescriptor` contains pointers to the begin and the end of the list, together with the
//! currently active assignments.

mod mock_helpers;
#[cfg(test)]
mod tests;

use crate::{
	assigner_on_demand, configuration, paras,
	scheduler::common::{
		Assignment, AssignmentProvider, AssignmentProviderConfig, FixedAssignmentProvider,
		V0Assignment,
	},
};

use frame_support::{defensive, pallet_prelude::*};
use frame_system::pallet_prelude::*;
use pallet_broker::CoreAssignment;
use primitives::{CoreIndex, Id as ParaId};

use sp_std::prelude::*;

pub use pallet::*;

pub const MAX_ASSIGNMENTS_PER_SCHEDULE: u32 = 100;

/// Fraction expressed as a nominator with an assumed denominator of 57,600.
pub type PartsOf57600 = u16;

/// AssignmentSets as they are scheduled by block number
///
/// for a particular core.
#[derive(Encode, Decode, TypeInfo)]
#[cfg_attr(test, derive(PartialEq, RuntimeDebug))]
struct Schedule<N> {
	// Original assignments
	assignments: Vec<(CoreAssignment, PartsOf57600)>,
	/// When do our assignments become invalid, if at all?
	///
	/// If this is `Some`, then this `CoreState` will be dropped at that block number. If this is
	/// `None`, then we will keep serving our core assignments in a circle until a new set of
	/// assignments is scheduled.
	end_hint: Option<N>,

	/// The next queued schedule for this core.
	///
	/// Schedules are forming a queue.
	next_schedule: Option<N>,
}

/// Descriptor for a core.
///
/// Contains pointers to first and last schedule into `CoreSchedules` for that core and keeps track
/// of the currently active work as well.
#[derive(Encode, Decode, TypeInfo, Default)]
#[cfg_attr(test, derive(PartialEq, RuntimeDebug, Clone))]
struct CoreDescriptor<N> {
	/// Meta data about the queued schedules for this core.
	queue: Option<QueueDescriptor<N>>,
	/// Currently performed work.
	current_work: Option<WorkState<N>>,
}

/// Pointers into `CoreSchedules` for a particular core.
///
/// Schedules in `CoreSchedules` form a queue. `Schedule::next_schedule` always pointing to the next
/// item.
#[derive(Encode, Decode, TypeInfo, Copy, Clone)]
#[cfg_attr(test, derive(PartialEq, RuntimeDebug))]
struct QueueDescriptor<N> {
	/// First scheduled item, that is not yet active.
	first: N,
	/// Last scheduled item.
	last: N,
}

#[derive(Encode, Decode, TypeInfo)]
#[cfg_attr(test, derive(PartialEq, RuntimeDebug, Clone))]
struct WorkState<N> {
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
#[cfg_attr(test, derive(PartialEq, RuntimeDebug, Clone, Copy))]
struct AssignmentState {
	/// Ratio of the core this assignment has.
	///
	/// As initially received via `assign_core`.
	ratio: PartsOf57600,
	/// How many parts are remaining in this round?
	///
	/// At the end of each round (in preparation for the next), ratio will be added to remaining.
	/// Then every time we get scheduled we subtract a core worth of points. Once we reach 0 or a
	/// number lower than what a core is worth (`CoreState::step` size), we move on to the next
	/// item in the `Vec`.
	///
	/// The first round starts with remaining = ratio.
	remaining: PartsOf57600,
}

impl<N> From<Schedule<N>> for WorkState<N> {
	fn from(schedule: Schedule<N>) -> Self {
		let Schedule { assignments, end_hint, next_schedule: _ } = schedule;
		let step =
			if let Some(min_step_assignment) = assignments.iter().min_by(|a, b| a.1.cmp(&b.1)) {
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
	pub trait Config:
		frame_system::Config + configuration::Config + paras::Config + assigner_on_demand::Config
	{
	}

	/// Scheduled assignment sets.
	///
	/// Assignments as of the given block number. They will go into state once the block number is
	/// reached (and replace whatever was in there before).
	#[pallet::storage]
	pub(super) type CoreSchedules<T: Config> = StorageMap<
		_,
		Twox256,
		(BlockNumberFor<T>, CoreIndex),
		Schedule<BlockNumberFor<T>>,
		OptionQuery,
	>;

	/// Assignments which are currently active.
	///
	/// They will be picked from `PendingAssignments` once we reach the scheduled block number in
	/// `PendingAssignments`.
	#[pallet::storage]
	pub(super) type CoreDescriptors<T: Config> = StorageMap<
		_,
		Twox256,
		CoreIndex,
		CoreDescriptor<BlockNumberFor<T>>,
		ValueQuery,
		GetDefault,
	>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::error]
	pub enum Error<T> {
		AssignmentsEmpty,
		TooManyAssignments,
		/// Assignments together exceeded 57600.
		OverScheduled,
		/// Assignments together less than 57600
		UnderScheduled,
		/// assign_core is only allowed to append new assignments at the end of already existing
		/// ones.
		DisallowedInsert,
		/// Tried to insert a schedule for the same core and block number as an existing schedule
		DuplicateInsert,
		/// Tried to add an unsorted set of assignments
		AssignmentsNotSorted,
		/// Two or more of the same assignment contained in assignment set
		AssignmentsNotUnique,
	}
}

/// Assignments as provided by our `AssignmentProvider` implementation.
#[derive(Encode, Decode, TypeInfo, Debug)]
#[cfg_attr(test, derive(PartialEq))]
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

impl BulkAssignment<assigner_on_demand::OnDemandAssignment> {
	pub(crate) fn from_v0_assignment(v0: V0Assignment, core_index: CoreIndex) -> Self {
		// There have been no bulk cores previously:
		BulkAssignment::Instantaneous(assigner_on_demand::OnDemandAssignment::from_v0_assignment(
			v0, core_index,
		))
	}
}

impl<T: Config> AssignmentProvider<BlockNumberFor<T>> for Pallet<T> {
	type AssignmentType = BulkAssignmentType<T>;

	fn pop_assignment_for_core(core_idx: CoreIndex) -> Option<Self::AssignmentType> {
		let now = <frame_system::Pallet<T>>::block_number();

		CoreDescriptors::<T>::mutate(core_idx, |core_state| {
			Self::ensure_workload(now, core_idx, core_state);

			let work_state = match &mut core_state.current_work {
				None => return None,
				Some(w) => w,
			};

			work_state.pos = work_state.pos % work_state.assignments.len() as u16;
			let (a_type, a_state) = &mut work_state
				.assignments
				.get_mut(work_state.pos as usize)
				.expect("We limited pos to the size of the vec one line above. qed");

			// advance:
			a_state.remaining -= work_state.step;
			if a_state.remaining < work_state.step {
				// Assignment exhausted, need to move to the next and credit remaining for
				// next round.
				work_state.pos += 1;
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
				CoreAssignment::Task(para_id) =>
					return Some(BulkAssignment::Bulk((*para_id).into())),
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
		let config = <configuration::Pallet<T>>::config();
		AssignmentProviderConfig {
			max_availability_timeouts: config.on_demand_retries,
			ttl: config.on_demand_ttl,
		}
	}

	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn get_mock_assignment(_: CoreIndex, para_id: ParaId) -> Self::AssignmentType {
		// Given that we are not tracking anything in `Bulk` assignments, it is safe to always
		// return a bulk assignment.
		return BulkAssignment::Bulk(para_id)
	}
}

impl<T: Config> FixedAssignmentProvider<BlockNumberFor<T>> for Pallet<T> {
	fn session_core_count() -> u32 {
		// TODO: Rename this config (migration): https://github.com/paritytech/polkadot-sdk/issues/2268
		let config = <configuration::Pallet<T>>::config();
		config.on_demand_cores
	}
}

impl<T: Config> Pallet<T> {
	/// Ensure given workload for core is up to date.
	fn ensure_workload(
		now: BlockNumberFor<T>,
		core_idx: CoreIndex,
		descriptor: &mut CoreDescriptor<BlockNumberFor<T>>,
	) {
		// Workload expired?
		if let Some(end_hint) = descriptor.current_work.as_ref().and_then(|w| w.end_hint) {
			if end_hint >= now {
				descriptor.current_work = None;
			}
		}

		let queue = match descriptor.queue {
			// No queue => Nothing to do.
			None => return,
			Some(q) => q,
		};

		let next_scheduled = queue.first;

		if next_scheduled > now {
			// Not yet ready.
			return
		}

		// Update is needed:
		let update = CoreSchedules::<T>::take((next_scheduled, core_idx));
		let new_first = update.as_ref().and_then(|u| u.next_schedule);
		descriptor.current_work = update.map(|u| u.into());

		descriptor.queue = new_first.map(|new_first| {
			QueueDescriptor {
				first: new_first,
				// `last` stays unaffected, if not empty:
				last: queue.last,
			}
		});
	}

	/// Append another assignment for a core.
	///
	/// Important only appending is allowed. Meaning, all already existing assignments must have a
	/// begin smaller than the one passed here. This restriction exists, because it makes the
	/// insertion O(1) and the author could not think of a reason, why this restriction should be
	/// causing any problems. Inserting arbitrarily causes a `DispatchError::DisallowedInsert`
	/// error. This restriction could easily be lifted if need be and in fact an implementation is
	/// available
	/// [here](https://github.com/paritytech/polkadot-sdk/pull/1694/commits/c0c23b01fd2830910cde92c11960dad12cdff398#diff-0c85a46e448de79a5452395829986ee8747e17a857c27ab624304987d2dde8baR386).
	/// The problem is that insertion complexity then depends on the size of the existing queue,
	/// which makes determining weights hard and could lead to issues like overweight blocks (at
	/// least in theory).
	pub fn assign_core(
		core_idx: CoreIndex,
		begin: BlockNumberFor<T>,
		assignments: Vec<(CoreAssignment, PartsOf57600)>,
		end_hint: Option<BlockNumberFor<T>>,
	) -> Result<(), DispatchError> {
		// TODO: Add this assert once the calls `request_core_count` and `notify_core_count`
		// have been established. assert!(core < core_count);

		// There should be at least one assignment and at most 100
		ensure!(assignments.len() > 0usize, Error::<T>::AssignmentsEmpty);
		ensure!(
			assignments.len() <= MAX_ASSIGNMENTS_PER_SCHEDULE as usize,
			Error::<T>::TooManyAssignments
		);

		// Checking for sort and unique manually, since we don't have access to iterator tools.
		// This way of checking uniqueness only works since we also check sortedness.
		let assignment_targets = assignments.iter().map(|x| &x.0);
		let mut maybe_prior = None;
		for target in assignment_targets {
			if let Some(prior) = maybe_prior {
				ensure!(target != prior, Error::<T>::AssignmentsNotUnique);
				ensure!(target > prior, Error::<T>::AssignmentsNotSorted);
			}
			maybe_prior = Some(target);
		}

		// Check that the total parts between all assignments are equal to 57600
		let parts_sum = assignments
			.iter()
			.map(|assignment| assignment.1)
			.fold(PartsOf57600::from(0u16), |sum, parts| sum.saturating_add(parts));
		ensure!(parts_sum <= PartsOf57600::from(57600u16), Error::<T>::OverScheduled);
		ensure!(parts_sum >= PartsOf57600::from(57600u16), Error::<T>::UnderScheduled);

		CoreDescriptors::<T>::mutate(core_idx, |core_descriptor| {
			let new_queue = match core_descriptor.queue {
				Some(queue) => {
					ensure!(begin > queue.last, Error::<T>::DisallowedInsert,);

					CoreSchedules::<T>::try_mutate((queue.last, core_idx), |schedule| {
						if let Some(schedule) = schedule.as_mut() {
							debug_assert!(schedule.next_schedule.is_none(), "queue.end was supposed to be the end, so the next item must be `None`!");
							schedule.next_schedule = Some(begin);
						} else {
							defensive!("Queue end entry does not exist?");
						}
						CoreSchedules::<T>::try_mutate((begin, core_idx), |schedule| {
							// It should already be impossible to overwrite an existing schedule due
							// to strictly increasing block number. But we check here for safety and
							// in case the design changes.
							ensure!(schedule.is_none(), Error::<T>::DuplicateInsert);
							*schedule =
								Some(Schedule { assignments, end_hint, next_schedule: None });
							Ok::<(), DispatchError>(())
						})?;
						Ok::<(), DispatchError>(())
					})?;

					QueueDescriptor { first: queue.first, last: begin }
				},
				None => {
					// Queue empty, just insert:
					CoreSchedules::<T>::insert(
						(begin, core_idx),
						Schedule { assignments, end_hint, next_schedule: None },
					);
					QueueDescriptor { first: begin, last: begin }
				},
			};
			core_descriptor.queue = Some(new_queue);
			Ok(())
		})
	}
}

// Tests/Invariant:
// - next_schedule always points to next item in CoreSchedules. (handled by
//   next_schedule_always_points_to_next_work_plan_item)
// - Test insertion in the middle, beginning and end: Should fail in all cases but the last.
//   (handled by assign_core_enforces_higher_block_number)
// - Test insertion on empty queue. (handled by assign_core_works_with_no_prior_schedule)
// - Test that assignments are served correctly. E.g. two equal assignments will be served as ABABAB
//   (handled by equal_assignments_served_equally)
// - Have a test that checks that core is shared fairly, even in case of `ratio` not being divisible
//   by `step` (over multiple rounds). (handled by assignment_proportions_indivisible_by_step_work)
// - Test overwrite vs insert: Overwrite no longer allowed - should fail with error. (handled using
//   Error::DuplicateInsert, though earlier errors should prevent this error from ever triggering)
