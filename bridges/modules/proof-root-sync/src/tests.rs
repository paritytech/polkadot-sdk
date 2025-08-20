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

#![cfg(test)]

use super::*;
use mock::*;

use frame_support::traits::OnIdle;

#[test]
fn schedule_for_sync_works() {
	run_test(|| {
		Pallet::<TestRuntime>::schedule_for_sync(1, 1);
		Pallet::<TestRuntime>::schedule_for_sync(2, 2);
		Pallet::<TestRuntime>::schedule_for_sync(3, 3);
		Pallet::<TestRuntime>::schedule_for_sync(4, 4);
		Pallet::<TestRuntime>::schedule_for_sync(5, 5);
		assert_eq!(RootsToSend::<TestRuntime>::get(), vec![(1, 1), (2, 2), (3, 3), (4, 4), (5, 5)]);

		// Check ring buffer works and respects `RootsToKeep`.
		Pallet::<TestRuntime>::schedule_for_sync(6, 6);
		Pallet::<TestRuntime>::schedule_for_sync(7, 7);
		assert_eq!(RootsToSend::<TestRuntime>::get(), vec![(3, 3), (4, 4), (5, 5), (6, 6), (7, 7)]);
	});
}

#[test]
fn on_idle_processes_roots() {
	run_test(|| {
		// Schedule multiple roots
		Pallet::<TestRuntime>::schedule_for_sync(1, 1);
		Pallet::<TestRuntime>::schedule_for_sync(2, 2);
		Pallet::<TestRuntime>::schedule_for_sync(3, 3);
		Pallet::<TestRuntime>::schedule_for_sync(4, 4);
		Pallet::<TestRuntime>::schedule_for_sync(5, 5);
		assert_eq!(RootsToSend::<TestRuntime>::get(), vec![(1, 1), (2, 2), (3, 3), (4, 4), (5, 5)]);
		assert!(OnSendConsumer::get_consumed_roots().is_empty());

		// Trigger `on_send`.
		Pallet::<TestRuntime>::on_idle(1_u64, Weight::MAX);
		assert_eq!(OnSendConsumer::get_consumed_roots(), vec![(1, 1), (2, 2)]);
		Pallet::<TestRuntime>::on_idle(2_u64, Weight::MAX);
		assert_eq!(OnSendConsumer::get_consumed_roots(), vec![(3, 3), (4, 4)]);
		Pallet::<TestRuntime>::on_idle(3_u64, Weight::MAX);
		assert_eq!(OnSendConsumer::get_consumed_roots(), vec![(5, 5)]);
		Pallet::<TestRuntime>::on_idle(4_u64, Weight::MAX);
		assert!(OnSendConsumer::get_consumed_roots().is_empty());
	});
}
