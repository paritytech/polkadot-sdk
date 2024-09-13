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

//! # Scheduler tests.

use super::*;
use crate::mock::{
	logger::{self, Threshold},
	new_test_ext, root, run_to_block, LoggerCall, RuntimeCall, Scheduler, Test, *,
};
use frame_support::{
	assert_err, assert_noop, assert_ok,
	traits::{Contains, GetStorageVersion, OnInitialize, QueryPreimage, StorePreimage},
	Hashable,
};
use sp_runtime::traits::Hash;
use substrate_test_utils::assert_eq_uvec;

#[test]
#[docify::export]
fn basic_scheduling_works() {
	new_test_ext().execute_with(|| {
		// Call to schedule
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });

		// BaseCallFilter should be implemented to accept `Logger::log` runtime call which is
		// implemented for `BaseFilter` in the mock runtime
		assert!(!<Test as frame_system::Config>::BaseCallFilter::contains(&call));

		// Schedule call to be executed at the 4th block
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap()
		));

		// `log` runtime call should not have executed yet
		run_to_block(3);
		assert!(logger::log().is_empty());

		run_to_block(4);
		// `log` runtime call should have executed at block 4
		assert_eq!(logger::log(), vec![(root(), 42u32)]);

		run_to_block(100);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
	});
}

#[test]
#[docify::export]
fn scheduling_with_preimages_works() {
	new_test_ext().execute_with(|| {
		// Call to schedule
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });

		let hash = <Test as frame_system::Config>::Hashing::hash_of(&call);
		let len = call.using_encoded(|x| x.len()) as u32;

		// Important to use here `Bounded::Lookup` to ensure that that the Scheduler can request the
		// hash from PreImage to dispatch the call
		let hashed = Bounded::Lookup { hash, len };

		// Schedule call to be executed at block 4 with the PreImage hash
		assert_ok!(Scheduler::do_schedule(DispatchTime::At(4), None, 127, root(), hashed));

		// Register preimage on chain
		assert_ok!(Preimage::note_preimage(RuntimeOrigin::signed(0), call.encode()));
		assert!(Preimage::is_requested(&hash));

		// `log` runtime call should not have executed yet
		run_to_block(3);
		assert!(logger::log().is_empty());

		run_to_block(4);
		// preimage should not have been removed when executed by the scheduler
		assert!(!Preimage::len(&hash).is_some());
		assert!(!Preimage::is_requested(&hash));
		// `log` runtime call should have executed at block 4
		assert_eq!(logger::log(), vec![(root(), 42u32)]);

		run_to_block(100);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
	});
}

#[test]
fn schedule_after_works() {
	new_test_ext().execute_with(|| {
		run_to_block(2);
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		assert!(!<Test as frame_system::Config>::BaseCallFilter::contains(&call));
		// This will schedule the call 3 blocks after the next block... so block 3 + 3 = 6
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::After(3),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap()
		));
		run_to_block(5);
		assert!(logger::log().is_empty());
		run_to_block(6);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		run_to_block(100);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
	});
}

#[test]
fn schedule_after_zero_works() {
	new_test_ext().execute_with(|| {
		run_to_block(2);
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		assert!(!<Test as frame_system::Config>::BaseCallFilter::contains(&call));
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::After(0),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap()
		));
		// Will trigger on the next block.
		run_to_block(3);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		run_to_block(100);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
	});
}

#[test]
fn periodic_scheduling_works() {
	new_test_ext().execute_with(|| {
		// at #4, every 3 blocks, 3 times.
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			Some((3, 3)),
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));
		run_to_block(3);
		assert!(logger::log().is_empty());
		run_to_block(4);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		run_to_block(6);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		run_to_block(7);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32)]);
		run_to_block(9);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32)]);
		run_to_block(10);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32), (root(), 42u32)]);
		run_to_block(100);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32), (root(), 42u32)]);
	});
}

#[test]
fn retry_scheduling_works() {
	new_test_ext().execute_with(|| {
		// task fails until block 8 is reached
		Threshold::<Test>::put((8, 100));
		// task 42 at #4
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));
		assert!(Agenda::<Test>::get(4)[0].is_some());
		// retry 10 times every 3 blocks
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 10, 3));
		assert_eq!(Retries::<Test>::iter().count(), 1);
		run_to_block(3);
		assert!(logger::log().is_empty());
		assert!(Agenda::<Test>::get(4)[0].is_some());
		// task should be retried in block 7
		run_to_block(4);
		assert!(Agenda::<Test>::get(4).is_empty());
		assert!(Agenda::<Test>::get(7)[0].is_some());
		assert!(logger::log().is_empty());
		run_to_block(6);
		assert!(Agenda::<Test>::get(7)[0].is_some());
		assert!(logger::log().is_empty());
		// task still fails, should be retried in block 10
		run_to_block(7);
		assert!(Agenda::<Test>::get(7).is_empty());
		assert!(Agenda::<Test>::get(10)[0].is_some());
		assert!(logger::log().is_empty());
		run_to_block(8);
		assert!(Agenda::<Test>::get(10)[0].is_some());
		assert!(logger::log().is_empty());
		run_to_block(9);
		assert!(logger::log().is_empty());
		assert_eq!(Retries::<Test>::iter().count(), 1);
		// finally it should succeed
		run_to_block(10);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		assert_eq!(Retries::<Test>::iter().count(), 0);
		run_to_block(11);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		run_to_block(12);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		run_to_block(100);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
	});
}

#[test]
fn named_retry_scheduling_works() {
	new_test_ext().execute_with(|| {
		// task fails until block 8 is reached
		Threshold::<Test>::put((8, 100));
		// task 42 at #4
		let call = RuntimeCall::Logger(logger::Call::timed_log {
			i: 42,
			weight: Weight::from_parts(10, 0),
		});
		assert_eq!(
			Scheduler::do_schedule_named(
				[1u8; 32],
				DispatchTime::At(4),
				None,
				127,
				root(),
				Preimage::bound(call).unwrap(),
			)
			.unwrap(),
			(4, 0)
		);
		assert!(Agenda::<Test>::get(4)[0].is_some());
		// retry 10 times every 3 blocks
		assert_ok!(Scheduler::set_retry_named(root().into(), [1u8; 32], 10, 3));
		assert_eq!(Retries::<Test>::iter().count(), 1);
		run_to_block(3);
		assert!(logger::log().is_empty());
		assert!(Agenda::<Test>::get(4)[0].is_some());
		// task should be retried in block 7
		run_to_block(4);
		assert!(Agenda::<Test>::get(4).is_empty());
		assert!(Agenda::<Test>::get(7)[0].is_some());
		assert!(logger::log().is_empty());
		run_to_block(6);
		assert!(Agenda::<Test>::get(7)[0].is_some());
		assert!(logger::log().is_empty());
		// task still fails, should be retried in block 10
		run_to_block(7);
		assert!(Agenda::<Test>::get(7).is_empty());
		assert!(Agenda::<Test>::get(10)[0].is_some());
		assert!(logger::log().is_empty());
		run_to_block(8);
		assert!(Agenda::<Test>::get(10)[0].is_some());
		assert!(logger::log().is_empty());
		run_to_block(9);
		assert!(logger::log().is_empty());
		assert_eq!(Retries::<Test>::iter().count(), 1);
		// finally it should succeed
		run_to_block(10);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		assert_eq!(Retries::<Test>::iter().count(), 0);
		run_to_block(11);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		run_to_block(12);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		run_to_block(100);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
	});
}

#[test]
fn retry_scheduling_multiple_tasks_works() {
	new_test_ext().execute_with(|| {
		// task fails until block 8 is reached
		Threshold::<Test>::put((8, 100));
		// task 20 at #4
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 20,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));
		// task 42 at #4
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		assert_eq!(Agenda::<Test>::get(4).len(), 2);
		// task 20 will be retried 3 times every block
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 3, 1));
		// task 42 will be retried 10 times every 3 blocks
		assert_ok!(Scheduler::set_retry(root().into(), (4, 1), 10, 3));
		assert_eq!(Retries::<Test>::iter().count(), 2);
		run_to_block(3);
		assert!(logger::log().is_empty());
		assert_eq!(Agenda::<Test>::get(4).len(), 2);
		// both tasks fail
		run_to_block(4);
		assert!(Agenda::<Test>::get(4).is_empty());
		// 20 is rescheduled for next block
		assert_eq!(Agenda::<Test>::get(5).len(), 1);
		// 42 is rescheduled for block 7
		assert_eq!(Agenda::<Test>::get(7).len(), 1);
		assert!(logger::log().is_empty());
		// 20 still fails
		run_to_block(5);
		// 20 rescheduled for next block
		assert_eq!(Agenda::<Test>::get(6).len(), 1);
		assert_eq!(Agenda::<Test>::get(7).len(), 1);
		assert_eq!(Retries::<Test>::iter().count(), 2);
		assert!(logger::log().is_empty());
		// 20 still fails
		run_to_block(6);
		// rescheduled for next block together with 42
		assert_eq!(Agenda::<Test>::get(7).len(), 2);
		assert_eq!(Retries::<Test>::iter().count(), 2);
		assert!(logger::log().is_empty());
		// both tasks will fail, for 20 it was the last retry so it's dropped
		run_to_block(7);
		assert!(Agenda::<Test>::get(7).is_empty());
		assert!(Agenda::<Test>::get(8).is_empty());
		// 42 is rescheduled for block 10
		assert_eq!(Agenda::<Test>::get(10).len(), 1);
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert!(logger::log().is_empty());
		run_to_block(8);
		assert_eq!(Agenda::<Test>::get(10).len(), 1);
		assert!(logger::log().is_empty());
		run_to_block(9);
		assert!(logger::log().is_empty());
		assert_eq!(Retries::<Test>::iter().count(), 1);
		// 42 runs successfully
		run_to_block(10);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		assert_eq!(Retries::<Test>::iter().count(), 0);
		run_to_block(11);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		run_to_block(12);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		run_to_block(100);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
	});
}

