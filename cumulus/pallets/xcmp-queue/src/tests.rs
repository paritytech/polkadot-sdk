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

use super::{mock::EnqueuedMessages, *};
use XcmpMessageFormat::*;

use cumulus_primitives_core::XcmpMessageHandler;
use frame_support::{assert_noop, assert_ok};
use mock::{new_test_ext, RuntimeOrigin as Origin, Test, XcmpQueue};
use sp_runtime::traits::BadOrigin;
use std::iter::once;

#[test]
fn empty_concatenated_works() {
	new_test_ext().execute_with(|| {
		let data = ConcatenatedVersionedXcm.encode();

		XcmpQueue::handle_xcmp_messages(once((1000.into(), 1, data.as_slice())), Weight::MAX);
	})
}

#[test]
fn xcm_enqueueing_basic_works() {
	new_test_ext().execute_with(|| {
		let xcm = VersionedXcm::<Test>::from(Xcm::<Test>(vec![ClearOrigin])).encode();
		let data = [ConcatenatedVersionedXcm.encode(), xcm.clone()].concat();

		XcmpQueue::handle_xcmp_messages(once((1000.into(), 1, data.as_slice())), Weight::MAX);

		assert_eq!(EnqueuedMessages::get(), vec![(1000.into(), xcm)]);
	})
}

#[test]
fn xcm_enqueueing_many_works() {
	new_test_ext().execute_with(|| {
		let mut encoded_xcms = vec![];
		for i in 0..10 {
			let xcm = VersionedXcm::<Test>::from(Xcm::<Test>(vec![ClearOrigin; i as usize]));
			encoded_xcms.push(xcm.encode());
		}
		let mut data = ConcatenatedVersionedXcm.encode();
		data.extend(encoded_xcms.iter().flatten());

		XcmpQueue::handle_xcmp_messages(once((1000.into(), 1, data.as_slice())), Weight::MAX);

		assert_eq!(
			EnqueuedMessages::get(),
			encoded_xcms.into_iter().map(|xcm| (1000.into(), xcm)).collect::<Vec<_>>(),
		);
	})
}

#[test]
fn xcm_enqueueing_multiple_times_works() {
	new_test_ext().execute_with(|| {
		let mut encoded_xcms = vec![];
		for i in 0..10 {
			let xcm = VersionedXcm::<Test>::from(Xcm::<Test>(vec![ClearOrigin; i as usize]));
			encoded_xcms.push(xcm.encode());
		}
		let mut data = ConcatenatedVersionedXcm.encode();
		data.extend(encoded_xcms.iter().flatten());

		for i in 0..10 {
			XcmpQueue::handle_xcmp_messages(once((1000.into(), 1, data.as_slice())), Weight::MAX);
			assert_eq!((i + 1) * 10, EnqueuedMessages::get().len());
		}

		assert_eq!(
			EnqueuedMessages::get(),
			encoded_xcms
				.into_iter()
				.map(|xcm| (1000.into(), xcm))
				.cycle()
				.take(100)
				.collect::<Vec<_>>(),
		);
	})
}

/// Message blobs are not supported and panic in debug mode.
#[test]
#[should_panic = "Blob messages not handled"]
#[cfg(debug_assertions)]
fn bad_blob_message_panics() {
	new_test_ext().execute_with(|| {
		let data = [ConcatenatedEncodedBlob.encode(), vec![1].encode()].concat();

		XcmpQueue::handle_xcmp_messages(once((1000.into(), 1, data.as_slice())), Weight::MAX);
	});
}

/// Message blobs do not panic in release mode but are just a No-OP.
#[test]
#[cfg(not(debug_assertions))]
fn bad_blob_message_no_panic() {
	new_test_ext().execute_with(|| {
		let data = [ConcatenatedEncodedBlob.encode(), vec![1].encode()].concat();

		frame_support::assert_storage_noop!(XcmpQueue::handle_xcmp_messages(
			once((1000.into(), 1, data.as_slice())),
			Weight::MAX,
		));
	});
}

/// Invalid concatenated XCMs panic in debug mode.
#[test]
#[should_panic = "Could not parse incoming XCMP messages."]
#[cfg(debug_assertions)]
fn handle_invalid_data_panics() {
	new_test_ext().execute_with(|| {
		let data = [ConcatenatedVersionedXcm.encode(), Xcm::<Test>(vec![]).encode()].concat();

		XcmpQueue::handle_xcmp_messages(once((1000.into(), 1, data.as_slice())), Weight::MAX);
	});
}

/// Invalid concatenated XCMs do not panic in release mode but are just a No-OP.
#[test]
#[cfg(not(debug_assertions))]
fn handle_invalid_data_no_panic() {
	new_test_ext().execute_with(|| {
		let data = [ConcatenatedVersionedXcm.encode(), Xcm::<Test>(vec![]).encode()].concat();

		frame_support::assert_storage_noop!(XcmpQueue::handle_xcmp_messages(
			once((1000.into(), 1, data.as_slice())),
			Weight::MAX,
		));
	});
}

#[test]
fn suspend_xcm_execution_works() {
	new_test_ext().execute_with(|| {
		QueueSuspended::<Test>::put(true);

		let xcm = VersionedXcm::<Test>::from(Xcm::<Test>(vec![ClearOrigin])).encode();
		let data = [ConcatenatedVersionedXcm.encode(), xcm.clone()].concat();

		// This should have executed the incoming XCM, because it came from a system parachain
		XcmpQueue::handle_xcmp_messages(once((999.into(), 1, data.as_slice())), Weight::MAX);

		// This should have queue instead of executing since it comes from a sibling.
		XcmpQueue::handle_xcmp_messages(once((2000.into(), 1, data.as_slice())), Weight::MAX);

		let queued_xcm = mock::enqueued_messages(ParaId::from(2000));
		assert_eq!(queued_xcm, vec![xcm]);
	});
}

