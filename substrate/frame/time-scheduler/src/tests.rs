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
	logger, new_test_ext, root, run_to_time, LoggerCall, Preimage, RuntimeCall, RuntimeOrigin,
	Scheduler, Test, Timestamp,
};
use frame_support::{assert_err, assert_noop, assert_ok, traits::OnInitialize};
use sp_runtime::{traits::BadOrigin, DispatchError};

#[test]
#[docify::export]
fn basic_scheduling_works() {
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

#[test]
fn schedule_after_works() {
	new_test_ext().execute_with(|| {
		// Set initial time to minute 2 (120_000ms)
		Timestamp::set_timestamp(120_000);

		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });

		// Schedule call to execute 3 minutes after current time
		// With After(180_000), it should be scheduled at least one minute after current time
		// plus the delay, so approximately minute 5 or 6
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::After(180_000), // 3 minutes delay
			None,
			127,
			root(),
			Preimage::bound(call).unwrap()
		));

		// Should not execute at minute 4
		run_to_time(240_000);
		assert!(logger::log().is_empty());

		// Should execute at minute 6 (120_000 + 180_000 + 60_000 = 360_000)
		run_to_time(360_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);
	});
}

#[test]
fn schedule_after_zero_works() {
	new_test_ext().execute_with(|| {
		// Set initial time to minute 2 (120_000ms)
		Timestamp::set_timestamp(120_000);

		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });

		// Schedule call with After(0) - should schedule for next minute
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::After(0),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap()
		));

		// Should execute in the next minute (minute 3 = 180_000ms)
		run_to_time(180_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);
	});
}

#[test]
fn periodic_scheduling_works() {
	new_test_ext().execute_with(|| {
		// Set initial time to minute 1 (60_000ms)
		Timestamp::set_timestamp(60_000);

		// Schedule at minute 2, every 2 minutes, 3 times
		// Period: 120_000ms (2 minutes)
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(120_000), // minute 2
			Some((120_000, 3)),        // every 2 minutes, 3 times
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		// Minute 1 - not yet
		assert!(logger::log().is_empty());

		// Minute 2 - first execution
		run_to_time(120_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);

		// Minute 3 - nothing
		run_to_time(180_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);

		// Minute 4 - second execution
		run_to_time(240_000);
		assert_eq!(logger::log(), vec![(root(), 42), (root(), 42)]);

		// Minute 5 - nothing
		run_to_time(300_000);
		assert_eq!(logger::log(), vec![(root(), 42), (root(), 42)]);

		// Minute 6 - third execution
		run_to_time(360_000);
		assert_eq!(logger::log(), vec![(root(), 42), (root(), 42), (root(), 42)]);

		// Minute 8 - no more executions
		run_to_time(480_000);
		assert_eq!(logger::log(), vec![(root(), 42), (root(), 42), (root(), 42)]);
	});
}

#[test]
fn cancel_named_scheduling_works_with_normal_cancel() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		// Schedule named task at minute 2
		Scheduler::do_schedule_named(
			[1u8; 32],
			DispatchTime::At(120_000),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
				i: 69,
				weight: Weight::from_parts(10, 0),
			}))
			.unwrap(),
		)
		.unwrap();

		// Schedule anonymous task at minute 2
		let address = Scheduler::do_schedule(
			DispatchTime::At(120_000),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
				i: 42,
				weight: Weight::from_parts(10, 0),
			}))
			.unwrap(),
		)
		.unwrap();

		// Both tasks scheduled
		assert_eq!(Agenda::<Test>::get(2).len(), 2);

		// Cancel both tasks
		assert_ok!(Scheduler::do_cancel_named(None, [1u8; 32]));
		assert_ok!(Scheduler::do_cancel(None, address));

		// Run past the scheduled time
		run_to_time(120_000);
		run_to_time(600_000);

		// Nothing executed
		assert!(logger::log().is_empty());
	});
}

