// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

//! Runtime module that allows sending and receiving messages using lane concept:
//!
//! 1) the message is sent using `send_message()` call;
//! 2) every outbound message is assigned nonce;
//! 3) the messages are stored in the storage;
//! 4) external component (relay) delivers messages to bridged chain;
//! 5) messages are processed in order (ordered by assigned nonce);
//! 6) relay may send proof-of-delivery back to this chain.
//!
//! Once message is sent, its progress can be tracked by looking at module events.
//! The assigned nonce is reported using `MessageAccepted` event. When message is
//! delivered to the the bridged chain, it is reported using `MessagesDelivered` event.

#![cfg_attr(not(feature = "std"), no_std)]

use crate::inbound_lane::{InboundLane, InboundLaneStorage};
use crate::outbound_lane::{OutboundLane, OutboundLaneStorage};

use bp_message_lane::{
	source_chain::{LaneMessageVerifier, MessageDeliveryAndDispatchPayment, TargetHeaderChain},
	target_chain::{MessageDispatch, SourceHeaderChain},
	InboundLaneData, LaneId, MessageData, MessageKey, MessageNonce, OutboundLaneData,
};
use codec::{Decode, Encode};
use frame_support::{
	decl_error, decl_event, decl_module, decl_storage, sp_runtime::DispatchResult, traits::Get, weights::Weight,
	Parameter, StorageMap,
};
use frame_system::ensure_signed;
use sp_std::{marker::PhantomData, prelude::*};

mod inbound_lane;
mod outbound_lane;

#[cfg(test)]
mod mock;

// TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
/// Upper bound of delivery transaction weight.
const DELIVERY_BASE_WEIGHT: Weight = 0;

/// The module configuration trait
pub trait Trait<I = DefaultInstance>: frame_system::Trait {
	// General types

	/// They overarching event type.
	type Event: From<Event<Self, I>> + Into<<Self as frame_system::Trait>::Event>;
	/// Maximal number of messages that may be pruned during maintenance. Maintenance occurs
	/// whenever outbound lane is updated - i.e. when new message is sent, or receival is
	/// confirmed. The reason is that if you want to use lane, you should be ready to pay
	/// for it.
	type MaxMessagesToPruneAtOnce: Get<MessageNonce>;

	/// Payload type of outbound messages. This payload is dispatched on the bridged chain.
	type OutboundPayload: Parameter;
	/// Message fee type of outbound messages. This fee is paid on this chain.
	type OutboundMessageFee: Parameter;

	/// Payload type of inbound messages. This payload is dispatched on this chain.
	type InboundPayload: Decode;
	/// Message fee type of inbound messages. This fee is paid on the bridged chain.
	type InboundMessageFee: Decode;

	// Types that are used by outbound_lane (on source chain).

	/// Target header chain.
	type TargetHeaderChain: TargetHeaderChain<Self::OutboundPayload>;
	/// Message payload verifier.
	type LaneMessageVerifier: LaneMessageVerifier<Self::AccountId, Self::OutboundPayload, Self::OutboundMessageFee>;
	/// Message delivery payment.
	type MessageDeliveryAndDispatchPayment: MessageDeliveryAndDispatchPayment<Self::AccountId, Self::OutboundMessageFee>;

	// Types that are used by inbound_lane (on target chain).

	/// Source header chain, as it is represented on target chain.
	type SourceHeaderChain: SourceHeaderChain<Self::InboundMessageFee>;
	/// Message dispatch.
	type MessageDispatch: MessageDispatch<Self::InboundMessageFee, DispatchPayload = Self::InboundPayload>;
}

/// Shortcut to messages proof type for Trait.
type MessagesProofOf<T, I> =
	<<T as Trait<I>>::SourceHeaderChain as SourceHeaderChain<<T as Trait<I>>::InboundMessageFee>>::MessagesProof;
/// Shortcut to messages delivery proof type for Trait.
type MessagesDeliveryProofOf<T, I> =
	<<T as Trait<I>>::TargetHeaderChain as TargetHeaderChain<<T as Trait<I>>::OutboundPayload>>::MessagesDeliveryProof;