#[test]
fn retry_scheduling_multiple_named_tasks_works() {
	new_test_ext().execute_with(|| {
		// task fails until we reach block 8
		Threshold::<Test>::put((8, 100));
		// task 20 at #4
		assert_ok!(Scheduler::do_schedule_named(
			[20u8; 32],
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 20,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));
		// task 42 at #4
		assert_ok!(Scheduler::do_schedule_named(
			[42u8; 32],
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		assert_eq!(Agenda::<Test>::get(4).len(), 2);
		// task 20 will be retried 3 times every block
		assert_ok!(Scheduler::set_retry_named(root().into(), [20u8; 32], 3, 1));
		// task 42 will be retried 10 times every 3 block
		assert_ok!(Scheduler::set_retry_named(root().into(), [42u8; 32], 10, 3));
		assert_eq!(Retries::<Test>::iter().count(), 2);
		run_to_block(3);
		assert!(logger::log().is_empty());
		assert_eq!(Agenda::<Test>::get(4).len(), 2);
		// both tasks fail
		run_to_block(4);
		assert!(Agenda::<Test>::get(4).is_empty());
		// 42 is rescheduled for block 7
		assert_eq!(Agenda::<Test>::get(7).len(), 1);
		// 20 is rescheduled for next block
		assert_eq!(Agenda::<Test>::get(5).len(), 1);
		assert!(logger::log().is_empty());
		// 20 still fails
		run_to_block(5);
		// 20 rescheduled for next block
		assert_eq!(Agenda::<Test>::get(6).len(), 1);
		assert_eq!(Agenda::<Test>::get(7).len(), 1);
		assert_eq!(Retries::<Test>::iter().count(), 2);
		assert!(logger::log().is_empty());
		// 20 still fails
		run_to_block(6);
		// 20 rescheduled for next block together with 42
		assert_eq!(Agenda::<Test>::get(7).len(), 2);
		assert_eq!(Retries::<Test>::iter().count(), 2);
		assert!(logger::log().is_empty());
		// both tasks will fail, for 20 it was the last retry so it's dropped
		run_to_block(7);
		assert!(Agenda::<Test>::get(7).is_empty());
		assert!(Agenda::<Test>::get(8).is_empty());
		// 42 is rescheduled for block 10
		assert_eq!(Agenda::<Test>::get(10).len(), 1);
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert!(logger::log().is_empty());
		run_to_block(8);
		assert_eq!(Agenda::<Test>::get(10).len(), 1);
		assert!(logger::log().is_empty());
		run_to_block(9);
		assert!(logger::log().is_empty());
		assert_eq!(Retries::<Test>::iter().count(), 1);
		// 42 runs successfully
		run_to_block(10);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		assert_eq!(Retries::<Test>::iter().count(), 0);
		run_to_block(11);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		run_to_block(12);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		run_to_block(100);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
	});
}

#[test]
fn retry_scheduling_with_period_works() {
	new_test_ext().execute_with(|| {
		// tasks fail until we reach block 4 and after we're past block 8
		Threshold::<Test>::put((4, 8));
		// task 42 at #4, every 3 blocks, 6 times
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			Some((3, 6)),
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		assert!(Agenda::<Test>::get(4)[0].is_some());
		// 42 will be retried 10 times every 2 blocks
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 10, 2));
		assert_eq!(Retries::<Test>::iter().count(), 1);
		run_to_block(3);
		assert!(logger::log().is_empty());
		assert!(Agenda::<Test>::get(4)[0].is_some());
		// 42 runs successfully once, it will run again at block 7
		run_to_block(4);
		assert!(Agenda::<Test>::get(4).is_empty());
		assert!(Agenda::<Test>::get(7)[0].is_some());
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		// nothing changed
		run_to_block(6);
		assert!(Agenda::<Test>::get(7)[0].is_some());
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		// 42 runs successfully again, it will run again at block 10
		run_to_block(7);
		assert!(Agenda::<Test>::get(7).is_empty());
		assert!(Agenda::<Test>::get(10)[0].is_some());
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32)]);
		run_to_block(9);
		assert!(Agenda::<Test>::get(10)[0].is_some());
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32)]);
		// 42 has 10 retries left out of a total of 10
		assert_eq!(Retries::<Test>::get((10, 0)).unwrap().remaining, 10);
		// 42 will fail because we're outside the set threshold (block number in `4..8`), so it
		// should be retried in 2 blocks (at block 12)
		run_to_block(10);
		// should be queued for the normal period of 3 blocks
		assert!(Agenda::<Test>::get(13)[0].is_some());
		// should also be queued to be retried in 2 blocks
		assert!(Agenda::<Test>::get(12)[0].is_some());
		// 42 has consumed one retry attempt
		assert_eq!(Retries::<Test>::get((12, 0)).unwrap().remaining, 9);
		assert_eq!(Retries::<Test>::get((13, 0)).unwrap().remaining, 10);
		assert_eq!(Retries::<Test>::iter().count(), 2);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32)]);
		// 42 will fail again
		run_to_block(12);
		// should still be queued for the normal period
		assert!(Agenda::<Test>::get(13)[0].is_some());
		// should be queued to be retried in 2 blocks
		assert!(Agenda::<Test>::get(14)[0].is_some());
		// 42 has consumed another retry attempt
		assert_eq!(Retries::<Test>::get((14, 0)).unwrap().remaining, 8);
		assert_eq!(Retries::<Test>::get((13, 0)).unwrap().remaining, 10);
		assert_eq!(Retries::<Test>::iter().count(), 2);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32)]);
		// 42 will fail for the regular periodic run
		run_to_block(13);
		// should still be queued for the normal period
		assert!(Agenda::<Test>::get(16)[0].is_some());
		// should still be queued to be retried next block
		assert!(Agenda::<Test>::get(14)[0].is_some());
		// 42 consumed another periodic run, which failed, so another retry is queued for block 15
		assert!(Agenda::<Test>::get(16)[0].as_ref().unwrap().maybe_periodic.is_some());
		assert!(Agenda::<Test>::get(15)[0].as_ref().unwrap().maybe_periodic.is_none());
		assert!(Agenda::<Test>::get(14)[0].as_ref().unwrap().maybe_periodic.is_none());
		assert_eq!(Retries::<Test>::iter().count(), 3);
		assert!(Retries::<Test>::get((14, 0)).unwrap().remaining == 8);
		assert!(Retries::<Test>::get((15, 0)).unwrap().remaining == 9);
		assert!(Retries::<Test>::get((16, 0)).unwrap().remaining == 10);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32)]);
		// change the threshold to allow the task to succeed
		Threshold::<Test>::put((14, 100));
		// first retry should now succeed
		run_to_block(14);
		assert!(Agenda::<Test>::get(15)[0].as_ref().unwrap().maybe_periodic.is_none());
		assert_eq!(Agenda::<Test>::get(16).iter().filter(|entry| entry.is_some()).count(), 1);
		assert!(Agenda::<Test>::get(16)[0].is_some());
		assert_eq!(Retries::<Test>::get((15, 0)).unwrap().remaining, 9);
		assert_eq!(Retries::<Test>::get((16, 0)).unwrap().remaining, 10);
		assert_eq!(Retries::<Test>::iter().count(), 2);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32), (root(), 42u32)]);
		// second retry should also succeed
		run_to_block(15);
		assert_eq!(Agenda::<Test>::get(16).iter().filter(|entry| entry.is_some()).count(), 1);
		assert!(Agenda::<Test>::get(16)[0].is_some());
		assert!(Agenda::<Test>::get(17).is_empty());
		assert_eq!(Retries::<Test>::get((16, 0)).unwrap().remaining, 10);
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert_eq!(
			logger::log(),
			vec![(root(), 42u32), (root(), 42u32), (root(), 42u32), (root(), 42u32)]
		);
		// normal periodic run on block 16 will succeed
		run_to_block(16);
		// next periodic run at block 19
		assert!(Agenda::<Test>::get(19)[0].is_some());
		assert!(Agenda::<Test>::get(18).is_empty());
		assert!(Agenda::<Test>::get(17).is_empty());
		assert_eq!(Retries::<Test>::get((19, 0)).unwrap().remaining, 10);
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert_eq!(
			logger::log(),
			vec![
				(root(), 42u32),
				(root(), 42u32),
				(root(), 42u32),
				(root(), 42u32),
				(root(), 42u32)
			]
		);
		// final periodic run on block 19 will succeed
		run_to_block(19);
		// next periodic run at block 19
		assert_eq!(Agenda::<Test>::iter().count(), 0);
		assert_eq!(Retries::<Test>::iter().count(), 0);
		assert_eq!(
			logger::log(),
			vec![
				(root(), 42u32),
				(root(), 42u32),
				(root(), 42u32),
				(root(), 42u32),
				(root(), 42u32),
				(root(), 42u32)
			]
		);
	});
}

#[test]
fn named_retry_scheduling_with_period_works() {
	new_test_ext().execute_with(|| {
		// tasks fail until we reach block 4 and after we're past block 8
		Threshold::<Test>::put((4, 8));
		// task 42 at #4, every 3 blocks, 6 times
		assert_ok!(Scheduler::do_schedule_named(
			[42u8; 32],
			DispatchTime::At(4),
			Some((3, 6)),
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		assert!(Agenda::<Test>::get(4)[0].is_some());
		// 42 will be retried 10 times every 2 blocks
		assert_ok!(Scheduler::set_retry_named(root().into(), [42u8; 32], 10, 2));
		assert_eq!(Retries::<Test>::iter().count(), 1);
		run_to_block(3);
		assert!(logger::log().is_empty());
		assert!(Agenda::<Test>::get(4)[0].is_some());
		// 42 runs successfully once, it will run again at block 7
		run_to_block(4);
		assert!(Agenda::<Test>::get(4).is_empty());
		assert!(Agenda::<Test>::get(7)[0].is_some());
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		// nothing changed
		run_to_block(6);
		assert!(Agenda::<Test>::get(7)[0].is_some());
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		// 42 runs successfully again, it will run again at block 10
		run_to_block(7);
		assert!(Agenda::<Test>::get(7).is_empty());
		assert!(Agenda::<Test>::get(10)[0].is_some());
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32)]);
		run_to_block(9);
		assert!(Agenda::<Test>::get(10)[0].is_some());
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32)]);
		// 42 has 10 retries left out of a total of 10
		assert_eq!(Retries::<Test>::get((10, 0)).unwrap().remaining, 10);
		// 42 will fail because we're outside the set threshold (block number in `4..8`), so it
		// should be retried in 2 blocks (at block 12)
		run_to_block(10);
		// should be queued for the normal period of 3 blocks
		assert!(Agenda::<Test>::get(13)[0].is_some());
		// should also be queued to be retried in 2 blocks
		assert!(Agenda::<Test>::get(12)[0].is_some());
		// 42 has consumed one retry attempt
		assert_eq!(Retries::<Test>::get((12, 0)).unwrap().remaining, 9);
		assert_eq!(Retries::<Test>::get((13, 0)).unwrap().remaining, 10);
		assert_eq!(Retries::<Test>::iter().count(), 2);
		assert_eq!(Lookup::<Test>::get([42u8; 32]).unwrap(), (13, 0));
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32)]);
		// 42 will fail again
		run_to_block(12);
		// should still be queued for the normal period
		assert!(Agenda::<Test>::get(13)[0].is_some());
		// should be queued to be retried in 2 blocks
		assert!(Agenda::<Test>::get(14)[0].is_some());
		// 42 has consumed another retry attempt
		assert_eq!(Retries::<Test>::get((14, 0)).unwrap().remaining, 8);
		assert_eq!(Retries::<Test>::get((13, 0)).unwrap().remaining, 10);
		assert_eq!(Retries::<Test>::iter().count(), 2);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32)]);
		// 42 will fail for the regular periodic run
		run_to_block(13);
		// should still be queued for the normal period
		assert!(Agenda::<Test>::get(16)[0].is_some());
		// should still be queued to be retried next block
		assert!(Agenda::<Test>::get(14)[0].is_some());
		// 42 consumed another periodic run, which failed, so another retry is queued for block 15
		assert!(Agenda::<Test>::get(16)[0].as_ref().unwrap().maybe_periodic.is_some());
		assert!(Agenda::<Test>::get(15)[0].as_ref().unwrap().maybe_periodic.is_none());
		assert!(Agenda::<Test>::get(14)[0].as_ref().unwrap().maybe_periodic.is_none());
		assert_eq!(Retries::<Test>::iter().count(), 3);
		assert!(Retries::<Test>::get((14, 0)).unwrap().remaining == 8);
		assert!(Retries::<Test>::get((15, 0)).unwrap().remaining == 9);
		assert!(Retries::<Test>::get((16, 0)).unwrap().remaining == 10);
		assert_eq!(Lookup::<Test>::get([42u8; 32]).unwrap(), (16, 0));
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32)]);
		// change the threshold to allow the task to succeed
		Threshold::<Test>::put((14, 100));
		// first retry should now succeed
		run_to_block(14);
		assert!(Agenda::<Test>::get(15)[0].as_ref().unwrap().maybe_periodic.is_none());
		assert_eq!(Agenda::<Test>::get(16).iter().filter(|entry| entry.is_some()).count(), 1);
		assert!(Agenda::<Test>::get(16)[0].is_some());
		assert_eq!(Retries::<Test>::get((15, 0)).unwrap().remaining, 9);
		assert_eq!(Retries::<Test>::get((16, 0)).unwrap().remaining, 10);
		assert_eq!(Retries::<Test>::iter().count(), 2);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32), (root(), 42u32)]);
		// second retry should also succeed
		run_to_block(15);
		assert_eq!(Agenda::<Test>::get(16).iter().filter(|entry| entry.is_some()).count(), 1);
		assert!(Agenda::<Test>::get(16)[0].is_some());
		assert!(Agenda::<Test>::get(17).is_empty());
		assert_eq!(Retries::<Test>::get((16, 0)).unwrap().remaining, 10);
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert_eq!(Lookup::<Test>::get([42u8; 32]).unwrap(), (16, 0));
		assert_eq!(
			logger::log(),
			vec![(root(), 42u32), (root(), 42u32), (root(), 42u32), (root(), 42u32)]
		);
		// normal periodic run on block 16 will succeed
		run_to_block(16);
		// next periodic run at block 19
		assert!(Agenda::<Test>::get(19)[0].is_some());
		assert!(Agenda::<Test>::get(18).is_empty());
		assert!(Agenda::<Test>::get(17).is_empty());
		assert_eq!(Retries::<Test>::get((19, 0)).unwrap().remaining, 10);
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert_eq!(Lookup::<Test>::get([42u8; 32]).unwrap(), (19, 0));
		assert_eq!(
			logger::log(),
			vec![
				(root(), 42u32),
				(root(), 42u32),
				(root(), 42u32),
				(root(), 42u32),
				(root(), 42u32)
			]
		);
		// final periodic run on block 19 will succeed
		run_to_block(19);
		// next periodic run at block 19
		assert_eq!(Agenda::<Test>::iter().count(), 0);
		assert_eq!(Retries::<Test>::iter().count(), 0);
		assert_eq!(Lookup::<Test>::iter().count(), 0);
		assert_eq!(
			logger::log(),
			vec![
				(root(), 42u32),
				(root(), 42u32),
				(root(), 42u32),
				(root(), 42u32),
				(root(), 42u32),
				(root(), 42u32)
			]
		);
	});
}