#[test]
fn cancel_named_periodic_scheduling_works() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		// Schedule named periodic task: at minute 2, every 2 minutes, 3 times
		Scheduler::do_schedule_named(
			[1u8; 32],
			DispatchTime::At(120_000),
			Some((120_000, 3)),
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
				i: 42,
				weight: Weight::from_parts(10, 0),
			}))
			.unwrap(),
		)
		.unwrap();

		// Same id results in error
		assert!(Scheduler::do_schedule_named(
			[1u8; 32],
			DispatchTime::At(120_000),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
				i: 69,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap(),
		)
		.is_err());

		// Different id is ok
		Scheduler::do_schedule_named(
			[2u8; 32],
			DispatchTime::At(480_000), // minute 8
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
				i: 69,
				weight: Weight::from_parts(10, 0),
			}))
			.unwrap(),
		)
		.unwrap();

		// First execution at minute 2
		run_to_time(120_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);

		// Cancel the periodic task after first execution
		assert_ok!(Scheduler::do_cancel_named(None, [1u8; 32]));

		// Run to minute 8 - only task 69 should execute
		run_to_time(480_000);
		assert_eq!(logger::log(), vec![(root(), 42), (root(), 69)]);
	});
}

#[test]
fn reschedule_works() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });

		// Schedule at minute 2
		let address = Scheduler::do_schedule(
			DispatchTime::At(120_000),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();

		assert_eq!(address, (2, 0));

		// Reschedule to minute 3
		let new_address = Scheduler::do_reschedule(address, DispatchTime::At(180_000)).unwrap();
		assert_eq!(new_address, (3, 0));

		// Cannot reschedule to same minute
		assert_noop!(
			Scheduler::do_reschedule(new_address, DispatchTime::At(180_000)),
			Error::<Test>::RescheduleNoChange
		);

		// Minute 2 - nothing
		run_to_time(120_000);
		assert!(logger::log().is_empty());

		// Minute 3 - executes
		run_to_time(180_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);
	});
}

#[test]
fn reschedule_named_works() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });

		// Schedule named task at minute 2
		let address = Scheduler::do_schedule_named(
			[1u8; 32],
			DispatchTime::At(120_000),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();

		assert_eq!(address, (2, 0));

		// Reschedule to minute 3
		let new_address =
			Scheduler::do_reschedule_named([1u8; 32], DispatchTime::At(180_000)).unwrap();
		assert_eq!(new_address, (3, 0));

		// Cannot reschedule to same minute
		assert_noop!(
			Scheduler::do_reschedule_named([1u8; 32], DispatchTime::At(180_000)),
			Error::<Test>::RescheduleNoChange
		);

		// Minute 2 - nothing
		run_to_time(120_000);
		assert!(logger::log().is_empty());

		// Minute 3 - executes
		run_to_time(180_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);
	});
}

#[test]
fn reschedule_named_periodic_works() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });

		// Schedule named periodic task: at minute 2, every 2 minutes, 3 times
		let address = Scheduler::do_schedule_named(
			[1u8; 32],
			DispatchTime::At(120_000),
			Some((120_000, 3)),
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();

		assert_eq!(address, (2, 0));

		// Reschedule first execution to minute 3
		let new_address =
			Scheduler::do_reschedule_named([1u8; 32], DispatchTime::At(180_000)).unwrap();
		assert_eq!(new_address, (3, 0));

		// Can reschedule again to minute 4
		let new_address =
			Scheduler::do_reschedule_named([1u8; 32], DispatchTime::At(240_000)).unwrap();
		assert_eq!(new_address, (4, 0));

		// Minute 3 - nothing (was rescheduled)
		run_to_time(180_000);
		assert!(logger::log().is_empty());

		// Minute 4 - first execution
		run_to_time(240_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);

		// After execution, task is rescheduled to minute 6 (4 + 2)
		// Reschedule it to minute 7 instead
		assert_ok!(Scheduler::do_reschedule_named([1u8; 32], DispatchTime::At(420_000)));

		// Minute 5 - nothing
		run_to_time(300_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);

		// Minute 6 - nothing (was rescheduled to minute 7)
		run_to_time(360_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);

		// Minute 7 - second execution
		run_to_time(420_000);
		assert_eq!(logger::log(), vec![(root(), 42), (root(), 42)]);

		// Minute 9 - third and final execution (7 + 2 minutes)
		run_to_time(540_000);
		assert_eq!(logger::log(), vec![(root(), 42), (root(), 42), (root(), 42)]);

		// No more executions
		run_to_time(600_000);
		assert_eq!(logger::log(), vec![(root(), 42), (root(), 42), (root(), 42)]);
	});
}

