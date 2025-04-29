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

//! Primitives of messages module, that are used on the source chain.

use crate::{MessageNonce, UnrewardedRelayer};

use bp_runtime::{raw_storage_proof_size, RawStorageProof, Size};
use codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;
use sp_core::RuntimeDebug;
use sp_std::{
	collections::{btree_map::BTreeMap, vec_deque::VecDeque},
	fmt::Debug,
	ops::RangeInclusive,
};

/// Messages delivery proof from the bridged chain.
///
/// It contains everything required to prove that our (this chain) messages have been
/// delivered to the bridged (target) chain:
///
/// - hash of finalized header;
///
/// - storage proof of the inbound lane state;
///
/// - lane id.
#[derive(Clone, Decode, DecodeWithMemTracking, Encode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
pub struct FromBridgedChainMessagesDeliveryProof<BridgedHeaderHash, LaneId> {
	/// Hash of the bridge header the proof is for.
	pub bridged_header_hash: BridgedHeaderHash,
	/// Storage trie proof generated for [`Self::bridged_header_hash`].
	pub storage_proof: RawStorageProof,
	/// Lane id of which messages were delivered and the proof is for.
	pub lane: LaneId,
}

impl<BridgedHeaderHash, LaneId> Size
	for FromBridgedChainMessagesDeliveryProof<BridgedHeaderHash, LaneId>
{
	fn size(&self) -> u32 {
		use frame_support::sp_runtime::SaturatedConversion;
		raw_storage_proof_size(&self.storage_proof).saturated_into()
	}
}

/// Number of messages, delivered by relayers.
pub type RelayersRewards<AccountId> = BTreeMap<AccountId, MessageNonce>;

/// Manages payments that are happening at the source chain during delivery confirmation
/// transaction.
pub trait DeliveryConfirmationPayments<AccountId, LaneId> {
	/// Error type.
	type Error: Debug + Into<&'static str>;

	/// Pay rewards for delivering messages to the given relayers.
	///
	/// The implementation may also choose to pay reward to the `confirmation_relayer`, which is
	/// a relayer that has submitted delivery confirmation transaction.
	///
	/// Returns number of actually rewarded relayers.
	fn pay_reward(
		lane_id: LaneId,
		messages_relayers: VecDeque<UnrewardedRelayer<AccountId>>,
		confirmation_relayer: &AccountId,
		received_range: &RangeInclusive<MessageNonce>,
	) -> MessageNonce;
}

impl<AccountId, LaneId> DeliveryConfirmationPayments<AccountId, LaneId> for () {
	type Error = &'static str;

	fn pay_reward(
		_lane_id: LaneId,
		_messages_relayers: VecDeque<UnrewardedRelayer<AccountId>>,
		_confirmation_relayer: &AccountId,
		_received_range: &RangeInclusive<MessageNonce>,
	) -> MessageNonce {
		// this implementation is not rewarding relayers at all
		0
	}
}

/// Callback that is called at the source chain (bridge hub) when we get delivery confirmation
/// for new messages.
pub trait OnMessagesDelivered<LaneId> {
	/// New messages delivery has been confirmed.
	///
	/// The only argument of the function is the number of yet undelivered messages
	fn on_messages_delivered(lane: LaneId, enqueued_messages: MessageNonce);
}

impl<LaneId> OnMessagesDelivered<LaneId> for () {
	fn on_messages_delivered(_lane: LaneId, _enqueued_messages: MessageNonce) {}
}

/// Send message artifacts.
#[derive(Eq, RuntimeDebug, PartialEq)]
pub struct SendMessageArtifacts {
	/// Nonce of the message.
	pub nonce: MessageNonce,
	/// Number of enqueued messages at the lane, after the message is sent.
	pub enqueued_messages: MessageNonce,
}

/// Messages bridge API to be used from other pallets.
pub trait MessagesBridge<Payload, LaneId> {
	/// Error type.
	type Error: Debug;

	/// Intermediary structure returned by `validate_message()`.
	///
	/// It can than be passed to `send_message()` in order to actually send the message
	/// on the bridge.
	type SendMessageArgs;

	/// Check if the message can be sent over the bridge.
	fn validate_message(
		lane: LaneId,
		message: &Payload,
	) -> Result<Self::SendMessageArgs, Self::Error>;

	/// Send message over the bridge.
	///
	/// Returns unique message nonce or error if send has failed.
	fn send_message(message: Self::SendMessageArgs) -> SendMessageArtifacts;
}

/// Structure that may be used in place `MessageDeliveryAndDispatchPayment` on chains,
/// where outbound messages are forbidden.
pub struct ForbidOutboundMessages;

impl<AccountId, LaneId> DeliveryConfirmationPayments<AccountId, LaneId> for ForbidOutboundMessages {
	type Error = &'static str;

	fn pay_reward(
		_lane_id: LaneId,
		_messages_relayers: VecDeque<UnrewardedRelayer<AccountId>>,
		_confirmation_relayer: &AccountId,
		_received_range: &RangeInclusive<MessageNonce>,
	) -> MessageNonce {
		0
	}
}