#[test]
fn retry_scheduling_expires() {
	new_test_ext().execute_with(|| {
		// task will fail if we're past block 3
		Threshold::<Test>::put((1, 3));
		// task 42 at #4
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));
		assert!(Agenda::<Test>::get(4)[0].is_some());
		// task 42 will be retried 3 times every block
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 3, 1));
		assert_eq!(Retries::<Test>::iter().count(), 1);
		run_to_block(3);
		assert!(logger::log().is_empty());
		// task 42 is scheduled for next block
		assert!(Agenda::<Test>::get(4)[0].is_some());
		// task fails because we're past block 3
		run_to_block(4);
		// task is scheduled for next block
		assert!(Agenda::<Test>::get(4).is_empty());
		assert!(Agenda::<Test>::get(5)[0].is_some());
		// one retry attempt is consumed
		assert_eq!(Retries::<Test>::get((5, 0)).unwrap().remaining, 2);
		assert!(logger::log().is_empty());
		// task fails again
		run_to_block(5);
		// task is scheduled for next block
		assert!(Agenda::<Test>::get(5).is_empty());
		assert!(Agenda::<Test>::get(6)[0].is_some());
		// another retry attempt is consumed
		assert_eq!(Retries::<Test>::get((6, 0)).unwrap().remaining, 1);
		assert!(logger::log().is_empty());
		// task fails again
		run_to_block(6);
		// task is scheduled for next block
		assert!(Agenda::<Test>::get(6).is_empty());
		assert!(Agenda::<Test>::get(7)[0].is_some());
		// another retry attempt is consumed
		assert_eq!(Retries::<Test>::get((7, 0)).unwrap().remaining, 0);
		assert!(logger::log().is_empty());
		// task fails again
		run_to_block(7);
		// task ran out of retries so it gets dropped
		assert_eq!(Agenda::<Test>::iter().count(), 0);
		assert_eq!(Retries::<Test>::iter().count(), 0);
		assert!(logger::log().is_empty());
	});
}

#[test]
fn set_retry_bad_origin() {
	new_test_ext().execute_with(|| {
		// task 42 at #4 with account 101 as origin
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			127,
			101.into(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		assert!(Agenda::<Test>::get(4)[0].is_some());
		// try to change the retry config with a different (non-root) account
		let res: Result<(), DispatchError> =
			Scheduler::set_retry(RuntimeOrigin::signed(102), (4, 0), 10, 2);
		assert_eq!(res, Err(BadOrigin.into()));
	});
}

#[test]
fn set_named_retry_bad_origin() {
	new_test_ext().execute_with(|| {
		// task 42 at #4 with account 101 as origin
		assert_ok!(Scheduler::do_schedule_named(
			[42u8; 32],
			DispatchTime::At(4),
			None,
			127,
			101.into(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		assert!(Agenda::<Test>::get(4)[0].is_some());
		// try to change the retry config with a different (non-root) account
		let res: Result<(), DispatchError> =
			Scheduler::set_retry_named(RuntimeOrigin::signed(102), [42u8; 32], 10, 2);
		assert_eq!(res, Err(BadOrigin.into()));
	});
}

#[test]
fn set_retry_works() {
	new_test_ext().execute_with(|| {
		// task 42 at #4
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		assert!(Agenda::<Test>::get(4)[0].is_some());
		// make sure the retry configuration was stored
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 10, 2));
		assert_eq!(
			Retries::<Test>::get((4, 0)),
			Some(RetryConfig { total_retries: 10, remaining: 10, period: 2 })
		);
	});
}

#[test]
fn set_named_retry_works() {
	new_test_ext().execute_with(|| {
		// task 42 at #4 with account 101 as origin
		assert_ok!(Scheduler::do_schedule_named(
			[42u8; 32],
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		assert!(Agenda::<Test>::get(4)[0].is_some());
		// make sure the retry configuration was stored
		assert_ok!(Scheduler::set_retry_named(root().into(), [42u8; 32], 10, 2));
		let address = Lookup::<Test>::get([42u8; 32]).unwrap();
		assert_eq!(
			Retries::<Test>::get(address),
			Some(RetryConfig { total_retries: 10, remaining: 10, period: 2 })
		);
	});
}

#[test]
fn retry_periodic_full_cycle() {
	new_test_ext().execute_with(|| {
		// tasks fail after we pass block 1000
		Threshold::<Test>::put((1, 1000));
		// task 42 at #4, every 100 blocks, 4 times
		assert_ok!(Scheduler::do_schedule_named(
			[42u8; 32],
			DispatchTime::At(10),
			Some((100, 4)),
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		assert!(Agenda::<Test>::get(10)[0].is_some());
		// 42 will be retried 2 times every block
		assert_ok!(Scheduler::set_retry_named(root().into(), [42u8; 32], 2, 1));
		assert_eq!(Retries::<Test>::iter().count(), 1);
		run_to_block(9);
		assert!(logger::log().is_empty());
		assert!(Agenda::<Test>::get(10)[0].is_some());
		// 42 runs successfully once, it will run again at block 110
		run_to_block(10);
		assert!(Agenda::<Test>::get(10).is_empty());
		assert!(Agenda::<Test>::get(110)[0].is_some());
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		// nothing changed
		run_to_block(109);
		assert!(Agenda::<Test>::get(110)[0].is_some());
		// original task still has 2 remaining retries
		assert_eq!(Retries::<Test>::get((110, 0)).unwrap().remaining, 2);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		// make 42 fail next block
		Threshold::<Test>::put((1, 2));
		// 42 will fail because we're outside the set threshold (block number in `1..2`), so it
		// should be retried next block (at block 111)
		run_to_block(110);
		// should be queued for the normal period of 100 blocks
		assert!(Agenda::<Test>::get(210)[0].is_some());
		// should also be queued to be retried next block
		assert!(Agenda::<Test>::get(111)[0].is_some());
		// 42 retry clone has consumed one retry attempt
		assert_eq!(Retries::<Test>::get((111, 0)).unwrap().remaining, 1);
		// 42 original task still has the original remaining attempts
		assert_eq!(Retries::<Test>::get((210, 0)).unwrap().remaining, 2);
		assert_eq!(Retries::<Test>::iter().count(), 2);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		// 42 retry will fail again
		run_to_block(111);
		// should still be queued for the normal period
		assert!(Agenda::<Test>::get(210)[0].is_some());
		// should be queued to be retried next block
		assert!(Agenda::<Test>::get(112)[0].is_some());
		// 42 has consumed another retry attempt
		assert_eq!(Retries::<Test>::get((210, 0)).unwrap().remaining, 2);
		assert_eq!(Retries::<Test>::get((112, 0)).unwrap().remaining, 0);
		assert_eq!(Retries::<Test>::iter().count(), 2);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		// 42 retry will fail again
		run_to_block(112);
		// should still be queued for the normal period
		assert!(Agenda::<Test>::get(210)[0].is_some());
		// 42 retry clone ran out of retries, must have been evicted
		assert_eq!(Agenda::<Test>::iter().count(), 1);

		// advance
		run_to_block(209);
		// should still be queued for the normal period
		assert!(Agenda::<Test>::get(210)[0].is_some());
		// 42 retry clone ran out of retries, must have been evicted
		assert_eq!(Agenda::<Test>::iter().count(), 1);
		// 42 should fail again and should spawn another retry clone
		run_to_block(210);
		// should be queued for the normal period of 100 blocks
		assert!(Agenda::<Test>::get(310)[0].is_some());
		// should also be queued to be retried next block
		assert!(Agenda::<Test>::get(211)[0].is_some());
		// 42 retry clone has consumed one retry attempt
		assert_eq!(Retries::<Test>::get((211, 0)).unwrap().remaining, 1);
		// 42 original task still has the original remaining attempts
		assert_eq!(Retries::<Test>::get((310, 0)).unwrap().remaining, 2);
		assert_eq!(Retries::<Test>::iter().count(), 2);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		// make 42 run successfully again
		Threshold::<Test>::put((1, 1000));
		// 42 retry clone should now succeed
		run_to_block(211);
		// should be queued for the normal period of 100 blocks
		assert!(Agenda::<Test>::get(310)[0].is_some());
		// retry was successful, retry task should have been discarded
		assert_eq!(Agenda::<Test>::iter().count(), 1);
		// 42 original task still has the original remaining attempts
		assert_eq!(Retries::<Test>::get((310, 0)).unwrap().remaining, 2);
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32)]);

		// fast forward to the last periodic run of 42
		run_to_block(310);
		// 42 was successful, the period ended as this was the 4th scheduled periodic run so 42 must
		// have been discarded
		assert_eq!(Agenda::<Test>::iter().count(), 0);
		// agenda is empty so no retries should exist
		assert_eq!(Retries::<Test>::iter().count(), 0);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32), (root(), 42u32)]);
	});
}

