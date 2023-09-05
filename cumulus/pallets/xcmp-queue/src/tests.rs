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

use super::{
	mock::{mk_page, v2_xcm, v3_xcm, EnqueuedMessages, MAGIC_PARA_ID},
	*,
};
use XcmpMessageFormat::*;

use codec::Compact;
use cumulus_primitives_core::XcmpMessageHandler;
use frame_support::{assert_err, assert_noop, assert_ok, experimental_hypothetically};
use mock::{new_test_ext, RuntimeOrigin as Origin, Test, XcmpQueue};
use sp_runtime::traits::BadOrigin;
use std::iter::{once, repeat};

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
		for _ in 0..10 {
			let xcm = VersionedXcm::<Test>::from(Xcm::<Test>(vec![ClearOrigin]));
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

#[test]
#[cfg_attr(debug_assertions, should_panic = "Defensive failure")]
fn xcm_enqueueing_starts_dropping_on_overflow() {
	new_test_ext().execute_with(|| {
		let xcm = VersionedXcm::<Test>::from(Xcm::<Test>(vec![ClearOrigin]));
		let data = (ConcatenatedVersionedXcm, xcm).encode();
		// Its possible to enqueue 256 messages at most:
		let limit = 256;

		XcmpQueue::handle_xcmp_messages(
			repeat((1000.into(), 1, data.as_slice())).take(limit * 2),
			Weight::MAX,
		);
		assert_eq!(EnqueuedMessages::get().len(), limit);
		// The drop threshold for pages is 48, the others numbers dont really matter:
		assert_eq!(
			<Test as Config>::XcmpQueue::footprint(1000.into()),
			Footprint { count: 256, size: 768, pages: 48 }
		);
	})
}

/// First enqueue 10 good, 1 bad and then 10 good XCMs.
///
/// It should only process the first 10 good though.
#[test]
#[cfg(not(debug_assertions))]
fn xcm_enqueueing_broken_xcm_works() {
	new_test_ext().execute_with(|| {
		let mut encoded_xcms = vec![];
		for _ in 0..10 {
			let xcm = VersionedXcm::<Test>::from(Xcm::<Test>(vec![ClearOrigin]));
			encoded_xcms.push(xcm.encode());
		}
		let mut good = ConcatenatedVersionedXcm.encode();
		good.extend(encoded_xcms.iter().flatten());

		let mut bad = ConcatenatedVersionedXcm.encode();
		bad.extend(vec![0u8].into_iter());

		// Of we enqueue them in multiple pages, then its fine.
		XcmpQueue::handle_xcmp_messages(once((1000.into(), 1, good.as_slice())), Weight::MAX);
		XcmpQueue::handle_xcmp_messages(once((1000.into(), 1, bad.as_slice())), Weight::MAX);
		XcmpQueue::handle_xcmp_messages(once((1000.into(), 1, good.as_slice())), Weight::MAX);
		assert_eq!(20, EnqueuedMessages::get().len());

		assert_eq!(
			EnqueuedMessages::get(),
			encoded_xcms
				.clone()
				.into_iter()
				.map(|xcm| (1000.into(), xcm))
				.cycle()
				.take(20)
				.collect::<Vec<_>>(),
		);
		EnqueuedMessages::set(&vec![]);

		// But if we do it all in one page, then it only uses the first 10:
		XcmpQueue::handle_xcmp_messages(
			vec![(1000.into(), 1, good.as_slice()), (1000.into(), 1, bad.as_slice())].into_iter(),
			Weight::MAX,
		);
		assert_eq!(10, EnqueuedMessages::get().len());
		assert_eq!(
			EnqueuedMessages::get(),
			encoded_xcms
				.into_iter()
				.map(|xcm| (1000.into(), xcm))
				.cycle()
				.take(10)
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
#[should_panic = "HRMP inbound decode stream broke; page will be dropped."]
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

#[test]
#[cfg(not(debug_assertions))]
fn xcm_enqueueing_backpressure_works() {
	let para: ParaId = 1000.into();
	new_test_ext().execute_with(|| {
		let xcm = VersionedXcm::<Test>::from(Xcm::<Test>(vec![ClearOrigin]));
		let data = (ConcatenatedVersionedXcm, xcm).encode();

		XcmpQueue::handle_xcmp_messages(repeat((para, 1, data.as_slice())).take(170), Weight::MAX);

		assert_eq!(EnqueuedMessages::get().len(), 170,);
		// Not yet suspended:
		assert!(InboundXcmpSuspended::<Test>::get().is_empty());
		// Enqueueing one more will suspend it:
		let xcm = VersionedXcm::<Test>::from(Xcm::<Test>(vec![ClearOrigin])).encode();
		let small = [ConcatenatedVersionedXcm.encode(), xcm].concat();

		XcmpQueue::handle_xcmp_messages(once((para, 1, small.as_slice())), Weight::MAX);
		// Suspended:
		assert_eq!(InboundXcmpSuspended::<Test>::get().iter().collect::<Vec<_>>(), vec![&para]);

		// Now enqueueing many more will only work until the drop threshold:
		XcmpQueue::handle_xcmp_messages(repeat((para, 1, data.as_slice())).take(100), Weight::MAX);
		assert_eq!(mock::EnqueuedMessages::get().len(), 256);

		crate::mock::EnqueueToLocalStorage::<Pallet<Test>>::sweep_queue(para);
		XcmpQueue::handle_xcmp_messages(once((para, 1, small.as_slice())), Weight::MAX);
		// Got resumed:
		assert!(InboundXcmpSuspended::<Test>::get().is_empty());
		// Still resumed:
		XcmpQueue::handle_xcmp_messages(once((para, 1, small.as_slice())), Weight::MAX);
		assert!(InboundXcmpSuspended::<Test>::get().is_empty());
	});
}

#[test]
fn update_suspend_threshold_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(<QueueConfig<Test>>::get().suspend_threshold, 32);
		assert_noop!(XcmpQueue::update_suspend_threshold(Origin::signed(2), 49), BadOrigin);

		assert_ok!(XcmpQueue::update_suspend_threshold(Origin::root(), 33));
		assert_eq!(<QueueConfig<Test>>::get().suspend_threshold, 33);
	});
}

#[test]
fn update_drop_threshold_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(<QueueConfig<Test>>::get().drop_threshold, 48);
		assert_ok!(XcmpQueue::update_drop_threshold(Origin::root(), 4000));
		assert_noop!(XcmpQueue::update_drop_threshold(Origin::signed(2), 7), BadOrigin);

		assert_eq!(<QueueConfig<Test>>::get().drop_threshold, 4000);
	});
}

