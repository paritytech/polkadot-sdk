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

use super::*;
use cumulus_primitives_core::{ParaId, XcmpMessageHandler};
use frame_support::{assert_noop, assert_ok};
use mock::{new_test_ext, ParachainSystem, RuntimeCall, RuntimeOrigin, Test, XcmpQueue};
use sp_runtime::traits::BadOrigin;

#[test]
fn one_message_does_not_panic() {
	new_test_ext().execute_with(|| {
		let message_format = XcmpMessageFormat::ConcatenatedVersionedXcm.encode();
		let messages = vec![(Default::default(), 1u32, message_format.as_slice())];

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
			&mut 0,
			Weight::from_parts(10_000_000_000, 0),
			Weight::from_parts(10_000_000_000, 0),
		);
	});
}

/// Tests that a blob message is handled. Currently this isn't implemented and panics when debug
/// assertions are enabled. When this feature is enabled, this test should be rewritten properly.
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
			&mut 0,
			Weight::from_parts(10_000_000_000, 0),
			Weight::from_parts(10_000_000_000, 0),
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
			&mut 0,
			Weight::from_parts(10_000_000_000, 0),
			Weight::from_parts(10_000_000_000, 0),
		);
	});
}

#[test]
fn service_overweight_unknown() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			XcmpQueue::service_overweight(RuntimeOrigin::root(), 0, Weight::from_parts(1000, 1000)),
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
			XcmpQueue::service_overweight(RuntimeOrigin::root(), 0, Weight::from_parts(1000, 1000)),
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
		let messages = vec![(ParaId::from(999), 1u32, message_format.as_slice())];

		// This should have executed the incoming XCM, because it came from a system parachain
		XcmpQueue::handle_xcmp_messages(messages.into_iter(), Weight::MAX);

		let queued_xcm = InboundXcmpMessages::<Test>::get(ParaId::from(999), 1u32);
		assert!(queued_xcm.is_empty());

		let messages = vec![(ParaId::from(2000), 1u32, message_format.as_slice())];

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
		assert_eq!(data.threshold_weight, Weight::from_parts(100_000, 0));
		assert_ok!(XcmpQueue::update_threshold_weight(
			RuntimeOrigin::root(),
			Weight::from_parts(10_000, 0)
		));
		assert_noop!(
			XcmpQueue::update_threshold_weight(
				RuntimeOrigin::signed(5),
				Weight::from_parts(10_000_000, 0),
			),
			BadOrigin
		);
		let data: QueueConfigData = <QueueConfig<Test>>::get();

		assert_eq!(data.threshold_weight, Weight::from_parts(10_000, 0));
	});
}

#[test]
fn update_weight_restrict_decay_works() {
	new_test_ext().execute_with(|| {
		let data: QueueConfigData = <QueueConfig<Test>>::get();
		assert_eq!(data.weight_restrict_decay, Weight::from_parts(2, 0));
		assert_ok!(XcmpQueue::update_weight_restrict_decay(
			RuntimeOrigin::root(),
			Weight::from_parts(5, 0)
		));
		assert_noop!(
			XcmpQueue::update_weight_restrict_decay(
				RuntimeOrigin::signed(6),
				Weight::from_parts(4, 0),
			),
			BadOrigin
		);
		let data: QueueConfigData = <QueueConfig<Test>>::get();

		assert_eq!(data.weight_restrict_decay, Weight::from_parts(5, 0));
	});
}

#[test]
fn update_xcmp_max_individual_weight() {
	new_test_ext().execute_with(|| {
		let data: QueueConfigData = <QueueConfig<Test>>::get();
		assert_eq!(
			data.xcmp_max_individual_weight,
			Weight::from_parts(20u64 * WEIGHT_REF_TIME_PER_MILLIS, DEFAULT_POV_SIZE),
		);
		assert_ok!(XcmpQueue::update_xcmp_max_individual_weight(
			RuntimeOrigin::root(),
			Weight::from_parts(30u64 * WEIGHT_REF_TIME_PER_MILLIS, 0)
		));
		assert_noop!(
			XcmpQueue::update_xcmp_max_individual_weight(
				RuntimeOrigin::signed(3),
				Weight::from_parts(10u64 * WEIGHT_REF_TIME_PER_MILLIS, 0)
			),
			BadOrigin
		);
		let data: QueueConfigData = <QueueConfig<Test>>::get();

		assert_eq!(
			data.xcmp_max_individual_weight,
			Weight::from_parts(30u64 * WEIGHT_REF_TIME_PER_MILLIS, 0)
		);
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
fn xcmp_queue_send_xcm_works() {
	new_test_ext().execute_with(|| {
		let sibling_para_id = ParaId::from(12345);
		let dest = (Parent, X1(Parachain(sibling_para_id.into()))).into();
		let msg = Xcm(vec![ClearOrigin]);

		// try to send without opened HRMP channel to the sibling_para_id
		assert_eq!(
			send_xcm::<XcmpQueue>(dest, msg.clone()),
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
fn verify_fee_factor_increase_and_decrease() {
	use cumulus_primitives_core::AbridgedHrmpChannel;
	use sp_runtime::FixedU128;

	let sibling_para_id = ParaId::from(12345);
	let destination = (Parent, Parachain(sibling_para_id.into())).into();
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
		assert_ok!(send_xcm::<XcmpQueue>(destination, xcm.clone())); // Size 104
		assert_ok!(send_xcm::<XcmpQueue>(destination, xcm.clone())); // Size 208
		assert_ok!(send_xcm::<XcmpQueue>(destination, xcm.clone())); // Size 312
		assert_ok!(send_xcm::<XcmpQueue>(destination, xcm.clone())); // Size 416
		assert_eq!(DeliveryFeeFactor::<Test>::get(sibling_para_id), initial);

		// Sending the message right now is cheap
		let (_, delivery_fees) =
			validate_send::<XcmpQueue>(destination, xcm.clone()).expect("message can be sent; qed");
		let Fungible(delivery_fee_amount) = delivery_fees.inner()[0].fun else {
			unreachable!("asset is fungible; qed");
		};
		assert_eq!(delivery_fee_amount, 402_000_000);

		let smaller_xcm = Xcm(vec![ClearOrigin; 30]);

		// When we get to half of `max_total_size`, because `THRESHOLD_FACTOR` is 2,
		// then the fee factor starts to increase.
		assert_ok!(send_xcm::<XcmpQueue>(destination, xcm.clone())); // Size 520
		assert_eq!(DeliveryFeeFactor::<Test>::get(sibling_para_id), FixedU128::from_float(1.05));

		for _ in 0..12 {
			// We finish at size 929
			assert_ok!(send_xcm::<XcmpQueue>(destination, smaller_xcm.clone()));
		}
		assert!(DeliveryFeeFactor::<Test>::get(sibling_para_id) > FixedU128::from_float(1.88));

		// Sending the message right now is expensive
		let (_, delivery_fees) =
			validate_send::<XcmpQueue>(destination, xcm.clone()).expect("message can be sent; qed");
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
