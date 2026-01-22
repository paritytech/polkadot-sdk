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
use core::num::NonZeroU32;
use frame_support::{assert_err, assert_noop, assert_ok, traits::OnInitialize};
use sp_runtime::{traits::BadOrigin, DispatchError};

/// Helper to create NonZeroU32 for retry periods in tests
fn nz(n: u32) -> NonZeroU32 {
	NonZeroU32::new(n).expect("retry period must be non-zero")
}

/// Helper to count tasks in a bucket (using BoundedVec)
fn agenda_task_count(bucket: u64) -> usize {
	Agenda::<Test>::get(bucket).iter().filter(|x| x.is_some()).count()
}

/// Helper to check if a specific task exists
fn task_exists(bucket: u64, index: u32) -> bool {
	Agenda::<Test>::get(bucket)
		.get(index as usize)
		.and_then(|x| x.as_ref())
		.is_some()
}

/// Helper to check if bucket is empty
fn bucket_is_empty(bucket: u64) -> bool {
	agenda_task_count(bucket) == 0
}

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
		assert!(!bucket_is_empty(2));
		assert!(logger::log().is_empty());

		// Advance timestamp to 120_000ms and run on_initialize
		Timestamp::set_timestamp(120_000);
		Scheduler::on_initialize(2);

		// Check that the log was executed
		assert_eq!(logger::log(), vec![(root(), 42)]);

		// Agenda should be cleaned up after dispatch
		assert!(bucket_is_empty(2));
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

		// Schedule call with After(0) - schedules for current time (same bucket)
		// Since buckets can span multiple blocks, tasks scheduled within the same bucket
		// will execute in a subsequent block within that bucket (or when the bucket is processed)
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::After(0),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap()
		));

		// Task is in bucket 2 (120_000 / 60_000 = 2)
		assert_eq!(agenda_task_count(2), 1);

		// Should execute when we advance time and process bucket 2 again
		// Simulating "next block" at same time (still bucket 2)
		run_to_time(120_000);
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
		assert_eq!(agenda_task_count(2), 2);

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

		// Cannot reschedule to same bucket
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

		// Cannot reschedule to same bucket
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
		assert!(task_exists(4, 0));

		// Retry 10 times, advancing 3 buckets each time
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 10, nz(3), false));
		assert_eq!(Retries::<Test>::iter().count(), 1);

		// Minute 3 - not yet
		run_to_time(180_000);
		assert!(logger::log().is_empty());
		assert!(task_exists(4, 0));

		// Minute 4 - task fails, should be retried at minute 7 (240_000 + 180_000 = 420_000)
		run_to_time(240_000);
		assert!(bucket_is_empty(4));
		assert!(task_exists(7, 0));
		assert!(logger::log().is_empty());

		// Minute 6 - still waiting
		run_to_time(360_000);
		assert!(task_exists(7, 0));
		assert!(logger::log().is_empty());

		// Minute 7 - task still fails, should be retried at minute 10
		run_to_time(420_000);
		assert!(bucket_is_empty(7));
		assert!(task_exists(10, 0));
		assert!(logger::log().is_empty());

		// Minute 8 - still waiting (threshold now allows success)
		run_to_time(480_000);
		assert!(task_exists(10, 0));
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
		assert!(task_exists(4, 0));

		// Retry 10 times, advancing 3 buckets each time
		assert_ok!(Scheduler::set_retry_named(root().into(), [1u8; 32], 10, nz(3), false));
		assert_eq!(Retries::<Test>::iter().count(), 1);

		// Minute 3 - not yet
		run_to_time(180_000);
		assert!(logger::log().is_empty());
		assert!(task_exists(4, 0));

		// Minute 4 - task fails, should be retried at minute 7
		run_to_time(240_000);
		assert!(bucket_is_empty(4));
		assert!(task_exists(7, 0));
		assert!(logger::log().is_empty());

		// Minute 7 - task still fails, should be retried at minute 10
		run_to_time(420_000);
		assert!(bucket_is_empty(7));
		assert!(task_exists(10, 0));
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
		assert!(task_exists(4, 0));

		// Task 42 will be retried 3 times, advancing 1 bucket each time
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 3, nz(1), false));
		assert_eq!(Retries::<Test>::iter().count(), 1);

		// Minute 3 - not yet
		run_to_time(180_000);
		assert!(logger::log().is_empty());
		assert!(task_exists(4, 0));

		// Minute 4 - task fails, scheduled for minute 5
		run_to_time(240_000);
		assert!(bucket_is_empty(4));
		assert!(task_exists(5, 0));
		assert_eq!(Retries::<Test>::get((5, 0)).unwrap().remaining, 2);
		assert!(logger::log().is_empty());

		// Minute 5 - task fails again, scheduled for minute 6
		run_to_time(300_000);
		assert!(bucket_is_empty(5));
		assert!(task_exists(6, 0));
		assert_eq!(Retries::<Test>::get((6, 0)).unwrap().remaining, 1);
		assert!(logger::log().is_empty());

		// Minute 6 - task fails again, scheduled for minute 7
		run_to_time(360_000);
		assert!(bucket_is_empty(6));
		assert!(task_exists(7, 0));
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

		assert!(task_exists(4, 0));
		// Make sure the retry configuration was stored
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 10, nz(2), false));
		assert_eq!(
			Retries::<Test>::get((4, 0)),
			Some(RetryConfig { total_retries: 10, remaining: 10, period: nz(2), try_same_bucket_first: false })
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

		assert!(task_exists(4, 0));
		// Make sure the retry configuration was stored
		assert_ok!(Scheduler::set_retry_named(root().into(), [42u8; 32], 10, nz(2), false));
		let address = Lookup::<Test>::get([42u8; 32]).unwrap();
		assert_eq!(
			Retries::<Test>::get(address),
			Some(RetryConfig { total_retries: 10, remaining: 10, period: nz(2), try_same_bucket_first: false })
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

		assert!(task_exists(4, 0));
		// Try to change the retry config with a different (non-root) account
		let res: Result<(), DispatchError> =
			Scheduler::set_retry(RuntimeOrigin::signed(102), (4, 0), 10, nz(2), false);
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

		assert_eq!(agenda_task_count(4), 2);
		// Task 20 will be retried 10 times, advancing 1 bucket each time
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 10, nz(1), false));
		// Task 42 will be retried 10 times, advancing 1 bucket each time
		assert_ok!(Scheduler::set_retry_named(root().into(), [1u8; 32], 10, nz(1), false));
		assert_eq!(Retries::<Test>::iter().count(), 2);

		// Minute 3 - not yet
		run_to_time(180_000);
		assert!(logger::log().is_empty());
		assert_eq!(agenda_task_count(4), 2);

		// Minute 4 - both tasks fail
		run_to_time(240_000);
		assert!(bucket_is_empty(4));
		// 42 and 20 are rescheduled for minute 5
		assert_eq!(agenda_task_count(5), 2);
		assert!(logger::log().is_empty());

		// Minute 5 - 42 and 20 still fail
		run_to_time(300_000);
		// 42 and 20 rescheduled for minute 6
		assert_eq!(agenda_task_count(6), 2);
		assert_eq!(Retries::<Test>::iter().count(), 2);
		assert!(logger::log().is_empty());

		// Even though 42 is being retried, the tasks scheduled for retries are not named
		assert_eq!(Lookup::<Test>::iter().count(), 0);
		assert!(Scheduler::cancel(root().into(), 6, 0).is_ok());

		// 20 is removed, 42 still fails
		run_to_time(360_000);
		// 42 rescheduled for minute 7
		assert_eq!(agenda_task_count(7), 1);
		// 20's retry entry is removed
		assert!(!Retries::<Test>::contains_key((4, 0)));
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert!(logger::log().is_empty());

		assert!(Scheduler::cancel(root().into(), 7, 0).is_ok());

		// Both tasks are canceled, everything is removed now
		run_to_time(420_000);
		assert!(bucket_is_empty(8));
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

		assert_eq!(agenda_task_count(4), 2);
		// Task 20 will be retried 10 times, advancing 1 bucket each time
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 10, nz(1), false));
		// Task 42 will be retried 10 times, advancing 1 bucket each time
		assert_ok!(Scheduler::set_retry_named(root().into(), [1u8; 32], 10, nz(1), false));
		assert_eq!(Retries::<Test>::iter().count(), 2);

		// Minute 3 - not yet
		run_to_time(180_000);
		assert!(logger::log().is_empty());
		assert_eq!(agenda_task_count(4), 2);

		// Cancel the retry config for 20
		assert_ok!(Scheduler::cancel_retry(root().into(), (4, 0)));
		assert_eq!(Retries::<Test>::iter().count(), 1);
		// Cancel the retry config for 42
		assert_ok!(Scheduler::cancel_retry_named(root().into(), [1u8; 32]));
		assert_eq!(Retries::<Test>::iter().count(), 0);

		// Minute 4 - both tasks failed and there are no more retries, so they are evicted
		run_to_time(240_000);
		assert_eq!(agenda_task_count(4), 0);
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

		// Scheduling at current time (180_000ms) is allowed - tasks can be scheduled
		// within the same bucket and will execute in a subsequent block
		assert_ok!(Scheduler::schedule(RuntimeOrigin::root(), 180_000, None, 127, call3));
		// Task should be in bucket 3
		assert_eq!(agenda_task_count(3), 1);
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

		// Two tasks in bucket
		assert!(agenda_task_count(when) == 2);
		assert_ok!(Scheduler::do_cancel(None, address));
		// With DoubleMap, cancelled tasks are removed from storage immediately
		assert!(agenda_task_count(when) == 1);
		// Cancel last task from agenda
		assert_ok!(Scheduler::do_cancel(None, address2));
		// Bucket should be empty
		assert!(agenda_task_count(when) == 0);
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

		// Two tasks in bucket
		assert!(agenda_task_count(when) == 2);
		assert_ok!(Scheduler::do_cancel_named(None, [2u8; 32]));
		// With DoubleMap, cancelled tasks are removed from storage immediately
		assert!(agenda_task_count(when) == 1);
		// Cancel last task from agenda
		assert_ok!(Scheduler::do_cancel_named(None, [1u8; 32]));
		// Bucket should be empty
		assert!(agenda_task_count(when) == 0);
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

		// Two tasks in bucket
		assert!(agenda_task_count(when) == 2);
		assert_ok!(Scheduler::do_cancel(None, address));
		// With DoubleMap, cancelled tasks are removed from storage immediately
		assert!(agenda_task_count(when) == 1);
		// Reschedule last task from agenda to minute 5
		assert_eq!(
			Scheduler::do_reschedule(address2, DispatchTime::At(300_000)).unwrap(),
			(5, 0)
		);
		// Bucket should be empty after reschedule
		assert!(agenda_task_count(when) == 0);
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
		assert_eq!(agenda_task_count(4), 2);
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
		assert_eq!(agenda_task_count(4), 2);
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
		assert_eq!(agenda_task_count(4), 2);
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

