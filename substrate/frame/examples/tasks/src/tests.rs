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

//! Tests for `pallet-example-tasks`.
#![cfg(test)]

use crate::{mock::*, Numbers, Total};
use frame_support::{assert_noop, assert_ok, traits::Task};
use sp_runtime::BuildStorage;

// This function basically just builds a genesis storage key/value store according to
// our desired mockup.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = RuntimeGenesisConfig {
		// We use default for brevity, but you can configure as desired if needed.
		system: Default::default(),
	}
	.build_storage()
	.unwrap();
	t.into()
}

#[test]
fn task_enumerate_works() {
	new_test_ext().execute_with(|| {
		Numbers::<Runtime>::insert(0, 1);
		assert_eq!(crate::pallet::Task::<Runtime>::iter().collect::<Vec<_>>().len(), 1);
	});
}

#[test]
fn runtime_task_enumerate_works_via_frame_system_config() {
	new_test_ext().execute_with(|| {
		Numbers::<Runtime>::insert(0, 1);
		Numbers::<Runtime>::insert(1, 4);
		assert_eq!(
			<Runtime as frame_system::Config>::RuntimeTask::iter().collect::<Vec<_>>().len(),
			2
		);
	});
}

#[test]
fn runtime_task_enumerate_works_via_pallet_config() {
	new_test_ext().execute_with(|| {
		Numbers::<Runtime>::insert(1, 4);
		assert_eq!(
			<Runtime as crate::pallet::Config>::RuntimeTask::iter()
				.collect::<Vec<_>>()
				.len(),
			1
		);
	});
}

#[test]
fn task_index_works_at_pallet_level() {
	new_test_ext().execute_with(|| {
		assert_eq!(crate::pallet::Task::<Runtime>::AddNumberIntoTotal { i: 2u32 }.task_index(), 0);
	});
}

#[test]
fn task_index_works_at_runtime_level() {
	new_test_ext().execute_with(|| {
		assert_eq!(
			<Runtime as frame_system::Config>::RuntimeTask::TasksExample(crate::pallet::Task::<
				Runtime,
			>::AddNumberIntoTotal {
				i: 1u32
			})
			.task_index(),
			0
		);
	});
}

#[test]
fn task_execution_works() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		Numbers::<Runtime>::insert(0, 1);
		Numbers::<Runtime>::insert(1, 4);

		let task =
			<Runtime as frame_system::Config>::RuntimeTask::TasksExample(crate::pallet::Task::<
				Runtime,
			>::AddNumberIntoTotal {
				i: 1u32,
			});
		assert_ok!(System::do_task(RuntimeOrigin::signed(1), task.clone(),));
		assert_eq!(Numbers::<Runtime>::get(0), Some(1));
		assert_eq!(Numbers::<Runtime>::get(1), None);
		assert_eq!(Total::<Runtime>::get(), (1, 4));
		System::assert_last_event(frame_system::Event::<Runtime>::TaskCompleted { task }.into());
	});
}

#[test]
fn task_execution_fails_for_invalid_task() {
	new_test_ext().execute_with(|| {
		Numbers::<Runtime>::insert(1, 4);
		assert_noop!(
			System::do_task(
				RuntimeOrigin::signed(1),
				<Runtime as frame_system::Config>::RuntimeTask::TasksExample(
					crate::pallet::Task::<Runtime>::AddNumberIntoTotal { i: 0u32 }
				),
			),
			frame_system::Error::<Runtime>::InvalidTask
		);
	});
}
