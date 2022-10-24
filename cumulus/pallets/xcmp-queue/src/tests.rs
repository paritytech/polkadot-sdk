// Copyright (C) 2021 Parity Technologies (UK) Ltd.
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

use super::*;
use cumulus_primitives_core::XcmpMessageHandler;
use frame_support::{assert_noop, assert_ok};
use mock::{new_test_ext, RuntimeCall, RuntimeOrigin, Test, XcmpQueue};
use sp_runtime::traits::BadOrigin;

#[test]
fn one_message_does_not_panic() {
	new_test_ext().execute_with(|| {
		let message_format = XcmpMessageFormat::ConcatenatedVersionedXcm.encode();
		let messages = vec![(Default::default(), 1u32.into(), message_format.as_slice())];

		// This shouldn't cause a panic
		XcmpQueue::handle_xcmp_messages(messages.into_iter(), Weight::MAX);
	})
}

#[test]
#[should_panic = "Invalid incoming blob message data"]
#[cfg(debug_assertions)]
fn bad_message_is_handled() {
	new_test_ext().execute_with(|| {
		let bad_data = vec![
			1, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 64, 239, 139, 0,
			0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0, 0, 0, 0, 37, 0,
			0, 0, 0, 0, 0, 0, 16, 0, 127, 147,
		];
		InboundXcmpMessages::<Test>::insert(ParaId::from(1000), 1, bad_data);
		let format = XcmpMessageFormat::ConcatenatedEncodedBlob;
		// This should exit with an error.
		XcmpQueue::process_xcmp_message(
			1000.into(),
			(1, format),
			Weight::from_ref_time(10_000_000_000),
			Weight::from_ref_time(10_000_000_000),
		);
	});
}

/// Tests that a blob message is handled. Currently this isn't implemented and panics when debug assertions
/// are enabled. When this feature is enabled, this test should be rewritten properly.
#[test]
#[should_panic = "Blob messages not handled."]
#[cfg(debug_assertions)]
fn handle_blob_message() {
	new_test_ext().execute_with(|| {
		let bad_data = vec![
			1, 1, 1, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 64, 239,
			139, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0, 0, 0, 0,
			37, 0, 0, 0, 0, 0, 0, 0, 16, 0, 127, 147,
		];
		InboundXcmpMessages::<Test>::insert(ParaId::from(1000), 1, bad_data);
		let format = XcmpMessageFormat::ConcatenatedEncodedBlob;
		XcmpQueue::process_xcmp_message(
			1000.into(),
			(1, format),
			Weight::from_ref_time(10_000_000_000),
			Weight::from_ref_time(10_000_000_000),
		);
	});
}

#[test]
#[should_panic = "Invalid incoming XCMP message data"]
#[cfg(debug_assertions)]
fn handle_invalid_data() {
	new_test_ext().execute_with(|| {
		let data = Xcm::<Test>(vec![]).encode();
		InboundXcmpMessages::<Test>::insert(ParaId::from(1000), 1, data);
		let format = XcmpMessageFormat::ConcatenatedVersionedXcm;
		XcmpQueue::process_xcmp_message(
			1000.into(),
			(1, format),
			Weight::from_ref_time(10_000_000_000),
			Weight::from_ref_time(10_000_000_000),
		);
	});
}

#[test]
fn service_overweight_unknown() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			XcmpQueue::service_overweight(RuntimeOrigin::root(), 0, 1000),
			Error::<Test>::BadOverweightIndex,
		);
	});
}

#[test]
fn service_overweight_bad_xcm_format() {
	new_test_ext().execute_with(|| {
		let bad_xcm = vec![255];
		Overweight::<Test>::insert(0, (ParaId::from(1000), 0, bad_xcm));

		assert_noop!(
			XcmpQueue::service_overweight(RuntimeOrigin::root(), 0, 1000),
			Error::<Test>::BadXcm
		);
	});
}

#[test]
fn suspend_xcm_execution_works() {
	new_test_ext().execute_with(|| {
		QueueSuspended::<Test>::put(true);

		let xcm =
			VersionedXcm::from(Xcm::<RuntimeCall>(vec![Instruction::<RuntimeCall>::ClearOrigin]))
				.encode();
		let mut message_format = XcmpMessageFormat::ConcatenatedVersionedXcm.encode();
		message_format.extend(xcm.clone());
		let messages = vec![(ParaId::from(999), 1u32.into(), message_format.as_slice())];

		// This should have executed the incoming XCM, because it came from a system parachain
		XcmpQueue::handle_xcmp_messages(messages.into_iter(), Weight::MAX);

		let queued_xcm = InboundXcmpMessages::<Test>::get(ParaId::from(999), 1u32);
		assert!(queued_xcm.is_empty());

		let messages = vec![(ParaId::from(2000), 1u32.into(), message_format.as_slice())];

		// This shouldn't have executed the incoming XCM
		XcmpQueue::handle_xcmp_messages(messages.into_iter(), Weight::MAX);

		let queued_xcm = InboundXcmpMessages::<Test>::get(ParaId::from(2000), 1u32);
		assert_eq!(queued_xcm, xcm);
	});
}

