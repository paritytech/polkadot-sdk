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

//! Running the broker on JAM.
//!
//! There is no relay chain on JAM: the Coretime chain decides how the cores are used, and a core
//! assignment reaches JAM as an `UpwardMessage::AssignCore` that the Parachain Service applies.
//! `AssignCore` queues authorizer hashes instead of tasks, so every task needs its hash registered
//! in [`AuthorizerHashes`].

use crate::*;
use alloc::vec::Vec;
#[cfg(not(test))]
use codec::Encode;
use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};
use parachain_service_core::upward_message::UpwardMessage;
use sp_runtime::traits::{BlockNumberProvider, SaturatedConversion, Zero};

/// The [`CoretimeInterface`] of a Coretime chain running on JAM.
///
/// `SlotProvider` is the JAM timeslot, which stands in for the relay chain block number.
pub struct JamCoretime<T, SlotProvider>(PhantomData<(T, SlotProvider)>);

impl<T, S> CoretimeInterface for JamCoretime<T, S>
where
	T: Config<Coretime = Self>,
	S: BlockNumberProvider + 'static,
{
	type AccountId = T::AccountId;
	type Balance = BalanceOf<T>;
	type RelayChainBlockNumberProvider = S;

	/// The number of cores is fixed by JAM and this chain decides how they are used, so the
	/// request is granted right away.
	fn request_core_count(count: CoreIndex) {
		CoreCountInbox::<T>::put(count);
	}

	/// There is no instantaneous coretime market on JAM, so there is never any revenue.
	fn request_revenue_info_at(when: RCBlockNumberOf<Self>) {
		RevenueInbox::<T>::put(OnDemandRevenueRecord { until: when, amount: Zero::zero() });
	}

	/// There is no instantaneous coretime market on JAM to credit.
	fn credit_account(_who: Self::AccountId, _amount: Self::Balance) {}

	/// JAM applies the assignment as soon as the Parachain Service accumulates it, so `begin` and
	/// `end_hint` have nothing to act on.
	fn assign_core(
		core: CoreIndex,
		_begin: RCBlockNumberOf<Self>,
		assignment: Vec<(CoreAssignment, PartsOf57600)>,
		_end_hint: Option<RCBlockNumberOf<Self>>,
	) {
		Pallet::<T>::send_assign_core(core, &assignment);
	}
}

/// The authorizer queue of a core running `assignment`.
///
/// Every task's authorizer hash is queued once for every 1/80th of the core it owns. The Parachain
/// Service cycles through the queue, so the tasks share the core in these proportions. Pool and
/// idle parts have no authorizer and are left out, as are tasks without an authorizer hash.
pub fn authorizer_queue(
	assignment: &[(CoreAssignment, PartsOf57600)],
	authorizer: impl Fn(TaskId) -> Option<[u8; 32]>,
) -> Vec<[u8; 32]> {
	let mut queue = Vec::new();
	for (task, parts) in assignment {
		let CoreAssignment::Task(task) = task else { continue };
		let Some(hash) = authorizer(*task) else {
			log::warn!(
				target: crate::LOG_TARGET,
				"Task {task} has no authorizer hash, its part of the core is not assigned on JAM"
			);
			continue;
		};
		let entries = (*parts as usize * CORE_MASK_BITS / 57_600).max(1);
		queue.extend(core::iter::repeat(hash).take(entries));
	}
	queue
}

impl<T: Config> Pallet<T> {
	/// The JAM slot the relay chain block number provider is at.
	fn jam_slot() -> u32 {
		RCBlockNumberProviderOf::<T::Coretime>::current_block_number().saturated_into()
	}

	/// The authorizer hash JAM uses for `task` from `slot` on.
	fn authorizer_at(task: TaskId, slot: u32) -> Option<[u8; 32]> {
		match ScheduledAuthorizers::<T>::get(task) {
			Some((from_slot, hash)) if from_slot <= slot => Some(hash),
			_ => AuthorizerHashes::<T>::get(task),
		}
	}

