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
	mock::{mk_page, v2_xcm, v3_xcm, EnqueuedMessages, HRMP_PARA_ID},
	*,
};
use XcmpMessageFormat::*;

use codec::Input;
use cumulus_primitives_core::{ParaId, XcmpMessageHandler};
use frame_support::{
	assert_err, assert_noop, assert_ok, assert_storage_noop, hypothetically, traits::Hooks,
	StorageNoopGuard,
};
use mock::{new_test_ext, ParachainSystem, RuntimeOrigin as Origin, Test, XcmpQueue};
use sp_runtime::traits::{BadOrigin, Zero};
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
#[cfg_attr(debug_assertions, should_panic = "Could not enqueue XCMP messages.")]
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
		EnqueuedMessages::take();

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
#[should_panic = "Blob messages are unhandled"]
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
		assert!(!XcmpQueue::is_paused(&2000.into()));
		QueueSuspended::<Test>::put(true);
		assert!(XcmpQueue::is_paused(&2000.into()));
		// System parachains can bypass suspension:
		assert!(!XcmpQueue::is_paused(&999.into()));
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

	fn fixed_delivery_asset() -> Assets {
		Assets::new()
	}

	fn expected_delivery_result() -> Result<(XcmHash, Assets), SendError> {
		Ok((Self::FIXED_XCM_HASH, Self::fixed_delivery_asset()))
	}
}
impl SendXcm for OkFixedXcmHashWithAssertingRequiredInputsSender {
	type Ticket = ();