#[test]
fn reschedule_works() {
	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		assert!(!<Test as frame_system::Config>::BaseCallFilter::contains(&call));
		assert_eq!(
			Scheduler::do_schedule(
				DispatchTime::At(4),
				None,
				127,
				root(),
				Preimage::bound(call).unwrap()
			)
			.unwrap(),
			(4, 0)
		);

		run_to_block(3);
		assert!(logger::log().is_empty());

		assert_eq!(Scheduler::do_reschedule((4, 0), DispatchTime::At(6)).unwrap(), (6, 0));

		assert_noop!(
			Scheduler::do_reschedule((6, 0), DispatchTime::At(6)),
			Error::<Test>::RescheduleNoChange
		);

		run_to_block(4);
		assert!(logger::log().is_empty());

		run_to_block(6);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);

		run_to_block(100);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
	});
}

#[test]
fn reschedule_named_works() {
	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		assert!(!<Test as frame_system::Config>::BaseCallFilter::contains(&call));
		assert_eq!(
			Scheduler::do_schedule_named(
				[1u8; 32],
				DispatchTime::At(4),
				None,
				127,
				root(),
				Preimage::bound(call).unwrap(),
			)
			.unwrap(),
			(4, 0)
		);

		run_to_block(3);
		assert!(logger::log().is_empty());

		assert_eq!(Scheduler::do_reschedule_named([1u8; 32], DispatchTime::At(6)).unwrap(), (6, 0));

		assert_noop!(
			Scheduler::do_reschedule_named([1u8; 32], DispatchTime::At(6)),
			Error::<Test>::RescheduleNoChange
		);

		run_to_block(4);
		assert!(logger::log().is_empty());

		run_to_block(6);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);

		run_to_block(100);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
	});
}

#[test]
fn reschedule_named_periodic_works() {
	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		assert!(!<Test as frame_system::Config>::BaseCallFilter::contains(&call));
		assert_eq!(
			Scheduler::do_schedule_named(
				[1u8; 32],
				DispatchTime::At(4),
				Some((3, 3)),
				127,
				root(),
				Preimage::bound(call).unwrap(),
			)
			.unwrap(),
			(4, 0)
		);

		run_to_block(3);
		assert!(logger::log().is_empty());

		assert_eq!(Scheduler::do_reschedule_named([1u8; 32], DispatchTime::At(5)).unwrap(), (5, 0));
		assert_eq!(Scheduler::do_reschedule_named([1u8; 32], DispatchTime::At(6)).unwrap(), (6, 0));

		run_to_block(5);
		assert!(logger::log().is_empty());

		run_to_block(6);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);

		assert_eq!(
			Scheduler::do_reschedule_named([1u8; 32], DispatchTime::At(10)).unwrap(),
			(10, 0)
		);

		run_to_block(9);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);

		run_to_block(10);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32)]);

		run_to_block(13);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32), (root(), 42u32)]);

		run_to_block(100);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 42u32), (root(), 42u32)]);
	});
}

#[test]
fn cancel_named_scheduling_works_with_normal_cancel() {
	new_test_ext().execute_with(|| {
		// at #4.
		Scheduler::do_schedule_named(
			[1u8; 32],
			DispatchTime::At(4),
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
		let i = Scheduler::do_schedule(
			DispatchTime::At(4),
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
		run_to_block(3);
		assert!(logger::log().is_empty());
		assert_ok!(Scheduler::do_cancel_named(None, [1u8; 32]));
		assert_ok!(Scheduler::do_cancel(None, i));
		run_to_block(100);
		assert!(logger::log().is_empty());
	});
}

#[test]
fn cancel_named_periodic_scheduling_works() {
	new_test_ext().execute_with(|| {
		// at #4, every 3 blocks, 3 times.
		Scheduler::do_schedule_named(
			[1u8; 32],
			DispatchTime::At(4),
			Some((3, 3)),
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
				i: 42,
				weight: Weight::from_parts(10, 0),
			}))
			.unwrap(),
		)
		.unwrap();
		// same id results in error.
		assert!(Scheduler::do_schedule_named(
			[1u8; 32],
			DispatchTime::At(4),
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
		// different id is ok.
		Scheduler::do_schedule_named(
			[2u8; 32],
			DispatchTime::At(8),
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
		run_to_block(3);
		assert!(logger::log().is_empty());
		run_to_block(4);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		run_to_block(6);
		assert_ok!(Scheduler::do_cancel_named(None, [1u8; 32]));
		run_to_block(100);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 69u32)]);
	});
}

#[test]
fn scheduler_respects_weight_limits() {
	let max_weight: Weight = <Test as Config>::MaximumWeight::get();
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::Logger(LoggerCall::log { i: 42, weight: max_weight / 3 * 2 });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		));
		let call = RuntimeCall::Logger(LoggerCall::log { i: 69, weight: max_weight / 3 * 2 });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		));
		// 69 and 42 do not fit together
		run_to_block(4);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		run_to_block(5);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 69u32)]);
	});
}

#[test]
fn retry_respects_weight_limits() {
	let max_weight: Weight = <Test as Config>::MaximumWeight::get();
	new_test_ext().execute_with(|| {
		// schedule 42
		let call = RuntimeCall::Logger(LoggerCall::log { i: 42, weight: max_weight / 3 * 2 });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(8),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		));
		// schedule 20 with a call that will fail until we reach block 8
		Threshold::<Test>::put((8, 100));
		let call = RuntimeCall::Logger(LoggerCall::timed_log { i: 20, weight: max_weight / 3 * 2 });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		));
		// set a retry config for 20 for 10 retries every block
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 10, 1));
		// 20 should fail and be retried later
		run_to_block(4);
		assert!(Agenda::<Test>::get(5)[0].is_some());
		assert!(Agenda::<Test>::get(8)[0].is_some());
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert!(logger::log().is_empty());
		// 20 still fails but is scheduled next block together with 42
		run_to_block(7);
		assert_eq!(Agenda::<Test>::get(8).len(), 2);
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert!(logger::log().is_empty());
		// 20 and 42 do not fit together
		// 42 is executed as it was first in the queue
		// 20 is still on the 8th block's agenda
		run_to_block(8);
		assert!(Agenda::<Test>::get(8)[0].is_none());
		assert!(Agenda::<Test>::get(8)[1].is_some());
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		// 20 is executed and the schedule is cleared
		run_to_block(9);
		assert_eq!(Agenda::<Test>::iter().count(), 0);
		assert_eq!(Retries::<Test>::iter().count(), 0);
		assert_eq!(logger::log(), vec![(root(), 42u32), (root(), 20u32)]);
	});
}

#[test]
fn try_schedule_retry_respects_weight_limits() {
	let max_weight: Weight = <Test as Config>::MaximumWeight::get();
	new_test_ext().execute_with(|| {
		let service_agendas_weight = <Test as Config>::WeightInfo::service_agendas_base();
		let service_agenda_weight = <Test as Config>::WeightInfo::service_agenda_base(
			<Test as Config>::MaxScheduledPerBlock::get(),
		);
		let actual_service_agenda_weight = <Test as Config>::WeightInfo::service_agenda_base(1);
		// Some weight for `service_agenda` will be refunded, so we need to make sure the weight
		// `try_schedule_retry` is going to ask for is greater than this difference, and we take a
		// safety factor of 10 to make sure we're over that limit.
		let meter = WeightMeter::with_limit(
			<Test as Config>::WeightInfo::schedule_retry(
				<Test as Config>::MaxScheduledPerBlock::get(),
			) / 10,
		);
		assert!(meter.can_consume(service_agenda_weight - actual_service_agenda_weight));

		let reference_call =
			RuntimeCall::Logger(LoggerCall::timed_log { i: 20, weight: max_weight / 3 * 2 });
		let bounded = <Test as Config>::Preimages::bound(reference_call).unwrap();
		let base_weight = <Test as Config>::WeightInfo::service_task(
			bounded.lookup_len().map(|x| x as usize),
			false,
			false,
		);
		// we make the call cost enough so that all checks have enough weight to run aside from
		// `try_schedule_retry`
		let call_weight = max_weight - service_agendas_weight - service_agenda_weight - base_weight;
		let call = RuntimeCall::Logger(LoggerCall::timed_log { i: 20, weight: call_weight });
		// schedule 20 with a call that will fail until we reach block 8
		Threshold::<Test>::put((8, 100));

		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		));
		// set a retry config for 20 for 10 retries every block
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 10, 1));
		// 20 should fail and, because of insufficient weight, it should not be scheduled again
		run_to_block(4);
		// nothing else should be scheduled
		assert_eq!(Agenda::<Test>::iter().count(), 0);
		assert_eq!(Retries::<Test>::iter().count(), 0);
		assert_eq!(logger::log(), vec![]);
		// check the `RetryFailed` event happened
		let events = frame_system::Pallet::<Test>::events();
		let system_event: <Test as frame_system::Config>::RuntimeEvent =
			Event::RetryFailed { task: (4, 0), id: None }.into();
		// compare to the last event record
		let frame_system::EventRecord { event, .. } = &events[events.len() - 1];
		assert_eq!(event, &system_event);
	});
}

/// Permanently overweight calls are not deleted but also not executed.
#[test]
fn scheduler_does_not_delete_permanently_overweight_call() {
	let max_weight: Weight = <Test as Config>::MaximumWeight::get();
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::Logger(LoggerCall::log { i: 42, weight: max_weight });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		));
		// Never executes.
		run_to_block(100);
		assert_eq!(logger::log(), vec![]);

		// Assert the `PermanentlyOverweight` event.
		assert_eq!(
			System::events().last().unwrap().event,
			crate::Event::PermanentlyOverweight { task: (4, 0), id: None }.into(),
		);
		// The call is still in the agenda.
		assert!(Agenda::<Test>::get(4)[0].is_some());
	});
}

#[test]
fn scheduler_handles_periodic_failure() {
	let max_weight: Weight = <Test as Config>::MaximumWeight::get();
	let max_per_block = <Test as Config>::MaxScheduledPerBlock::get();

	new_test_ext().execute_with(|| {
		let call = RuntimeCall::Logger(LoggerCall::log { i: 42, weight: (max_weight / 3) * 2 });
		let bound = Preimage::bound(call).unwrap();

		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			Some((4, u32::MAX)),
			127,
			root(),
			bound.clone(),
		));
		// Executes 5 times till block 20.
		run_to_block(20);
		assert_eq!(logger::log().len(), 5);

		// Block 28 will already be full.
		for _ in 0..max_per_block {
			assert_ok!(Scheduler::do_schedule(
				DispatchTime::At(28),
				None,
				120,
				root(),
				bound.clone(),
			));
		}

		// Going to block 24 will emit a `PeriodicFailed` event.
		run_to_block(24);
		assert_eq!(logger::log().len(), 6);

		assert_eq!(
			System::events().last().unwrap().event,
			crate::Event::PeriodicFailed { task: (24, 0), id: None }.into(),
		);
	});
}

