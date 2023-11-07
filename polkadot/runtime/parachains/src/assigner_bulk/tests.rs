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

use super::*;

use crate::{
	assigner_bulk::{mock_helpers::GenesisConfigBuilder, pallet::Error, Schedule},
	assigner_on_demand::OnDemandAssignment,
	initializer::SessionChangeNotification,
	mock::{
		new_test_ext, Balances, BulkAssigner, OnDemandAssigner, Paras, ParasShared, RuntimeOrigin,
		Scheduler, System, Test,
	},
	paras::{ParaGenesisArgs, ParaKind},
};
use frame_support::{assert_noop, assert_ok, pallet_prelude::*, traits::Currency};
use pallet_broker::TaskId;
use primitives::{BlockNumber, SessionIndex, ValidationCode};
use sp_std::collections::btree_map::BTreeMap;

fn schedule_blank_para(id: ParaId, parakind: ParaKind) {
	let validation_code: ValidationCode = vec![1, 2, 3].into();
	assert_ok!(Paras::schedule_para_initialize(
		id,
		ParaGenesisArgs {
			genesis_head: Vec::new().into(),
			validation_code: validation_code.clone(),
			para_kind: parakind,
		}
	));

	assert_ok!(Paras::add_trusted_validation_code(RuntimeOrigin::root(), validation_code));
}

fn run_to_block(
	to: BlockNumber,
	new_session: impl Fn(BlockNumber) -> Option<SessionChangeNotification<BlockNumber>>,
) {
	while System::block_number() < to {
		let b = System::block_number();

		Scheduler::initializer_finalize();
		Paras::initializer_finalize(b);

		if let Some(notification) = new_session(b + 1) {
			let mut notification_with_session_index = notification;
			// We will make every session change trigger an action queue. Normally this may require
			// 2 or more session changes.
			if notification_with_session_index.session_index == SessionIndex::default() {
				notification_with_session_index.session_index = ParasShared::scheduled_session();
			}
			Paras::initializer_on_new_session(&notification_with_session_index);
			Scheduler::initializer_on_new_session(&notification_with_session_index);
		}

		System::on_finalize(b);

		System::on_initialize(b + 1);
		System::set_block_number(b + 1);

		Paras::initializer_initialize(b + 1);
		Scheduler::initializer_initialize(b + 1);

		// In the real runtime this is expected to be called by the `InclusionInherent` pallet.
		Scheduler::free_cores_and_fill_claimqueue(BTreeMap::new(), b + 1);
	}
}

#[test]
// Should update end hint of current workload and add new schedule to
// Workplan
fn assign_core_works_with_no_prior_schedule() {
	let core_idx = CoreIndex(0);

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		run_to_block(10, |n| if n == 10 { Some(Default::default()) } else { None });

		// Call assign_core
		assert_ok!(BulkAssigner::assign_core(
			Schedule::default(),
			core_idx,
			BlockNumberFor::<Test>::from(11u32),
		));

		// Check Workplan
		assert_eq!(
			Workplan::<Test>::get((BlockNumberFor::<Test>::from(11u32), core_idx)),
			Some(Schedule::default())
		);

		// Check CoreState end_hint
		assert_eq!(
			Workload::<Test>::get(core_idx).end_hint,
			Some(BlockNumberFor::<Test>::from(10u32))
		);
	});
}

#[test]
// Should update the end hint of prior schedule and add new schedule
// to Workplan
fn assign_core_works_with_prior_schedule() {
	let core_idx = CoreIndex(0);

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		run_to_block(10, |n| if n == 10 { Some(Default::default()) } else { None });
		let default_with_end_hint = Schedule { end_hint: Some(14u32), ..Schedule::default() };

		// Call assign_core twice
		assert_ok!(BulkAssigner::assign_core(
			Schedule::default(),
			core_idx,
			BlockNumberFor::<Test>::from(11u32),
		));

		assert_ok!(BulkAssigner::assign_core(
			Schedule::default(),
			core_idx,
			BlockNumberFor::<Test>::from(15u32),
		));

		// Check CoreState end_hint
		assert_eq!(
			Workload::<Test>::get(core_idx).end_hint,
			Some(BlockNumberFor::<Test>::from(10u32))
		);

		// Check Workplan for two entries
		assert_eq!(
			Workplan::<Test>::get((BlockNumberFor::<Test>::from(11u32), core_idx)),
			Some(default_with_end_hint)
		);
		assert_eq!(
			Workplan::<Test>::get((BlockNumberFor::<Test>::from(15u32), core_idx)),
			Some(Schedule::default())
		);
	});
}

