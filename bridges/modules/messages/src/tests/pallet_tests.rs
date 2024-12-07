// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Pallet-level tests.

use crate::{
	active_outbound_lane,
	lanes_manager::RuntimeInboundLaneStorage,
	outbound_lane::ReceptionConfirmationError,
	tests::mock::{RuntimeEvent as TestEvent, *},
	weights_ext::WeightInfoExt,
	Call, Config, Error, Event, InboundLanes, LanesManagerError, OutboundLanes, OutboundMessages,
	Pallet, PalletOperatingMode, PalletOwner, StoredInboundLaneData,
};

use bp_messages::{
	source_chain::{FromBridgedChainMessagesDeliveryProof, MessagesBridge},
	target_chain::{FromBridgedChainMessagesProof, MessageDispatch},
	BridgeMessagesCall, ChainWithMessages, DeliveredMessages, InboundLaneData,
	InboundMessageDetails, LaneIdType, LaneState, MessageKey, MessageNonce, MessagesOperatingMode,
	OutboundLaneData, OutboundMessageDetails, UnrewardedRelayer, UnrewardedRelayersState,
	VerificationError,
};
use bp_runtime::{BasicOperatingMode, PreComputedSize, RangeInclusiveExt, Size};
use bp_test_utils::generate_owned_bridge_module_tests;
use codec::Encode;
use frame_support::{
	assert_err, assert_noop, assert_ok,
	dispatch::Pays,
	storage::generator::{StorageMap, StorageValue},
	weights::Weight,
};
use frame_system::{EventRecord, Pallet as System, Phase};
use sp_runtime::{BoundedVec, DispatchError};

fn get_ready_for_events() {
	System::<TestRuntime>::set_block_number(1);
	System::<TestRuntime>::reset_events();
}

fn send_regular_message(lane_id: TestLaneIdType) {
	get_ready_for_events();

	let outbound_lane = active_outbound_lane::<TestRuntime, ()>(lane_id).unwrap();
	let message_nonce = outbound_lane.data().latest_generated_nonce + 1;
	let prev_enqueued_messages = outbound_lane.data().queued_messages().saturating_len();
	let valid_message = Pallet::<TestRuntime, ()>::validate_message(lane_id, &REGULAR_PAYLOAD)
		.expect("validate_message has failed");
	let artifacts = Pallet::<TestRuntime, ()>::send_message(valid_message);
	assert_eq!(artifacts.enqueued_messages, prev_enqueued_messages + 1);

	// check event with assigned nonce
	assert_eq!(
		System::<TestRuntime>::events(),
		vec![EventRecord {
			phase: Phase::Initialization,
			event: TestEvent::Messages(Event::MessageAccepted {
				lane_id: lane_id.into(),
				nonce: message_nonce
			}),
			topics: vec![],
		}],
	);
}

fn receive_messages_delivery_proof() {
	System::<TestRuntime>::set_block_number(1);
	System::<TestRuntime>::reset_events();

	assert_ok!(Pallet::<TestRuntime>::receive_messages_delivery_proof(
		RuntimeOrigin::signed(1),
		prepare_messages_delivery_proof(
			test_lane_id(),
			InboundLaneData {
				state: LaneState::Opened,
				last_confirmed_nonce: 1,
				relayers: vec![UnrewardedRelayer {
					relayer: 0,
					messages: DeliveredMessages::new(1),
				}]
				.into(),
			},
		),
		UnrewardedRelayersState {
			unrewarded_relayer_entries: 1,
			messages_in_oldest_entry: 1,
			total_messages: 1,
			last_delivered_nonce: 1,
		},
	));
	assert_ok!(Pallet::<TestRuntime>::do_try_state());

	assert_eq!(
		System::<TestRuntime>::events(),
		vec![EventRecord {
			phase: Phase::Initialization,
			event: TestEvent::Messages(Event::MessagesDelivered {
				lane_id: test_lane_id().into(),
				messages: DeliveredMessages::new(1),
			}),
			topics: vec![],
		}],
	);
}

#[test]
fn pallet_rejects_transactions_if_halted() {
	run_test(|| {
		// send message first to be able to check that delivery_proof fails later
		send_regular_message(test_lane_id());

		PalletOperatingMode::<TestRuntime, ()>::put(MessagesOperatingMode::Basic(
			BasicOperatingMode::Halted,
		));

		assert_noop!(
			Pallet::<TestRuntime, ()>::validate_message(test_lane_id(), &REGULAR_PAYLOAD),
			Error::<TestRuntime, ()>::NotOperatingNormally,
		);

		let messages_proof = prepare_messages_proof(vec![message(2, REGULAR_PAYLOAD)], None);
		assert_noop!(
			Pallet::<TestRuntime>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				messages_proof,
				1,
				REGULAR_PAYLOAD.declared_weight,
			),
			Error::<TestRuntime, ()>::BridgeModule(bp_runtime::OwnedBridgeModuleError::Halted),
		);

		let delivery_proof = prepare_messages_delivery_proof(
			test_lane_id(),
			InboundLaneData {
				state: LaneState::Opened,
				last_confirmed_nonce: 1,
				relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)].into(),
			},
		);
		assert_noop!(
			Pallet::<TestRuntime>::receive_messages_delivery_proof(
				RuntimeOrigin::signed(1),
				delivery_proof,
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					messages_in_oldest_entry: 1,
					total_messages: 1,
					last_delivered_nonce: 1,
				},
			),
			Error::<TestRuntime, ()>::BridgeModule(bp_runtime::OwnedBridgeModuleError::Halted),
		);
		assert_ok!(Pallet::<TestRuntime>::do_try_state());
	});
}