#[test]
fn retry_scheduling_works() {
	new_test_ext().execute_with(|| {
		// Task fails until we reach minute 8 (480_000ms)
		logger::set_time_threshold(480_000, 6_000_000);

		// Set initial time to minute 1
		Timestamp::set_timestamp(60_000);

		// Task 42 at minute 4 (240_000ms)
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(240_000),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(LoggerCall::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));
		assert!(Agenda::<Test>::get(4)[0].is_some());

		// Retry 10 times every 3 minutes (180_000ms)
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 10, 180_000));
		assert_eq!(Retries::<Test>::iter().count(), 1);

		// Minute 3 - not yet
		run_to_time(180_000);
		assert!(logger::log().is_empty());
		assert!(Agenda::<Test>::get(4)[0].is_some());

		// Minute 4 - task fails, should be retried at minute 7 (240_000 + 180_000 = 420_000)
		run_to_time(240_000);
		assert!(Agenda::<Test>::get(4).is_empty());
		assert!(Agenda::<Test>::get(7)[0].is_some());
		assert!(logger::log().is_empty());

		// Minute 6 - still waiting
		run_to_time(360_000);
		assert!(Agenda::<Test>::get(7)[0].is_some());
		assert!(logger::log().is_empty());

		// Minute 7 - task still fails, should be retried at minute 10
		run_to_time(420_000);
		assert!(Agenda::<Test>::get(7).is_empty());
		assert!(Agenda::<Test>::get(10)[0].is_some());
		assert!(logger::log().is_empty());

		// Minute 8 - still waiting (threshold now allows success)
		run_to_time(480_000);
		assert!(Agenda::<Test>::get(10)[0].is_some());
		assert!(logger::log().is_empty());

		// Minute 9 - still waiting
		run_to_time(540_000);
		assert!(logger::log().is_empty());
		assert_eq!(Retries::<Test>::iter().count(), 1);

		// Minute 10 - finally succeeds
		run_to_time(600_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);
		assert_eq!(Retries::<Test>::iter().count(), 0);

		// No more executions
		run_to_time(660_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);

		logger::clear_time_threshold();
	});
}

#[test]
fn named_retry_scheduling_works() {
	new_test_ext().execute_with(|| {
		// Task fails until we reach minute 8 (480_000ms)
		logger::set_time_threshold(480_000, 6_000_000);

		// Set initial time to minute 1
		Timestamp::set_timestamp(60_000);

		// Named task 42 at minute 4 (240_000ms)
		let call = RuntimeCall::Logger(LoggerCall::timed_log {
			i: 42,
			weight: Weight::from_parts(10, 0),
		});
		assert_eq!(
			Scheduler::do_schedule_named(
				[1u8; 32],
				DispatchTime::At(240_000),
				None,
				127,
				root(),
				Preimage::bound(call).unwrap(),
			)
			.unwrap(),
			(4, 0)
		);
		assert!(Agenda::<Test>::get(4)[0].is_some());

		// Retry 10 times every 3 minutes (180_000ms)
		assert_ok!(Scheduler::set_retry_named(root().into(), [1u8; 32], 10, 180_000));
		assert_eq!(Retries::<Test>::iter().count(), 1);

		// Minute 3 - not yet
		run_to_time(180_000);
		assert!(logger::log().is_empty());
		assert!(Agenda::<Test>::get(4)[0].is_some());

		// Minute 4 - task fails, should be retried at minute 7
		run_to_time(240_000);
		assert!(Agenda::<Test>::get(4).is_empty());
		assert!(Agenda::<Test>::get(7)[0].is_some());
		assert!(logger::log().is_empty());

		// Minute 7 - task still fails, should be retried at minute 10
		run_to_time(420_000);
		assert!(Agenda::<Test>::get(7).is_empty());
		assert!(Agenda::<Test>::get(10)[0].is_some());
		assert!(logger::log().is_empty());

		// Minute 10 - finally succeeds
		run_to_time(600_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);
		assert_eq!(Retries::<Test>::iter().count(), 0);

		logger::clear_time_threshold();
	});
}

