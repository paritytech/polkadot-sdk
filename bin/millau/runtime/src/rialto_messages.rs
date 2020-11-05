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

//! Everything required to serve Millau <-> Rialto message lanes.

use crate::Runtime;

use bp_message_lane::{
	source_chain::TargetHeaderChain,
	target_chain::{ProvedMessages, SourceHeaderChain},
	InboundLaneData, LaneId, Message, MessageNonce,
};
use bp_runtime::InstanceId;
use bridge_runtime_common::messages::{self, ChainWithMessageLanes, MessageBridge};
use frame_support::{
	weights::{Weight, WeightToFeePolynomial},
	RuntimeDebug,
};
use sp_core::storage::StorageKey;
use sp_trie::StorageProof;

/// Storage key of the Millau -> Rialto message in the runtime storage.
pub fn message_key(lane: &LaneId, nonce: MessageNonce) -> StorageKey {
	pallet_message_lane::storage_keys::message_key::<Runtime, <Millau as ChainWithMessageLanes>::MessageLaneInstance>(
		lane, nonce,
	)
}

/// Storage key of the Millau -> Rialto message lane state in the runtime storage.
pub fn outbound_lane_data_key(lane: &LaneId) -> StorageKey {
	pallet_message_lane::storage_keys::outbound_lane_data_key::<<Millau as ChainWithMessageLanes>::MessageLaneInstance>(
		lane,
	)
}

/// Storage key of the Rialto -> Millau message lane state in the runtime storage.
pub fn inbound_lane_data_key(lane: &LaneId) -> StorageKey {
	pallet_message_lane::storage_keys::inbound_lane_data_key::<
		Runtime,
		<Millau as ChainWithMessageLanes>::MessageLaneInstance,
	>(lane)
}

/// Message payload for Millau -> Rialto messages.
pub type ToRialtoMessagePayload = messages::source::FromThisChainMessagePayload<WithRialtoMessageBridge>;

/// Message verifier for Millau -> Rialto messages.
pub type ToRialtoMessageVerifier = messages::source::FromThisChainMessageVerifier<WithRialtoMessageBridge>;

/// Message payload for Rialto -> Millau messages.
pub type FromRialtoMessagePayload = messages::target::FromBridgedChainMessagePayload<WithRialtoMessageBridge>;

/// Call-dispatch based message dispatch for Rialto -> Millau messages.
pub type FromRialtoMessageDispatch = messages::target::FromBridgedChainMessageDispatch<
	WithRialtoMessageBridge,
	crate::Runtime,
	pallet_bridge_call_dispatch::DefaultInstance,
>;

/// Millau <-> Rialto message bridge.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct WithRialtoMessageBridge;

impl MessageBridge for WithRialtoMessageBridge {
	const INSTANCE: InstanceId = *b"rlto";

	const RELAYER_FEE_PERCENT: u32 = 10;

	type ThisChain = Millau;
	type BridgedChain = Rialto;

	fn maximal_dispatch_weight_of_message_on_bridged_chain() -> Weight {
		// we don't want to relay too large messages + keep reserve for future upgrades
		bp_rialto::MAXIMUM_EXTRINSIC_WEIGHT / 2
	}

	fn weight_of_delivery_transaction() -> Weight {
		0 // TODO: https://github.com/paritytech/parity-bridges-common/issues/391
	}

	fn weight_of_delivery_confirmation_transaction_on_this_chain() -> Weight {
		0 // TODO: https://github.com/paritytech/parity-bridges-common/issues/391
	}

	fn weight_of_reward_confirmation_transaction_on_target_chain() -> Weight {
		0 // TODO: https://github.com/paritytech/parity-bridges-common/issues/391
	}

	fn this_weight_to_balance(weight: Weight) -> bp_rialto::Balance {
		<crate::Runtime as pallet_transaction_payment::Trait>::WeightToFee::calc(&weight)
	}

	fn bridged_weight_to_balance(weight: Weight) -> bp_millau::Balance {
		// we're using the same weights in both chains now
		<crate::Runtime as pallet_transaction_payment::Trait>::WeightToFee::calc(&weight)
	}

	fn this_chain_balance_to_bridged_chain_balance(this_balance: bp_rialto::Balance) -> bp_millau::Balance {
		// 1:1 conversion that will probably change in the future
		this_balance
	}
}

/// Millau chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct Millau;

impl messages::ChainWithMessageLanes for Millau {
	type Hash = bp_millau::Hash;
	type AccountId = bp_millau::AccountId;
	type Signer = bp_millau::AccountSigner;
	type Signature = bp_millau::Signature;
	type Call = crate::Call;
	type Weight = Weight;
	type Balance = bp_millau::Balance;

	type MessageLaneInstance = pallet_message_lane::DefaultInstance;
}

/// Rialto chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct Rialto;

impl messages::ChainWithMessageLanes for Rialto {
	type Hash = bp_rialto::Hash;
	type AccountId = bp_rialto::AccountId;
	type Signer = bp_rialto::AccountSigner;
	type Signature = bp_rialto::Signature;
	type Call = (); // unknown to us
	type Weight = Weight;
	type Balance = bp_rialto::Balance;

	type MessageLaneInstance = pallet_message_lane::DefaultInstance;
}

impl TargetHeaderChain<ToRialtoMessagePayload, bp_rialto::AccountId> for Rialto {
	type Error = &'static str;
	// The proof is:
	// - hash of the header this proof has been created with;
	// - the storage proof or one or several keys;
	// - id of the lane we prove state of.
	type MessagesDeliveryProof = (bp_rialto::Hash, StorageProof, LaneId);

	fn verify_message(payload: &ToRialtoMessagePayload) -> Result<(), Self::Error> {
		if payload.weight > WithRialtoMessageBridge::maximal_dispatch_weight_of_message_on_bridged_chain() {
			return Err("Too large weight declared");
		}

		Ok(())
	}

	fn verify_messages_delivery_proof(
		_proof: Self::MessagesDeliveryProof,
	) -> Result<(LaneId, InboundLaneData<bp_rialto::AccountId>), Self::Error> {
		unimplemented!("https://github.com/paritytech/parity-bridges-common/issues/397")
	}
}

impl SourceHeaderChain<bp_rialto::Balance> for Rialto {
	type Error = &'static str;
	// The proof is:
	// - hash of the header this proof has been created with;
	// - the storage proof or one or several keys;
	// - id of the lane we prove messages for;
	// - inclusive range of messages nonces that are proved.
	type MessagesProof = (bp_rialto::Hash, StorageProof, LaneId, MessageNonce, MessageNonce);

	fn verify_messages_proof(
		_proof: Self::MessagesProof,
	) -> Result<ProvedMessages<Message<bp_rialto::Balance>>, Self::Error> {
		unimplemented!("https://github.com/paritytech/parity-bridges-common/issues/397")
	}
}