decl_error! {
	pub enum Error for Module<T: Trait<I>, I: Instance> {
		/// Message has been treated as invalid by chain verifier.
		MessageRejectedByChainVerifier,
		/// Message has been treated as invalid by lane verifier.
		MessageRejectedByLaneVerifier,
		/// Submitter has failed to pay fee for delivering and dispatching messages.
		FailedToWithdrawMessageFee,
		/// Invalid messages has been submitted.
		InvalidMessagesProof,
		/// Invalid messages dispatch weight has been declared by the relayer.
		InvalidMessagesDispatchWeight,
		/// Invalid messages delivery proof has been submitted.
		InvalidMessagesDeliveryProof,
	}
}

decl_storage! {
	trait Store for Module<T: Trait<I>, I: Instance = DefaultInstance> as MessageLane {
		/// Map of lane id => inbound lane data.
		InboundLanes: map hasher(blake2_128_concat) LaneId => InboundLaneData;
		/// Map of lane id => outbound lane data.
		OutboundLanes: map hasher(blake2_128_concat) LaneId => OutboundLaneData;
		/// All queued outbound messages.
		OutboundMessages: map hasher(blake2_128_concat) MessageKey => Option<MessageData<T::OutboundMessageFee>>;
	}
}

decl_event!(
	pub enum Event<T, I = DefaultInstance> where
		<T as frame_system::Trait>::AccountId,
	{
		/// Message has been accepted and is waiting to be delivered.
		MessageAccepted(LaneId, MessageNonce),
		/// Messages in the inclusive range have been delivered and processed by the bridged chain.
		MessagesDelivered(LaneId, MessageNonce, MessageNonce),
		/// Phantom member, never used.
		Dummy(PhantomData<(AccountId, I)>),
	}
);