#[test]
fn retry_scheduling_expires() {
	new_test_ext().execute_with(|| {
		// Task will always fail (threshold is in the past)
		logger::set_time_threshold(1, 60_000);

		// Set initial time to minute 1
		Timestamp::set_timestamp(60_000);

		// Task 42 at minute 4 (240_000ms)
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(240_000),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(LoggerCall::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));
		assert!(Agenda::<Test>::get(4)[0].is_some());

		// Task 42 will be retried 3 times every minute (60_000ms)
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 3, 60_000));
		assert_eq!(Retries::<Test>::iter().count(), 1);

		// Minute 3 - not yet
		run_to_time(180_000);
		assert!(logger::log().is_empty());
		assert!(Agenda::<Test>::get(4)[0].is_some());

		// Minute 4 - task fails, scheduled for minute 5
		run_to_time(240_000);
		assert!(Agenda::<Test>::get(4).is_empty());
		assert!(Agenda::<Test>::get(5)[0].is_some());
		assert_eq!(Retries::<Test>::get((5, 0)).unwrap().remaining, 2);
		assert!(logger::log().is_empty());

		// Minute 5 - task fails again, scheduled for minute 6
		run_to_time(300_000);
		assert!(Agenda::<Test>::get(5).is_empty());
		assert!(Agenda::<Test>::get(6)[0].is_some());
		assert_eq!(Retries::<Test>::get((6, 0)).unwrap().remaining, 1);
		assert!(logger::log().is_empty());

		// Minute 6 - task fails again, scheduled for minute 7
		run_to_time(360_000);
		assert!(Agenda::<Test>::get(6).is_empty());
		assert!(Agenda::<Test>::get(7)[0].is_some());
		assert_eq!(Retries::<Test>::get((7, 0)).unwrap().remaining, 0);
		assert!(logger::log().is_empty());

		// Minute 7 - task fails, no more retries, gets dropped
		run_to_time(420_000);
		assert_eq!(Agenda::<Test>::iter().count(), 0);
		assert_eq!(Retries::<Test>::iter().count(), 0);
		assert!(logger::log().is_empty());

		logger::clear_time_threshold();
	});
}

#[test]
fn set_retry_works() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		// Task 42 at minute 4
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(240_000),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(LoggerCall::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		assert!(Agenda::<Test>::get(4)[0].is_some());
		// Make sure the retry configuration was stored
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 10, 120_000));
		assert_eq!(
			Retries::<Test>::get((4, 0)),
			Some(RetryConfig { total_retries: 10, remaining: 10, period: 120_000 })
		);
	});
}

#[test]
fn set_named_retry_works() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		// Named task 42 at minute 4
		assert_ok!(Scheduler::do_schedule_named(
			[42u8; 32],
			DispatchTime::At(240_000),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(LoggerCall::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		assert!(Agenda::<Test>::get(4)[0].is_some());
		// Make sure the retry configuration was stored
		assert_ok!(Scheduler::set_retry_named(root().into(), [42u8; 32], 10, 120_000));
		let address = Lookup::<Test>::get([42u8; 32]).unwrap();
		assert_eq!(
			Retries::<Test>::get(address),
			Some(RetryConfig { total_retries: 10, remaining: 10, period: 120_000 })
		);
	});
}

#[test]
fn set_retry_bad_origin() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		// Task 42 at minute 4 with account 101 as origin
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(240_000),
			None,
			127,
			frame_system::RawOrigin::Signed(101).into(),
			Preimage::bound(RuntimeCall::Logger(LoggerCall::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		assert!(Agenda::<Test>::get(4)[0].is_some());
		// Try to change the retry config with a different (non-root) account
		let res: Result<(), DispatchError> =
			Scheduler::set_retry(RuntimeOrigin::signed(102), (4, 0), 10, 120_000);
		assert_eq!(res, Err(BadOrigin.into()));
	});
}

