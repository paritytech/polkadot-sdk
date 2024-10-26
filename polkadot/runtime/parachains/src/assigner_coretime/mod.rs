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

//! The parachain coretime assignment module.
//!
//! Handles scheduling of assignments coming from the coretime/broker chain. For on-demand
//! assignments it relies on the separate on-demand assignment provider, where it forwards requests
//! to.
//!
//! `CoreDescriptor` contains pointers to the begin and the end of a list of schedules, together
//! with the currently active assignments.

mod mock_helpers;
#[cfg(test)]
mod tests;

use crate::{configuration, on_demand, paras::AssignCoretime, ParaId};

use alloc::{
	collections::{BTreeMap, VecDeque},
	vec,
	vec::Vec,
};
use frame_support::{defensive, pallet_prelude::*};
use frame_system::pallet_prelude::*;
use pallet_broker::CoreAssignment;
use polkadot_primitives::CoreIndex;
use scale_info::TypeInfo;
use sp_runtime::{
	codec::{Decode, Encode},
	traits::{One, Saturating},
	RuntimeDebug,
};

pub use pallet::*;

/// Fraction expressed as a nominator with an assumed denominator of 57,600.
#[derive(RuntimeDebug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
pub struct PartsOf57600(u16);

/// Assignment (ParaId -> CoreIndex).
#[derive(Encode, Decode, TypeInfo, RuntimeDebug, Clone, PartialEq)]
pub enum Assignment {
	/// A pool assignment.
	Pool(ParaId),
	/// A bulk assignment.
	Bulk(ParaId),
}

impl Assignment {
	/// Returns the [`ParaId`] this assignment is associated to.
	pub fn para_id(&self) -> ParaId {
		match self {
			Self::Pool(para_id) => *para_id,
			Self::Bulk(para_id) => *para_id,
		}
	}
}

impl PartsOf57600 {
	pub const ZERO: Self = Self(0);
	pub const FULL: Self = Self(57600);

	pub fn new_saturating(v: u16) -> Self {
		Self::ZERO.saturating_add(Self(v))
	}

	pub fn is_full(&self) -> bool {
		*self == Self::FULL
	}

	pub fn saturating_add(self, rhs: Self) -> Self {
		let inner = self.0.saturating_add(rhs.0);
		if inner > 57600 {
			Self(57600)
		} else {
			Self(inner)
		}
	}

	pub fn saturating_sub(self, rhs: Self) -> Self {
		Self(self.0.saturating_sub(rhs.0))
	}

	pub fn checked_add(self, rhs: Self) -> Option<Self> {
		let inner = self.0.saturating_add(rhs.0);
		if inner > 57600 {
			None
		} else {
			Some(Self(inner))
		}
	}
}

/// Assignments as they are scheduled by block number
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

/// How storage is accessed.
#[derive(Copy, Clone)]
enum AccessMode {
	/// We only want to peek (no side effects).
	Peek,
	/// We need to update state.
	Pop,
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
				PartsOf57600(1)
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
	pub trait Config: frame_system::Config + configuration::Config + on_demand::Config {}

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
	/// TODO: Migration
	#[pallet::storage]
	pub(super) type CoreDescriptors<T: Config> = StorageValue<
		_,
		BTreeMap<CoreIndex, CoreDescriptor<BlockNumberFor<T>>>,
		ValueQuery,
		GetDefault,
	>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::error]
	pub enum Error<T> {
		AssignmentsEmpty,
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
	}
}

impl<T: Config> Pallet<T> {
	/// Peek `num_entries` into the future.
	///
	/// First element in the returne vec will show you what you would get when you call
	/// `pop_assignment_for_core` now. The second what you would get in the next block when you
	/// called `pop_assignment_for_core` again (prediction).
	///
	/// The predictions are accurate in the sense that if an assignment `B` was predicted, it will
	/// never happen that `pop_assignment_for_core` at that block will retrieve an assignment `A`.
	/// What can happen though is that the prediction is empty (returned vec does not contain that
	/// element), but `pop_assignment_for_core` at that block will then return something
	/// regardless.
	///
	/// Invariant to maintain: `pop_assignment_for_core` must be called for each core each block
	/// exactly once for the prediction offered by `peek` to stay accurate. If
	/// `pop_assignment_for_core` was not yet called at this very block, then the first entry in
	/// the vec returned by `peek` will be the assignment statements should be ready for
	/// at this block.
	pub fn peek(num_entries: u8) -> BTreeMap<CoreIndex, VecDeque<ParaId>> {
		let now = frame_system::Pallet::<T>::block_number();
		Self::peek_impl(now, num_entries)
	}