	/// The earliest JAM slot at which an authorizer hash of a task in `assignment` changes.
	fn pending_authorizer_switch(
		assignment: &[(CoreAssignment, PartsOf57600)],
		now: u32,
	) -> Option<u32> {
		assignment
			.iter()
			.filter_map(|(assignment, _)| match assignment {
				CoreAssignment::Task(task) => ScheduledAuthorizers::<T>::get(task)
					.map(|(from_slot, _)| from_slot)
					.filter(|from_slot| *from_slot > now),
				_ => None,
			})
			.min()
	}

	/// Send the assignment of `core` to the Parachain Service.
	pub(crate) fn send_assign_core(core: CoreIndex, assignment: &[(CoreAssignment, PartsOf57600)]) {
		let now = Self::jam_slot();
		Self::send_assign_core_at(core, assignment, now, |task| Self::authorizer_at(task, now));

		// The Parachain Service keeps one pending assignment per core and an inline assignment
		// replaces it, so send the scheduled one again right after.
		if let Some(from_slot) = Self::pending_authorizer_switch(assignment, now) {
			Self::send_assign_core_at(core, assignment, from_slot, |task| {
				Self::authorizer_at(task, from_slot)
			});
		}
	}

	/// Send the assignment of `core` to the Parachain Service, taking effect at `jam_slot`.
	fn send_assign_core_at(
		core: CoreIndex,
		assignment: &[(CoreAssignment, PartsOf57600)],
		jam_slot: u32,
		authorizer: impl Fn(TaskId) -> Option<[u8; 32]>,
	) {
		let queue = authorizer_queue(assignment, authorizer);
		if queue.is_empty() {
			log::warn!(
				target: crate::LOG_TARGET,
				"No task of core {core} has an authorizer hash, nothing is sent to JAM"
			);
			return;
		}
		let message = UpwardMessage::AssignCore {
			core,
			queue,
			// The Parachain Service stays the assigner of the core.
			new_assigner: None,
			jam_slot,
		};
		#[cfg(not(test))]
		cumulus_jam_state_reader::jam_upward::send_upward_message(&message.encode());
		#[cfg(test)]
		test_seam::record(message);
	}

	/// Schedule the authorizer hash of `task` for the JAM slot `from_slot`.
	pub(crate) fn do_schedule_authorizer(task: TaskId, from_slot: u32, authorizer: [u8; 32]) {
		ScheduledAuthorizers::<T>::insert(task, (from_slot, authorizer));
		Self::deposit_event(Event::<T>::AuthorizerScheduled { task, from_slot, authorizer });

		// Every core currently running the task gets an assignment taking effect at `from_slot`.
		for (core, workload) in Workload::<T>::iter() {
			if !workload.iter().any(|item| item.assignment == CoreAssignment::Task(task)) {
				continue;
			}
			Self::send_assign_core_at(
				core,
				&Self::schedule_to_assignment(workload),
				from_slot,
				|task| Self::authorizer_at(task, from_slot),
			);
		}
	}

	/// Make every scheduled authorizer hash whose JAM slot was reached the current one.
	pub(crate) fn process_scheduled_authorizers() -> Weight {
		let now = Self::jam_slot();
		let mut due = Vec::new();
		let mut scanned = 0u64;
		for (task, (from_slot, hash)) in ScheduledAuthorizers::<T>::iter() {
			scanned = scanned.saturating_add(1);
			if from_slot <= now {
				due.push((task, hash));
			}
		}
		for (task, authorizer) in &due {
			AuthorizerHashes::<T>::insert(task, authorizer);
			ScheduledAuthorizers::<T>::remove(task);
			Self::deposit_event(Event::<T>::AuthorizerActivated {
				task: *task,
				authorizer: *authorizer,
			});
		}
		T::DbWeight::get().reads_writes(1 + scanned, 2 * due.len() as u64)
	}