#[test]
fn cancel_removes_retry_entry() {
	new_test_ext().execute_with(|| {
		// Task fails until minute 99
		logger::set_time_threshold(99 * 60_000, 100 * 60_000);

		// Set initial time
		Timestamp::set_timestamp(60_000);

		// Task 20 at minute 4
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(240_000),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(LoggerCall::timed_log {
				i: 20,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));
		// Named task 42 at minute 4
		assert_ok!(Scheduler::do_schedule_named(
			[1u8; 32],
			DispatchTime::At(240_000),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(LoggerCall::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		assert_eq!(Agenda::<Test>::get(4).len(), 2);
		// Task 20 will be retried 10 times every minute
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 10, 60_000));
		// Task 42 will be retried 10 times every minute
		assert_ok!(Scheduler::set_retry_named(root().into(), [1u8; 32], 10, 60_000));
		assert_eq!(Retries::<Test>::iter().count(), 2);

		// Minute 3 - not yet
		run_to_time(180_000);
		assert!(logger::log().is_empty());
		assert_eq!(Agenda::<Test>::get(4).len(), 2);

		// Minute 4 - both tasks fail
		run_to_time(240_000);
		assert!(Agenda::<Test>::get(4).is_empty());
		// 42 and 20 are rescheduled for minute 5
		assert_eq!(Agenda::<Test>::get(5).len(), 2);
		assert!(logger::log().is_empty());

		// Minute 5 - 42 and 20 still fail
		run_to_time(300_000);
		// 42 and 20 rescheduled for minute 6
		assert_eq!(Agenda::<Test>::get(6).len(), 2);
		assert_eq!(Retries::<Test>::iter().count(), 2);
		assert!(logger::log().is_empty());

		// Even though 42 is being retried, the tasks scheduled for retries are not named
		assert_eq!(Lookup::<Test>::iter().count(), 0);
		assert!(Scheduler::cancel(root().into(), 6, 0).is_ok());

		// 20 is removed, 42 still fails
		run_to_time(360_000);
		// 42 rescheduled for minute 7
		assert_eq!(Agenda::<Test>::get(7).len(), 1);
		// 20's retry entry is removed
		assert!(!Retries::<Test>::contains_key((4, 0)));
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert!(logger::log().is_empty());

		assert!(Scheduler::cancel(root().into(), 7, 0).is_ok());

		// Both tasks are canceled, everything is removed now
		run_to_time(420_000);
		assert!(Agenda::<Test>::get(8).is_empty());
		assert_eq!(Retries::<Test>::iter().count(), 0);

		logger::clear_time_threshold();
	});
}

#[test]
fn cancel_retries_works() {
	new_test_ext().execute_with(|| {
		// Task fails until minute 99
		logger::set_time_threshold(99 * 60_000, 100 * 60_000);

		// Set initial time
		Timestamp::set_timestamp(60_000);

		// Task 20 at minute 4
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(240_000),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(LoggerCall::timed_log {
				i: 20,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));
		// Named task 42 at minute 4
		assert_ok!(Scheduler::do_schedule_named(
			[1u8; 32],
			DispatchTime::At(240_000),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(LoggerCall::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		assert_eq!(Agenda::<Test>::get(4).len(), 2);
		// Task 20 will be retried 10 times every minute
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 10, 60_000));
		// Task 42 will be retried 10 times every minute
		assert_ok!(Scheduler::set_retry_named(root().into(), [1u8; 32], 10, 60_000));
		assert_eq!(Retries::<Test>::iter().count(), 2);

		// Minute 3 - not yet
		run_to_time(180_000);
		assert!(logger::log().is_empty());
		assert_eq!(Agenda::<Test>::get(4).len(), 2);

		// Cancel the retry config for 20
		assert_ok!(Scheduler::cancel_retry(root().into(), (4, 0)));
		assert_eq!(Retries::<Test>::iter().count(), 1);
		// Cancel the retry config for 42
		assert_ok!(Scheduler::cancel_retry_named(root().into(), [1u8; 32]));
		assert_eq!(Retries::<Test>::iter().count(), 0);

		// Minute 4 - both tasks failed and there are no more retries, so they are evicted
		run_to_time(240_000);
		assert_eq!(Agenda::<Test>::get(4).len(), 0);
		assert_eq!(Retries::<Test>::iter().count(), 0);

		logger::clear_time_threshold();
	});
}

#[test]
fn scheduler_respects_weight_limits() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let max_weight: Weight = <Test as Config>::MaximumWeight::get();
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: max_weight / 3u64 * 2u64 });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(240_000), // minute 4
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		));
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 69, weight: max_weight / 3u64 * 2u64 });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(240_000), // minute 4
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		));

		// 69 and 42 do not fit together
		run_to_time(240_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);

		// 69 executes in the next minute
		run_to_time(300_000);
		assert_eq!(logger::log(), vec![(root(), 42), (root(), 69)]);
	});
}

#[test]
fn scheduler_respects_priority_ordering() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let max_weight: Weight = <Test as Config>::MaximumWeight::get();

		// Task with lower priority (higher number = lower priority)
		let call = RuntimeCall::Logger(LoggerCall::log { i: 42, weight: max_weight / 3u64 });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(240_000), // minute 4
			None,
			1, // lower priority
			root(),
			Preimage::bound(call).unwrap(),
		));

		// Task with higher priority (lower number = higher priority)
		let call = RuntimeCall::Logger(LoggerCall::log { i: 69, weight: max_weight / 3u64 });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(240_000), // minute 4
			None,
			0, // higher priority
			root(),
			Preimage::bound(call).unwrap(),
		));

		// 69 should execute first due to higher priority
		run_to_time(240_000);
		assert_eq!(logger::log(), vec![(root(), 69), (root(), 42)]);
	});
}