#[test]
fn scheduler_handles_periodic_unavailable_preimage() {
	let max_weight: Weight = <Test as Config>::MaximumWeight::get();

	new_test_ext().execute_with(|| {
		let call = RuntimeCall::Logger(LoggerCall::log { i: 42, weight: (max_weight / 3) * 2 });
		let hash = <Test as frame_system::Config>::Hashing::hash_of(&call);
		let len = call.using_encoded(|x| x.len()) as u32;
		// Important to use here `Bounded::Lookup` to ensure that we request the hash.
		let bound = Bounded::Lookup { hash, len };
		// The preimage isn't requested yet.
		assert!(!Preimage::is_requested(&hash));

		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			Some((4, u32::MAX)),
			127,
			root(),
			bound.clone(),
		));

		// The preimage is requested.
		assert!(Preimage::is_requested(&hash));

		// Note the preimage.
		assert_ok!(Preimage::note_preimage(RuntimeOrigin::signed(1), call.encode()));

		// Executes 1 times till block 4.
		run_to_block(4);
		assert_eq!(logger::log().len(), 1);

		// As the public api doesn't support to remove a noted preimage, we need to first unnote it
		// and then request it again. Basically this should not happen in real life (whatever you
		// call real life;).
		Preimage::unnote(&hash);
		Preimage::request(&hash);

		// Does not ever execute again.
		run_to_block(100);
		assert_eq!(logger::log().len(), 1);

		// The preimage is not requested anymore.
		assert!(!Preimage::is_requested(&hash));
	});
}

#[test]
fn scheduler_respects_priority_ordering() {
	let max_weight: Weight = <Test as Config>::MaximumWeight::get();
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::Logger(LoggerCall::log { i: 42, weight: max_weight / 3 });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			1,
			root(),
			Preimage::bound(call).unwrap(),
		));
		let call = RuntimeCall::Logger(LoggerCall::log { i: 69, weight: max_weight / 3 });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			0,
			root(),
			Preimage::bound(call).unwrap(),
		));
		run_to_block(4);
		assert_eq!(logger::log(), vec![(root(), 69u32), (root(), 42u32)]);
	});
}

#[test]
fn scheduler_respects_priority_ordering_with_soft_deadlines() {
	new_test_ext().execute_with(|| {
		let max_weight: Weight = <Test as Config>::MaximumWeight::get();
		let call = RuntimeCall::Logger(LoggerCall::log { i: 42, weight: max_weight / 5 * 2 });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			255,
			root(),
			Preimage::bound(call).unwrap(),
		));
		let call = RuntimeCall::Logger(LoggerCall::log { i: 69, weight: max_weight / 5 * 2 });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		));
		let call = RuntimeCall::Logger(LoggerCall::log { i: 2600, weight: max_weight / 5 * 4 });
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			126,
			root(),
			Preimage::bound(call).unwrap(),
		));

		// 2600 does not fit with 69 or 42, but has higher priority, so will go through
		run_to_block(4);
		assert_eq!(logger::log(), vec![(root(), 2600u32)]);
		// 69 and 42 fit together
		run_to_block(5);
		assert_eq!(logger::log(), vec![(root(), 2600u32), (root(), 69u32), (root(), 42u32)]);
	});
}

#[test]
fn on_initialize_weight_is_correct() {
	new_test_ext().execute_with(|| {
		let call_weight = Weight::from_parts(25, 0);

		// Named
		let call = RuntimeCall::Logger(LoggerCall::log {
			i: 3,
			weight: call_weight + Weight::from_parts(1, 0),
		});
		assert_ok!(Scheduler::do_schedule_named(
			[1u8; 32],
			DispatchTime::At(3),
			None,
			255,
			root(),
			Preimage::bound(call).unwrap(),
		));
		let call = RuntimeCall::Logger(LoggerCall::log {
			i: 42,
			weight: call_weight + Weight::from_parts(2, 0),
		});
		// Anon Periodic
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(2),
			Some((1000, 3)),
			128,
			root(),
			Preimage::bound(call).unwrap(),
		));
		let call = RuntimeCall::Logger(LoggerCall::log {
			i: 69,
			weight: call_weight + Weight::from_parts(3, 0),
		});
		// Anon
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(2),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		));
		// Named Periodic
		let call = RuntimeCall::Logger(LoggerCall::log {
			i: 2600,
			weight: call_weight + Weight::from_parts(4, 0),
		});
		assert_ok!(Scheduler::do_schedule_named(
			[2u8; 32],
			DispatchTime::At(1),
			Some((1000, 3)),
			126,
			root(),
			Preimage::bound(call).unwrap(),
		));

		// Will include the named periodic only
		assert_eq!(
			Scheduler::on_initialize(1),
			TestWeightInfo::service_agendas_base() +
				TestWeightInfo::service_agenda_base(1) +
				<TestWeightInfo as MarginalWeightInfo>::service_task(None, true, true) +
				TestWeightInfo::execute_dispatch_unsigned() +
				call_weight + Weight::from_parts(4, 0)
		);
		assert_eq!(IncompleteSince::<Test>::get(), None);
		assert_eq!(logger::log(), vec![(root(), 2600u32)]);

		// Will include anon and anon periodic
		assert_eq!(
			Scheduler::on_initialize(2),
			TestWeightInfo::service_agendas_base() +
				TestWeightInfo::service_agenda_base(2) +
				<TestWeightInfo as MarginalWeightInfo>::service_task(None, false, true) +
				TestWeightInfo::execute_dispatch_unsigned() +
				call_weight + Weight::from_parts(3, 0) +
				<TestWeightInfo as MarginalWeightInfo>::service_task(None, false, false) +
				TestWeightInfo::execute_dispatch_unsigned() +
				call_weight + Weight::from_parts(2, 0)
		);
		assert_eq!(IncompleteSince::<Test>::get(), None);
		assert_eq!(logger::log(), vec![(root(), 2600u32), (root(), 69u32), (root(), 42u32)]);

		// Will include named only
		assert_eq!(
			Scheduler::on_initialize(3),
			TestWeightInfo::service_agendas_base() +
				TestWeightInfo::service_agenda_base(1) +
				<TestWeightInfo as MarginalWeightInfo>::service_task(None, true, false) +
				TestWeightInfo::execute_dispatch_unsigned() +
				call_weight + Weight::from_parts(1, 0)
		);
		assert_eq!(IncompleteSince::<Test>::get(), None);
		assert_eq!(
			logger::log(),
			vec![(root(), 2600u32), (root(), 69u32), (root(), 42u32), (root(), 3u32)]
		);

		// Will contain none
		let actual_weight = Scheduler::on_initialize(4);
		assert_eq!(
			actual_weight,
			TestWeightInfo::service_agendas_base() + TestWeightInfo::service_agenda_base(0)
		);
	});
}

#[test]
fn root_calls_works() {
	new_test_ext().execute_with(|| {
		let call = Box::new(RuntimeCall::Logger(LoggerCall::log {
			i: 69,
			weight: Weight::from_parts(10, 0),
		}));
		let call2 = Box::new(RuntimeCall::Logger(LoggerCall::log {
			i: 42,
			weight: Weight::from_parts(10, 0),
		}));
		assert_ok!(
			Scheduler::schedule_named(RuntimeOrigin::root(), [1u8; 32], 4, None, 127, call,)
		);
		assert_ok!(Scheduler::schedule(RuntimeOrigin::root(), 4, None, 127, call2));
		run_to_block(3);
		// Scheduled calls are in the agenda.
		assert_eq!(Agenda::<Test>::get(4).len(), 2);
		assert!(logger::log().is_empty());
		assert_ok!(Scheduler::cancel_named(RuntimeOrigin::root(), [1u8; 32]));
		assert_ok!(Scheduler::cancel(RuntimeOrigin::root(), 4, 1));
		// Scheduled calls are made NONE, so should not effect state
		run_to_block(100);
		assert!(logger::log().is_empty());
	});
}

#[test]
fn fails_to_schedule_task_in_the_past() {
	new_test_ext().execute_with(|| {
		run_to_block(3);

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

		assert_noop!(
			Scheduler::schedule_named(RuntimeOrigin::root(), [1u8; 32], 2, None, 127, call1),
			Error::<Test>::TargetBlockNumberInPast,
		);

		assert_noop!(
			Scheduler::schedule(RuntimeOrigin::root(), 2, None, 127, call2),
			Error::<Test>::TargetBlockNumberInPast,
		);

		assert_noop!(
			Scheduler::schedule(RuntimeOrigin::root(), 3, None, 127, call3),
			Error::<Test>::TargetBlockNumberInPast,
		);
	});
}

#[test]
fn should_use_origin() {
	new_test_ext().execute_with(|| {
		let call = Box::new(RuntimeCall::Logger(LoggerCall::log {
			i: 69,
			weight: Weight::from_parts(10, 0),
		}));
		let call2 = Box::new(RuntimeCall::Logger(LoggerCall::log {
			i: 42,
			weight: Weight::from_parts(10, 0),
		}));
		assert_ok!(Scheduler::schedule_named(
			system::RawOrigin::Signed(1).into(),
			[1u8; 32],
			4,
			None,
			127,
			call,
		));
		assert_ok!(Scheduler::schedule(system::RawOrigin::Signed(1).into(), 4, None, 127, call2,));
		run_to_block(3);
		// Scheduled calls are in the agenda.
		assert_eq!(Agenda::<Test>::get(4).len(), 2);
		assert!(logger::log().is_empty());
		assert_ok!(Scheduler::cancel_named(system::RawOrigin::Signed(1).into(), [1u8; 32]));
		assert_ok!(Scheduler::cancel(system::RawOrigin::Signed(1).into(), 4, 1));
		// Scheduled calls are made NONE, so should not effect state
		run_to_block(100);
		assert!(logger::log().is_empty());
	});
}

#[test]
fn should_check_origin() {
	new_test_ext().execute_with(|| {
		let call = Box::new(RuntimeCall::Logger(LoggerCall::log {
			i: 69,
			weight: Weight::from_parts(10, 0),
		}));
		let call2 = Box::new(RuntimeCall::Logger(LoggerCall::log {
			i: 42,
			weight: Weight::from_parts(10, 0),
		}));
		assert_noop!(
			Scheduler::schedule_named(
				system::RawOrigin::Signed(2).into(),
				[1u8; 32],
				4,
				None,
				127,
				call
			),
			BadOrigin
		);
		assert_noop!(
			Scheduler::schedule(system::RawOrigin::Signed(2).into(), 4, None, 127, call2),
			BadOrigin
		);
	});
}

