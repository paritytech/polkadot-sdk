// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Primitives of messages module, that are used on the target chain.

use crate::{Message, MessageKey, MessageNonce, MessagePayload, OutboundLaneData};

use bp_runtime::{messages::MessageDispatchResult, raw_storage_proof_size, RawStorageProof, Size};
use codec::{Decode, DecodeWithMemTracking, Encode, Error as CodecError};
use frame_support::weights::Weight;
use scale_info::TypeInfo;
use sp_core::RuntimeDebug;
use sp_std::{fmt::Debug, marker::PhantomData, prelude::*};

/// Messages proof from bridged chain.
///
/// It contains everything required to prove that bridged (source) chain has
/// sent us some messages:
///
/// - hash of finalized header;
///
/// - storage proof of messages and (optionally) outbound lane state;
///
/// - lane id;
///
/// - nonces (inclusive range) of messages which are included in this proof.
#[derive(Clone, Decode, DecodeWithMemTracking, Encode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
pub struct FromBridgedChainMessagesProof<BridgedHeaderHash, Lane> {
	/// Hash of the finalized bridged header the proof is for.
	pub bridged_header_hash: BridgedHeaderHash,
	/// A storage trie proof of messages being delivered.
	pub storage_proof: RawStorageProof,
	/// Messages in this proof are sent over this lane.
	pub lane: Lane,
	/// Nonce of the first message being delivered.
	pub nonces_start: MessageNonce,
	/// Nonce of the last message being delivered.
	pub nonces_end: MessageNonce,
}

impl<BridgedHeaderHash, Lane> Size for FromBridgedChainMessagesProof<BridgedHeaderHash, Lane> {
	fn size(&self) -> u32 {
		use frame_support::sp_runtime::SaturatedConversion;
		raw_storage_proof_size(&self.storage_proof).saturated_into()
	}
}

/// Proved messages from the source chain.
pub type ProvedMessages<LaneId, Message> = (LaneId, ProvedLaneMessages<Message>);

/// Proved messages from single lane of the source chain.
#[derive(RuntimeDebug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
pub struct ProvedLaneMessages<Message> {
	/// Optional outbound lane state.
	pub lane_state: Option<OutboundLaneData>,
	/// Messages sent through this lane.
	pub messages: Vec<Message>,
}

/// Message data with decoded dispatch payload.
#[derive(RuntimeDebug)]
pub struct DispatchMessageData<DispatchPayload> {
	/// Result of dispatch payload decoding.
	pub payload: Result<DispatchPayload, CodecError>,
}

/// Message with decoded dispatch payload.
#[derive(RuntimeDebug)]
pub struct DispatchMessage<DispatchPayload, LaneId: Encode> {
	/// Message key.
	pub key: MessageKey<LaneId>,
	/// Message data with decoded dispatch payload.
	pub data: DispatchMessageData<DispatchPayload>,
}

/// Called when inbound message is received.
pub trait MessageDispatch {
	/// Decoded message payload type. Valid message may contain invalid payload. In this case
	/// message is delivered, but dispatch fails. Therefore, two separate types of payload
	/// (opaque `MessagePayload` used in delivery and this `DispatchPayload` used in dispatch).
	type DispatchPayload: Decode;

	/// Fine-grained result of single message dispatch (for better diagnostic purposes)
	type DispatchLevelResult: Clone + sp_std::fmt::Debug + Eq;

	/// Lane identifier type.
	type LaneId: Encode;

	/// Returns `true` if dispatcher is ready to accept additional messages. The `false` should
	/// be treated as a hint by both dispatcher and its consumers - i.e. dispatcher shall not
	/// simply drop messages if it returns `false`. The consumer may still call the `dispatch`
	/// if dispatcher has returned `false`.
	///
	/// We check it in the messages delivery transaction prologue. So if it becomes `false`
	/// after some portion of messages is already dispatched, it doesn't fail the whole transaction.
	fn is_active(lane: Self::LaneId) -> bool;

	/// Estimate dispatch weight.
	///
	/// This function must return correct upper bound of dispatch weight. The return value
	/// of this function is expected to match return value of the corresponding
	/// `From<Chain>InboundLaneApi::message_details().dispatch_weight` call.
	fn dispatch_weight(
		message: &mut DispatchMessage<Self::DispatchPayload, Self::LaneId>,
	) -> Weight;

	/// Called when inbound message is received.
	///
	/// It is up to the implementers of this trait to determine whether the message
	/// is invalid (i.e. improperly encoded, has too large weight, ...) or not.
	fn dispatch(
		message: DispatchMessage<Self::DispatchPayload, Self::LaneId>,
	) -> MessageDispatchResult<Self::DispatchLevelResult>;
}

/// Manages payments that are happening at the target chain during message delivery transaction.
pub trait DeliveryPayments<AccountId> {
	/// Error type.
	type Error: Debug + Into<&'static str>;

	/// Pay rewards for delivering messages to the given relayer.
	///
	/// This method is called during message delivery transaction which has been submitted
	/// by the `relayer`. The transaction brings `total_messages` messages  but only
	/// `valid_messages` have been accepted. The post-dispatch transaction weight is the
	/// `actual_weight`.
	fn pay_reward(
		relayer: AccountId,
		total_messages: MessageNonce,
		valid_messages: MessageNonce,
		actual_weight: Weight,
	);
}

impl<Message> Default for ProvedLaneMessages<Message> {
	fn default() -> Self {
		ProvedLaneMessages { lane_state: None, messages: Vec::new() }
	}
}

impl<DispatchPayload: Decode, LaneId: Encode> From<Message<LaneId>>
	for DispatchMessage<DispatchPayload, LaneId>
{
	fn from(message: Message<LaneId>) -> Self {
		DispatchMessage { key: message.key, data: message.payload.into() }
	}
}

impl<DispatchPayload: Decode> From<MessagePayload> for DispatchMessageData<DispatchPayload> {
	fn from(payload: MessagePayload) -> Self {
		DispatchMessageData { payload: DispatchPayload::decode(&mut &payload[..]) }
	}
}

impl<AccountId> DeliveryPayments<AccountId> for () {
	type Error = &'static str;

	fn pay_reward(
		_relayer: AccountId,
		_total_messages: MessageNonce,
		_valid_messages: MessageNonce,
		_actual_weight: Weight,
	) {
		// this implementation is not rewarding relayer at all
	}
}

/// Structure that may be used in place of  `MessageDispatch` on chains,
/// where inbound messages are forbidden.
pub struct ForbidInboundMessages<DispatchPayload, LaneId>(PhantomData<(DispatchPayload, LaneId)>);

impl<DispatchPayload: Decode, LaneId: Encode> MessageDispatch
	for ForbidInboundMessages<DispatchPayload, LaneId>
{
	type DispatchPayload = DispatchPayload;
	type DispatchLevelResult = ();
	type LaneId = LaneId;

	fn is_active(_: LaneId) -> bool {
		false
	}

	fn dispatch_weight(
		_message: &mut DispatchMessage<Self::DispatchPayload, Self::LaneId>,
	) -> Weight {
		Weight::MAX
	}

	fn dispatch(
		_: DispatchMessage<Self::DispatchPayload, Self::LaneId>,
	) -> MessageDispatchResult<Self::DispatchLevelResult> {
		MessageDispatchResult { unspent_weight: Weight::zero(), dispatch_level_result: () }
	}
}