#[test]
// Invariants: We assume that Workplan is append only and consumed. In other words new schedules
// inserted for a core must have a higher block number than all of the already existing
// schedules.
fn assign_core_enforces_higher_block_number() {
	let core_idx = CoreIndex(0);

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		run_to_block(10, |n| if n == 10 { Some(Default::default()) } else { None });

		// Call assign core once with higher starting block number
		assert_ok!(BulkAssigner::assign_core(
			Schedule::default(),
			core_idx,
			BlockNumberFor::<Test>::from(11u32),
		));

		// Call again with lower starting block number, expecting an error
		assert_noop!(
			BulkAssigner::assign_core(
				Schedule::default(),
				core_idx,
				BlockNumberFor::<Test>::from(10u32),
			),
			Error::<Test>::InvalidScheduleAssigned
		);
	});
}

#[test]
fn assign_core_enforces_well_formed_schedule() {
	let para_id = ParaId::from(1u32);
	let core_idx = CoreIndex(0);

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		run_to_block(10, |n| if n == 10 { Some(Default::default()) } else { None });

		// Making invalid schedules
		let bad_end_hint = Schedule { end_hint: Some(20), ..Schedule::default() };
		let bad_assignment_count = Schedule { assignments: vec![], ..Schedule::default() };
		let bad_parts_sum = Schedule {
			assignments: vec![
				(CoreAssignment::Task(para_id.into()), PartsOf57600::from(57600u16)),
				(CoreAssignment::Pool, PartsOf57600::from(57600u16)),
			],
			..Schedule::default()
		};

		// Attempting to assign_core with bad schedules
		assert_noop!(
			BulkAssigner::assign_core(bad_end_hint, core_idx, BlockNumberFor::<Test>::from(11u32),),
			Error::<Test>::InvalidScheduleAssigned
		);
		assert_noop!(
			BulkAssigner::assign_core(
				bad_assignment_count,
				core_idx,
				BlockNumberFor::<Test>::from(11u32),
			),
			Error::<Test>::InvalidScheduleAssigned
		);
		assert_noop!(
			BulkAssigner::assign_core(bad_parts_sum, core_idx, BlockNumberFor::<Test>::from(11u32),),
			Error::<Test>::InvalidScheduleAssigned
		);
	});
}