#[test]
fn receive_messages_fails_if_dispatcher_is_inactive() {
	run_test(|| {
		TestMessageDispatch::deactivate(test_lane_id());
		let proof = prepare_messages_proof(vec![message(1, REGULAR_PAYLOAD)], None);
		assert_noop!(
			Pallet::<TestRuntime>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				proof,
				1,
				REGULAR_PAYLOAD.declared_weight,
			),
			Error::<TestRuntime, ()>::LanesManager(LanesManagerError::LaneDispatcherInactive),
		);
	});
}

#[test]
fn pallet_rejects_new_messages_in_rejecting_outbound_messages_operating_mode() {
	run_test(|| {
		// send message first to be able to check that delivery_proof fails later
		send_regular_message(test_lane_id());

		PalletOperatingMode::<TestRuntime, ()>::put(
			MessagesOperatingMode::RejectingOutboundMessages,
		);

		assert_noop!(
			Pallet::<TestRuntime, ()>::validate_message(test_lane_id(), &REGULAR_PAYLOAD),
			Error::<TestRuntime, ()>::NotOperatingNormally,
		);

		assert_ok!(Pallet::<TestRuntime>::receive_messages_proof(
			RuntimeOrigin::signed(1),
			TEST_RELAYER_A,
			prepare_messages_proof(vec![message(1, REGULAR_PAYLOAD)], None),
			1,
			REGULAR_PAYLOAD.declared_weight,
		),);

		assert_ok!(Pallet::<TestRuntime>::receive_messages_delivery_proof(
			RuntimeOrigin::signed(1),
			prepare_messages_delivery_proof(
				test_lane_id(),
				InboundLaneData {
					state: LaneState::Opened,
					last_confirmed_nonce: 1,
					relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)].into(),
				},
			),
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 1,
				messages_in_oldest_entry: 1,
				total_messages: 1,
				last_delivered_nonce: 1,
			},
		));
		assert_ok!(Pallet::<TestRuntime>::do_try_state());
	});
}

#[test]
fn send_message_works() {
	run_test(|| {
		send_regular_message(test_lane_id());
	});
}

#[test]
fn send_message_rejects_too_large_message() {
	run_test(|| {
		let mut message_payload = message_payload(1, 0);
		// the payload isn't simply extra, so it'll definitely overflow
		// `max_outbound_payload_size` if we add `max_outbound_payload_size` bytes to extra
		let max_outbound_payload_size = BridgedChain::maximal_incoming_message_size();
		message_payload
			.extra
			.extend_from_slice(&vec![0u8; max_outbound_payload_size as usize]);
		assert_noop!(
			Pallet::<TestRuntime, ()>::validate_message(test_lane_id(), &message_payload.clone(),),
			Error::<TestRuntime, ()>::MessageRejectedByPallet(VerificationError::MessageTooLarge),
		);

		// let's check that we're able to send `max_outbound_payload_size` messages
		while message_payload.encoded_size() as u32 > max_outbound_payload_size {
			message_payload.extra.pop();
		}
		assert_eq!(message_payload.encoded_size() as u32, max_outbound_payload_size);

		let valid_message =
			Pallet::<TestRuntime, ()>::validate_message(test_lane_id(), &message_payload)
				.expect("validate_message has failed");
		Pallet::<TestRuntime, ()>::send_message(valid_message);
	})
}

#[test]
fn receive_messages_proof_works() {
	run_test(|| {
		assert_ok!(Pallet::<TestRuntime>::receive_messages_proof(
			RuntimeOrigin::signed(1),
			TEST_RELAYER_A,
			prepare_messages_proof(vec![message(1, REGULAR_PAYLOAD)], None),
			1,
			REGULAR_PAYLOAD.declared_weight,
		));

		assert_eq!(
			InboundLanes::<TestRuntime>::get(test_lane_id())
				.unwrap()
				.0
				.last_delivered_nonce(),
			1
		);

		assert!(TestDeliveryPayments::is_reward_paid(1));
	});
}