#[test]
fn update_resume_threshold_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(<QueueConfig<Test>>::get().resume_threshold, 8);
		assert_noop!(
			XcmpQueue::update_resume_threshold(Origin::root(), 0),
			Error::<Test>::BadQueueConfig
		);
		assert_noop!(
			XcmpQueue::update_resume_threshold(Origin::root(), 33),
			Error::<Test>::BadQueueConfig
		);
		assert_ok!(XcmpQueue::update_resume_threshold(Origin::root(), 16));
		assert_noop!(XcmpQueue::update_resume_threshold(Origin::signed(7), 3), BadOrigin);

		assert_eq!(<QueueConfig<Test>>::get().resume_threshold, 16);
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

#[test]
fn xcmp_queue_validate_nested_xcm_works() {
	let dest = (Parent, X1(Parachain(5555)));
	// Message that is not too deeply nested:
	let mut good = Xcm(vec![ClearOrigin]);
	for _ in 0..MAX_XCM_DECODE_DEPTH - 1 {
		good = Xcm(vec![SetAppendix(good)]);
	}

	// Check that the good message is validated:
	assert_ok!(<XcmpQueue as SendXcm>::validate(&mut Some(dest.into()), &mut Some(good.clone())));

	// Nesting the message one more time should reject it:
	let bad = Xcm(vec![SetAppendix(good)]);
	assert_eq!(
		Err(SendError::ExceedsMaxMessageSize),
		<XcmpQueue as SendXcm>::validate(&mut Some(dest.into()), &mut Some(bad))
	);
}