#[test]
fn end_hint_always_points_to_next_work_plan_item() {
	let core_idx = CoreIndex(0);

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		run_to_block(10, |n| if n == 10 { Some(Default::default()) } else { None });
		let start_1 = 10u32;
		let start_2 = 15u32;
		let start_3 = 20u32;
		let start_4 = 25u32;
		let start_5 = 30u32;

		let expected_core_state_3 =
			CoreState { end_hint: Some(start_4 - 1), pos: 1u16, ..CoreState::default() };
		let expected_schedule_4 = Schedule { end_hint: Some(start_5 - 1), ..Schedule::default() };
		let expected_schedule_5 = Schedule::default();

		// Call assign_core for each of five schedules
		assert_ok!(BulkAssigner::assign_core(
			Schedule::default(),
			core_idx,
			BlockNumberFor::<Test>::from(start_1),
		));

		assert_ok!(BulkAssigner::assign_core(
			Schedule::default(),
			core_idx,
			BlockNumberFor::<Test>::from(start_2),
		));

		assert_ok!(BulkAssigner::assign_core(
			Schedule::default(),
			core_idx,
			BlockNumberFor::<Test>::from(start_3),
		));

		assert_ok!(BulkAssigner::assign_core(
			Schedule::default(),
			core_idx,
			BlockNumberFor::<Test>::from(start_4),
		));

		assert_ok!(BulkAssigner::assign_core(
			Schedule::default(),
			core_idx,
			BlockNumberFor::<Test>::from(start_5),
		));

		// Rotate through the first three schedules
		BulkAssigner::pop_assignment_for_core(core_idx);
		run_to_block(15, |n| if n == 15 { Some(Default::default()) } else { None });
		BulkAssigner::pop_assignment_for_core(core_idx);
		run_to_block(20, |n| if n == 20 { Some(Default::default()) } else { None });
		BulkAssigner::pop_assignment_for_core(core_idx);

		// Use saved starting block numbers to check that schedules chain
		// together correctly
		assert_eq!(Workload::<Test>::get(core_idx), expected_core_state_3);

		assert_eq!(
			Workplan::<Test>::get((BlockNumberFor::<Test>::from(start_4), core_idx)),
			Some(expected_schedule_4)
		);
		assert_eq!(
			Workplan::<Test>::get((BlockNumberFor::<Test>::from(start_5), core_idx)),
			Some(expected_schedule_5)
		);
	});
}

#[test]
fn ensure_workload_works() {
	let core_idx = CoreIndex(0);
	let task_1 = TaskId::from(1u32);
	let task_2 = TaskId::from(2u32);
	let test_assignment_state = AssignmentState {
		ratio: PartsOf57600::from(57600u16),
		remaining: PartsOf57600::from(57600u16),
	};

	let mut workload: CoreState<BlockNumberFor<Test>> = CoreState {
		assignments: vec![(CoreAssignment::Task(task_1), test_assignment_state)],
		..CoreState::default()
	};
	let after_case_1 = CoreState { ..workload.clone() };
	let after_case_2 =
		CoreState { end_hint: Some(BlockNumberFor::<Test>::from(10u32)), ..workload.clone() };
	let after_case_3 = CoreState {
		assignments: vec![(CoreAssignment::Task(task_2), test_assignment_state)],
		..CoreState::default()
	};

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		run_to_block(10, |n| if n == 10 { Some(Default::default()) } else { None });

		// Case 1: No schedules in workplan for core
		BulkAssigner::ensure_workload(10u32, core_idx, &mut workload);

		assert_eq!(workload, after_case_1);

		// Case 2: Schedule in workplan for core, but end_hint not reached
		let schedule: Schedule<BlockNumberFor<Test>> = Schedule {
			assignments: vec![(CoreAssignment::Task(task_2), PartsOf57600::from(57600u16))],
			..Schedule::default()
		};

		assert_ok!(BulkAssigner::assign_core(
			schedule,
			core_idx,
			BlockNumberFor::<Test>::from(11u32),
		));

		// Propagate end_hint modification due to assign_core from CoreState in
		// storage to local core state. Normally pop_assignment_for_core would
		// handle this.
		workload.end_hint = Workload::<Test>::get(core_idx).end_hint;

		BulkAssigner::ensure_workload(10u32, core_idx, &mut workload);

		assert_eq!(workload, after_case_2);

		// Case 3: Schedule in workplan for core, end_hint reached. Swaps new CoreState
		// into Workload from Workplan.
		BulkAssigner::ensure_workload(11u32, core_idx, &mut workload);

		assert_eq!(workload, after_case_3);
	});
}