decl_module! {
	pub struct Module<T: Trait<I>, I: Instance = DefaultInstance> for enum Call where origin: T::Origin {
		/// Deposit one of this module's events by using the default implementation.
		fn deposit_event() = default;

		/// Send message over lane.
		#[weight = 0] // TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
		pub fn send_message(
			origin,
			lane_id: LaneId,
			payload: T::OutboundPayload,
			delivery_and_dispatch_fee: T::OutboundMessageFee,
		) -> DispatchResult {
			let submitter = ensure_signed(origin)?;

			// let's first check if message can be delivered to target chain
			T::TargetHeaderChain::verify_message(&payload).map_err(|err| {
				frame_support::debug::trace!(
					target: "runtime",
					"Message to lane {:?} is rejected by target chain: {:?}",
					lane_id,
					err,
				);

				Error::<T, I>::MessageRejectedByChainVerifier
			})?;

			// now let's enforce any additional lane rules
			T::LaneMessageVerifier::verify_message(
				&submitter,
				&delivery_and_dispatch_fee,
				&lane_id,
				&payload,
			).map_err(|err| {
				frame_support::debug::trace!(
					target: "runtime",
					"Message to lane {:?} is rejected by lane verifier: {:?}",
					lane_id,
					err,
				);

				Error::<T, I>::MessageRejectedByLaneVerifier
			})?;

			// let's withdraw delivery and dispatch fee from submitter
			T::MessageDeliveryAndDispatchPayment::pay_delivery_and_dispatch_fee(
				&submitter,
				&delivery_and_dispatch_fee,
			).map_err(|err| {
				frame_support::debug::trace!(
					target: "runtime",
					"Message to lane {:?} is rejected because submitter {:?} is unable to pay fee {:?}: {:?}",
					lane_id,
					submitter,
					delivery_and_dispatch_fee,
					err,
				);

				Error::<T, I>::FailedToWithdrawMessageFee
			})?;

			// finally, save message in outbound storage and emit event
			let mut lane = outbound_lane::<T, I>(lane_id);
			let nonce = lane.send_message(MessageData {
				payload: payload.encode(),
				fee: delivery_and_dispatch_fee,
			});
			lane.prune_messages(T::MaxMessagesToPruneAtOnce::get());

			frame_support::debug::trace!(
				target: "runtime",
				"Accepted message {} to lane {:?}",
				nonce,
				lane_id,
			);

			Self::deposit_event(RawEvent::MessageAccepted(lane_id, nonce));

			Ok(())
		}

		/// Receive messages proof from bridged chain.
		#[weight = DELIVERY_BASE_WEIGHT + dispatch_weight]
		pub fn receive_messages_proof(
			origin,
			proof: MessagesProofOf<T, I>,
			dispatch_weight: Weight,
		) -> DispatchResult {
			let _ = ensure_signed(origin)?;

			// verify messages proof && convert proof into messages
			let messages = T::SourceHeaderChain::verify_messages_proof(proof).map_err(|err| {
				frame_support::debug::trace!(
					target: "runtime",
					"Rejecting invalid messages proof: {:?}",
					err,
				);

				Error::<T, I>::InvalidMessagesProof
			})?;

			// try to decode message payloads
			let messages: Vec<_> = messages.into_iter().map(Into::into).collect();

			// verify that relayer is paying actual dispatch weight
			let actual_dispatch_weight: Weight = messages
				.iter()
				.map(T::MessageDispatch::dispatch_weight)
				.sum();
			if dispatch_weight < actual_dispatch_weight {
				frame_support::debug::trace!(
					target: "runtime",
					"Rejecting messages proof because of dispatch weight mismatch: declared={}, expected={}",
					dispatch_weight,
					actual_dispatch_weight,
				);

				return Err(Error::<T, I>::InvalidMessagesDispatchWeight.into());
			}

			// dispatch messages
			let total_messages = messages.len();
			let mut valid_messages = 0;
			for message in messages {
				let mut lane = inbound_lane::<T, I>(message.key.lane_id);
				if lane.receive_message::<T::MessageDispatch>(message.key.nonce, message.data) {
					valid_messages += 1;
				}
			}

			frame_support::debug::trace!(
				target: "runtime",
				"Received messages: total={}, valid={}",
				total_messages,
				valid_messages,
			);

			Ok(())
		}

		/// Receive messages delivery proof from bridged chain.
		#[weight = 0] // TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
		pub fn receive_messages_delivery_proof(origin, proof: MessagesDeliveryProofOf<T, I>) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			let (lane_id, nonce) = T::TargetHeaderChain::verify_messages_delivery_proof(proof).map_err(|err| {
				frame_support::debug::trace!(
					target: "runtime",
					"Rejecting invalid messages delivery proof: {:?}",
					err,
				);

				Error::<T, I>::InvalidMessagesDeliveryProof
			})?;

			let mut lane = outbound_lane::<T, I>(lane_id);
			let received_range = lane.confirm_delivery(nonce);
			if let Some(received_range) = received_range {
				Self::deposit_event(RawEvent::MessagesDelivered(lane_id, received_range.0, received_range.1));
			}

			frame_support::debug::trace!(
				target: "runtime",
				"Received messages delivery proof up to (and including) {} at lane {:?}",
				nonce,
				lane_id,
			);

			Ok(())
		}
	}
}

/// Creates new inbound lane object, backed by runtime storage.
fn inbound_lane<T: Trait<I>, I: Instance>(lane_id: LaneId) -> InboundLane<RuntimeInboundLaneStorage<T, I>> {
	InboundLane::new(RuntimeInboundLaneStorage {
		lane_id,
		_phantom: Default::default(),
	})
}

/// Creates new outbound lane object, backed by runtime storage.
fn outbound_lane<T: Trait<I>, I: Instance>(lane_id: LaneId) -> OutboundLane<RuntimeOutboundLaneStorage<T, I>> {
	OutboundLane::new(RuntimeOutboundLaneStorage {
		lane_id,
		_phantom: Default::default(),
	})
}

/// Runtime inbound lane storage.
struct RuntimeInboundLaneStorage<T, I = DefaultInstance> {
	lane_id: LaneId,
	_phantom: PhantomData<(T, I)>,
}

impl<T: Trait<I>, I: Instance> InboundLaneStorage for RuntimeInboundLaneStorage<T, I> {
	type MessageFee = T::InboundMessageFee;

	fn id(&self) -> LaneId {
		self.lane_id
	}

	fn data(&self) -> InboundLaneData {
		InboundLanes::<I>::get(&self.lane_id)
	}

	fn set_data(&mut self, data: InboundLaneData) {
		InboundLanes::<I>::insert(&self.lane_id, data)
	}
}