#[test]
fn send_xcm_nested_works() {
	let dest = (Parent, X1(Parachain(MAGIC_PARA_ID)));
	// Message that is not too deeply nested:
	let mut good = Xcm(vec![ClearOrigin]);
	for _ in 0..MAX_XCM_DECODE_DEPTH - 1 {
		good = Xcm(vec![SetAppendix(good)]);
	}

	// Check that the good message is sent:
	new_test_ext().execute_with(|| {
		assert_ok!(send_xcm::<XcmpQueue>(dest.into(), good.clone()));
		assert_eq!(
			XcmpQueue::take_outbound_messages(usize::MAX),
			vec![(
				MAGIC_PARA_ID.into(),
				(
					XcmpMessageFormat::ConcatenatedVersionedXcm,
					MaybeDoubleEncodedVersionedXcm(VersionedXcm::V3(good.clone()))
				)
					.encode(),
			)]
		);
	});

	// Nesting the message one more time should not send it:
	let bad = Xcm(vec![SetAppendix(good)]);
	new_test_ext().execute_with(|| {
		assert_err!(send_xcm::<XcmpQueue>(dest.into(), bad), SendError::ExceedsMaxMessageSize);
		assert!(XcmpQueue::take_outbound_messages(usize::MAX).is_empty());
	});
}

#[test]
fn hrmp_signals_are_prioritized() {
	let message = Xcm(vec![Trap(5)]);

	let dest = (Parent, X1(Parachain(MAGIC_PARA_ID)));
	let mut dest_wrapper = Some(dest.into());
	let mut msg_wrapper = Some(message.clone());
	<XcmpQueue as SendXcm>::validate(&mut dest_wrapper, &mut msg_wrapper).unwrap();

	// check wrapper were consumed
	assert_eq!(None, dest_wrapper.take());
	assert_eq!(None, msg_wrapper.take());

	new_test_ext().execute_with(|| {
		OutboundXcmpStatus::<Test>::set(vec![OutboundChannelDetails {
			recipient: MAGIC_PARA_ID.into(),
			state: OutboundState::Ok,
			signals_exist: false,
			first_index: 0,
			last_index: 0,
		}]);

		// Enqueue some messages
		for _ in 0..129 {
			send_xcm::<XcmpQueue>(dest.into(), message.clone()).unwrap();
		}

		// Without a signal we get the messages in order:
		let mut expected_msg = XcmpMessageFormat::ConcatenatedVersionedXcm.encode();
		for _ in 0..21 {
			expected_msg
				.extend(MaybeDoubleEncodedVersionedXcm(VersionedXcm::V3(message.clone())).encode());
		}

		experimental_hypothetically!({
			let taken = XcmpQueue::take_outbound_messages(130);
			assert_eq!(taken, vec![(MAGIC_PARA_ID.into(), expected_msg,)]);
		});

		// But a signal gets prioritized instead of the messages:
		XcmpQueue::send_signal(MAGIC_PARA_ID.into(), ChannelSignal::Suspend).unwrap();

		let taken = XcmpQueue::take_outbound_messages(130);
		assert_eq!(
			taken,
			vec![(
				MAGIC_PARA_ID.into(),
				(XcmpMessageFormat::Signals, ChannelSignal::Suspend).encode()
			)]
		);
	});
}

#[test]
fn maybe_double_encoded_versioned_xcm_works() {
	// pre conditions
	assert_eq!(VersionedXcm::<()>::V2(Default::default()).encode(), &[2, 0]);
	assert_eq!(VersionedXcm::<()>::V3(Default::default()).encode(), &[3, 0]);
}