#[test]
fn within_bucket_scheduling_works() {
	new_test_ext().execute_with(|| {
		// This test verifies that tasks can be scheduled within the same bucket
		// and will execute in subsequent blocks within that bucket.
		//
		// Bucket resolution is 60_000ms (1 minute).
		// A bucket can span multiple blocks (e.g., 10 blocks with 6-second block time).

		// Set initial time to the start of bucket 2 (120_000ms)
		Timestamp::set_timestamp(120_000);

		// First block in bucket 2: process any existing tasks (none)
		run_to_time(120_000);
		assert!(logger::log().is_empty());

		// Schedule a task within the same bucket (at current time)
		let call1 =
			RuntimeCall::Logger(LoggerCall::log { i: 1, weight: Weight::from_parts(10, 0) });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(120_000), // Same bucket
			None,
			127,
			root(),
			Preimage::bound(call1).unwrap()
		));

		// Task is in bucket 2
		assert_eq!(agenda_task_count(2), 1);

		// Simulate "second block" still within bucket 2 (time: 126_000ms, 6 seconds later)
		run_to_time(126_000);
		// Task should execute now
		assert_eq!(logger::log(), vec![(root(), 1)]);

		// Clear log for next test
		logger::clear_log();

		// Schedule another task with After(0) - should go to current bucket
		let call2 =
			RuntimeCall::Logger(LoggerCall::log { i: 2, weight: Weight::from_parts(10, 0) });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::After(0),
			None,
			127,
			root(),
			Preimage::bound(call2).unwrap()
		));

		// Task is in bucket 2 (126_000 / 60_000 = 2)
		assert_eq!(agenda_task_count(2), 1);

		// Simulate "third block" still within bucket 2 (time: 132_000ms)
		run_to_time(132_000);
		assert_eq!(logger::log(), vec![(root(), 2)]);
	});
}