/// Runtime outbound lane storage.
struct RuntimeOutboundLaneStorage<T, I = DefaultInstance> {
	lane_id: LaneId,
	_phantom: PhantomData<(T, I)>,
}

impl<T: Trait<I>, I: Instance> OutboundLaneStorage for RuntimeOutboundLaneStorage<T, I> {
	type MessageFee = T::OutboundMessageFee;

	fn id(&self) -> LaneId {
		self.lane_id
	}

	fn data(&self) -> OutboundLaneData {
		OutboundLanes::<I>::get(&self.lane_id)
	}

	fn set_data(&mut self, data: OutboundLaneData) {
		OutboundLanes::<I>::insert(&self.lane_id, data)
	}

	#[cfg(test)]
	fn message(&self, nonce: &MessageNonce) -> Option<MessageData<T::OutboundMessageFee>> {
		OutboundMessages::<T, I>::get(MessageKey {
			lane_id: self.lane_id,
			nonce: *nonce,
		})
	}

	fn save_message(&mut self, nonce: MessageNonce, mesage_data: MessageData<T::OutboundMessageFee>) {
		OutboundMessages::<T, I>::insert(
			MessageKey {
				lane_id: self.lane_id,
				nonce,
			},
			mesage_data,
		);
	}

	fn remove_message(&mut self, nonce: &MessageNonce) {
		OutboundMessages::<T, I>::remove(MessageKey {
			lane_id: self.lane_id,
			nonce: *nonce,
		});
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{
		message, run_test, Origin, TestEvent, TestMessageDeliveryAndDispatchPayment, TestRuntime,
		PAYLOAD_REJECTED_BY_TARGET_CHAIN, REGULAR_PAYLOAD, TEST_LANE_ID,
	};
	use frame_support::{assert_noop, assert_ok};
	use frame_system::{EventRecord, Module as System, Phase};

	fn send_regular_message() {
		System::<TestRuntime>::set_block_number(1);
		System::<TestRuntime>::reset_events();

		assert_ok!(Module::<TestRuntime>::send_message(
			Origin::signed(1),
			TEST_LANE_ID,
			REGULAR_PAYLOAD,
			REGULAR_PAYLOAD.1,
		));

		// check event with assigned nonce
		assert_eq!(
			System::<TestRuntime>::events(),
			vec![EventRecord {
				phase: Phase::Initialization,
				event: TestEvent::message_lane(RawEvent::MessageAccepted(TEST_LANE_ID, 1)),
				topics: vec![],
			}],
		);

		// check that fee has been withdrawn from submitter
		assert!(TestMessageDeliveryAndDispatchPayment::is_fee_paid(1, REGULAR_PAYLOAD.1));
	}

	fn receive_messages_delivery_proof() {
		System::<TestRuntime>::set_block_number(1);
		System::<TestRuntime>::reset_events();

		assert_ok!(Module::<TestRuntime>::receive_messages_delivery_proof(
			Origin::signed(1),
			Ok((TEST_LANE_ID, 1)),
		));

		assert_eq!(
			System::<TestRuntime>::events(),
			vec![EventRecord {
				phase: Phase::Initialization,
				event: TestEvent::message_lane(RawEvent::MessagesDelivered(TEST_LANE_ID, 1, 1)),
				topics: vec![],
			}],
		);
	}

	#[test]
	fn send_message_works() {
		run_test(|| {
			send_regular_message();
		});
	}

	#[test]
	fn chain_verifier_rejects_invalid_message_in_send_message() {
		run_test(|| {
			// messages with this payload are rejected by target chain verifier
			assert_noop!(
				Module::<TestRuntime>::send_message(
					Origin::signed(1),
					TEST_LANE_ID,
					PAYLOAD_REJECTED_BY_TARGET_CHAIN,
					PAYLOAD_REJECTED_BY_TARGET_CHAIN.1
				),
				Error::<TestRuntime, DefaultInstance>::MessageRejectedByChainVerifier,
			);
		});
	}

	#[test]
	fn lane_verifier_rejects_invalid_message_in_send_message() {
		run_test(|| {
			// messages with zero fee are rejected by lane verifier
			assert_noop!(
				Module::<TestRuntime>::send_message(Origin::signed(1), TEST_LANE_ID, REGULAR_PAYLOAD, 0),
				Error::<TestRuntime, DefaultInstance>::MessageRejectedByLaneVerifier,
			);
		});
	}

	#[test]
	fn message_send_fails_if_submitter_cant_pay_message_fee() {
		run_test(|| {
			TestMessageDeliveryAndDispatchPayment::reject_payments();
			assert_noop!(
				Module::<TestRuntime>::send_message(
					Origin::signed(1),
					TEST_LANE_ID,
					REGULAR_PAYLOAD,
					REGULAR_PAYLOAD.1
				),
				Error::<TestRuntime, DefaultInstance>::FailedToWithdrawMessageFee,
			);
		});
	}

	#[test]
	fn receive_messages_proof_works() {
		run_test(|| {
			assert_ok!(Module::<TestRuntime>::receive_messages_proof(
				Origin::signed(1),
				Ok(vec![message(1, REGULAR_PAYLOAD)]),
				REGULAR_PAYLOAD.1,
			));

			assert_eq!(
				InboundLanes::<DefaultInstance>::get(TEST_LANE_ID).latest_received_nonce,
				1
			);
		});
	}

	#[test]
	fn receive_messages_proof_rejects_invalid_dispatch_weight() {
		run_test(|| {
			assert_noop!(
				Module::<TestRuntime>::receive_messages_proof(
					Origin::signed(1),
					Ok(vec![message(1, REGULAR_PAYLOAD)]),
					REGULAR_PAYLOAD.1 - 1,
				),
				Error::<TestRuntime, DefaultInstance>::InvalidMessagesDispatchWeight,
			);
		});
	}

	#[test]
	fn receive_messages_proof_rejects_invalid_proof() {
		run_test(|| {
			assert_noop!(
				Module::<TestRuntime, DefaultInstance>::receive_messages_proof(Origin::signed(1), Err(()), 0),
				Error::<TestRuntime, DefaultInstance>::InvalidMessagesProof,
			);
		});
	}

	#[test]
	fn receive_messages_delivery_proof_works() {
		run_test(|| {
			send_regular_message();
			receive_messages_delivery_proof();

			assert_eq!(
				OutboundLanes::<DefaultInstance>::get(&TEST_LANE_ID).latest_received_nonce,
				1,
			);
		});
	}

	#[test]
	fn receive_messages_delivery_proof_rejects_invalid_proof() {
		run_test(|| {
			assert_noop!(
				Module::<TestRuntime>::receive_messages_delivery_proof(Origin::signed(1), Err(()),),
				Error::<TestRuntime, DefaultInstance>::InvalidMessagesDeliveryProof,
			);
		});
	}

	#[test]
	fn receive_messages_accepts_single_message_with_invalid_payload() {
		run_test(|| {
			let mut invalid_message = message(1, REGULAR_PAYLOAD);
			invalid_message.data.payload = Vec::new();

			assert_ok!(Module::<TestRuntime, DefaultInstance>::receive_messages_proof(
				Origin::signed(1),
				Ok(vec![invalid_message]),
				0, // weight may be zero in this case (all messages are improperly encoded)
			),);

			assert_eq!(
				InboundLanes::<DefaultInstance>::get(&TEST_LANE_ID).latest_received_nonce,
				1,
			);
		});
	}

	#[test]
	fn receive_messages_accepts_batch_with_message_with_invalid_payload() {
		run_test(|| {
			let mut invalid_message = message(2, REGULAR_PAYLOAD);
			invalid_message.data.payload = Vec::new();

			assert_ok!(Module::<TestRuntime, DefaultInstance>::receive_messages_proof(
				Origin::signed(1),
				Ok(vec![
					message(1, REGULAR_PAYLOAD),
					invalid_message,
					message(3, REGULAR_PAYLOAD),
				]),
				REGULAR_PAYLOAD.1 + REGULAR_PAYLOAD.1,
			),);

			assert_eq!(
				InboundLanes::<DefaultInstance>::get(&TEST_LANE_ID).latest_received_nonce,
				3,
			);
		});
	}
}
