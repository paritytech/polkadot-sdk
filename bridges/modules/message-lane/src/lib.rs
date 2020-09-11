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
//! 6) relay may send proof-of-receiving and proof-of-processing back to this chain.
//!
//! Once message is sent, its progress can be tracked by looking at module events.
//! The assigned nonce is reported using `MessageAccepted` event. When message is
//! accepted by the bridged chain, `MessagesDelivered` is fired. When message is
//! processedby the bridged chain, `MessagesProcessed` by the bridged chain.

#![cfg_attr(not(feature = "std"), no_std)]

use crate::inbound_lane::{InboundLane, InboundLaneStorage};
use crate::outbound_lane::{OutboundLane, OutboundLaneStorage};

use bp_message_lane::{
	InboundLaneData, LaneId, Message, MessageKey, MessageNonce, OnMessageReceived, OutboundLaneData,
};
use frame_support::{decl_event, decl_module, decl_storage, traits::Get, Parameter, StorageMap};
use frame_system::ensure_signed;
use sp_std::{marker::PhantomData, prelude::*};

mod inbound_lane;
mod outbound_lane;

#[cfg(test)]
mod mock;

/// The module configuration trait
pub trait Trait<I = DefaultInstance>: frame_system::Trait {
	/// They overarching event type.
	type Event: From<Event<Self, I>> + Into<<Self as frame_system::Trait>::Event>;
	/// Message payload.
	type Payload: Parameter;
	/// Maximal number of messages that may be pruned during maintenance. Maintenance occurs
	/// whenever outbound lane is updated - i.e. when new message is sent, or receival is
	/// confirmed. The reason is that if you want to use lane, you should be ready to pay
	/// for it.
	type MaxMessagesToPruneAtOnce: Get<MessageNonce>;
	/// Called when message has been received.
	type OnMessageReceived: Default + OnMessageReceived<Self::Payload>;
}

decl_storage! {
	trait Store for Module<T: Trait<I>, I: Instance = DefaultInstance> as MessageLane {
		/// Map of lane id => inbound lane data.
		InboundLanes: map hasher(blake2_128_concat) LaneId => InboundLaneData;
		/// All stored (unprocessed) inbound messages.
		InboundMessages: map hasher(blake2_128_concat) MessageKey => Option<T::Payload>;
		/// Map of lane id => outbound lane data.
		OutboundLanes: map hasher(blake2_128_concat) LaneId => OutboundLaneData;
		/// All queued outbound messages.
		OutboundMessages: map hasher(blake2_128_concat) MessageKey => Option<T::Payload>;
	}
}

decl_event!(
	pub enum Event<T, I = DefaultInstance> where
		<T as frame_system::Trait>::AccountId,
	{
		/// Message has been accepted and is waiting to be delivered.
		MessageAccepted(LaneId, MessageNonce),
		/// Messages in the inclusive range have been delivered to the bridged chain.
		MessagesDelivered(LaneId, MessageNonce, MessageNonce),
		/// Messages in the inclusive range have been processed by the bridged chain.
		MessagesProcessed(LaneId, MessageNonce, MessageNonce),
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
			payload: T::Payload,
		) {
			let _ = ensure_signed(origin)?;
			let mut lane = outbound_lane::<T, I>(lane_id);
			let nonce = lane.send_message(payload);
			lane.prune_messages(T::MaxMessagesToPruneAtOnce::get());

			Self::deposit_event(RawEvent::MessageAccepted(lane_id, nonce));
		}
	}
}

impl<T: Trait<I>, I: Instance> Module<T, I> {
	// =========================================================================================
	// === Exposed mutables ====================================================================
	// =========================================================================================

	/// Receive new TRUSTED lane messages.
	///
	/// Trusted here means that the function itself doesn't check whether message has actually
	/// been sent through the other end of the channel. We only check that we are receiving
	/// and processing messages in order here.
	///
	/// Messages vector is required to be sorted by nonce within each lane. Otherise messages
	/// will be rejected.
	pub fn receive_messages(messages: Vec<Message<T::Payload>>) -> MessageNonce {
		let mut correct_messages = 0;
		let mut processor = T::OnMessageReceived::default();
		for message in messages {
			let mut lane = inbound_lane::<T, I>(message.key.lane_id);
			if lane.receive_message(message.key.nonce, message.payload, &mut processor) {
				correct_messages += 1;
			}
		}

		correct_messages
	}