	/// Send the assignment of every core the Parachain Service rejected again.
	pub(crate) fn retry_rejected_assignments() -> Weight {
		// Reading the rejections is a JAM state read, so skip it while no core was ever assigned.
		if Workload::<T>::iter_keys().next().is_none() {
			return T::DbWeight::get().reads(1);
		}
		let mut retried = Vec::new();
		for core in T::RejectedCores::get() {
			if retried.contains(&core) {
				continue;
			}
			let workload = Workload::<T>::get(core);
			if workload.is_empty() {
				continue;
			}
			Self::send_assign_core(core, &Self::schedule_to_assignment(workload));
			Self::deposit_event(Event::<T>::AssignmentRetried { core });
			retried.push(core);
		}
		T::DbWeight::get().reads(1 + retried.len() as u64)
	}
}

/// Collects the upward messages a test would send to the Parachain Service.
#[cfg(test)]
pub(crate) mod test_seam {
	use super::UpwardMessage;
	use alloc::vec::Vec;
	use core::cell::RefCell;

	thread_local! {
		static MESSAGES: RefCell<Vec<UpwardMessage>> = RefCell::new(Vec::new());
	}

	pub(crate) fn record(message: UpwardMessage) {
		MESSAGES.with(|messages| messages.borrow_mut().push(message));
	}