#[test]
fn fails_to_schedule_task_in_the_past() {
	new_test_ext().execute_with(|| {
		// Set time to minute 3 (180_000ms)
		Timestamp::set_timestamp(180_000);

		let call1 = Box::new(RuntimeCall::Logger(LoggerCall::log {
			i: 69,
			weight: Weight::from_parts(10, 0),
		}));
		let call2 = Box::new(RuntimeCall::Logger(LoggerCall::log {
			i: 42,
			weight: Weight::from_parts(10, 0),
		}));
		let call3 = Box::new(RuntimeCall::Logger(LoggerCall::log {
			i: 42,
			weight: Weight::from_parts(10, 0),
		}));

		// Try to schedule at minute 2 (120_000ms) - in the past
		assert_noop!(
			Scheduler::schedule_named(RuntimeOrigin::root(), [1u8; 32], 120_000, None, 127, call1),
			Error::<Test>::TargetTimestampInPast,
		);

		// Try to schedule at minute 2 (120_000ms) - in the past
		assert_noop!(
			Scheduler::schedule(RuntimeOrigin::root(), 120_000, None, 127, call2),
			Error::<Test>::TargetTimestampInPast,
		);

		// Try to schedule at current minute (180_000ms) - also in the past
		assert_noop!(
			Scheduler::schedule(RuntimeOrigin::root(), 180_000, None, 127, call3),
			Error::<Test>::TargetTimestampInPast,
		);
	});
}

#[test]
fn cancel_last_task_removes_agenda() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let when = 4u64; // minute 4
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let address = Scheduler::do_schedule(
			DispatchTime::At(240_000), // minute 4
			None,
			127,
			root(),
			Preimage::bound(call.clone()).unwrap(),
		)
		.unwrap();
		let address2 = Scheduler::do_schedule(
			DispatchTime::At(240_000), // minute 4
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();

		// Two tasks at agenda
		assert!(Agenda::<Test>::get(when).len() == 2);
		assert_ok!(Scheduler::do_cancel(None, address));
		// Still two tasks at agenda, `None` and `Some`
		assert!(Agenda::<Test>::get(when).len() == 2);
		// Cancel last task from agenda
		assert_ok!(Scheduler::do_cancel(None, address2));
		// If all tasks `None`, agenda fully removed
		assert!(Agenda::<Test>::get(when).len() == 0);
	});
}

#[test]
fn cancel_named_last_task_removes_agenda() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let when = 4u64; // minute 4
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		Scheduler::do_schedule_named(
			[1u8; 32],
			DispatchTime::At(240_000), // minute 4
			None,
			127,
			root(),
			Preimage::bound(call.clone()).unwrap(),
		)
		.unwrap();
		Scheduler::do_schedule_named(
			[2u8; 32],
			DispatchTime::At(240_000), // minute 4
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();

		// Two tasks at agenda
		assert!(Agenda::<Test>::get(when).len() == 2);
		assert_ok!(Scheduler::do_cancel_named(None, [2u8; 32]));
		// Removes trailing `None` and leaves one task
		assert!(Agenda::<Test>::get(when).len() == 1);
		// Cancel last task from agenda
		assert_ok!(Scheduler::do_cancel_named(None, [1u8; 32]));
		// If all tasks `None`, agenda fully removed
		assert!(Agenda::<Test>::get(when).len() == 0);
	});
}

#[test]
fn reschedule_last_task_removes_agenda() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let when = 4u64; // minute 4
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let address = Scheduler::do_schedule(
			DispatchTime::At(240_000), // minute 4
			None,
			127,
			root(),
			Preimage::bound(call.clone()).unwrap(),
		)
		.unwrap();
		let address2 = Scheduler::do_schedule(
			DispatchTime::At(240_000), // minute 4
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();

		// Two tasks at agenda
		assert!(Agenda::<Test>::get(when).len() == 2);
		assert_ok!(Scheduler::do_cancel(None, address));
		// Still two tasks at agenda, `None` and `Some`
		assert!(Agenda::<Test>::get(when).len() == 2);
		// Reschedule last task from agenda to minute 5
		assert_eq!(
			Scheduler::do_reschedule(address2, DispatchTime::At(300_000)).unwrap(),
			(5, 0)
		);
		// If all tasks `None`, agenda fully removed
		assert!(Agenda::<Test>::get(when).len() == 0);
	});
}