	/// Process stored lane messages.
	///
	/// Stops processing either when all messages are processed, or when processor returns
	/// MessageResult::NotProcessed.
	pub fn process_lane_messages(lane_id: &LaneId, processor: &mut impl OnMessageReceived<T::Payload>) {
		inbound_lane::<T, I>(*lane_id).process_messages(processor);
	}

	/// Receive TRUSTED proof of message receival.
	///
	/// Trusted here means that the function itself doesn't check whether the bridged chain has
	/// actually received these messages.
	///
	/// The caller may break the channel by providing `latest_received_nonce` that is larger
	/// than actual one. Not-yet-sent messages may be pruned in this case.
	pub fn confirm_receival(lane_id: &LaneId, latest_received_nonce: MessageNonce) {
		let mut lane = outbound_lane::<T, I>(*lane_id);
		let received_range = lane.confirm_receival(latest_received_nonce);

		if let Some(received_range) = received_range {
			Self::deposit_event(RawEvent::MessagesDelivered(
				*lane_id,
				received_range.0,
				received_range.1,
			));
		}
	}

	/// Receive TRUSTED proof of message processing.
	///
	/// Trusted here means that the function itself doesn't check whether the bridged chain has
	/// actually processed these messages.
	pub fn confirm_processing(lane_id: &LaneId, latest_processed_nonce: MessageNonce) {
		let mut lane = outbound_lane::<T, I>(*lane_id);
		let processed_range = lane.confirm_processing(latest_processed_nonce);

		if let Some(processed_range) = processed_range {
			Self::deposit_event(RawEvent::MessagesProcessed(
				*lane_id,
				processed_range.0,
				processed_range.1,
			));
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
	type Payload = T::Payload;

	fn id(&self) -> LaneId {
		self.lane_id
	}

	fn data(&self) -> InboundLaneData {
		InboundLanes::<I>::get(&self.lane_id)
	}

	fn set_data(&mut self, data: InboundLaneData) {
		InboundLanes::<I>::insert(&self.lane_id, data)
	}

	fn message(&self, nonce: &MessageNonce) -> Option<Self::Payload> {
		InboundMessages::<T, I>::get(MessageKey {
			lane_id: self.lane_id,
			nonce: *nonce,
		})
	}

	fn save_message(&mut self, nonce: MessageNonce, payload: T::Payload) {
		InboundMessages::<T, I>::insert(
			MessageKey {
				lane_id: self.lane_id,
				nonce,
			},
			payload,
		);
	}

	fn remove_message(&mut self, nonce: &MessageNonce) {
		InboundMessages::<T, I>::remove(MessageKey {
			lane_id: self.lane_id,
			nonce: *nonce,
		});
	}
}

/// Runtime outbound lane storage.
struct RuntimeOutboundLaneStorage<T, I = DefaultInstance> {
	lane_id: LaneId,
	_phantom: PhantomData<(T, I)>,
}

impl<T: Trait<I>, I: Instance> OutboundLaneStorage for RuntimeOutboundLaneStorage<T, I> {
	type Payload = T::Payload;

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
	fn message(&self, nonce: &MessageNonce) -> Option<Self::Payload> {
		OutboundMessages::<T, I>::get(MessageKey {
			lane_id: self.lane_id,
			nonce: *nonce,
		})
	}

	fn save_message(&mut self, nonce: MessageNonce, payload: T::Payload) {
		OutboundMessages::<T, I>::insert(
			MessageKey {
				lane_id: self.lane_id,
				nonce,
			},
			payload,
		);
	}

	fn remove_message(&mut self, nonce: &MessageNonce) {
		OutboundMessages::<T, I>::remove(MessageKey {
			lane_id: self.lane_id,
			nonce: *nonce,
		});
	}
}