	/// Take every message recorded since the last call.
	pub(crate) fn take() -> Vec<UpwardMessage> {
		MESSAGES.with(|messages| core::mem::take(&mut *messages.borrow_mut()))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;
	use frame_support::{assert_noop, assert_ok, BoundedVec};
	use sp_runtime::DispatchError;

	const A: [u8; 32] = [1; 32];
	const B: [u8; 32] = [2; 32];

	fn registered(task: TaskId) -> Option<[u8; 32]> {
		match task {
			1 => Some(A),
			2 => Some(B),
			_ => None,
		}
	}

	#[test]
	fn a_full_core_queues_its_task_80_times_works() {
		assert_eq!(authorizer_queue(&[(CoreAssignment::Task(1), 57_600)], registered), vec![A; 80]);
	}

	#[test]
	fn a_shared_core_queues_tasks_by_their_parts_works() {
		let queue = authorizer_queue(
			&[(CoreAssignment::Task(1), 14_400), (CoreAssignment::Task(2), 43_200)],
			registered,
		);
		assert_eq!(queue, [vec![A; 20], vec![B; 60]].concat());
	}

	#[test]
	fn pool_idle_and_unregistered_parts_are_left_out_works() {
		let queue = authorizer_queue(
			&[
				(CoreAssignment::Idle, 720),
				(CoreAssignment::Pool, 720),
				(CoreAssignment::Task(3), 720),
				(CoreAssignment::Task(2), 720),
			],
			registered,
		);
		assert_eq!(queue, vec![B]);
	}

	#[test]
	fn register_authorizer_works() {
		TestExt::new().execute_with(|| {
			assert_ok!(Broker::register_authorizer(RuntimeOrigin::root(), 1, Some(A)));
			assert_eq!(AuthorizerHashes::<Test>::get(1), Some(A));
			assert_ok!(Broker::register_authorizer(RuntimeOrigin::root(), 1, None));
			assert_eq!(AuthorizerHashes::<Test>::get(1), None);
		});
	}

	#[test]
	fn register_authorizer_needs_admin_origin_errors() {
		TestExt::new().execute_with(|| {
			assert_noop!(
				Broker::register_authorizer(RuntimeOrigin::signed(2), 1, Some(A)),
				DispatchError::BadOrigin
			);
		});
	}

	fn workload_of(task: TaskId) -> Schedule {
		BoundedVec::truncate_from(vec![ScheduleItem {
			mask: CoreMask::complete(),
			assignment: CoreAssignment::Task(task),
		}])
	}

	/// A core assignment as sent to JAM for `task` with `hash`.
	fn assignment_on(core: CoreIndex, hash: [u8; 32], jam_slot: u32) -> UpwardMessage {
		UpwardMessage::AssignCore { core, queue: vec![hash; 80], new_assigner: None, jam_slot }
	}

	#[test]
	fn schedule_authorizer_sends_the_assignment_at_its_slot_works() {
		TestExt::new().execute_with(|| {
			System::set_block_number(1);
			AuthorizerHashes::<Test>::insert(1, A);
			Workload::<Test>::insert(0, workload_of(1));
			Workload::<Test>::insert(1, workload_of(1));
			test_seam::take();

			assert_ok!(Broker::schedule_authorizer(RuntimeOrigin::root(), 1, 10, B));

			assert_eq!(ScheduledAuthorizers::<Test>::get(1), Some((10, B)));
			assert_eq!(test_seam::take(), vec![assignment_on(0, B, 10), assignment_on(1, B, 10)]);
			System::assert_last_event(
				Event::AuthorizerScheduled { task: 1, from_slot: 10, authorizer: B }.into(),
			);
		});
	}

	#[test]
	fn schedule_authorizer_needs_admin_origin_errors() {
		TestExt::new().execute_with(|| {
			assert_noop!(
				Broker::schedule_authorizer(RuntimeOrigin::signed(2), 1, 10, B),
				DispatchError::BadOrigin
			);
		});
	}

	#[test]
	fn scheduled_authorizer_activates_at_its_slot_works() {
		TestExt::new().execute_with(|| {
			AuthorizerHashes::<Test>::insert(1, A);
			assert_ok!(Broker::schedule_authorizer(RuntimeOrigin::root(), 1, 10, B));
			System::set_block_number(1);

			advance_to(9);
			assert_eq!(AuthorizerHashes::<Test>::get(1), Some(A));
			assert_eq!(ScheduledAuthorizers::<Test>::get(1), Some((10, B)));

			advance_to(10);
			assert_eq!(AuthorizerHashes::<Test>::get(1), Some(B));
			assert_eq!(ScheduledAuthorizers::<Test>::get(1), None);
			System::assert_last_event(Event::AuthorizerActivated { task: 1, authorizer: B }.into());
		});
	}

	#[test]
	fn inline_assignment_resends_the_scheduled_one_works() {
		TestExt::new().execute_with(|| {
			AuthorizerHashes::<Test>::insert(1, A);
			Workload::<Test>::insert(0, workload_of(1));
			assert_ok!(Broker::schedule_authorizer(RuntimeOrigin::root(), 1, 10, B));
			test_seam::take();

			Pallet::<Test>::send_assign_core(
				0,
				&Pallet::<Test>::schedule_to_assignment(Workload::<Test>::get(0)),
			);

			assert_eq!(test_seam::take(), vec![assignment_on(0, A, 0), assignment_on(0, B, 10)]);
		});
	}

	#[test]
	fn an_activated_schedule_is_not_resent_works() {
		TestExt::new().execute_with(|| {
			AuthorizerHashes::<Test>::insert(1, A);
			Workload::<Test>::insert(0, workload_of(1));
			assert_ok!(Broker::schedule_authorizer(RuntimeOrigin::root(), 1, 10, B));
			System::set_block_number(1);
			advance_to(10);
			test_seam::take();

			Pallet::<Test>::send_assign_core(
				0,
				&Pallet::<Test>::schedule_to_assignment(Workload::<Test>::get(0)),
			);

			assert_eq!(test_seam::take(), vec![assignment_on(0, B, 10)]);
		});
	}

	#[test]
	fn schedule_without_an_assigned_core_sends_nothing_works() {
		TestExt::new().execute_with(|| {
			System::set_block_number(1);
			test_seam::take();

			assert_ok!(Broker::schedule_authorizer(RuntimeOrigin::root(), 1, 10, B));

			assert_eq!(ScheduledAuthorizers::<Test>::get(1), Some((10, B)));
			assert_eq!(test_seam::take(), vec![]);
		});
	}
}