#[test]
fn maybe_double_encoded_versioned_xcm_version_works() {
	let v2 = VersionedXcm::<()>::V2(Default::default());
	let dv2 = MaybeDoubleEncodedVersionedXcm(v2);
	// Encoding is `[Magic=0, XcmLen=Compact(3), XcmVersion=2, VecLen=0]`.
	let mut v = vec![0u8];
	Compact::<u32>(2).using_encoded(|c| v.extend(c));
	v.extend(&[2, 0u8]);
	assert_eq!(dv2.encode(), v);

	let v3 = VersionedXcm::<()>::V3(Default::default());
	let dv3 = MaybeDoubleEncodedVersionedXcm(v3);
	let mut v = vec![0u8];
	Compact::<u32>(2).using_encoded(|c| v.extend(c));
	v.extend(&[3, 0u8]);
	assert_eq!(dv3.encode(), v);
}

#[test]
fn maybe_double_encoded_versioned_xcm_decode_works() {
	// `Maybe` decodes from XCM v2
	let buff = v2_xcm().encode();
	let xcm = MaybeDoubleEncodedVersionedXcm::decode(&mut &buff[..]).unwrap();
	assert_eq!(xcm, Either::Left(v2_xcm()));

	// `Maybe` decodes from XCM v3
	let buff = v3_xcm().encode();
	let xcm = MaybeDoubleEncodedVersionedXcm::decode(&mut &buff[..]).unwrap();
	assert_eq!(xcm, Either::Left(v3_xcm()));

	// `Maybe` decodes from `Maybe` v2
	let buff = MaybeDoubleEncodedVersionedXcm(v2_xcm()).encode();
	let xcm = MaybeDoubleEncodedVersionedXcm::decode(&mut &buff[..]).unwrap();
	let Either::Right(mut double) = xcm else {
		panic!();
	};
	let xcm = double.take_decoded().unwrap();
	assert_eq!(xcm, v2_xcm());

	// `Maybe` decodes from `Maybe` v3
	let buff = MaybeDoubleEncodedVersionedXcm(v3_xcm()).encode();
	let xcm = MaybeDoubleEncodedVersionedXcm::decode(&mut &buff[..]).unwrap();
	let Either::Right(mut double) = xcm else {
		panic!();
	};
	let xcm = double.take_decoded().unwrap();
	assert_eq!(xcm, v3_xcm());
}

// Now also testing a page instead of just concat messages.
#[test]
fn maybe_double_encoded_versioned_xcm_decode_page_works() {
	let page = mk_page();

	// Now try to decode the page.
	let input = &mut &page[..];
	for i in 0..100 {
		match (i % 5, MaybeDoubleEncodedVersionedXcm::decode(input)) {
			(0, Ok(Either::Left(xcm))) => {
				assert_eq!(xcm, v2_xcm());
			},
			(1, Ok(Either::Left(xcm))) => {
				assert_eq!(xcm, v3_xcm());
			},
			(2, Ok(Either::Right(mut double))) => {
				assert_eq!(double.take_decoded().unwrap(), v2_xcm());
			},
			(3, Ok(Either::Right(mut double))) => {
				assert_eq!(double.take_decoded().unwrap(), v3_xcm());
			},
			(4, Ok(Either::Right(mut double))) => {
				// A decoding error does *not* break the stream.
				assert!(double.take_decoded().is_err());
			},
			unexpected => unreachable!("{:?}", unexpected),
		}
	}

	assert_eq!(input.remaining_len(), Ok(Some(0)), "All data consumed");
}

#[test]
fn split_concatenated_xcms_works() {
	let page = mk_page();
	let input = &mut &page[..];

	for i in 0..100 {
		let xcm = XcmpQueue::split_concatenated_xcms(input, &mut WeightMeter::new()).unwrap();
		match (i % 5, xcm) {
			// The `MaybeDoubleEncodedVersionedXcm` gets flattened:
			(0, data) | (2, data) => {
				assert_eq!(data, v2_xcm().encode());
			},
			(1, data) | (3, data) => {
				assert_eq!(data, v3_xcm().encode());
			},
			(4, data) => {
				let xcm = VersionedXcm::decode(&mut data.into_inner().as_ref()).unwrap();
				assert!(crate::validate_xcm_nesting(&xcm).is_err(), "precondition");
			},
			unexpected => unreachable!("{:?}", unexpected),
		}
	}
	assert_eq!(input.remaining_len(), Ok(Some(0)), "All data consumed");
}
