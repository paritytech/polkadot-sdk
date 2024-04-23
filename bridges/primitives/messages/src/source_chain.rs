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

use crate::{InboundLaneData, LaneId, MessageNonce, VerificationError};

use crate::UnrewardedRelayer;
use bp_runtime::Size;
use frame_support::Parameter;
use sp_core::RuntimeDebug;
use sp_std::{
	collections::{btree_map::BTreeMap, vec_deque::VecDeque},
	fmt::Debug,
	ops::RangeInclusive,
};

/// Number of messages, delivered by relayers.
pub type RelayersRewards<AccountId> = BTreeMap<AccountId, MessageNonce>;

/// Target chain API. Used by source chain to verify target chain proofs.
///
/// All implementations of this trait should only work with finalized data that
/// can't change. Wrong implementation may lead to invalid lane states (i.e. lane
/// that's stuck) and/or processing messages without paying fees.
///
/// The `Payload` type here means the payload of the message that is sent from the
/// source chain to the target chain. The `AccountId` type here means the account
/// type used by the source chain.
pub trait TargetHeaderChain<Payload, AccountId> {
	/// Proof that messages have been received by target chain.
	type MessagesDeliveryProof: Parameter + Size;

	/// Verify message payload before we accept it.
	///
	/// **CAUTION**: this is very important function. Incorrect implementation may lead
	/// to stuck lanes and/or relayers loses.
	///
	/// The proper implementation must ensure that the delivery-transaction with this
	/// payload would (at least) be accepted into target chain transaction pool AND
	/// eventually will be successfully mined. The most obvious incorrect implementation
	/// example would be implementation for BTC chain that accepts payloads larger than
	/// 1MB. BTC nodes aren't accepting transactions that are larger than 1MB, so relayer
	/// will be unable to craft valid transaction => this (and all subsequent) messages will
	/// never be delivered.
	fn verify_message(payload: &Payload) -> Result<(), VerificationError>;

	/// Verify messages delivery proof and return lane && nonce of the latest received message.
	fn verify_messages_delivery_proof(
		proof: Self::MessagesDeliveryProof,
	) -> Result<(LaneId, InboundLaneData<AccountId>), VerificationError>;
}

/// Manages payments that are happening at the source chain during delivery confirmation
/// transaction.
pub trait DeliveryConfirmationPayments<AccountId> {
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

impl<AccountId> DeliveryConfirmationPayments<AccountId> for () {
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
pub trait OnMessagesDelivered {
	/// New messages delivery has been confirmed.
	///
	/// The only argument of the function is the number of yet undelivered messages
	fn on_messages_delivered(lane: LaneId, enqueued_messages: MessageNonce);
}

impl OnMessagesDelivered for () {
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
pub trait MessagesBridge<Payload> {
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

/// Structure that may be used in place of `TargetHeaderChain` and
/// `MessageDeliveryAndDispatchPayment` on chains, where outbound messages are forbidden.
pub struct ForbidOutboundMessages;

/// Error message that is used in `ForbidOutboundMessages` implementation.
const ALL_OUTBOUND_MESSAGES_REJECTED: &str =
	"This chain is configured to reject all outbound messages";

impl<Payload, AccountId> TargetHeaderChain<Payload, AccountId> for ForbidOutboundMessages {
	type MessagesDeliveryProof = ();

	fn verify_message(_payload: &Payload) -> Result<(), VerificationError> {
		Err(VerificationError::Other(ALL_OUTBOUND_MESSAGES_REJECTED))
	}

	fn verify_messages_delivery_proof(
		_proof: Self::MessagesDeliveryProof,
	) -> Result<(LaneId, InboundLaneData<AccountId>), VerificationError> {
		Err(VerificationError::Other(ALL_OUTBOUND_MESSAGES_REJECTED))
	}
}

impl<AccountId> DeliveryConfirmationPayments<AccountId> for ForbidOutboundMessages {
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