#[test]
fn should_check_origin_for_cancel() {
	new_test_ext().execute_with(|| {
		let call = Box::new(RuntimeCall::Logger(LoggerCall::log_without_filter {
			i: 69,
			weight: Weight::from_parts(10, 0),
		}));
		let call2 = Box::new(RuntimeCall::Logger(LoggerCall::log_without_filter {
			i: 42,
			weight: Weight::from_parts(10, 0),
		}));
		assert_ok!(Scheduler::schedule_named(
			system::RawOrigin::Signed(1).into(),
			[1u8; 32],
			4,
			None,
			127,
			call,
		));
		assert_ok!(Scheduler::schedule(system::RawOrigin::Signed(1).into(), 4, None, 127, call2,));
		run_to_block(3);
		// Scheduled calls are in the agenda.
		assert_eq!(Agenda::<Test>::get(4).len(), 2);
		assert!(logger::log().is_empty());
		assert_noop!(
			Scheduler::cancel_named(system::RawOrigin::Signed(2).into(), [1u8; 32]),
			BadOrigin
		);
		assert_noop!(Scheduler::cancel(system::RawOrigin::Signed(2).into(), 4, 1), BadOrigin);
		assert_noop!(Scheduler::cancel_named(system::RawOrigin::Root.into(), [1u8; 32]), BadOrigin);
		assert_noop!(Scheduler::cancel(system::RawOrigin::Root.into(), 4, 1), BadOrigin);
		run_to_block(5);
		assert_eq!(
			logger::log(),
			vec![
				(system::RawOrigin::Signed(1).into(), 69u32),
				(system::RawOrigin::Signed(1).into(), 42u32)
			]
		);
	});
}

#[test]
fn cancel_removes_retry_entry() {
	new_test_ext().execute_with(|| {
		// task fails until block 99 is reached
		Threshold::<Test>::put((99, 100));
		// task 20 at #4
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 20,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));
		// named task 42 at #4
		assert_ok!(Scheduler::do_schedule_named(
			[1u8; 32],
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		assert_eq!(Agenda::<Test>::get(4).len(), 2);
		// task 20 will be retried 3 times every block
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 10, 1));
		// task 42 will be retried 10 times every 3 blocks
		assert_ok!(Scheduler::set_retry_named(root().into(), [1u8; 32], 10, 1));
		assert_eq!(Retries::<Test>::iter().count(), 2);
		run_to_block(3);
		assert!(logger::log().is_empty());
		assert_eq!(Agenda::<Test>::get(4).len(), 2);
		// both tasks fail
		run_to_block(4);
		assert!(Agenda::<Test>::get(4).is_empty());
		// 42 and 20 are rescheduled for next block
		assert_eq!(Agenda::<Test>::get(5).len(), 2);
		assert!(logger::log().is_empty());
		// 42 and 20 still fail
		run_to_block(5);
		// 42 and 20 rescheduled for next block
		assert_eq!(Agenda::<Test>::get(6).len(), 2);
		assert_eq!(Retries::<Test>::iter().count(), 2);
		assert!(logger::log().is_empty());

		// even though 42 is being retried, the tasks scheduled for retries are not named
		assert_eq!(Lookup::<Test>::iter().count(), 0);
		assert!(Scheduler::cancel(root().into(), 6, 0).is_ok());

		// 20 is removed, 42 still fails
		run_to_block(6);
		// 42 rescheduled for next block
		assert_eq!(Agenda::<Test>::get(7).len(), 1);
		// 20's retry entry is removed
		assert!(!Retries::<Test>::contains_key((4, 0)));
		assert_eq!(Retries::<Test>::iter().count(), 1);
		assert!(logger::log().is_empty());

		assert!(Scheduler::cancel(root().into(), 7, 0).is_ok());

		// both tasks are canceled, everything is removed now
		run_to_block(7);
		assert!(Agenda::<Test>::get(8).is_empty());
		assert_eq!(Retries::<Test>::iter().count(), 0);
	});
}

#[test]
fn cancel_retries_works() {
	new_test_ext().execute_with(|| {
		// task fails until block 99 is reached
		Threshold::<Test>::put((99, 100));
		// task 20 at #4
		assert_ok!(Scheduler::do_schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 20,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));
		// named task 42 at #4
		assert_ok!(Scheduler::do_schedule_named(
			[1u8; 32],
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(RuntimeCall::Logger(logger::Call::timed_log {
				i: 42,
				weight: Weight::from_parts(10, 0)
			}))
			.unwrap()
		));

		assert_eq!(Agenda::<Test>::get(4).len(), 2);
		// task 20 will be retried 3 times every block
		assert_ok!(Scheduler::set_retry(root().into(), (4, 0), 10, 1));
		// task 42 will be retried 10 times every 3 blocks
		assert_ok!(Scheduler::set_retry_named(root().into(), [1u8; 32], 10, 1));
		assert_eq!(Retries::<Test>::iter().count(), 2);
		run_to_block(3);
		assert!(logger::log().is_empty());
		assert_eq!(Agenda::<Test>::get(4).len(), 2);
		// cancel the retry config for 20
		assert_ok!(Scheduler::cancel_retry(root().into(), (4, 0)));
		assert_eq!(Retries::<Test>::iter().count(), 1);
		// cancel the retry config for 42
		assert_ok!(Scheduler::cancel_retry_named(root().into(), [1u8; 32]));
		assert_eq!(Retries::<Test>::iter().count(), 0);
		run_to_block(4);
		// both tasks failed and there are no more retries, so they are evicted
		assert_eq!(Agenda::<Test>::get(4).len(), 0);
		assert_eq!(Retries::<Test>::iter().count(), 0);
	});
}

#[test]
fn migration_to_v4_works() {
	new_test_ext().execute_with(|| {
		for i in 0..3u64 {
			let k = i.twox_64_concat();
			let old = vec![
				Some(ScheduledV1 {
					maybe_id: None,
					priority: i as u8 + 10,
					call: RuntimeCall::Logger(LoggerCall::log {
						i: 96,
						weight: Weight::from_parts(100, 0),
					}),
					maybe_periodic: None,
				}),
				None,
				Some(ScheduledV1 {
					maybe_id: Some(b"test".to_vec()),
					priority: 123,
					call: RuntimeCall::Logger(LoggerCall::log {
						i: 69,
						weight: Weight::from_parts(10, 0),
					}),
					maybe_periodic: Some((456u64, 10)),
				}),
			];
			frame_support::migration::put_storage_value(b"Scheduler", b"Agenda", &k, old);
		}

		Scheduler::migrate_v1_to_v4();

		let mut x = Agenda::<Test>::iter().map(|x| (x.0, x.1.into_inner())).collect::<Vec<_>>();
		x.sort_by_key(|x| x.0);
		let expected = vec![
			(
				0,
				vec![
					Some(ScheduledOf::<Test> {
						maybe_id: None,
						priority: 10,
						call: Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
							i: 96,
							weight: Weight::from_parts(100, 0),
						}))
						.unwrap(),
						maybe_periodic: None,
						origin: root(),
						_phantom: PhantomData::<u64>::default(),
					}),
					None,
					Some(ScheduledOf::<Test> {
						maybe_id: Some(blake2_256(&b"test"[..])),
						priority: 123,
						call: Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
							i: 69,
							weight: Weight::from_parts(10, 0),
						}))
						.unwrap(),
						maybe_periodic: Some((456u64, 10)),
						origin: root(),
						_phantom: PhantomData::<u64>::default(),
					}),
				],
			),
			(
				1,
				vec![
					Some(ScheduledOf::<Test> {
						maybe_id: None,
						priority: 11,
						call: Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
							i: 96,
							weight: Weight::from_parts(100, 0),
						}))
						.unwrap(),
						maybe_periodic: None,
						origin: root(),
						_phantom: PhantomData::<u64>::default(),
					}),
					None,
					Some(ScheduledOf::<Test> {
						maybe_id: Some(blake2_256(&b"test"[..])),
						priority: 123,
						call: Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
							i: 69,
							weight: Weight::from_parts(10, 0),
						}))
						.unwrap(),
						maybe_periodic: Some((456u64, 10)),
						origin: root(),
						_phantom: PhantomData::<u64>::default(),
					}),
				],
			),
			(
				2,
				vec![
					Some(ScheduledOf::<Test> {
						maybe_id: None,
						priority: 12,
						call: Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
							i: 96,
							weight: Weight::from_parts(100, 0),
						}))
						.unwrap(),
						maybe_periodic: None,
						origin: root(),
						_phantom: PhantomData::<u64>::default(),
					}),
					None,
					Some(ScheduledOf::<Test> {
						maybe_id: Some(blake2_256(&b"test"[..])),
						priority: 123,
						call: Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
							i: 69,
							weight: Weight::from_parts(10, 0),
						}))
						.unwrap(),
						maybe_periodic: Some((456u64, 10)),
						origin: root(),
						_phantom: PhantomData::<u64>::default(),
					}),
				],
			),
		];
		for (i, j) in x.iter().zip(expected.iter()) {
			assert_eq!(i.0, j.0);
			for (x, y) in i.1.iter().zip(j.1.iter()) {
				assert_eq!(x, y);
			}
		}
		assert_eq_uvec!(x, expected);

		assert_eq!(Scheduler::on_chain_storage_version(), 4);
	});
}

#[test]
fn test_migrate_origin() {
	new_test_ext().execute_with(|| {
		for i in 0..3u64 {
			let k = i.twox_64_concat();
			let old: Vec<Option<Scheduled<[u8; 32], BoundedCallOf<Test>, u64, u32, u64>>> = vec![
				Some(Scheduled {
					maybe_id: None,
					priority: i as u8 + 10,
					call: Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
						i: 96,
						weight: Weight::from_parts(100, 0),
					}))
					.unwrap(),
					origin: 3u32,
					maybe_periodic: None,
					_phantom: Default::default(),
				}),
				None,
				Some(Scheduled {
					maybe_id: Some(blake2_256(&b"test"[..])),
					priority: 123,
					origin: 2u32,
					call: Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
						i: 69,
						weight: Weight::from_parts(10, 0),
					}))
					.unwrap(),
					maybe_periodic: Some((456u64, 10)),
					_phantom: Default::default(),
				}),
			];
			frame_support::migration::put_storage_value(b"Scheduler", b"Agenda", &k, old);
		}

		impl Into<OriginCaller> for u32 {
			fn into(self) -> OriginCaller {
				match self {
					3u32 => system::RawOrigin::Root.into(),
					2u32 => system::RawOrigin::None.into(),
					101u32 => system::RawOrigin::Signed(101).into(),
					102u32 => system::RawOrigin::Signed(102).into(),
					_ => unreachable!("test make no use of it"),
				}
			}
		}

		Scheduler::migrate_origin::<u32>();

		assert_eq_uvec!(
			Agenda::<Test>::iter().map(|x| (x.0, x.1.into_inner())).collect::<Vec<_>>(),
			vec![
				(
					0,
					vec![
						Some(ScheduledOf::<Test> {
							maybe_id: None,
							priority: 10,
							call: Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
								i: 96,
								weight: Weight::from_parts(100, 0)
							}))
							.unwrap(),
							maybe_periodic: None,
							origin: system::RawOrigin::Root.into(),
							_phantom: PhantomData::<u64>::default(),
						}),
						None,
						Some(Scheduled {
							maybe_id: Some(blake2_256(&b"test"[..])),
							priority: 123,
							call: Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
								i: 69,
								weight: Weight::from_parts(10, 0)
							}))
							.unwrap(),
							maybe_periodic: Some((456u64, 10)),
							origin: system::RawOrigin::None.into(),
							_phantom: PhantomData::<u64>::default(),
						}),
					]
				),
				(
					1,
					vec![
						Some(Scheduled {
							maybe_id: None,
							priority: 11,
							call: Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
								i: 96,
								weight: Weight::from_parts(100, 0)
							}))
							.unwrap(),
							maybe_periodic: None,
							origin: system::RawOrigin::Root.into(),
							_phantom: PhantomData::<u64>::default(),
						}),
						None,
						Some(Scheduled {
							maybe_id: Some(blake2_256(&b"test"[..])),
							priority: 123,
							call: Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
								i: 69,
								weight: Weight::from_parts(10, 0)
							}))
							.unwrap(),
							maybe_periodic: Some((456u64, 10)),
							origin: system::RawOrigin::None.into(),
							_phantom: PhantomData::<u64>::default(),
						}),
					]
				),
				(
					2,
					vec![
						Some(Scheduled {
							maybe_id: None,
							priority: 12,
							call: Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
								i: 96,
								weight: Weight::from_parts(100, 0)
							}))
							.unwrap(),
							maybe_periodic: None,
							origin: system::RawOrigin::Root.into(),
							_phantom: PhantomData::<u64>::default(),
						}),
						None,
						Some(Scheduled {
							maybe_id: Some(blake2_256(&b"test"[..])),
							priority: 123,
							call: Preimage::bound(RuntimeCall::Logger(LoggerCall::log {
								i: 69,
								weight: Weight::from_parts(10, 0)
							}))
							.unwrap(),
							maybe_periodic: Some((456u64, 10)),
							origin: system::RawOrigin::None.into(),
							_phantom: PhantomData::<u64>::default(),
						}),
					]
				)
			]
		);
	});
}

