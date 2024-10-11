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

//! Defines structures related to calls of the `pallet-bridge-messages` pallet.

use crate::{MessageNonce, UnrewardedRelayersState};

use codec::{Decode, Encode};
use frame_support::weights::Weight;
use scale_info::TypeInfo;
use sp_core::RuntimeDebug;
use sp_std::ops::RangeInclusive;

/// A minimized version of `pallet-bridge-messages::Call` that can be used without a runtime.
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum BridgeMessagesCall<AccountId, MessagesProof, MessagesDeliveryProof> {
	/// `pallet-bridge-messages::Call::receive_messages_proof`
	#[codec(index = 2)]
	receive_messages_proof {
		/// Account id of relayer at the **bridged** chain.
		relayer_id_at_bridged_chain: AccountId,
		/// Messages proof.
		proof: MessagesProof,
		/// A number of messages in the proof.
		messages_count: u32,
		/// Total dispatch weight of messages in the proof.
		dispatch_weight: Weight,
	},
	/// `pallet-bridge-messages::Call::receive_messages_delivery_proof`
	#[codec(index = 3)]
	receive_messages_delivery_proof {
		/// Messages delivery proof.
		proof: MessagesDeliveryProof,
		/// "Digest" of unrewarded relayers state at the bridged chain.
		relayers_state: UnrewardedRelayersState,
	},
}

/// Generic info about a messages delivery/confirmation proof.
#[derive(PartialEq, RuntimeDebug)]
pub struct BaseMessagesProofInfo<LaneId> {
	/// Message lane, used by the call.
	pub lane_id: LaneId,
	/// Nonces of messages, included in the call.
	///
	/// For delivery transaction, it is nonces of bundled messages. For confirmation
	/// transaction, it is nonces that are to be confirmed during the call.
	pub bundled_range: RangeInclusive<MessageNonce>,
	/// Nonce of the best message, stored by this chain before the call is dispatched.
	///
	/// For delivery transaction, it is the nonce of best delivered message before the call.
	/// For confirmation transaction, it is the nonce of best confirmed message before the call.
	pub best_stored_nonce: MessageNonce,
}

impl<LaneId> BaseMessagesProofInfo<LaneId> {
	/// Returns true if `bundled_range` continues the `0..=best_stored_nonce` range.
	pub fn appends_to_stored_nonce(&self) -> bool {
		Some(*self.bundled_range.start()) == self.best_stored_nonce.checked_add(1)
	}
}

/// Occupation state of the unrewarded relayers vector.
#[derive(PartialEq, RuntimeDebug)]
#[cfg_attr(test, derive(Default))]
pub struct UnrewardedRelayerOccupation {
	/// The number of remaining unoccupied entries for new relayers.
	pub free_relayer_slots: MessageNonce,
	/// The number of messages that we are ready to accept.
	pub free_message_slots: MessageNonce,
}

/// Info about a `ReceiveMessagesProof` call which tries to update a single lane.
#[derive(PartialEq, RuntimeDebug)]
pub struct ReceiveMessagesProofInfo<LaneId> {
	/// Base messages proof info
	pub base: BaseMessagesProofInfo<LaneId>,
	/// State of unrewarded relayers vector.
	pub unrewarded_relayers: UnrewardedRelayerOccupation,
}

impl<LaneId> ReceiveMessagesProofInfo<LaneId> {
	/// Returns true if:
	///
	/// - either inbound lane is ready to accept bundled messages;
	///
	/// - or there are no bundled messages, but the inbound lane is blocked by too many unconfirmed
	///   messages and/or unrewarded relayers.
	pub fn is_obsolete(&self, is_dispatcher_active: bool) -> bool {
		// if dispatcher is inactive, we don't accept any delivery transactions
		if !is_dispatcher_active {
			return true
		}

		// transactions with zero bundled nonces are not allowed, unless they're message
		// delivery transactions, which brings reward confirmations required to unblock
		// the lane
		if self.base.bundled_range.is_empty() {
			let empty_transactions_allowed =
				// we allow empty transactions when we can't accept delivery from new relayers
				self.unrewarded_relayers.free_relayer_slots == 0 ||
				// or if we can't accept new messages at all
				self.unrewarded_relayers.free_message_slots == 0;

			return !empty_transactions_allowed
		}

		// otherwise we require bundled messages to continue stored range
		!self.base.appends_to_stored_nonce()
	}
}

/// Info about a `ReceiveMessagesDeliveryProof` call which tries to update a single lane.
#[derive(PartialEq, RuntimeDebug)]
pub struct ReceiveMessagesDeliveryProofInfo<LaneId>(pub BaseMessagesProofInfo<LaneId>);

impl<LaneId> ReceiveMessagesDeliveryProofInfo<LaneId> {
	/// Returns true if outbound lane is ready to accept confirmations of bundled messages.
	pub fn is_obsolete(&self) -> bool {
		self.0.bundled_range.is_empty() || !self.0.appends_to_stored_nonce()
	}
}

/// Info about a `ReceiveMessagesProof` or a `ReceiveMessagesDeliveryProof` call
/// which tries to update a single lane.
#[derive(PartialEq, RuntimeDebug)]
pub enum MessagesCallInfo<LaneId: Clone + Copy> {
	/// Messages delivery call info.
	ReceiveMessagesProof(ReceiveMessagesProofInfo<LaneId>),
	/// Messages delivery confirmation call info.
	ReceiveMessagesDeliveryProof(ReceiveMessagesDeliveryProofInfo<LaneId>),
}

impl<LaneId: Clone + Copy> MessagesCallInfo<LaneId> {
	/// Returns lane, used by the call.
	pub fn lane_id(&self) -> LaneId {
		match *self {
			Self::ReceiveMessagesProof(ref info) => info.base.lane_id,
			Self::ReceiveMessagesDeliveryProof(ref info) => info.0.lane_id,
		}
	}

	/// Returns range of messages, bundled with the call.
	pub fn bundled_messages(&self) -> RangeInclusive<MessageNonce> {
		match *self {
			Self::ReceiveMessagesProof(ref info) => info.base.bundled_range.clone(),
			Self::ReceiveMessagesDeliveryProof(ref info) => info.0.bundled_range.clone(),
		}
	}
}