#[test]
fn root_calls_works() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let call = Box::new(RuntimeCall::Logger(LoggerCall::log {
			i: 69,
			weight: Weight::from_parts(10, 0),
		}));
		let call2 = Box::new(RuntimeCall::Logger(LoggerCall::log {
			i: 42,
			weight: Weight::from_parts(10, 0),
		}));

		assert_ok!(Scheduler::schedule_named(
			RuntimeOrigin::root(),
			[1u8; 32],
			240_000, // minute 4
			None,
			127,
			call,
		));
		assert_ok!(Scheduler::schedule(
			RuntimeOrigin::root(),
			240_000, // minute 4
			None,
			127,
			call2
		));

		// Minute 3 - scheduled calls are in the agenda
		run_to_time(180_000);
		assert_eq!(Agenda::<Test>::get(4).len(), 2);
		assert!(logger::log().is_empty());

		assert_ok!(Scheduler::cancel_named(RuntimeOrigin::root(), [1u8; 32]));
		assert_ok!(Scheduler::cancel(RuntimeOrigin::root(), 4, 1));

		// Scheduled calls are made NONE, so should not effect state
		run_to_time(600_000);
		assert!(logger::log().is_empty());
	});
}

#[test]
fn should_use_origin() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let call = Box::new(RuntimeCall::Logger(LoggerCall::log {
			i: 69,
			weight: Weight::from_parts(10, 0),
		}));
		let call2 = Box::new(RuntimeCall::Logger(LoggerCall::log {
			i: 42,
			weight: Weight::from_parts(10, 0),
		}));

		assert_ok!(Scheduler::schedule_named(
			frame_system::RawOrigin::Signed(1).into(),
			[1u8; 32],
			240_000, // minute 4
			None,
			127,
			call,
		));
		assert_ok!(Scheduler::schedule(
			frame_system::RawOrigin::Signed(1).into(),
			240_000, // minute 4
			None,
			127,
			call2,
		));

		// Minute 3 - scheduled calls are in the agenda
		run_to_time(180_000);
		assert_eq!(Agenda::<Test>::get(4).len(), 2);
		assert!(logger::log().is_empty());

		assert_ok!(Scheduler::cancel_named(
			frame_system::RawOrigin::Signed(1).into(),
			[1u8; 32]
		));
		assert_ok!(Scheduler::cancel(frame_system::RawOrigin::Signed(1).into(), 4, 1));

		// Scheduled calls are made NONE, so should not effect state
		run_to_time(600_000);
		assert!(logger::log().is_empty());
	});
}

#[test]
fn should_check_origin() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let call = Box::new(RuntimeCall::Logger(LoggerCall::log {
			i: 69,
			weight: Weight::from_parts(10, 0),
		}));
		let call2 = Box::new(RuntimeCall::Logger(LoggerCall::log {
			i: 42,
			weight: Weight::from_parts(10, 0),
		}));

		// Account 2 is not authorized to schedule
		assert_noop!(
			Scheduler::schedule_named(
				frame_system::RawOrigin::Signed(2).into(),
				[1u8; 32],
				240_000, // minute 4
				None,
				127,
				call
			),
			BadOrigin
		);
		assert_noop!(
			Scheduler::schedule(
				frame_system::RawOrigin::Signed(2).into(),
				240_000, // minute 4
				None,
				127,
				call2
			),
			BadOrigin
		);
	});
}