#[test]
fn postponed_named_task_cannot_be_rescheduled() {
	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(1000, 0) });
		let hash = <Test as frame_system::Config>::Hashing::hash_of(&call);
		let len = call.using_encoded(|x| x.len()) as u32;
		// Important to use here `Bounded::Lookup` to ensure that we request the hash.
		let hashed = Bounded::Lookup { hash, len };
		let name: [u8; 32] = hash.as_ref().try_into().unwrap();

		let address = Scheduler::do_schedule_named(
			name,
			DispatchTime::At(4),
			None,
			127,
			root(),
			hashed.clone(),
		)
		.unwrap();
		assert!(Preimage::is_requested(&hash));
		assert!(Lookup::<Test>::contains_key(name));

		// Run to a very large block.
		run_to_block(10);

		// It was not executed.
		assert!(logger::log().is_empty());

		// Preimage was not available
		assert_eq!(
			System::events().last().unwrap().event,
			crate::Event::CallUnavailable { task: (4, 0), id: Some(name) }.into()
		);

		// So it should not be requested.
		assert!(!Preimage::is_requested(&hash));
		// Postponing removes the lookup.
		assert!(!Lookup::<Test>::contains_key(name));

		// The agenda still contains the call.
		let agenda = Agenda::<Test>::iter().collect::<Vec<_>>();
		assert_eq!(agenda.len(), 1);
		assert_eq!(
			agenda[0].1,
			vec![Some(Scheduled {
				maybe_id: Some(name),
				priority: 127,
				call: hashed,
				maybe_periodic: None,
				origin: root().into(),
				_phantom: Default::default(),
			})]
		);

		// Finally add the preimage.
		assert_ok!(Preimage::note_preimage(RuntimeOrigin::signed(0), call.encode()));

		run_to_block(1000);
		// It did not execute.
		assert!(logger::log().is_empty());
		assert!(!Preimage::is_requested(&hash));

		// Manually re-schedule the call by name does not work.
		assert_err!(
			Scheduler::do_reschedule_named(name, DispatchTime::At(1001)),
			Error::<Test>::NotFound
		);
		// Manually re-scheduling the call by address errors.
		assert_err!(
			Scheduler::do_reschedule(address, DispatchTime::At(1001)),
			Error::<Test>::Named
		);
	});
}

/// Using the scheduler as `v3::Anon` works.
#[test]
fn scheduler_v3_anon_basic_works() {
	use frame_support::traits::schedule::v3::Anon;
	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });

		// Schedule a call.
		let _address = <Scheduler as Anon<_, _, _>>::schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();

		run_to_block(3);
		// Did not execute till block 3.
		assert!(logger::log().is_empty());
		// Executes in block 4.
		run_to_block(4);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		// ... but not again.
		run_to_block(100);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
	});
}

#[test]
fn scheduler_v3_anon_cancel_works() {
	use frame_support::traits::schedule::v3::Anon;
	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let bound = Preimage::bound(call).unwrap();

		// Schedule a call.
		let address = <Scheduler as Anon<_, _, _>>::schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			bound.clone(),
		)
		.unwrap();
		// Cancel the call.
		assert_ok!(<Scheduler as Anon<_, _, _>>::cancel(address));
		// It did not get executed.
		run_to_block(100);
		assert!(logger::log().is_empty());
		// Cannot cancel again.
		assert_err!(<Scheduler as Anon<_, _, _>>::cancel(address), DispatchError::Unavailable);
	});
}

#[test]
fn scheduler_v3_anon_reschedule_works() {
	use frame_support::traits::schedule::v3::Anon;
	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });

		// Schedule a call.
		let address = <Scheduler as Anon<_, _, _>>::schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();

		run_to_block(3);
		// Did not execute till block 3.
		assert!(logger::log().is_empty());

		// Cannot re-schedule into the same block.
		assert_noop!(
			<Scheduler as Anon<_, _, _>>::reschedule(address, DispatchTime::At(4)),
			Error::<Test>::RescheduleNoChange
		);
		// Cannot re-schedule into the past.
		assert_noop!(
			<Scheduler as Anon<_, _, _>>::reschedule(address, DispatchTime::At(3)),
			Error::<Test>::TargetBlockNumberInPast
		);
		// Re-schedule to block 5.
		assert_ok!(<Scheduler as Anon<_, _, _>>::reschedule(address, DispatchTime::At(5)));
		// Scheduled for block 5.
		run_to_block(4);
		assert!(logger::log().is_empty());
		run_to_block(5);
		// Does execute in block 5.
		assert_eq!(logger::log(), vec![(root(), 42)]);
		// Cannot re-schedule executed task.
		assert_noop!(
			<Scheduler as Anon<_, _, _>>::reschedule(address, DispatchTime::At(10)),
			DispatchError::Unavailable
		);
	});
}

#[test]
fn scheduler_v3_anon_next_schedule_time_works() {
	use frame_support::traits::schedule::v3::Anon;
	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let bound = Preimage::bound(call).unwrap();

		// Schedule a call.
		let address = <Scheduler as Anon<_, _, _>>::schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			bound.clone(),
		)
		.unwrap();

		run_to_block(3);
		// Did not execute till block 3.
		assert!(logger::log().is_empty());

		// Scheduled for block 4.
		assert_eq!(<Scheduler as Anon<_, _, _>>::next_dispatch_time(address), Ok(4));
		// Block 4 executes it.
		run_to_block(4);
		assert_eq!(logger::log(), vec![(root(), 42)]);

		// It has no dispatch time anymore.
		assert_noop!(
			<Scheduler as Anon<_, _, _>>::next_dispatch_time(address),
			DispatchError::Unavailable
		);
	});
}

/// Re-scheduling a task changes its next dispatch time.
#[test]
fn scheduler_v3_anon_reschedule_and_next_schedule_time_work() {
	use frame_support::traits::schedule::v3::Anon;
	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let bound = Preimage::bound(call).unwrap();

		// Schedule a call.
		let old_address = <Scheduler as Anon<_, _, _>>::schedule(
			DispatchTime::At(4),
			None,
			127,
			root(),
			bound.clone(),
		)
		.unwrap();

		run_to_block(3);
		// Did not execute till block 3.
		assert!(logger::log().is_empty());

		// Scheduled for block 4.
		assert_eq!(<Scheduler as Anon<_, _, _>>::next_dispatch_time(old_address), Ok(4));
		// Re-schedule to block 5.
		let address =
			<Scheduler as Anon<_, _, _>>::reschedule(old_address, DispatchTime::At(5)).unwrap();
		assert!(address != old_address);
		// Scheduled for block 5.
		assert_eq!(<Scheduler as Anon<_, _, _>>::next_dispatch_time(address), Ok(5));

		// Block 4 does nothing.
		run_to_block(4);
		assert!(logger::log().is_empty());
		// Block 5 executes it.
		run_to_block(5);
		assert_eq!(logger::log(), vec![(root(), 42)]);
	});
}

#[test]
fn scheduler_v3_anon_schedule_agenda_overflows() {
	use frame_support::traits::schedule::v3::Anon;
	let max: u32 = <Test as Config>::MaxScheduledPerBlock::get();

	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let bound = Preimage::bound(call).unwrap();

		// Schedule the maximal number allowed per block.
		for _ in 0..max {
			<Scheduler as Anon<_, _, _>>::schedule(
				DispatchTime::At(4),
				None,
				127,
				root(),
				bound.clone(),
			)
			.unwrap();
		}

		// One more time and it errors.
		assert_noop!(
			<Scheduler as Anon<_, _, _>>::schedule(DispatchTime::At(4), None, 127, root(), bound,),
			DispatchError::Exhausted
		);

		run_to_block(4);
		// All scheduled calls are executed.
		assert_eq!(logger::log().len() as u32, max);
	});
}

/// Cancelling and scheduling does not overflow the agenda but fills holes.
#[test]
fn scheduler_v3_anon_cancel_and_schedule_fills_holes() {
	use frame_support::traits::schedule::v3::Anon;
	let max: u32 = <Test as Config>::MaxScheduledPerBlock::get();
	assert!(max > 3, "This test only makes sense for MaxScheduledPerBlock > 3");

	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let bound = Preimage::bound(call).unwrap();
		let mut addrs = Vec::<_>::default();

		// Schedule the maximal number allowed per block.
		for _ in 0..max {
			addrs.push(
				<Scheduler as Anon<_, _, _>>::schedule(
					DispatchTime::At(4),
					None,
					127,
					root(),
					bound.clone(),
				)
				.unwrap(),
			);
		}
		// Cancel three of them.
		for addr in addrs.into_iter().take(3) {
			<Scheduler as Anon<_, _, _>>::cancel(addr).unwrap();
		}
		// Schedule three new ones.
		for i in 0..3 {
			let (_block, index) = <Scheduler as Anon<_, _, _>>::schedule(
				DispatchTime::At(4),
				None,
				127,
				root(),
				bound.clone(),
			)
			.unwrap();
			assert_eq!(i, index);
		}

		run_to_block(4);
		// Maximum number of calls are executed.
		assert_eq!(logger::log().len() as u32, max);
	});
}

/// Re-scheduling does not overflow the agenda but fills holes.
#[test]
fn scheduler_v3_anon_reschedule_fills_holes() {
	use frame_support::traits::schedule::v3::Anon;
	let max: u32 = <Test as Config>::MaxScheduledPerBlock::get();
	assert!(max > 3, "pre-condition: This test only makes sense for MaxScheduledPerBlock > 3");

	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let bound = Preimage::bound(call).unwrap();
		let mut addrs = Vec::<_>::default();

		// Schedule the maximal number allowed per block.
		for _ in 0..max {
			addrs.push(
				<Scheduler as Anon<_, _, _>>::schedule(
					DispatchTime::At(4),
					None,
					127,
					root(),
					bound.clone(),
				)
				.unwrap(),
			);
		}
		let mut new_addrs = Vec::<_>::default();
		// Reversed last three elements of block 4.
		let last_three = addrs.into_iter().rev().take(3).collect::<Vec<_>>();
		// Re-schedule three of them to block 5.
		for addr in last_three.iter().cloned() {
			new_addrs
				.push(<Scheduler as Anon<_, _, _>>::reschedule(addr, DispatchTime::At(5)).unwrap());
		}
		// Re-scheduling them back into block 3 should result in the same addrs.
		for (old, want) in new_addrs.into_iter().zip(last_three.into_iter().rev()) {
			let new = <Scheduler as Anon<_, _, _>>::reschedule(old, DispatchTime::At(4)).unwrap();
			assert_eq!(new, want);
		}

		run_to_block(4);
		// Maximum number of calls are executed.
		assert_eq!(logger::log().len() as u32, max);
	});
}