#[test]
fn update_suspend_threshold_works() {
	new_test_ext().execute_with(|| {
		let data: QueueConfigData = <QueueConfig<Test>>::get();
		assert_eq!(data.suspend_threshold, 2);
		assert_ok!(XcmpQueue::update_suspend_threshold(RuntimeOrigin::root(), 3));
		assert_noop!(XcmpQueue::update_suspend_threshold(RuntimeOrigin::signed(2), 5), BadOrigin);
		let data: QueueConfigData = <QueueConfig<Test>>::get();

		assert_eq!(data.suspend_threshold, 3);
	});
}

#[test]
fn update_drop_threshold_works() {
	new_test_ext().execute_with(|| {
		let data: QueueConfigData = <QueueConfig<Test>>::get();
		assert_eq!(data.drop_threshold, 5);
		assert_ok!(XcmpQueue::update_drop_threshold(RuntimeOrigin::root(), 6));
		assert_noop!(XcmpQueue::update_drop_threshold(RuntimeOrigin::signed(2), 7), BadOrigin);
		let data: QueueConfigData = <QueueConfig<Test>>::get();

		assert_eq!(data.drop_threshold, 6);
	});
}

#[test]
fn update_resume_threshold_works() {
	new_test_ext().execute_with(|| {
		let data: QueueConfigData = <QueueConfig<Test>>::get();
		assert_eq!(data.resume_threshold, 1);
		assert_ok!(XcmpQueue::update_resume_threshold(RuntimeOrigin::root(), 2));
		assert_noop!(XcmpQueue::update_resume_threshold(RuntimeOrigin::signed(7), 3), BadOrigin);
		let data: QueueConfigData = <QueueConfig<Test>>::get();

		assert_eq!(data.resume_threshold, 2);
	});
}

#[test]
fn update_threshold_weight_works() {
	new_test_ext().execute_with(|| {
		let data: QueueConfigData = <QueueConfig<Test>>::get();
		assert_eq!(data.threshold_weight, Weight::from_ref_time(100_000));
		assert_ok!(XcmpQueue::update_threshold_weight(RuntimeOrigin::root(), 10_000));
		assert_noop!(
			XcmpQueue::update_threshold_weight(RuntimeOrigin::signed(5), 10_000_000),
			BadOrigin
		);
		let data: QueueConfigData = <QueueConfig<Test>>::get();

		assert_eq!(data.threshold_weight, Weight::from_ref_time(10_000));
	});
}

#[test]
fn update_weight_restrict_decay_works() {
	new_test_ext().execute_with(|| {
		let data: QueueConfigData = <QueueConfig<Test>>::get();
		assert_eq!(data.weight_restrict_decay, Weight::from_ref_time(2));
		assert_ok!(XcmpQueue::update_weight_restrict_decay(RuntimeOrigin::root(), 5));
		assert_noop!(
			XcmpQueue::update_weight_restrict_decay(RuntimeOrigin::signed(6), 4),
			BadOrigin
		);
		let data: QueueConfigData = <QueueConfig<Test>>::get();

		assert_eq!(data.weight_restrict_decay, Weight::from_ref_time(5));
	});
}

#[test]
fn update_xcmp_max_individual_weight() {
	new_test_ext().execute_with(|| {
		let data: QueueConfigData = <QueueConfig<Test>>::get();
		assert_eq!(
			data.xcmp_max_individual_weight,
			Weight::from_parts(20u64 * WEIGHT_PER_MILLIS.ref_time(), DEFAULT_POV_SIZE),
		);
		assert_ok!(XcmpQueue::update_xcmp_max_individual_weight(
			RuntimeOrigin::root(),
			30u64 * WEIGHT_PER_MILLIS.ref_time()
		));
		assert_noop!(
			XcmpQueue::update_xcmp_max_individual_weight(
				RuntimeOrigin::signed(3),
				10u64 * WEIGHT_PER_MILLIS.ref_time()
			),
			BadOrigin
		);
		let data: QueueConfigData = <QueueConfig<Test>>::get();

		assert_eq!(data.xcmp_max_individual_weight, 30u64 * WEIGHT_PER_MILLIS);
	});
}