#[test]
fn receive_messages_proof_updates_confirmed_message_nonce() {
	run_test(|| {
		// say we have received 10 messages && last confirmed message is 8
		InboundLanes::<TestRuntime, ()>::insert(
			test_lane_id(),
			InboundLaneData {
				state: LaneState::Opened,
				last_confirmed_nonce: 8,
				relayers: vec![
					unrewarded_relayer(9, 9, TEST_RELAYER_A),
					unrewarded_relayer(10, 10, TEST_RELAYER_B),
				]
				.into(),
			},
		);
		assert_eq!(
			inbound_unrewarded_relayers_state(test_lane_id()),
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 2,
				messages_in_oldest_entry: 1,
				total_messages: 2,
				last_delivered_nonce: 10,
			},
		);

		// message proof includes outbound lane state with latest confirmed message updated to 9
		assert_ok!(Pallet::<TestRuntime>::receive_messages_proof(
			RuntimeOrigin::signed(1),
			TEST_RELAYER_A,
			prepare_messages_proof(
				vec![message(11, REGULAR_PAYLOAD)],
				Some(OutboundLaneData { latest_received_nonce: 9, ..Default::default() }),
			),
			1,
			REGULAR_PAYLOAD.declared_weight,
		));

		assert_eq!(
			InboundLanes::<TestRuntime>::get(test_lane_id()).unwrap().0,
			InboundLaneData {
				state: LaneState::Opened,
				last_confirmed_nonce: 9,
				relayers: vec![
					unrewarded_relayer(10, 10, TEST_RELAYER_B),
					unrewarded_relayer(11, 11, TEST_RELAYER_A)
				]
				.into(),
			},
		);
		assert_eq!(
			inbound_unrewarded_relayers_state(test_lane_id()),
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 2,
				messages_in_oldest_entry: 1,
				total_messages: 2,
				last_delivered_nonce: 11,
			},
		);
	});
}

#[test]
fn receive_messages_proof_fails_when_dispatcher_is_inactive() {
	run_test(|| {
		// "enqueue" enough (to deactivate dispatcher) messages at dispatcher
		let latest_received_nonce = BridgedChain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX + 1;
		for _ in 1..=latest_received_nonce {
			TestMessageDispatch::emulate_enqueued_message(test_lane_id());
		}
		assert!(!TestMessageDispatch::is_active(test_lane_id()));
		InboundLanes::<TestRuntime, ()>::insert(
			test_lane_id(),
			InboundLaneData {
				state: LaneState::Opened,
				last_confirmed_nonce: latest_received_nonce,
				relayers: vec![].into(),
			},
		);

		// try to delvier next message - it should fail because dispatcher is in "suspended" state
		// at the beginning of the call
		let messages_proof =
			prepare_messages_proof(vec![message(latest_received_nonce + 1, REGULAR_PAYLOAD)], None);
		assert_noop!(
			Pallet::<TestRuntime>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				messages_proof,
				1,
				REGULAR_PAYLOAD.declared_weight,
			),
			Error::<TestRuntime, ()>::LanesManager(LanesManagerError::LaneDispatcherInactive)
		);
		assert!(!TestMessageDispatch::is_active(test_lane_id()));
	});
}

#[test]
fn receive_messages_succeeds_when_dispatcher_becomes_inactive_in_the_middle_of_transaction() {
	run_test(|| {
		// "enqueue" enough (to deactivate dispatcher) messages at dispatcher
		let latest_received_nonce = BridgedChain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX / 2;
		for _ in 1..=latest_received_nonce {
			TestMessageDispatch::emulate_enqueued_message(test_lane_id());
		}
		assert!(TestMessageDispatch::is_active(test_lane_id()));
		InboundLanes::<TestRuntime, ()>::insert(
			test_lane_id(),
			InboundLaneData {
				state: LaneState::Opened,
				last_confirmed_nonce: latest_received_nonce,
				relayers: vec![].into(),
			},
		);

		// try to delvier next `BridgedChain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX` messages
		// - it will lead to dispatcher deactivation, but the transaction shall not fail and all
		// messages must be delivered
		let messages_begin = latest_received_nonce + 1;
		let messages_end =
			messages_begin + BridgedChain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
		let messages_range = messages_begin..messages_end;
		let messages_count = BridgedChain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
		assert_ok!(Pallet::<TestRuntime>::receive_messages_proof(
			RuntimeOrigin::signed(1),
			TEST_RELAYER_A,
			prepare_messages_proof(
				messages_range.map(|nonce| message(nonce, REGULAR_PAYLOAD)).collect(),
				None,
			),
			messages_count as _,
			REGULAR_PAYLOAD.declared_weight * messages_count,
		),);
		assert_eq!(
			inbound_unrewarded_relayers_state(test_lane_id()).last_delivered_nonce,
			messages_end - 1,
		);
		assert!(!TestMessageDispatch::is_active(test_lane_id()));
	});
}

#[test]
fn receive_messages_proof_does_not_accept_message_if_dispatch_weight_is_not_enough() {
	run_test(|| {
		let proof = prepare_messages_proof(vec![message(1, REGULAR_PAYLOAD)], None);
		let mut declared_weight = REGULAR_PAYLOAD.declared_weight;
		*declared_weight.ref_time_mut() -= 1;

		assert_noop!(
			Pallet::<TestRuntime>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				proof,
				1,
				declared_weight,
			),
			Error::<TestRuntime, ()>::InsufficientDispatchWeight
		);
		assert_eq!(
			InboundLanes::<TestRuntime>::get(test_lane_id()).unwrap().last_delivered_nonce(),
			0
		);
	});
}

#[test]
fn receive_messages_proof_rejects_invalid_proof() {
	run_test(|| {
		let mut proof = prepare_messages_proof(vec![message(1, REGULAR_PAYLOAD)], None);
		proof.nonces_end += 1;

		assert_noop!(
			Pallet::<TestRuntime, ()>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				proof,
				1,
				Weight::zero(),
			),
			Error::<TestRuntime, ()>::InvalidMessagesProof,
		);
	});
}