	/// Pops an [`Assignment`] from the provider for a specified [`CoreIndex`].
	///
	/// This is where assignments come into existence.
	pub fn pop_assignments() -> BTreeMap<CoreIndex, Assignment> {
		let now = frame_system::Pallet::<T>::block_number();

		let assignments = CoreDescriptors::<T>::mutate(|core_states| {
			Self::pop_assignments_single_impl(now, core_states, AccessMode::Pop)
				.collect::<BTreeMap<_, _>>()
		});

		// Assignments missing?
		if assignments.len() == Self::num_coretime_cores() {
			return assignments
		}

		// Try to fill missing assignments from the next position (duplication to allow asynchronous
		// backing even for first assignment coming in on a previously empty core):
		let next = now.saturating_add(One::one());
		let mut core_states = CoreDescriptors::<T>::get();
		let next_assignments =
			Self::pop_assignments_single_impl(next, core_states, AccessMode::Peek).collect();
		let mut final_assignments = BTreeMap::new();
		'outer: loop {
			match (assignments.next(), next_assignments.next()) {
				(Some(mut current), Some(mut next)) => {
					// Catch left up:
					while current.0 < next.0 {
						let (core_idx, assignment) = current;
						final_assignments.insert(core_idx, assignment);
						match assignments.next() {
							Some(a) => current = a,
							None => {
								let (core_idx, assignment) = next;
								final_assignments.insert(core_idx, assignment);
								continue 'outer;
							},
						}
					}
					// Catch right up:
					while next.0 < current.0 {
						let (core_idx, assignment) = next;
						final_assignments.insert(core_idx, assignment);
						match next_assignments.next() {
							Some(a) => next = a,
							None => {
								let (core_idx, assignment) = current;
								final_assignments.insert(core_idx, assignment);
								continue 'outer;
							},
						}
					}
					// Equal: Prefer current.
					let (core_idx, assignment) = current;
					final_assignments.insert(core_idx, assignment);
				},
				(Some((core_idx, assignment)), None) =>
					final_assignments.insert(core_idx, assignment),
				(None, Some((core_idx, assignment))) =>
					final_assignments.insert(core_idx, assignment),
			}
		}
	}

	/// Push back a previously popped assignment.
	///
	/// If the assignment could not be processed, it can be pushed back
	/// to the assignment provider in order to be popped again later.
	///
	/// Invariants: The order of assignments returned by `peek` is maintained: The pushed back
	/// assignment will come after the elements returned by `peek` as of before that call or not at
	/// all. The implementation is free to drop the pushed back assignment.
	pub fn push_back_assignment(assignment: Assignment) {
		match assignment {
			Assignment::Pool(para_id) => on_demand::Pallet::<T>::push_back_order(para_id),
			Assignment::Bulk(_) => {
				// Pushing back assignments is not a thing for bulk.
			},
		}
	}

	pub(crate) fn coretime_cores() -> impl Iterator<Item = CoreIndex> {
		(0..Self::num_coretime_cores()).map(|i| CoreIndex(i as _))
	}

	fn num_coretime_cores() -> u32 {
		configuration::ActiveConfig::get().coretime_cores
	}

	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn get_mock_assignment(_: CoreIndex, para_id: polkadot_primitives::Id) -> Assignment {
		// Given that we are not tracking anything in `Bulk` assignments, it is safe to always
		// return a bulk assignment.
		Assignment::Bulk(para_id)
	}

	fn peek_impl(
		mut now: BlockNumberFor<T>,
		num_entries: u8,
	) -> BTreeMap<CoreIndex, VecDeque<ParaId>> {
		let mut core_states = CoreDescriptors::<T>::get();
		let mut result = BTreeMap::with_capacity(Self::num_coretime_cores());
		for i in 0..num_entries {
			let assignments =
				Self::pop_assignments_single_impl(now, &mut core_states, AccessMode::Peek);
			for (core_idx, assignment) in assignments {
				let claim_queue = result.entry(core_idx).or_default();
				// Stop filling on holes, otherwise we get claims at the wrong positions.
				if claim_queue.len() == i {
					claim_queue.push_back(assignment.para_id())
				} else if claim_queue.len() == 0 && i == 1 {
					// Except for position 1: Claim queue was empty before. We now have an incoming
					// assignment on position 1: Duplicate it to position 0 so the chain will
					// get a full asynchronous backing opportunity (and a bonus synchronous
					// backing opportunity).
					claim_queue.push_back(assignment.para_id());
					// And fill position 1:
					claim_queue.push_back(assignment.para_id());
				}
			}
			now.saturating_add(One::one());
		}
		result
	}

	/// Pop assignments for `now`.
	fn pop_assignments_single_impl(
		now: BlockNumberFor<T>,
		core_states: &mut BTreeMap<CoreIndex, CoreDescriptor<BlockNumberFor<T>>>,
		mode: AccessMode,
	) -> impl Iterator<Item = (CoreIndex, Assignment)> {
		let mut bulk_assignments = Vec::with_capacity(Self::num_coretime_cores() as _);
		let mut pool_cores = Vec::with_capacity(Self::num_coretime_cores() as _);
		for (core_idx, core_state) in core_states.iter_mut() {
			Self::ensure_workload(now, *core_idx, core_state, mode);

			let Some(work_state) = core_state.current_work.as_mut() else { continue };

			// Wrap around:
			work_state.pos = work_state.pos % work_state.assignments.len() as u16;
			let (a_type, a_state) = &mut work_state
				.assignments
				.get_mut(work_state.pos as usize)
				.expect("We limited pos to the size of the vec one line above. qed");

			// advance for next pop:
			a_state.remaining = a_state.remaining.saturating_sub(work_state.step);
			if a_state.remaining < work_state.step {
				// Assignment exhausted, need to move to the next and credit remaining for
				// next round.
				work_state.pos += 1;
				// Reset to ratio + still remaining "credits":
				a_state.remaining = a_state.remaining.saturating_add(a_state.ratio);
			}
			match *a_type {
				CoreAssignment::Pool => pool_cores.push(*core_idx),
				CoreAssignment::Task(para_id) =>
					bulk_assignments.push((*core_idx, Assignment::Bulk(para_id.into()))),
				CoreAssignment::Idle => {},
			}
		}

		let pool_assignments =
			on_demand::Pallet::<T>::pop_assignment_for_cores(now, pool_cores.len() as _)
				.map(Assignment::Pool);

		bulk_assignments.into_iter().chain(pool_cores.into_iter().zip(pool_assignments))
	}

	/// Ensure given workload for core is up to date.
	fn ensure_workload(
		now: BlockNumberFor<T>,
		core_idx: CoreIndex,
		descriptor: &mut CoreDescriptor<BlockNumberFor<T>>,
		mode: AccessMode,
	) {
		// Workload expired?
		if descriptor
			.current_work
			.as_ref()
			.and_then(|w| w.end_hint)
			.map_or(false, |e| e <= now)
		{
			descriptor.current_work = None;
		}

		let Some(queue) = descriptor.queue else {
			// No queue.
			return
		};

		let mut next_scheduled = queue.first;

		if next_scheduled > now {
			// Not yet ready.
			return
		}

		// Update is needed:
		let update = loop {
			let Some(update) = (match mode {
				AccessMode::Peek => CoreSchedules::<T>::get((next_scheduled, core_idx)),
				AccessMode::Pop => CoreSchedules::<T>::take((next_scheduled, core_idx)),
			}) else {
				break None
			};

			// Still good?
			if update.end_hint.map_or(true, |e| e > now) {
				break Some(update)
			}
			// Move on if possible:
			if let Some(n) = update.next_schedule {
				next_scheduled = n;
			} else {
				break None
			}
		};

		let new_first = update.as_ref().and_then(|u| u.next_schedule);
		descriptor.current_work = update.map(Into::into);

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
		// There should be at least one assignment.
		ensure!(!assignments.is_empty(), Error::<T>::AssignmentsEmpty);

		// Checking for sort and unique manually, since we don't have access to iterator tools.
		// This way of checking uniqueness only works since we also check sortedness.
		assignments.iter().map(|x| &x.0).try_fold(None, |prev, cur| {
			if prev.map_or(false, |p| p >= cur) {
				Err(Error::<T>::AssignmentsNotSorted)
			} else {
				Ok(Some(cur))
			}
		})?;

		// Check that the total parts between all assignments are equal to 57600
		let parts_sum = assignments
			.iter()
			.map(|assignment| assignment.1)
			.try_fold(PartsOf57600::ZERO, |sum, parts| {
				sum.checked_add(parts).ok_or(Error::<T>::OverScheduled)
			})?;
		ensure!(parts_sum.is_full(), Error::<T>::UnderScheduled);

		CoreDescriptors::<T>::mutate(|core_descriptors| {
			let core_descriptor = core_descriptors.entry(core_idx).or_default();
			let new_queue = match core_descriptor.queue {
				Some(queue) => {
					ensure!(begin > queue.last, Error::<T>::DisallowedInsert);

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

impl<T: Config> AssignCoretime for Pallet<T> {
	fn assign_coretime(id: ParaId) -> DispatchResult {
		let current_block = frame_system::Pallet::<T>::block_number();

		// Add a new core and assign the para to it.
		let mut config = configuration::ActiveConfig::<T>::get();
		let core = config.scheduler_params.num_cores;
		config.scheduler_params.num_cores.saturating_inc();

		// `assign_coretime` is only called at genesis or by root, so setting the active
		// config here is fine.
		configuration::Pallet::<T>::force_set_active_config(config);

		let begin = current_block + One::one();
		let assignment = vec![(pallet_broker::CoreAssignment::Task(id.into()), PartsOf57600::FULL)];
		Pallet::<T>::assign_core(CoreIndex(core), begin, assignment, None)
	}
}