#[test]
fn pop_assignment_for_core_works() {
	let para_id = ParaId::from(1);
	let core_idx = CoreIndex(0);
	let alice = 1u64;
	let amt = 10_000_000u128;
	let on_demand_assignment = OnDemandAssignment::new(para_id, CoreIndex(0));

	let schedule_idle: Schedule<BlockNumberFor<Test>> = Schedule {
		assignments: vec![(CoreAssignment::Idle, PartsOf57600::from(57600u16))],
		..Schedule::default()
	};
	let schedule_task: Schedule<BlockNumberFor<Test>> = Schedule {
		assignments: vec![(CoreAssignment::Task(para_id.into()), PartsOf57600::from(57600u16))],
		..Schedule::default()
	};

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// Initialize the parathread, wait for it to be ready, then add an
		// on demand order to later pop with our bulk assigner.
		schedule_blank_para(para_id, ParaKind::Parathread);
		Balances::make_free_balance_be(&alice, amt);
		run_to_block(10, |n| if n == 10 { Some(Default::default()) } else { None });
		assert_ok!(OnDemandAssigner::place_order_allow_death(
			RuntimeOrigin::signed(alice),
			amt,
			para_id
		));

		// Case 1: Assignment idle
		assert_ok!(BulkAssigner::assign_core(
			schedule_idle,
			core_idx,
			BlockNumberFor::<Test>::from(10u32),
		));

		assert_eq!(BulkAssigner::pop_assignment_for_core(core_idx), None);

		// Case 2: Assignment pool
		assert_ok!(BulkAssigner::assign_core(
			Schedule::default(), //Default is pool
			core_idx,
			BlockNumberFor::<Test>::from(11u32),
		));

		run_to_block(11, |n| if n == 11 { Some(Default::default()) } else { None });

		assert_eq!(
			BulkAssigner::pop_assignment_for_core(core_idx),
			Some(BulkAssignment::Instantaneous(on_demand_assignment))
		);

		// Case 3: Assignment task
		assert_ok!(BulkAssigner::assign_core(
			schedule_task,
			core_idx,
			BlockNumberFor::<Test>::from(12u32),
		));

		run_to_block(12, |n| if n == 12 { Some(Default::default()) } else { None });

		assert_eq!(
			BulkAssigner::pop_assignment_for_core(core_idx),
			Some(BulkAssignment::Bulk(para_id))
		);
	});
}

#[test]
fn assignment_proportions_in_core_state_work() {
	let core_idx = CoreIndex(0);
	let task_1 = TaskId::from(1u32);
	let task_2 = TaskId::from(2u32);

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		run_to_block(10, |n| if n == 10 { Some(Default::default()) } else { None });

		// Task 1 gets 2/3 core usage, while task 2 gets 1/3
		let test_schedule = Schedule {
			assignments: vec![
				(CoreAssignment::Task(task_1), PartsOf57600::from(57600u16 / 3 * 2)),
				(CoreAssignment::Task(task_2), PartsOf57600::from(57600u16 / 3)),
			],
			..Schedule::default()
		};

		assert_ok!(BulkAssigner::assign_core(
			test_schedule,
			core_idx,
			BlockNumberFor::<Test>::from(10u32),
		));

		// Case 1: Current assignment remaining >= step after pop
		{
			assert_eq!(
				BulkAssigner::pop_assignment_for_core(core_idx),
				Some(BulkAssignment::Bulk(task_1.into()))
			);

			assert_eq!(Workload::<Test>::get(core_idx).pos, 0u16);
			// Consumed step should be 1/3 of core parts, leaving 1/3 remaining
			assert_eq!(
				Workload::<Test>::get(core_idx).assignments[0].1.remaining,
				PartsOf57600::from(57600u16 / 3)
			);
		}

		// Case 2: Current assignment remaning < step after pop
		{
			assert_eq!(
				BulkAssigner::pop_assignment_for_core(core_idx),
				Some(BulkAssignment::Bulk(task_1.into()))
			);
			// Pos should have incremented, as assignment had remaining < step
			assert_eq!(Workload::<Test>::get(core_idx).pos, 1u16);
			// Remaining should have started at 1/3 of core work parts. We then subtract
			// step (1/3) and add back ratio (2/3), leaving us with 2/3 of core work parts.
			assert_eq!(
				Workload::<Test>::get(core_idx).assignments[0].1.remaining,
				PartsOf57600::from(57600u16 / 3 * 2)
			);
		}
	});
}