#[test]
fn receive_messages_proof_rejects_proof_with_too_many_messages() {
	run_test(|| {
		let proof = prepare_messages_proof(vec![message(1, REGULAR_PAYLOAD)], None);
		assert_noop!(
			Pallet::<TestRuntime, ()>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				proof,
				u32::MAX,
				Weight::zero(),
			),
			Error::<TestRuntime, ()>::TooManyMessagesInTheProof,
		);
	});
}

#[test]
fn receive_messages_delivery_proof_works() {
	run_test(|| {
		assert_eq!(
			OutboundLanes::<TestRuntime, ()>::get(test_lane_id())
				.unwrap()
				.latest_received_nonce,
			0,
		);
		assert_eq!(
			OutboundLanes::<TestRuntime, ()>::get(test_lane_id())
				.unwrap()
				.oldest_unpruned_nonce,
			1,
		);

		send_regular_message(test_lane_id());
		receive_messages_delivery_proof();

		assert_eq!(
			OutboundLanes::<TestRuntime, ()>::get(test_lane_id())
				.unwrap()
				.latest_received_nonce,
			1,
		);
		assert_eq!(
			OutboundLanes::<TestRuntime, ()>::get(test_lane_id())
				.unwrap()
				.oldest_unpruned_nonce,
			2,
		);
	});
}

#[test]
fn receive_messages_delivery_proof_works_on_closed_outbound_lanes() {
	run_test(|| {
		send_regular_message(test_lane_id());
		active_outbound_lane::<TestRuntime, ()>(test_lane_id())
			.unwrap()
			.set_state(LaneState::Closed);
		receive_messages_delivery_proof();

		assert_eq!(
			OutboundLanes::<TestRuntime, ()>::get(test_lane_id())
				.unwrap()
				.latest_received_nonce,
			1,
		);
	});
}

#[test]
fn receive_messages_delivery_proof_rewards_relayers() {
	run_test(|| {
		send_regular_message(test_lane_id());
		send_regular_message(test_lane_id());

		// this reports delivery of message 1 => reward is paid to TEST_RELAYER_A
		let single_message_delivery_proof = prepare_messages_delivery_proof(
			test_lane_id(),
			InboundLaneData {
				relayers: vec![unrewarded_relayer(1, 1, TEST_RELAYER_A)].into(),
				..Default::default()
			},
		);
		let single_message_delivery_proof_size = single_message_delivery_proof.size();
		let result = Pallet::<TestRuntime>::receive_messages_delivery_proof(
			RuntimeOrigin::signed(1),
			single_message_delivery_proof,
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 1,
				messages_in_oldest_entry: 1,
				total_messages: 1,
				last_delivered_nonce: 1,
			},
		);
		assert_ok!(result);
		assert_ok!(Pallet::<TestRuntime>::do_try_state());
		assert_eq!(
			result.unwrap().actual_weight.unwrap(),
			TestWeightInfo::receive_messages_delivery_proof_weight(
				&PreComputedSize(single_message_delivery_proof_size as _),
				&UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					total_messages: 1,
					..Default::default()
				},
			)
		);
		assert!(TestDeliveryConfirmationPayments::is_reward_paid(TEST_RELAYER_A, 1));
		assert!(!TestDeliveryConfirmationPayments::is_reward_paid(TEST_RELAYER_B, 1));

		// this reports delivery of both message 1 and message 2 => reward is paid only to
		// TEST_RELAYER_B
		let two_messages_delivery_proof = prepare_messages_delivery_proof(
			test_lane_id(),
			InboundLaneData {
				relayers: vec![
					unrewarded_relayer(1, 1, TEST_RELAYER_A),
					unrewarded_relayer(2, 2, TEST_RELAYER_B),
				]
				.into(),
				..Default::default()
			},
		);
		let two_messages_delivery_proof_size = two_messages_delivery_proof.size();
		let result = Pallet::<TestRuntime>::receive_messages_delivery_proof(
			RuntimeOrigin::signed(1),
			two_messages_delivery_proof,
			UnrewardedRelayersState {
				unrewarded_relayer_entries: 2,
				messages_in_oldest_entry: 1,
				total_messages: 2,
				last_delivered_nonce: 2,
			},
		);
		assert_ok!(result);
		assert_ok!(Pallet::<TestRuntime>::do_try_state());
		// even though the pre-dispatch weight was for two messages, the actual weight is
		// for single message only
		assert_eq!(
			result.unwrap().actual_weight.unwrap(),
			TestWeightInfo::receive_messages_delivery_proof_weight(
				&PreComputedSize(two_messages_delivery_proof_size as _),
				&UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					total_messages: 1,
					..Default::default()
				},
			)
		);
		assert!(!TestDeliveryConfirmationPayments::is_reward_paid(TEST_RELAYER_A, 1));
		assert!(TestDeliveryConfirmationPayments::is_reward_paid(TEST_RELAYER_B, 1));
		assert_eq!(TestOnMessagesDelivered::call_arguments(), Some((test_lane_id(), 0)));
	});
}