#[test]
fn tasks_not_skipped_when_time_jumps() {
	new_test_ext().execute_with(|| {
		// This test verifies that tasks in intermediate buckets are not skipped
		// when time jumps forward across multiple buckets.

		// Set initial time to bucket 2
		Timestamp::set_timestamp(120_000);
		run_to_time(120_000);

		// Schedule a task for bucket 4
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(240_000), // bucket 4
			None,
			127,
			root(),
			Preimage::bound(call).unwrap()
		));

		// Task is in bucket 4
		assert_eq!(agenda_task_count(4), 1);
		assert!(logger::log().is_empty());

		// Time jumps to bucket 6 (skipping bucket 3, 4, 5)
		// The scheduler should still process bucket 4's tasks
		run_to_time(360_000);
		assert_eq!(logger::log(), vec![(root(), 42)]);
	});
}

#[test]
fn simple_bucket_3_execution() {
	// Simplified test to debug why bucket 3 isn't being processed
	new_test_ext().execute_with(|| {
		// Set initial time to bucket 2
		Timestamp::set_timestamp(120_000);

		// Schedule a simple task (not timed_log) at bucket 3
		let call = RuntimeCall::Logger(LoggerCall::log {
			i: 42,
			weight: Weight::from_parts(10, 0),
		});

		let address = Scheduler::do_schedule(
			DispatchTime::At(180_000), // bucket 3
			None,
			127,
			root(),
			Preimage::bound(call).unwrap()
		)
		.unwrap();

		// Verify task is at expected address
		assert_eq!(address, (3, 0), "Task should be at bucket 3, index 0");

		// Check agenda before run_to_time
		eprintln!("Before run_to_time: Agenda bucket 3 count = {}", agenda_task_count(3));
		eprintln!("Before run_to_time: timestamp = {}", pallet_timestamp::Pallet::<Test>::get());

		// Execute at bucket 3
		run_to_time(180_000);

		// Check timestamp after run_to_time
		eprintln!("After run_to_time: timestamp = {}", pallet_timestamp::Pallet::<Test>::get());

		// Check IncompleteSince value
		let incomplete = IncompleteSince::<Test>::get();
		// Debug: print all events
		let events = frame_system::Pallet::<Test>::events();
		eprintln!("All events count: {}", events.len());
		for e in &events {
			eprintln!("  Event: {:?}", e.event);
		}
		eprintln!("IncompleteSince: {:?}", incomplete);

		// Check agendas
		eprintln!("Agenda bucket 2 count: {}", agenda_task_count(2));
		eprintln!("Agenda bucket 3 count: {}", agenda_task_count(3));

		// Task should have executed
		eprintln!("Logger log: {:?}", logger::log());
		assert_eq!(logger::log(), vec![(root(), 42)], "Task should have executed");
	});
}