#[test]
fn should_check_origin_for_cancel() {
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let call = Box::new(RuntimeCall::Logger(LoggerCall::log_without_filter {
			i: 69,
			weight: Weight::from_parts(10, 0),
		}));
		let call2 = Box::new(RuntimeCall::Logger(LoggerCall::log_without_filter {
			i: 42,
			weight: Weight::from_parts(10, 0),
		}));

		assert_ok!(Scheduler::schedule_named(
			frame_system::RawOrigin::Signed(1).into(),
			[1u8; 32],
			240_000, // minute 4
			None,
			127,
			call,
		));
		assert_ok!(Scheduler::schedule(
			frame_system::RawOrigin::Signed(1).into(),
			240_000, // minute 4
			None,
			127,
			call2,
		));

		// Minute 3 - scheduled calls are in the agenda
		run_to_time(180_000);
		assert_eq!(Agenda::<Test>::get(4).len(), 2);
		assert!(logger::log().is_empty());

		// Account 2 cannot cancel tasks scheduled by account 1
		assert_noop!(
			Scheduler::cancel_named(frame_system::RawOrigin::Signed(2).into(), [1u8; 32]),
			BadOrigin
		);
		assert_noop!(
			Scheduler::cancel(frame_system::RawOrigin::Signed(2).into(), 4, 1),
			BadOrigin
		);
		// Root cannot cancel tasks scheduled by account 1 either
		assert_noop!(
			Scheduler::cancel_named(frame_system::RawOrigin::Root.into(), [1u8; 32]),
			BadOrigin
		);
		assert_noop!(Scheduler::cancel(frame_system::RawOrigin::Root.into(), 4, 1), BadOrigin);

		// Tasks should still execute at minute 5
		run_to_time(300_000);
		assert_eq!(
			logger::log(),
			vec![
				(frame_system::RawOrigin::Signed(1).into(), 69),
				(frame_system::RawOrigin::Signed(1).into(), 42)
			]
		);
	});
}

#[test]
fn time_scheduler_v1_anon_basic_works() {
	use frame_support::traits::time_schedule::v1::Anon;
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });

		// Schedule a call
		let _address = <Scheduler as Anon<_, _, _>>::schedule(
			DispatchTime::At(240_000), // minute 4
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();

		// Did not execute till minute 3
		run_to_time(180_000);
		assert!(logger::log().is_empty());

		// Executes at minute 4
		run_to_time(240_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);

		// ... but not again
		run_to_time(600_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);
	});
}

#[test]
fn time_scheduler_v1_anon_cancel_works() {
	use frame_support::traits::time_schedule::v1::Anon;
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let bound = Preimage::bound(call).unwrap();

		// Schedule a call
		let address = <Scheduler as Anon<_, _, _>>::schedule(
			DispatchTime::At(240_000), // minute 4
			None,
			127,
			root(),
			bound.clone(),
		)
		.unwrap();

		// Cancel the call
		assert_ok!(<Scheduler as Anon<_, _, _>>::cancel(address));

		// It did not get executed
		run_to_time(600_000);
		assert!(logger::log().is_empty());

		// Cannot cancel again
		assert_err!(<Scheduler as Anon<_, _, _>>::cancel(address), DispatchError::Unavailable);
	});
}

#[test]
fn time_scheduler_v1_named_basic_works() {
	use frame_support::traits::time_schedule::v1::Named;
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let name = [1u8; 32];

		// Schedule a call
		let _address = <Scheduler as Named<_, _, _>>::schedule_named(
			name,
			DispatchTime::At(240_000), // minute 4
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();

		// Did not execute till minute 3
		run_to_time(180_000);
		assert!(logger::log().is_empty());

		// Executes at minute 4
		run_to_time(240_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);

		// ... but not again
		run_to_time(600_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);
	});
}

#[test]
fn time_scheduler_v1_named_cancel_works() {
	use frame_support::traits::time_schedule::v1::Named;
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let bound = Preimage::bound(call).unwrap();
		let name = [1u8; 32];

		// Schedule a call
		<Scheduler as Named<_, _, _>>::schedule_named(
			name,
			DispatchTime::At(240_000), // minute 4
			None,
			127,
			root(),
			bound.clone(),
		)
		.unwrap();

		// Cancel the call by name
		assert_ok!(<Scheduler as Named<_, _, _>>::cancel_named(name));

		// It did not get executed
		run_to_time(600_000);
		assert!(logger::log().is_empty());

		// Cannot cancel again
		assert_noop!(
			<Scheduler as Named<_, _, _>>::cancel_named(name),
			DispatchError::Unavailable
		);
	});
}

#[test]
fn time_scheduler_v1_named_reschedule_works() {
	use frame_support::traits::time_schedule::v1::Named;
	new_test_ext().execute_with(|| {
		// Set initial time
		Timestamp::set_timestamp(60_000);

		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let name = [1u8; 32];

		// Schedule a call at minute 4
		<Scheduler as Named<_, _, _>>::schedule_named(
			name,
			DispatchTime::At(240_000), // minute 4
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();

		// Reschedule to minute 6
		assert_ok!(<Scheduler as Named<_, _, _>>::reschedule_named(
			name,
			DispatchTime::At(360_000)
		));

		// Did not execute at minute 4
		run_to_time(240_000);
		assert!(logger::log().is_empty());

		// Executes at minute 6
		run_to_time(360_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);
	});
}