#[test]
fn receive_messages_delivery_proof_rejects_invalid_proof() {
	run_test(|| {
		let mut proof = prepare_messages_delivery_proof(test_lane_id(), Default::default());
		proof.lane = TestLaneIdType::try_new(42, 84).unwrap();

		assert_noop!(
			Pallet::<TestRuntime>::receive_messages_delivery_proof(
				RuntimeOrigin::signed(1),
				proof,
				Default::default(),
			),
			Error::<TestRuntime, ()>::InvalidMessagesDeliveryProof,
		);
	});
}

#[test]
fn receive_messages_delivery_proof_rejects_proof_if_declared_relayers_state_is_invalid() {
	run_test(|| {
		// when number of relayers entries is invalid
		let proof = prepare_messages_delivery_proof(
			test_lane_id(),
			InboundLaneData {
				relayers: vec![
					unrewarded_relayer(1, 1, TEST_RELAYER_A),
					unrewarded_relayer(2, 2, TEST_RELAYER_B),
				]
				.into(),
				..Default::default()
			},
		);
		assert_noop!(
			Pallet::<TestRuntime>::receive_messages_delivery_proof(
				RuntimeOrigin::signed(1),
				proof,
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					total_messages: 2,
					last_delivered_nonce: 2,
					..Default::default()
				},
			),
			Error::<TestRuntime, ()>::InvalidUnrewardedRelayersState,
		);

		// when number of messages is invalid
		let proof = prepare_messages_delivery_proof(
			test_lane_id(),
			InboundLaneData {
				relayers: vec![
					unrewarded_relayer(1, 1, TEST_RELAYER_A),
					unrewarded_relayer(2, 2, TEST_RELAYER_B),
				]
				.into(),
				..Default::default()
			},
		);
		assert_noop!(
			Pallet::<TestRuntime>::receive_messages_delivery_proof(
				RuntimeOrigin::signed(1),
				proof,
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 2,
					total_messages: 1,
					last_delivered_nonce: 2,
					..Default::default()
				},
			),
			Error::<TestRuntime, ()>::InvalidUnrewardedRelayersState,
		);

		// when last delivered nonce is invalid
		let proof = prepare_messages_delivery_proof(
			test_lane_id(),
			InboundLaneData {
				relayers: vec![
					unrewarded_relayer(1, 1, TEST_RELAYER_A),
					unrewarded_relayer(2, 2, TEST_RELAYER_B),
				]
				.into(),
				..Default::default()
			},
		);
		assert_noop!(
			Pallet::<TestRuntime>::receive_messages_delivery_proof(
				RuntimeOrigin::signed(1),
				proof,
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 2,
					total_messages: 2,
					last_delivered_nonce: 8,
					..Default::default()
				},
			),
			Error::<TestRuntime, ()>::InvalidUnrewardedRelayersState,
		);
	});
}

#[test]
fn receive_messages_accepts_single_message_with_invalid_payload() {
	run_test(|| {
		let mut invalid_message = message(1, REGULAR_PAYLOAD);
		invalid_message.payload = Vec::new();

		assert_ok!(Pallet::<TestRuntime, ()>::receive_messages_proof(
			RuntimeOrigin::signed(1),
			TEST_RELAYER_A,
			prepare_messages_proof(vec![invalid_message], None),
			1,
			Weight::zero(), /* weight may be zero in this case (all messages are
			                 * improperly encoded) */
		),);

		assert_eq!(
			InboundLanes::<TestRuntime>::get(test_lane_id()).unwrap().last_delivered_nonce(),
			1,
		);
	});
}

#[test]
fn receive_messages_accepts_batch_with_message_with_invalid_payload() {
	run_test(|| {
		let mut invalid_message = message(2, REGULAR_PAYLOAD);
		invalid_message.payload = Vec::new();

		assert_ok!(Pallet::<TestRuntime, ()>::receive_messages_proof(
			RuntimeOrigin::signed(1),
			TEST_RELAYER_A,
			prepare_messages_proof(
				vec![message(1, REGULAR_PAYLOAD), invalid_message, message(3, REGULAR_PAYLOAD),],
				None
			),
			3,
			REGULAR_PAYLOAD.declared_weight + REGULAR_PAYLOAD.declared_weight,
		),);

		assert_eq!(
			InboundLanes::<TestRuntime>::get(test_lane_id()).unwrap().last_delivered_nonce(),
			3,
		);
	});
}

#[test]
fn actual_dispatch_weight_does_not_overflow() {
	run_test(|| {
		let message1 = message(1, message_payload(0, u64::MAX / 2));
		let message2 = message(2, message_payload(0, u64::MAX / 2));
		let message3 = message(3, message_payload(0, u64::MAX / 2));

		let proof = prepare_messages_proof(vec![message1, message2, message3], None);
		assert_noop!(
			Pallet::<TestRuntime, ()>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				// this may cause overflow if source chain storage is invalid
				proof,
				3,
				Weight::MAX,
			),
			Error::<TestRuntime, ()>::InsufficientDispatchWeight
		);
		assert_eq!(
			InboundLanes::<TestRuntime>::get(test_lane_id()).unwrap().last_delivered_nonce(),
			0
		);
	});
}

