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

//! # Time-Scheduler tests.

use super::*;
use crate::mock::{
	logger, new_test_ext, root, LoggerCall, RuntimeCall, RuntimeOrigin, Scheduler, Test,
	Timestamp,
};
use frame_support::{assert_ok, traits::OnInitialize};

// ==================== Time-based scheduling tests ====================

#[test]
#[docify::export]
fn basic_time_scheduling_works() {
	new_test_ext().execute_with(|| {
		// Set the initial timestamp (e.g., 60_000ms = 1 minute from epoch)
		Timestamp::set_timestamp(60_000);

		// Create a call to schedule
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });

		// Schedule call to be executed at 120_000ms (2 minutes from epoch)
		assert_ok!(Scheduler::schedule(
			RuntimeOrigin::root(),
			120_000, // when: 2 minutes from epoch
			None,    // not periodic
			0,       // priority
			Box::new(call),
		));

		// Check that the task is scheduled in minute 2 (120_000 / 60_000 = 2)
		assert!(!Agenda::<Test>::get(2).is_empty());
		assert!(logger::log().is_empty());

		// Advance timestamp to 120_000ms and run on_initialize
		Timestamp::set_timestamp(120_000);
		Scheduler::on_initialize(2);

		// Check that the log was executed
		assert_eq!(logger::log(), vec![(root(), 42)]);

		// Agenda should be cleaned up after dispatch
		assert!(Agenda::<Test>::get(2).is_empty());
	});
}
