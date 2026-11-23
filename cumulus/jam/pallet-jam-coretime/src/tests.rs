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

use crate::{authorizer_queue, mock::*, AuthorizerHashes, Event, Pallet, ScheduledAuthorizers};
use frame_support::{assert_noop, assert_ok, BoundedVec};
use pallet_broker::{
	ConfigRecord, CoreAssignment, CoreIndex, CoreMask, Schedule, ScheduleItem, TaskId, Workload,
};
use parachain_service_core::{types::MessageId, upward_message::UpwardMessage};
use sp_runtime::{DispatchError, Perbill};

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
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		assert_ok!(JamCoretime::register_authorizer(RuntimeOrigin::root(), 1, Some(A)));
		assert_eq!(AuthorizerHashes::<Test>::get(1), Some(A));
		System::assert_last_event(
			Event::AuthorizerRegistered { task: 1, authorizer: Some(A) }.into(),
		);

		// The broker's admin origin may change it too.
		assert_ok!(JamCoretime::register_authorizer(RuntimeOrigin::signed(Admin::get()), 1, None));
		assert_eq!(AuthorizerHashes::<Test>::get(1), None);
		System::assert_last_event(Event::AuthorizerRegistered { task: 1, authorizer: None }.into());
	});
}

#[test]
fn register_authorizer_needs_admin_origin_errors() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			JamCoretime::register_authorizer(RuntimeOrigin::signed(2), 1, Some(A)),
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

/// A core assignment as sent to JAM for `task` with `hash`: its message id is the core.
fn assignment_on(core: CoreIndex, hash: [u8; 32], jam_slot: u32) -> (MessageId, UpwardMessage) {
	(
		MessageId::from(u64::from(core)),
		UpwardMessage::AssignCore { core, queue: vec![hash; 80], new_assigner: None, jam_slot },
	)
}

#[test]
fn a_core_the_broker_assigns_reaches_jam_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(Broker::configure(
			RuntimeOrigin::root(),
			ConfigRecord {
				advance_notice: 2,
				interlude_length: 1,
				leadin_length: 1,
				ideal_bulk_proportion: Default::default(),
				limit_cores_offered: None,
				region_length: 3,
				renewal_bump: Perbill::from_percent(10),
				contribution_timeout: 5,
			}
		));
		assert_ok!(JamCoretime::register_authorizer(RuntimeOrigin::root(), 1, Some(A)));
		assert_ok!(Broker::reserve(RuntimeOrigin::root(), workload_of(1)));
		assert_ok!(Broker::start_sales(RuntimeOrigin::root(), 100, 0));

		advance_to(10);

		// The broker assigns the reserved core in block 6, and JAM applies it right away.
		assert_eq!(take_upward_messages(), vec![assignment_on(0, A, 6)]);
	});
}

#[test]
fn schedule_authorizer_sends_the_assignment_at_its_slot_works() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		AuthorizerHashes::<Test>::insert(1, A);
		Workload::<Test>::insert(0, workload_of(1));
		Workload::<Test>::insert(1, workload_of(1));

		assert_ok!(JamCoretime::schedule_authorizer(RuntimeOrigin::root(), 1, 10, B));

		assert_eq!(ScheduledAuthorizers::<Test>::get(1), Some((10, B)));
		assert_eq!(take_upward_messages(), vec![assignment_on(0, B, 10), assignment_on(1, B, 10)]);
		System::assert_last_event(
			Event::AuthorizerScheduled { task: 1, from_slot: 10, authorizer: B }.into(),
		);
	});
}

#[test]
fn schedule_authorizer_needs_admin_origin_errors() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			JamCoretime::schedule_authorizer(RuntimeOrigin::signed(2), 1, 10, B),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn scheduled_authorizer_activates_at_its_slot_works() {
	new_test_ext().execute_with(|| {
		AuthorizerHashes::<Test>::insert(1, A);
		assert_ok!(JamCoretime::schedule_authorizer(RuntimeOrigin::root(), 1, 10, B));
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
	new_test_ext().execute_with(|| {
		AuthorizerHashes::<Test>::insert(1, A);
		Workload::<Test>::insert(0, workload_of(1));
		assert_ok!(JamCoretime::schedule_authorizer(RuntimeOrigin::root(), 1, 10, B));
		take_upward_messages();

		Pallet::<Test>::send_assign_core(
			0,
			&Broker::schedule_to_assignment(Workload::<Test>::get(0)),
		);

		assert_eq!(take_upward_messages(), vec![assignment_on(0, A, 0), assignment_on(0, B, 10)]);
	});
}

#[test]
fn an_activated_schedule_is_not_resent_works() {
	new_test_ext().execute_with(|| {
		AuthorizerHashes::<Test>::insert(1, A);
		Workload::<Test>::insert(0, workload_of(1));
		assert_ok!(JamCoretime::schedule_authorizer(RuntimeOrigin::root(), 1, 10, B));
		System::set_block_number(1);
		advance_to(10);
		take_upward_messages();

		Pallet::<Test>::send_assign_core(
			0,
			&Broker::schedule_to_assignment(Workload::<Test>::get(0)),
		);

		assert_eq!(take_upward_messages(), vec![assignment_on(0, B, 10)]);
	});
}

#[test]
fn schedule_without_an_assigned_core_sends_nothing_works() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);

		assert_ok!(JamCoretime::schedule_authorizer(RuntimeOrigin::root(), 1, 10, B));

		assert_eq!(ScheduledAuthorizers::<Test>::get(1), Some((10, B)));
		assert_eq!(take_upward_messages(), vec![]);
	});
}

#[test]
fn a_rejected_assignment_is_sent_again_works() {
	new_test_ext().execute_with(|| {
		AuthorizerHashes::<Test>::insert(1, A);
		Workload::<Test>::insert(0, workload_of(1));
		// Core 0 is named twice; core 3 has no workload.
		RejectedCores::set(vec![0, 3, 0]);

		advance_to(1);

		assert_eq!(take_upward_messages(), vec![assignment_on(0, A, 1)]);
		let retried: Vec<_> = System::events()
			.into_iter()
			.filter_map(|record| match record.event {
				RuntimeEvent::JamCoretime(Event::AssignmentRetried { core }) => Some(core),
				_ => None,
			})
			.collect();
		assert_eq!(retried, vec![0]);
	});
}