#[test]
fn ref_time_refund_from_receive_messages_proof_works() {
	run_test(|| {
		fn submit_with_unspent_weight(
			nonce: MessageNonce,
			unspent_weight: u64,
		) -> (Weight, Weight) {
			let mut payload = REGULAR_PAYLOAD;
			*payload.dispatch_result.unspent_weight.ref_time_mut() = unspent_weight;
			let proof = prepare_messages_proof(vec![message(nonce, payload)], None);
			let messages_count = 1;
			let pre_dispatch_weight =
				<TestRuntime as Config>::WeightInfo::receive_messages_proof_weight(
					&*proof,
					messages_count,
					REGULAR_PAYLOAD.declared_weight,
				);
			let result = Pallet::<TestRuntime>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				proof,
				messages_count,
				REGULAR_PAYLOAD.declared_weight,
			)
			.expect("delivery has failed");
			let post_dispatch_weight =
				result.actual_weight.expect("receive_messages_proof always returns Some");

			// message delivery transactions are never free
			assert_eq!(result.pays_fee, Pays::Yes);

			(pre_dispatch_weight, post_dispatch_weight)
		}

		// when dispatch is returning `unspent_weight < declared_weight`
		let (pre, post) = submit_with_unspent_weight(1, 1);
		assert_eq!(post.ref_time(), pre.ref_time() - 1);

		// when dispatch is returning `unspent_weight = declared_weight`
		let (pre, post) = submit_with_unspent_weight(2, REGULAR_PAYLOAD.declared_weight.ref_time());
		assert_eq!(post.ref_time(), pre.ref_time() - REGULAR_PAYLOAD.declared_weight.ref_time());

		// when dispatch is returning `unspent_weight > declared_weight`
		let (pre, post) =
			submit_with_unspent_weight(3, REGULAR_PAYLOAD.declared_weight.ref_time() + 1);
		assert_eq!(post.ref_time(), pre.ref_time() - REGULAR_PAYLOAD.declared_weight.ref_time());

		// when there's no unspent weight
		let (pre, post) = submit_with_unspent_weight(4, 0);
		assert_eq!(post.ref_time(), pre.ref_time());

		// when dispatch is returning `unspent_weight < declared_weight`
		let (pre, post) = submit_with_unspent_weight(5, 1);
		assert_eq!(post.ref_time(), pre.ref_time() - 1);
	});
}

#[test]
fn proof_size_refund_from_receive_messages_proof_works() {
	run_test(|| {
		let max_entries = BridgedChain::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX as usize;

		// if there's maximal number of unrewarded relayer entries at the inbound lane, then
		// `proof_size` is unchanged in post-dispatch weight
		let proof = prepare_messages_proof(vec![message(101, REGULAR_PAYLOAD)], None);
		let messages_count = 1;
		let pre_dispatch_weight =
			<TestRuntime as Config>::WeightInfo::receive_messages_proof_weight(
				&*proof,
				messages_count,
				REGULAR_PAYLOAD.declared_weight,
			);
		InboundLanes::<TestRuntime>::insert(
			test_lane_id(),
			StoredInboundLaneData(InboundLaneData {
				state: LaneState::Opened,
				relayers: vec![
					UnrewardedRelayer {
						relayer: 42,
						messages: DeliveredMessages { begin: 0, end: 100 }
					};
					max_entries
				]
				.into(),
				last_confirmed_nonce: 0,
			}),
		);
		let post_dispatch_weight = Pallet::<TestRuntime>::receive_messages_proof(
			RuntimeOrigin::signed(1),
			TEST_RELAYER_A,
			proof.clone(),
			messages_count,
			REGULAR_PAYLOAD.declared_weight,
		)
		.unwrap()
		.actual_weight
		.unwrap();
		assert_eq!(post_dispatch_weight.proof_size(), pre_dispatch_weight.proof_size());

		// if count of unrewarded relayer entries is less than maximal, then some `proof_size`
		// must be refunded
		InboundLanes::<TestRuntime>::insert(
			test_lane_id(),
			StoredInboundLaneData(InboundLaneData {
				state: LaneState::Opened,
				relayers: vec![
					UnrewardedRelayer {
						relayer: 42,
						messages: DeliveredMessages { begin: 0, end: 100 }
					};
					max_entries - 1
				]
				.into(),
				last_confirmed_nonce: 0,
			}),
		);
		let post_dispatch_weight = Pallet::<TestRuntime>::receive_messages_proof(
			RuntimeOrigin::signed(1),
			TEST_RELAYER_A,
			proof,
			messages_count,
			REGULAR_PAYLOAD.declared_weight,
		)
		.unwrap()
		.actual_weight
		.unwrap();
		assert!(
			post_dispatch_weight.proof_size() < pre_dispatch_weight.proof_size(),
			"Expected post-dispatch PoV {} to be less than pre-dispatch PoV {}",
			post_dispatch_weight.proof_size(),
			pre_dispatch_weight.proof_size(),
		);
	});
}