/// The scheduler can be used as `v3::Named` trait.
#[test]
fn scheduler_v3_named_basic_works() {
	use frame_support::traits::schedule::v3::Named;

	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let name = [1u8; 32];

		// Schedule a call.
		let _address = <Scheduler as Named<_, _, _>>::schedule_named(
			name,
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();

		run_to_block(3);
		// Did not execute till block 3.
		assert!(logger::log().is_empty());
		// Executes in block 4.
		run_to_block(4);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
		// ... but not again.
		run_to_block(100);
		assert_eq!(logger::log(), vec![(root(), 42u32)]);
	});
}

/// A named task can be cancelled by its name.
#[test]
fn scheduler_v3_named_cancel_named_works() {
	use frame_support::traits::schedule::v3::Named;
	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let bound = Preimage::bound(call).unwrap();
		let name = [1u8; 32];

		// Schedule a call.
		<Scheduler as Named<_, _, _>>::schedule_named(
			name,
			DispatchTime::At(4),
			None,
			127,
			root(),
			bound.clone(),
		)
		.unwrap();
		// Cancel the call by name.
		assert_ok!(<Scheduler as Named<_, _, _>>::cancel_named(name));
		// It did not get executed.
		run_to_block(100);
		assert!(logger::log().is_empty());
		// Cannot cancel again.
		assert_noop!(<Scheduler as Named<_, _, _>>::cancel_named(name), DispatchError::Unavailable);
	});
}

/// A named task can also be cancelled by its address.
#[test]
fn scheduler_v3_named_cancel_without_name_works() {
	use frame_support::traits::schedule::v3::{Anon, Named};
	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let bound = Preimage::bound(call).unwrap();
		let name = [1u8; 32];

		// Schedule a call.
		let address = <Scheduler as Named<_, _, _>>::schedule_named(
			name,
			DispatchTime::At(4),
			None,
			127,
			root(),
			bound.clone(),
		)
		.unwrap();
		// Cancel the call by address.
		assert_ok!(<Scheduler as Anon<_, _, _>>::cancel(address));
		// It did not get executed.
		run_to_block(100);
		assert!(logger::log().is_empty());
		// Cannot cancel again.
		assert_err!(<Scheduler as Anon<_, _, _>>::cancel(address), DispatchError::Unavailable);
	});
}

/// A named task can be re-scheduled by its name but not by its address.
#[test]
fn scheduler_v3_named_reschedule_named_works() {
	use frame_support::traits::schedule::v3::{Anon, Named};
	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let name = [1u8; 32];

		// Schedule a call.
		let address = <Scheduler as Named<_, _, _>>::schedule_named(
			name,
			DispatchTime::At(4),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();

		run_to_block(3);
		// Did not execute till block 3.
		assert!(logger::log().is_empty());

		// Cannot re-schedule by address.
		assert_noop!(
			<Scheduler as Anon<_, _, _>>::reschedule(address, DispatchTime::At(10)),
			Error::<Test>::Named,
		);
		// Cannot re-schedule into the same block.
		assert_noop!(
			<Scheduler as Named<_, _, _>>::reschedule_named(name, DispatchTime::At(4)),
			Error::<Test>::RescheduleNoChange
		);
		// Cannot re-schedule into the past.
		assert_noop!(
			<Scheduler as Named<_, _, _>>::reschedule_named(name, DispatchTime::At(3)),
			Error::<Test>::TargetBlockNumberInPast
		);
		// Re-schedule to block 5.
		assert_ok!(<Scheduler as Named<_, _, _>>::reschedule_named(name, DispatchTime::At(5)));
		// Scheduled for block 5.
		run_to_block(4);
		assert!(logger::log().is_empty());
		run_to_block(5);
		// Does execute in block 5.
		assert_eq!(logger::log(), vec![(root(), 42)]);
		// Cannot re-schedule executed task.
		assert_noop!(
			<Scheduler as Named<_, _, _>>::reschedule_named(name, DispatchTime::At(10)),
			DispatchError::Unavailable
		);
		// Also not by address.
		assert_noop!(
			<Scheduler as Anon<_, _, _>>::reschedule(address, DispatchTime::At(10)),
			DispatchError::Unavailable
		);
	});
}

#[test]
fn scheduler_v3_named_next_schedule_time_works() {
	use frame_support::traits::schedule::v3::{Anon, Named};
	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let bound = Preimage::bound(call).unwrap();
		let name = [1u8; 32];

		// Schedule a call.
		let address = <Scheduler as Named<_, _, _>>::schedule_named(
			name,
			DispatchTime::At(4),
			None,
			127,
			root(),
			bound.clone(),
		)
		.unwrap();

		run_to_block(3);
		// Did not execute till block 3.
		assert!(logger::log().is_empty());

		// Scheduled for block 4.
		assert_eq!(<Scheduler as Named<_, _, _>>::next_dispatch_time(name), Ok(4));
		// Also works by address.
		assert_eq!(<Scheduler as Anon<_, _, _>>::next_dispatch_time(address), Ok(4));
		// Block 4 executes it.
		run_to_block(4);
		assert_eq!(logger::log(), vec![(root(), 42)]);

		// It has no dispatch time anymore.
		assert_noop!(
			<Scheduler as Named<_, _, _>>::next_dispatch_time(name),
			DispatchError::Unavailable
		);
		// Also not by address.
		assert_noop!(
			<Scheduler as Anon<_, _, _>>::next_dispatch_time(address),
			DispatchError::Unavailable
		);
	});
}

#[test]
fn cancel_last_task_removes_agenda() {
	new_test_ext().execute_with(|| {
		let when = 4;
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let address = Scheduler::do_schedule(
			DispatchTime::At(when),
			None,
			127,
			root(),
			Preimage::bound(call.clone()).unwrap(),
		)
		.unwrap();
		let address2 = Scheduler::do_schedule(
			DispatchTime::At(when),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();
		// two tasks at agenda.
		assert!(Agenda::<Test>::get(when).len() == 2);
		assert_ok!(Scheduler::do_cancel(None, address));
		// still two tasks at agenda, `None` and `Some`.
		assert!(Agenda::<Test>::get(when).len() == 2);
		// cancel last task from `when` agenda.
		assert_ok!(Scheduler::do_cancel(None, address2));
		// if all tasks `None`, agenda fully removed.
		assert!(Agenda::<Test>::get(when).len() == 0);
	});
}

#[test]
fn cancel_named_last_task_removes_agenda() {
	new_test_ext().execute_with(|| {
		let when = 4;
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		Scheduler::do_schedule_named(
			[1u8; 32],
			DispatchTime::At(when),
			None,
			127,
			root(),
			Preimage::bound(call.clone()).unwrap(),
		)
		.unwrap();
		Scheduler::do_schedule_named(
			[2u8; 32],
			DispatchTime::At(when),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();
		// two tasks at agenda.
		assert!(Agenda::<Test>::get(when).len() == 2);
		assert_ok!(Scheduler::do_cancel_named(None, [2u8; 32]));
		// removes trailing `None` and leaves one task.
		assert!(Agenda::<Test>::get(when).len() == 1);
		// cancel last task from `when` agenda.
		assert_ok!(Scheduler::do_cancel_named(None, [1u8; 32]));
		// if all tasks `None`, agenda fully removed.
		assert!(Agenda::<Test>::get(when).len() == 0);
	});
}

#[test]
fn reschedule_last_task_removes_agenda() {
	new_test_ext().execute_with(|| {
		let when = 4;
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let address = Scheduler::do_schedule(
			DispatchTime::At(when),
			None,
			127,
			root(),
			Preimage::bound(call.clone()).unwrap(),
		)
		.unwrap();
		let address2 = Scheduler::do_schedule(
			DispatchTime::At(when),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();
		// two tasks at agenda.
		assert!(Agenda::<Test>::get(when).len() == 2);
		assert_ok!(Scheduler::do_cancel(None, address));
		// still two tasks at agenda, `None` and `Some`.
		assert!(Agenda::<Test>::get(when).len() == 2);
		// reschedule last task from `when` agenda.
		assert_eq!(
			Scheduler::do_reschedule(address2, DispatchTime::At(when + 1)).unwrap(),
			(when + 1, 0)
		);
		// if all tasks `None`, agenda fully removed.
		assert!(Agenda::<Test>::get(when).len() == 0);
	});
}

#[test]
fn reschedule_named_last_task_removes_agenda() {
	new_test_ext().execute_with(|| {
		let when = 4;
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		Scheduler::do_schedule_named(
			[1u8; 32],
			DispatchTime::At(when),
			None,
			127,
			root(),
			Preimage::bound(call.clone()).unwrap(),
		)
		.unwrap();
		Scheduler::do_schedule_named(
			[2u8; 32],
			DispatchTime::At(when),
			None,
			127,
			root(),
			Preimage::bound(call).unwrap(),
		)
		.unwrap();
		// two tasks at agenda.
		assert!(Agenda::<Test>::get(when).len() == 2);
		assert_ok!(Scheduler::do_cancel_named(None, [1u8; 32]));
		// still two tasks at agenda, `None` and `Some`.
		assert!(Agenda::<Test>::get(when).len() == 2);
		// reschedule last task from `when` agenda.
		assert_eq!(
			Scheduler::do_reschedule_named([2u8; 32], DispatchTime::At(when + 1)).unwrap(),
			(when + 1, 0)
		);
		// if all tasks `None`, agenda fully removed.
		assert!(Agenda::<Test>::get(when).len() == 0);
	});
}

/// Ensures that an unavailable call sends an event.
#[test]
fn unavailable_call_is_detected() {
	use frame_support::traits::schedule::v3::Named;

	new_test_ext().execute_with(|| {
		let call =
			RuntimeCall::Logger(LoggerCall::log { i: 42, weight: Weight::from_parts(10, 0) });
		let hash = <Test as frame_system::Config>::Hashing::hash_of(&call);
		let len = call.using_encoded(|x| x.len()) as u32;
		// Important to use here `Bounded::Lookup` to ensure that we request the hash.
		let bound = Bounded::Lookup { hash, len };

		let name = [1u8; 32];

		// Schedule a call.
		let _address = <Scheduler as Named<_, _, _>>::schedule_named(
			name,
			DispatchTime::At(4),
			None,
			127,
			root(),
			bound.clone(),
		)
		.unwrap();

		// Ensure the preimage isn't available
		assert!(!Preimage::have(&bound));
		// But we have requested it
		assert!(Preimage::is_requested(&hash));

		// Executes in block 4.
		run_to_block(4);

		assert_eq!(
			System::events().last().unwrap().event,
			crate::Event::CallUnavailable { task: (4, 0), id: Some(name) }.into()
		);
		// It should not be requested anymore.
		assert!(!Preimage::is_requested(&hash));
	});
}