	fn validate(
		destination: &mut Option<Location>,
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
	let dest: Location = (Parent, Parachain(5555)).into();
	let mut dest_wrapper = Some(dest.clone());
	let mut msg_wrapper = Some(message.clone());

	new_test_ext().execute_with(|| {
		assert!(<XcmpQueue as SendXcm>::validate(&mut dest_wrapper, &mut msg_wrapper).is_ok());

		// check wrapper were consumed
		assert_eq!(None, dest_wrapper.take());
		assert_eq!(None, msg_wrapper.take());

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
	let dest = (Parent, Parachain(5555));
	// Message that is not too deeply nested:
	let mut good = Xcm(vec![ClearOrigin]);
	for _ in 0..MAX_XCM_DECODE_DEPTH - 1 {
		good = Xcm(vec![SetAppendix(good)]);
	}

	new_test_ext().execute_with(|| {
		// Check that the good message is validated:
		assert_ok!(<XcmpQueue as SendXcm>::validate(
			&mut Some(dest.into()),
			&mut Some(good.clone())
		));

		// Nesting the message one more time should reject it:
		let bad = Xcm(vec![SetAppendix(good)]);
		assert_eq!(
			Err(SendError::ExceedsMaxMessageSize),
			<XcmpQueue as SendXcm>::validate(&mut Some(dest.into()), &mut Some(bad))
		);
	});
}

#[test]
fn send_xcm_nested_works() {
	let dest = (Parent, Parachain(HRMP_PARA_ID));
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
				HRMP_PARA_ID.into(),
				(XcmpMessageFormat::ConcatenatedVersionedXcm, VersionedXcm::V4(good.clone()))
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

	let sibling_para_id = ParaId::from(12345);
	let dest = (Parent, Parachain(sibling_para_id.into()));
	let mut dest_wrapper = Some(dest.into());
	let mut msg_wrapper = Some(message.clone());

	new_test_ext().execute_with(|| {
		frame_system::Pallet::<Test>::set_block_number(1);
		<XcmpQueue as SendXcm>::validate(&mut dest_wrapper, &mut msg_wrapper).unwrap();

		// check wrapper were consumed
		assert_eq!(None, dest_wrapper.take());
		assert_eq!(None, msg_wrapper.take());

		ParachainSystem::open_custom_outbound_hrmp_channel_for_benchmarks_or_tests(
			sibling_para_id,
			cumulus_primitives_core::AbridgedHrmpChannel {
				max_capacity: 128,
				max_total_size: 1 << 16,
				max_message_size: 128,
				msg_count: 0,
				total_size: 0,
				mqc_head: None,
			},
		);

		let taken = XcmpQueue::take_outbound_messages(130);
		assert_eq!(taken, vec![]);

		// Enqueue some messages
		let num_events = frame_system::Pallet::<Test>::events().len();
		for _ in 0..256 {
			assert_ok!(send_xcm::<XcmpQueue>(dest.into(), message.clone()));
		}
		assert_eq!(num_events + 256, frame_system::Pallet::<Test>::events().len());

		// Without a signal we get the messages in order:
		let mut expected_msg = XcmpMessageFormat::ConcatenatedVersionedXcm.encode();
		for _ in 0..31 {
			expected_msg.extend(VersionedXcm::V4(message.clone()).encode());
		}

		hypothetically!({
			let taken = XcmpQueue::take_outbound_messages(usize::MAX);
			assert_eq!(taken, vec![(sibling_para_id.into(), expected_msg,)]);
		});

		// But a signal gets prioritized instead of the messages:
		XcmpQueue::send_signal(sibling_para_id.into(), ChannelSignal::Suspend);

		let taken = XcmpQueue::take_outbound_messages(130);
		assert_eq!(
			taken,
			vec![(
				sibling_para_id.into(),
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

// Now also testing a page instead of just concat messages.
#[test]
fn maybe_double_encoded_versioned_xcm_decode_page_works() {
	let page = mk_page();

	// Now try to decode the page.
	let input = &mut &page[..];
	for i in 0..100 {
		match (i % 2, VersionedXcm::<()>::decode(input)) {
			(0, Ok(xcm)) => {
				assert_eq!(xcm, v2_xcm());
			},
			(1, Ok(xcm)) => {
				assert_eq!(xcm, v3_xcm());
			},
			unexpected => unreachable!("{:?}", unexpected),
		}
	}

	assert_eq!(input.remaining_len(), Ok(Some(0)), "All data consumed");
}

/// Check that `take_first_concatenated_xcm` correctly splits a page into its XCMs.
#[test]
fn take_first_concatenated_xcm_works() {
	let page = mk_page();
	let input = &mut &page[..];

	for i in 0..100 {
		let xcm = XcmpQueue::take_first_concatenated_xcm(input, &mut WeightMeter::new()).unwrap();
		match (i % 2, xcm) {
			(0, data) | (2, data) => {
				assert_eq!(data, v2_xcm().encode());
			},
			(1, data) | (3, data) => {
				assert_eq!(data, v3_xcm().encode());
			},
			unexpected => unreachable!("{:?}", unexpected),
		}
	}
	assert_eq!(input.remaining_len(), Ok(Some(0)), "All data consumed");
}

/// A message that is not too deeply nested will be accepted by `take_first_concatenated_xcm`.
#[test]
fn take_first_concatenated_xcm_good_recursion_depth_works() {
	let mut good = Xcm::<()>(vec![ClearOrigin]);
	for _ in 0..MAX_XCM_DECODE_DEPTH - 1 {
		good = Xcm(vec![SetAppendix(good)]);
	}
	let good = VersionedXcm::V4(good);

	let page = good.encode();
	assert_ok!(XcmpQueue::take_first_concatenated_xcm(&mut &page[..], &mut WeightMeter::new()));
}

/// A message that is too deeply nested will be rejected by `take_first_concatenated_xcm`.
#[test]
fn take_first_concatenated_xcm_good_bad_depth_errors() {
	let mut bad = Xcm::<()>(vec![ClearOrigin]);
	for _ in 0..MAX_XCM_DECODE_DEPTH {
		bad = Xcm(vec![SetAppendix(bad)]);
	}
	let bad = VersionedXcm::V4(bad);

	let page = bad.encode();
	assert_err!(
		XcmpQueue::take_first_concatenated_xcm(&mut &page[..], &mut WeightMeter::new()),
		()
	);
}

#[test]
fn lazy_migration_works() {
	use crate::migration::v3::*;

	new_test_ext().execute_with(|| {
		EnqueuedMessages::set(vec![]);
		let _g = StorageNoopGuard::default(); // No storage is leaked.

		let mut channels = vec![];
		for i in 0..20 {
			let para = ParaId::from(i);
			let mut message_metadata = vec![];
			for block in 0..30 {
				message_metadata.push((block, XcmpMessageFormat::ConcatenatedVersionedXcm));
				InboundXcmpMessages::<Test>::insert(para, block, vec![(i + block) as u8]);
			}

			channels.push(InboundChannelDetails {
				sender: para,
				state: InboundState::Ok,
				message_metadata,
			});
		}
		InboundXcmpStatus::<Test>::set(Some(channels));

		for para in 0..20u32 {
			assert_eq!(InboundXcmpStatus::<Test>::get().unwrap().len() as u32, 20 - para);
			assert_eq!(InboundXcmpMessages::<Test>::iter_prefix(ParaId::from(para)).count(), 30);

			for block in 0..30 {
				XcmpQueue::on_idle(0u32.into(), Weight::MAX);
				assert_eq!(
					EnqueuedMessages::get(),
					vec![(para.into(), vec![(29 - block + para) as u8])]
				);
				EnqueuedMessages::set(vec![]);
			}
			// One more to jump to the next channel:
			XcmpQueue::on_idle(0u32.into(), Weight::MAX);

			assert_eq!(InboundXcmpStatus::<Test>::get().unwrap().len() as u32, 20 - para - 1);
			assert_eq!(InboundXcmpMessages::<Test>::iter_prefix(ParaId::from(para)).count(), 0);
		}
		// One more for the cleanup:
		XcmpQueue::on_idle(0u32.into(), Weight::MAX);

		assert!(!InboundXcmpStatus::<Test>::exists());
		assert_eq!(InboundXcmpMessages::<Test>::iter().count(), 0);
		EnqueuedMessages::set(vec![]);
	});
}

#[test]
fn lazy_migration_noop_when_out_of_weight() {
	use crate::migration::v3::*;
	assert!(!XcmpQueue::on_idle_weight().is_zero(), "pre condition");

	new_test_ext().execute_with(|| {
		let _g = StorageNoopGuard::default(); // No storage is leaked.
		let block = 5;
		let para = ParaId::from(4);
		let message_metadata = vec![(block, XcmpMessageFormat::ConcatenatedVersionedXcm)];

		InboundXcmpMessages::<Test>::insert(para, block, vec![123u8]);
		InboundXcmpStatus::<Test>::set(Some(vec![InboundChannelDetails {
			sender: para,
			state: InboundState::Ok,
			message_metadata,
		}]));

		// Hypothetically, it would do something with enough weight limit:
		hypothetically!({
			XcmpQueue::on_idle(0u32.into(), Weight::MAX);
			assert_eq!(EnqueuedMessages::get(), vec![(para, vec![123u8])]);
		});
		// But does not, since the limit is zero:
		assert_storage_noop!({ XcmpQueue::on_idle(0u32.into(), Weight::zero()) });

		InboundXcmpMessages::<Test>::remove(para, block);
		InboundXcmpStatus::<Test>::kill();
	});
}

#[test]
fn xcmp_queue_send_xcm_works() {
	new_test_ext().execute_with(|| {
		let sibling_para_id = ParaId::from(12345);
		let dest: Location = (Parent, Parachain(sibling_para_id.into())).into();
		let msg = Xcm(vec![ClearOrigin]);

		// try to send without opened HRMP channel to the sibling_para_id
		assert_eq!(
			send_xcm::<XcmpQueue>(dest.clone(), msg.clone()),
			Err(SendError::Transport("NoChannel")),
		);

		// open HRMP channel to the sibling_para_id
		ParachainSystem::open_outbound_hrmp_channel_for_benchmarks_or_tests(sibling_para_id);

		// check empty outbound queue
		assert!(XcmpQueue::take_outbound_messages(usize::MAX).is_empty());

		// now send works
		assert_ok!(send_xcm::<XcmpQueue>(dest, msg));

		// check outbound queue contains message/page for sibling_para_id
		assert!(XcmpQueue::take_outbound_messages(usize::MAX)
			.iter()
			.any(|(para_id, _)| para_id == &sibling_para_id));
	})
}

#[test]
fn xcmp_queue_send_too_big_xcm_fails() {
	new_test_ext().execute_with(|| {
		let sibling_para_id = ParaId::from(12345);
		let dest = (Parent, Parachain(sibling_para_id.into())).into();

		let max_message_size = 100_u32;

		// open HRMP channel to the sibling_para_id with a set `max_message_size`
		ParachainSystem::open_custom_outbound_hrmp_channel_for_benchmarks_or_tests(
			sibling_para_id,
			cumulus_primitives_core::AbridgedHrmpChannel {
				max_message_size,
				max_capacity: 10,
				max_total_size: 10_000_000_u32,
				msg_count: 0,
				total_size: 0,
				mqc_head: None,
			},
		);

		// Message is crafted to exceed `max_message_size`
		let mut message = Xcm::builder_unsafe();
		for _ in 0..97 {
			message = message.clear_origin();
		}
		let message = message.build();
		let encoded_message_size = message.encode().len();
		let versioned_size = 1; // VersionedXcm enum is added by `send_xcm` and it add one additional byte
		assert_eq!(encoded_message_size, max_message_size as usize - versioned_size);

		// check empty outbound queue
		assert!(XcmpQueue::take_outbound_messages(usize::MAX).is_empty());

		// Message is too big because after adding the VersionedXcm enum, it would reach
		// `max_message_size` Then, adding the format, which is the worst case scenario in which a
		// new page is needed, would get it over the limit
		assert_eq!(send_xcm::<XcmpQueue>(dest, message), Err(SendError::Transport("TooBig")),);

		// outbound queue is still empty
		assert!(XcmpQueue::take_outbound_messages(usize::MAX).is_empty());
	});
}

#[test]
fn verify_fee_factor_increase_and_decrease() {
	use cumulus_primitives_core::AbridgedHrmpChannel;
	use sp_runtime::FixedU128;

	let sibling_para_id = ParaId::from(12345);
	let destination: Location = (Parent, Parachain(sibling_para_id.into())).into();
	let xcm = Xcm(vec![ClearOrigin; 100]);
	let versioned_xcm = VersionedXcm::from(xcm.clone());
	let mut xcmp_message = XcmpMessageFormat::ConcatenatedVersionedXcm.encode();
	xcmp_message.extend(versioned_xcm.encode());

	new_test_ext().execute_with(|| {
		let initial = InitialFactor::get();
		assert_eq!(DeliveryFeeFactor::<Test>::get(sibling_para_id), initial);

		// Open channel so messages can actually be sent
		ParachainSystem::open_custom_outbound_hrmp_channel_for_benchmarks_or_tests(
			sibling_para_id,
			AbridgedHrmpChannel {
				max_capacity: 10,
				max_total_size: 1000,
				max_message_size: 104,
				msg_count: 0,
				total_size: 0,
				mqc_head: None,
			},
		);

		// Fee factor is only increased in `send_fragment`, which is called by `send_xcm`.
		// When queue is not congested, fee factor doesn't change.
		assert_ok!(send_xcm::<XcmpQueue>(destination.clone(), xcm.clone())); // Size 104
		assert_ok!(send_xcm::<XcmpQueue>(destination.clone(), xcm.clone())); // Size 208
		assert_ok!(send_xcm::<XcmpQueue>(destination.clone(), xcm.clone())); // Size 312
		assert_ok!(send_xcm::<XcmpQueue>(destination.clone(), xcm.clone())); // Size 416
		assert_eq!(DeliveryFeeFactor::<Test>::get(sibling_para_id), initial);

		// Sending the message right now is cheap
		let (_, delivery_fees) = validate_send::<XcmpQueue>(destination.clone(), xcm.clone())
			.expect("message can be sent; qed");
		let Fungible(delivery_fee_amount) = delivery_fees.inner()[0].fun else {
			unreachable!("asset is fungible; qed");
		};
		assert_eq!(delivery_fee_amount, 402_000_000);

		let smaller_xcm = Xcm(vec![ClearOrigin; 30]);

		// When we get to half of `max_total_size`, because `THRESHOLD_FACTOR` is 2,
		// then the fee factor starts to increase.
		assert_ok!(send_xcm::<XcmpQueue>(destination.clone(), xcm.clone())); // Size 520
		assert_eq!(DeliveryFeeFactor::<Test>::get(sibling_para_id), FixedU128::from_float(1.05));

		for _ in 0..12 {
			// We finish at size 929
			assert_ok!(send_xcm::<XcmpQueue>(destination.clone(), smaller_xcm.clone()));
		}
		assert!(DeliveryFeeFactor::<Test>::get(sibling_para_id) > FixedU128::from_float(1.88));

		// Sending the message right now is expensive
		let (_, delivery_fees) = validate_send::<XcmpQueue>(destination.clone(), xcm.clone())
			.expect("message can be sent; qed");
		let Fungible(delivery_fee_amount) = delivery_fees.inner()[0].fun else {
			unreachable!("asset is fungible; qed");
		};
		assert_eq!(delivery_fee_amount, 758_030_955);

		// Fee factor only decreases in `take_outbound_messages`
		for _ in 0..5 {
			// We take 5 100 byte pages
			XcmpQueue::take_outbound_messages(1);
		}
		assert!(DeliveryFeeFactor::<Test>::get(sibling_para_id) < FixedU128::from_float(1.72));
		XcmpQueue::take_outbound_messages(1);
		assert!(DeliveryFeeFactor::<Test>::get(sibling_para_id) < FixedU128::from_float(1.63));
	});
}