#[test]
fn receive_messages_delivery_proof_rejects_proof_if_trying_to_confirm_more_messages_than_expected()
{
	run_test(|| {
		// send message first to be able to check that delivery_proof fails later
		send_regular_message(test_lane_id());

		// 1) InboundLaneData declares that the `last_confirmed_nonce` is 1;
		// 2) InboundLaneData has no entries => `InboundLaneData::last_delivered_nonce()` returns
		//    `last_confirmed_nonce`;
		// 3) it means that we're going to confirm delivery of messages 1..=1;
		// 4) so the number of declared messages (see `UnrewardedRelayersState`) is `0` and numer of
		//    actually confirmed messages is `1`.
		let proof = prepare_messages_delivery_proof(
			test_lane_id(),
			InboundLaneData {
				state: LaneState::Opened,
				last_confirmed_nonce: 1,
				relayers: Default::default(),
			},
		);
		assert_noop!(
			Pallet::<TestRuntime>::receive_messages_delivery_proof(
				RuntimeOrigin::signed(1),
				proof,
				UnrewardedRelayersState { last_delivered_nonce: 1, ..Default::default() },
			),
			Error::<TestRuntime, ()>::ReceptionConfirmation(
				ReceptionConfirmationError::TryingToConfirmMoreMessagesThanExpected
			),
		);
	});
}

#[test]
fn storage_keys_computed_properly() {
	assert_eq!(
		PalletOperatingMode::<TestRuntime>::storage_value_final_key().to_vec(),
		bp_messages::storage_keys::operating_mode_key("Messages").0,
	);

	assert_eq!(
		OutboundMessages::<TestRuntime>::storage_map_final_key(MessageKey {
			lane_id: test_lane_id(),
			nonce: 42
		}),
		bp_messages::storage_keys::message_key("Messages", &test_lane_id(), 42).0,
	);

	assert_eq!(
		OutboundLanes::<TestRuntime>::storage_map_final_key(test_lane_id()),
		bp_messages::storage_keys::outbound_lane_data_key("Messages", &test_lane_id()).0,
	);

	assert_eq!(
		InboundLanes::<TestRuntime>::storage_map_final_key(test_lane_id()),
		bp_messages::storage_keys::inbound_lane_data_key("Messages", &test_lane_id()).0,
	);
}

#[test]
fn inbound_message_details_works() {
	run_test(|| {
		assert_eq!(
			Pallet::<TestRuntime>::inbound_message_data(
				test_lane_id(),
				REGULAR_PAYLOAD.encode(),
				OutboundMessageDetails { nonce: 0, dispatch_weight: Weight::zero(), size: 0 },
			),
			InboundMessageDetails { dispatch_weight: REGULAR_PAYLOAD.declared_weight },
		);
	});
}

#[test]
fn test_bridge_messages_call_is_correctly_defined() {
	run_test(|| {
		let account_id = 1;
		let message_proof = prepare_messages_proof(vec![message(1, REGULAR_PAYLOAD)], None);
		let message_delivery_proof = prepare_messages_delivery_proof(
			test_lane_id(),
			InboundLaneData {
				state: LaneState::Opened,
				last_confirmed_nonce: 1,
				relayers: vec![UnrewardedRelayer {
					relayer: 0,
					messages: DeliveredMessages::new(1),
				}]
				.into(),
			},
		);
		let unrewarded_relayer_state = UnrewardedRelayersState {
			unrewarded_relayer_entries: 1,
			total_messages: 1,
			last_delivered_nonce: 1,
			..Default::default()
		};

		let direct_receive_messages_proof_call = Call::<TestRuntime>::receive_messages_proof {
			relayer_id_at_bridged_chain: account_id,
			proof: message_proof.clone(),
			messages_count: 1,
			dispatch_weight: REGULAR_PAYLOAD.declared_weight,
		};
		let indirect_receive_messages_proof_call = BridgeMessagesCall::<
			AccountId,
			FromBridgedChainMessagesProof<BridgedHeaderHash, TestLaneIdType>,
			FromBridgedChainMessagesDeliveryProof<BridgedHeaderHash, TestLaneIdType>,
		>::receive_messages_proof {
			relayer_id_at_bridged_chain: account_id,
			proof: *message_proof,
			messages_count: 1,
			dispatch_weight: REGULAR_PAYLOAD.declared_weight,
		};
		assert_eq!(
			direct_receive_messages_proof_call.encode(),
			indirect_receive_messages_proof_call.encode()
		);

		let direct_receive_messages_delivery_proof_call =
			Call::<TestRuntime>::receive_messages_delivery_proof {
				proof: message_delivery_proof.clone(),
				relayers_state: unrewarded_relayer_state.clone(),
			};
		let indirect_receive_messages_delivery_proof_call = BridgeMessagesCall::<
			AccountId,
			FromBridgedChainMessagesProof<BridgedHeaderHash, TestLaneIdType>,
			FromBridgedChainMessagesDeliveryProof<BridgedHeaderHash, TestLaneIdType>,
		>::receive_messages_delivery_proof {
			proof: message_delivery_proof,
			relayers_state: unrewarded_relayer_state,
		};
		assert_eq!(
			direct_receive_messages_delivery_proof_call.encode(),
			indirect_receive_messages_delivery_proof_call.encode()
		);
	});
}

generate_owned_bridge_module_tests!(
	MessagesOperatingMode::Basic(BasicOperatingMode::Normal),
	MessagesOperatingMode::Basic(BasicOperatingMode::Halted)
);