#[test]
fn retry_falls_back_to_next_bucket_when_current_full() {
	new_test_ext().execute_with(|| {
		// Set initial time to bucket 2
		Timestamp::set_timestamp(120_000);

		// Schedule a task that will fail at bucket 3
		let call = RuntimeCall::Logger(LoggerCall::timed_log {
			i: 42,
			weight: Weight::from_parts(10, 0),
		});

		Scheduler::do_schedule(
			DispatchTime::At(180_000), // bucket 3
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();

		// Set retry with try_same_bucket_first, period 1 for fallback
		assert_ok!(Scheduler::set_retry(RuntimeOrigin::root(), (3, 0), 2, nz(1), true));

		// Fill up bucket 3 to max capacity (MaxScheduledPerBucket = 100 in mock)
		for i in 1..100 {
			let call = RuntimeCall::Logger(LoggerCall::log {
				i: 100 + i,
				weight: Weight::from_parts(10, 0),
			});
			Scheduler::do_schedule(
				DispatchTime::At(180_000),
				None,
				127,
				root(),
				Preimage::bound(call).unwrap(),
			)
			.unwrap();
		}
		assert_eq!(agenda_task_count(3), 100, "Bucket 3 should be full");

		// Set up the time threshold so the call fails
		logger::set_time_threshold(999_000, 999_999);

		// First attempt at bucket 3 - should fail, retry should go to bucket 4 (since 3 is full)
		run_to_time(180_000);

		// The retry should be in bucket 4 since bucket 3 was full
		assert_eq!(agenda_task_count(4), 1, "Retry should be in bucket 4");

		// Clean up
		logger::clear_time_threshold();
	});
}

#[test]
fn retry_same_bucket_first_with_space_available() {
	new_test_ext().execute_with(|| {
		// Set initial time to bucket 2
		Timestamp::set_timestamp(120_000);

		// Schedule a task that will fail at bucket 3
		let call = RuntimeCall::Logger(LoggerCall::timed_log {
			i: 42,
			weight: Weight::from_parts(10, 0),
		});

		Scheduler::do_schedule(
			DispatchTime::At(180_000), // bucket 3
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();

		// Set retry with try_same_bucket_first=true, period=2
		// Should retry in same bucket since there's space
		assert_ok!(Scheduler::set_retry(RuntimeOrigin::root(), (3, 0), 2, nz(2), true));

		// Set up time threshold so the call fails
		logger::set_time_threshold(999_000, 999_999);

		// Run bucket 3 - task fails, retry should go to same bucket (3) since there's space
		run_to_time(180_000);

		// Retry should be in bucket 3 (same bucket, since there's space)
		assert_eq!(agenda_task_count(3), 1, "Retry should be in bucket 3 (same bucket)");
		assert_eq!(agenda_task_count(5), 0, "Bucket 5 should be empty");

		// Clean up
		logger::clear_time_threshold();
	});
}

#[test]
fn retry_without_same_bucket_first_advances_by_period() {
	new_test_ext().execute_with(|| {
		// Set initial time to bucket 2
		Timestamp::set_timestamp(120_000);

		// Schedule a task that will fail at bucket 3
		let call = RuntimeCall::Logger(LoggerCall::timed_log {
			i: 42,
			weight: Weight::from_parts(10, 0),
		});

		Scheduler::do_schedule(
			DispatchTime::At(180_000), // bucket 3
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();

		// Set retry with try_same_bucket_first=false, period=2
		// Should retry in bucket 3+2=5, not same bucket
		assert_ok!(Scheduler::set_retry(RuntimeOrigin::root(), (3, 0), 2, nz(2), false));

		// Set up time threshold so the call fails
		logger::set_time_threshold(999_000, 999_999);

		// Run bucket 3 - task fails
		run_to_time(180_000);

		// Retry should be in bucket 5 (3 + period 2), not same bucket
		assert_eq!(agenda_task_count(3), 0, "Bucket 3 should be empty");
		assert_eq!(agenda_task_count(5), 1, "Retry should be in bucket 5");

		// Clean up
		logger::clear_time_threshold();
	});
}

