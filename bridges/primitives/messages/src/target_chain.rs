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

use crate::{
	LaneId, Message, MessageKey, MessageNonce, MessagePayload, OutboundLaneData, VerificationError,
};

use bp_runtime::{messages::MessageDispatchResult, Size};
use codec::{Decode, Encode, Error as CodecError};
use frame_support::{weights::Weight, Parameter};
use scale_info::TypeInfo;
use sp_core::RuntimeDebug;
use sp_std::{collections::btree_map::BTreeMap, fmt::Debug, marker::PhantomData, prelude::*};

/// Proved messages from the source chain.
pub type ProvedMessages<Message> = BTreeMap<LaneId, ProvedLaneMessages<Message>>;

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
pub struct DispatchMessage<DispatchPayload> {
	/// Message key.
	pub key: MessageKey,
	/// Message data with decoded dispatch payload.
	pub data: DispatchMessageData<DispatchPayload>,
}

/// Source chain API. Used by target chain, to verify source chain proofs.
///
/// All implementations of this trait should only work with finalized data that
/// can't change. Wrong implementation may lead to invalid lane states (i.e. lane
/// that's stuck) and/or processing messages without paying fees.
pub trait SourceHeaderChain {
	/// Proof that messages are sent from source chain. This may also include proof
	/// of corresponding outbound lane states.
	type MessagesProof: Parameter + Size;

	/// Verify messages proof and return proved messages.
	///
	/// Returns error if either proof is incorrect, or the number of messages in the proof
	/// is not matching the `messages_count`.
	///
	/// Messages vector is required to be sorted by nonce within each lane. Out-of-order
	/// messages will be rejected.
	///
	/// The `messages_count` argument verification (sane limits) is supposed to be made
	/// outside this function. This function only verifies that the proof declares exactly
	/// `messages_count` messages.
	fn verify_messages_proof(
		proof: Self::MessagesProof,
		messages_count: u32,
	) -> Result<ProvedMessages<Message>, VerificationError>;
}

/// Called when inbound message is received.
pub trait MessageDispatch {
	/// Decoded message payload type. Valid message may contain invalid payload. In this case
	/// message is delivered, but dispatch fails. Therefore, two separate types of payload
	/// (opaque `MessagePayload` used in delivery and this `DispatchPayload` used in dispatch).
	type DispatchPayload: Decode;

	/// Fine-grained result of single message dispatch (for better diagnostic purposes)
	type DispatchLevelResult: Clone + sp_std::fmt::Debug + Eq;

	/// Returns `true` if dispatcher is ready to accept additional messages. The `false` should
	/// be treated as a hint by both dispatcher and its consumers - i.e. dispatcher shall not
	/// simply drop messages if it returns `false`. The consumer may still call the `dispatch`
	/// if dispatcher has returned `false`.
	///
	/// We check it in the messages delivery transaction prologue. So if it becomes `false`
	/// after some portion of messages is already dispatched, it doesn't fail the whole transaction.
	fn is_active() -> bool;

	/// Estimate dispatch weight.
	///
	/// This function must return correct upper bound of dispatch weight. The return value
	/// of this function is expected to match return value of the corresponding
	/// `From<Chain>InboundLaneApi::message_details().dispatch_weight` call.
	fn dispatch_weight(message: &mut DispatchMessage<Self::DispatchPayload>) -> Weight;

	/// Called when inbound message is received.
	///
	/// It is up to the implementers of this trait to determine whether the message
	/// is invalid (i.e. improperly encoded, has too large weight, ...) or not.
	fn dispatch(
		message: DispatchMessage<Self::DispatchPayload>,
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

impl<DispatchPayload: Decode> From<Message> for DispatchMessage<DispatchPayload> {
	fn from(message: Message) -> Self {
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

/// Structure that may be used in place of `SourceHeaderChain` and `MessageDispatch` on chains,
/// where inbound messages are forbidden.
pub struct ForbidInboundMessages<MessagesProof, DispatchPayload>(
	PhantomData<(MessagesProof, DispatchPayload)>,
);

/// Error message that is used in `ForbidInboundMessages` implementation.
const ALL_INBOUND_MESSAGES_REJECTED: &str =
	"This chain is configured to reject all inbound messages";

impl<MessagesProof: Parameter + Size, DispatchPayload> SourceHeaderChain
	for ForbidInboundMessages<MessagesProof, DispatchPayload>
{
	type MessagesProof = MessagesProof;

	fn verify_messages_proof(
		_proof: Self::MessagesProof,
		_messages_count: u32,
	) -> Result<ProvedMessages<Message>, VerificationError> {
		Err(VerificationError::Other(ALL_INBOUND_MESSAGES_REJECTED))
	}
}

impl<MessagesProof, DispatchPayload: Decode> MessageDispatch
	for ForbidInboundMessages<MessagesProof, DispatchPayload>
{
	type DispatchPayload = DispatchPayload;
	type DispatchLevelResult = ();

	fn is_active() -> bool {
		false
	}

	fn dispatch_weight(_message: &mut DispatchMessage<Self::DispatchPayload>) -> Weight {
		Weight::MAX
	}

	fn dispatch(
		_: DispatchMessage<Self::DispatchPayload>,
	) -> MessageDispatchResult<Self::DispatchLevelResult> {
		MessageDispatchResult { unspent_weight: Weight::zero(), dispatch_level_result: () }
	}
}