#[test]
fn inbound_storage_extra_proof_size_bytes_works() {
	fn relayer_entry() -> UnrewardedRelayer<TestRelayer> {
		UnrewardedRelayer { relayer: 42u64, messages: DeliveredMessages { begin: 0, end: 100 } }
	}

	fn storage(relayer_entries: usize) -> RuntimeInboundLaneStorage<TestRuntime, ()> {
		RuntimeInboundLaneStorage {
			lane_id: TestLaneIdType::try_new(1, 2).unwrap(),
			cached_data: InboundLaneData {
				state: LaneState::Opened,
				relayers: vec![relayer_entry(); relayer_entries].into(),
				last_confirmed_nonce: 0,
			},
		}
	}

	let max_entries = BridgedChain::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX as usize;

	// when we have exactly `MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX` unrewarded relayers
	assert_eq!(storage(max_entries).extra_proof_size_bytes(), 0);

	// when we have less than `MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX` unrewarded relayers
	assert_eq!(
		storage(max_entries - 1).extra_proof_size_bytes(),
		relayer_entry().encode().len() as u64
	);
	assert_eq!(
		storage(max_entries - 2).extra_proof_size_bytes(),
		2 * relayer_entry().encode().len() as u64
	);

	// when we have more than `MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX` unrewarded relayers
	// (shall not happen in practice)
	assert_eq!(storage(max_entries + 1).extra_proof_size_bytes(), 0);
}

#[test]
fn send_messages_fails_if_outbound_lane_is_not_opened() {
	run_test(|| {
		assert_noop!(
			Pallet::<TestRuntime, ()>::validate_message(unknown_lane_id(), &REGULAR_PAYLOAD),
			Error::<TestRuntime, ()>::LanesManager(LanesManagerError::UnknownOutboundLane),
		);

		assert_noop!(
			Pallet::<TestRuntime, ()>::validate_message(closed_lane_id(), &REGULAR_PAYLOAD),
			Error::<TestRuntime, ()>::LanesManager(LanesManagerError::ClosedOutboundLane),
		);
	});
}

#[test]
fn receive_messages_proof_fails_if_inbound_lane_is_not_opened() {
	run_test(|| {
		let mut message = message(1, REGULAR_PAYLOAD);
		message.key.lane_id = unknown_lane_id();
		let proof = prepare_messages_proof(vec![message.clone()], None);

		assert_noop!(
			Pallet::<TestRuntime>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				proof,
				1,
				REGULAR_PAYLOAD.declared_weight,
			),
			Error::<TestRuntime, ()>::LanesManager(LanesManagerError::UnknownInboundLane),
		);

		message.key.lane_id = closed_lane_id();
		let proof = prepare_messages_proof(vec![message], None);

		assert_noop!(
			Pallet::<TestRuntime>::receive_messages_proof(
				RuntimeOrigin::signed(1),
				TEST_RELAYER_A,
				proof,
				1,
				REGULAR_PAYLOAD.declared_weight,
			),
			Error::<TestRuntime, ()>::LanesManager(LanesManagerError::ClosedInboundLane),
		);
	});
}

#[test]
fn receive_messages_delivery_proof_fails_if_outbound_lane_is_unknown() {
	run_test(|| {
		let make_proof = |lane: TestLaneIdType| {
			prepare_messages_delivery_proof(
				lane,
				InboundLaneData {
					state: LaneState::Opened,
					last_confirmed_nonce: 1,
					relayers: vec![UnrewardedRelayer {
						relayer: 0,
						messages: DeliveredMessages::new(1),
					}]
					.into(),
				},
			)
		};

		let proof = make_proof(unknown_lane_id());
		assert_noop!(
			Pallet::<TestRuntime>::receive_messages_delivery_proof(
				RuntimeOrigin::signed(1),
				proof,
				UnrewardedRelayersState {
					unrewarded_relayer_entries: 1,
					messages_in_oldest_entry: 1,
					total_messages: 1,
					last_delivered_nonce: 1,
				},
			),
			Error::<TestRuntime, ()>::LanesManager(LanesManagerError::UnknownOutboundLane),
		);
	});
}

#[test]
fn do_try_state_for_outbound_lanes_works() {
	run_test(|| {
		let lane_id = test_lane_id();

		// setup delivered nonce 1
		OutboundLanes::<TestRuntime>::insert(
			lane_id,
			OutboundLaneData {
				state: LaneState::Opened,
				oldest_unpruned_nonce: 2,
				latest_received_nonce: 1,
				latest_generated_nonce: 0,
			},
		);
		// store message for nonce 1
		OutboundMessages::<TestRuntime>::insert(
			MessageKey { lane_id, nonce: 1 },
			BoundedVec::default(),
		);
		assert_err!(
			Pallet::<TestRuntime>::do_try_state(),
			sp_runtime::TryRuntimeError::Other("Found unpruned lanes!")
		);

		// remove message for nonce 1
		OutboundMessages::<TestRuntime>::remove(MessageKey { lane_id, nonce: 1 });
		assert_ok!(Pallet::<TestRuntime>::do_try_state());
	})
}