#[test]
fn suspend_and_resume_xcm_execution_work() {
	new_test_ext().execute_with(|| {
		assert_noop!(XcmpQueue::suspend_xcm_execution(Origin::signed(1)), BadOrigin);
		assert_ok!(XcmpQueue::suspend_xcm_execution(Origin::root()));
		assert_noop!(
			XcmpQueue::suspend_xcm_execution(Origin::root()),
			Error::<Test>::AlreadySuspended
		);
		assert!(QueueSuspended::<Test>::get());

		assert_noop!(XcmpQueue::resume_xcm_execution(Origin::signed(1)), BadOrigin);
		assert_ok!(XcmpQueue::resume_xcm_execution(Origin::root()));
		assert_noop!(
			XcmpQueue::resume_xcm_execution(Origin::root()),
			Error::<Test>::AlreadyResumed
		);
		assert!(!QueueSuspended::<Test>::get());
	});
}

// FAIL-CI test back-pressure
#[test]
fn update_suspend_threshold_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(<QueueConfig<Test>>::get().suspend_threshold, 2048);
		assert_noop!(XcmpQueue::update_suspend_threshold(Origin::signed(2), 5), BadOrigin);

		assert_ok!(XcmpQueue::update_suspend_threshold(Origin::root(), 3000));
		assert_eq!(<QueueConfig<Test>>::get().suspend_threshold, 3000);
	});
}

#[test]
fn update_drop_threshold_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(<QueueConfig<Test>>::get().drop_threshold, 3096);
		assert_ok!(XcmpQueue::update_drop_threshold(Origin::root(), 4000));
		assert_noop!(XcmpQueue::update_drop_threshold(Origin::signed(2), 7), BadOrigin);

		assert_eq!(<QueueConfig<Test>>::get().drop_threshold, 4000);
	});
}

#[test]
fn update_resume_threshold_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(<QueueConfig<Test>>::get().resume_threshold, 1024);
		assert_noop!(
			XcmpQueue::update_resume_threshold(Origin::root(), 0),
			Error::<Test>::BadQueueConfig
		);
		assert_ok!(XcmpQueue::update_resume_threshold(Origin::root(), 110));
		assert_noop!(XcmpQueue::update_resume_threshold(Origin::signed(7), 3), BadOrigin);

		assert_eq!(<QueueConfig<Test>>::get().resume_threshold, 110);
	});
}

/// Validates [`validate`] for required Some(destination) and Some(message)
struct OkFixedXcmHashWithAssertingRequiredInputsSender;
impl OkFixedXcmHashWithAssertingRequiredInputsSender {
	const FIXED_XCM_HASH: [u8; 32] = [9; 32];

	fn fixed_delivery_asset() -> MultiAssets {
		MultiAssets::new()
	}

	fn expected_delivery_result() -> Result<(XcmHash, MultiAssets), SendError> {
		Ok((Self::FIXED_XCM_HASH, Self::fixed_delivery_asset()))
	}
}
impl SendXcm for OkFixedXcmHashWithAssertingRequiredInputsSender {
	type Ticket = ();

	fn validate(
		destination: &mut Option<MultiLocation>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		assert!(destination.is_some());
		assert!(message.is_some());
		Ok(((), OkFixedXcmHashWithAssertingRequiredInputsSender::fixed_delivery_asset()))
	}

	fn deliver(_: Self::Ticket) -> Result<XcmHash, SendError> {
		Ok(Self::FIXED_XCM_HASH)
	}
}

#[test]
fn xcmp_queue_does_not_consume_dest_or_msg_on_not_applicable() {
	// dummy message
	let message = Xcm(vec![Trap(5)]);

	// XcmpQueue - check dest is really not applicable
	let dest = (Parent, Parent, Parent);
	let mut dest_wrapper = Some(dest.into());
	let mut msg_wrapper = Some(message.clone());
	assert_eq!(
		Err(SendError::NotApplicable),
		<XcmpQueue as SendXcm>::validate(&mut dest_wrapper, &mut msg_wrapper)
	);

	// check wrapper were not consumed
	assert_eq!(Some(dest.into()), dest_wrapper.take());
	assert_eq!(Some(message.clone()), msg_wrapper.take());

	// another try with router chain with asserting sender
	assert_eq!(
		OkFixedXcmHashWithAssertingRequiredInputsSender::expected_delivery_result(),
		send_xcm::<(XcmpQueue, OkFixedXcmHashWithAssertingRequiredInputsSender)>(
			dest.into(),
			message
		)
	);
}

#[test]
fn xcmp_queue_consumes_dest_and_msg_on_ok_validate() {
	// dummy message
	let message = Xcm(vec![Trap(5)]);

	// XcmpQueue - check dest/msg is valid
	let dest = (Parent, X1(Parachain(5555)));
	let mut dest_wrapper = Some(dest.into());
	let mut msg_wrapper = Some(message.clone());
	assert!(<XcmpQueue as SendXcm>::validate(&mut dest_wrapper, &mut msg_wrapper).is_ok());

	// check wrapper were consumed
	assert_eq!(None, dest_wrapper.take());
	assert_eq!(None, msg_wrapper.take());

	new_test_ext().execute_with(|| {
		// another try with router chain with asserting sender
		assert_eq!(
			Err(SendError::Transport("NoChannel")),
			send_xcm::<(XcmpQueue, OkFixedXcmHashWithAssertingRequiredInputsSender)>(
				dest.into(),
				message
			)
		);
	});
}
