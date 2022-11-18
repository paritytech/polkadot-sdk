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

//! Everything required to serve Millau <-> Rialto messages.

use crate::{OriginCaller, RialtoGrandpaInstance, Runtime, RuntimeCall, RuntimeOrigin};

use bp_messages::{
	source_chain::TargetHeaderChain,
	target_chain::{ProvedMessages, SourceHeaderChain},
	InboundLaneData, LaneId, Message, MessageNonce,
};
use bp_runtime::{ChainId, MILLAU_CHAIN_ID, RIALTO_CHAIN_ID};
use bridge_runtime_common::messages::{self, MessageBridge};
use frame_support::{parameter_types, weights::Weight, RuntimeDebug};

/// Default lane that is used to send messages to Rialto.
pub const DEFAULT_XCM_LANE_TO_RIALTO: LaneId = [0, 0, 0, 0];
/// Weight of 2 XCM instructions is for simple `Trap(42)` program, coming through bridge
/// (it is prepended with `UniversalOrigin` instruction). It is used just for simplest manual
/// tests, confirming that we don't break encoding somewhere between.
pub const BASE_XCM_WEIGHT_TWICE: u64 = 2 * crate::xcm_config::BASE_XCM_WEIGHT;

parameter_types! {
	/// Weight credit for our test messages.
	///
	/// 2 XCM instructions is for simple `Trap(42)` program, coming through bridge
	/// (it is prepended with `UniversalOrigin` instruction).
	pub const WeightCredit: Weight = Weight::from_ref_time(BASE_XCM_WEIGHT_TWICE);
}

/// Message payload for Millau -> Rialto messages.
pub type ToRialtoMessagePayload = messages::source::FromThisChainMessagePayload;

/// Message verifier for Millau -> Rialto messages.
pub type ToRialtoMessageVerifier =
	messages::source::FromThisChainMessageVerifier<WithRialtoMessageBridge>;

/// Message payload for Rialto -> Millau messages.
pub type FromRialtoMessagePayload = messages::target::FromBridgedChainMessagePayload<RuntimeCall>;

/// Messages proof for Rialto -> Millau messages.
pub type FromRialtoMessagesProof = messages::target::FromBridgedChainMessagesProof<bp_rialto::Hash>;

/// Messages delivery proof for Millau -> Rialto messages.
pub type ToRialtoMessagesDeliveryProof =
	messages::source::FromBridgedChainMessagesDeliveryProof<bp_rialto::Hash>;

/// Call-dispatch based message dispatch for Rialto -> Millau messages.
pub type FromRialtoMessageDispatch = messages::target::FromBridgedChainMessageDispatch<
	WithRialtoMessageBridge,
	xcm_executor::XcmExecutor<crate::xcm_config::XcmConfig>,
	crate::xcm_config::XcmWeigher,
	WeightCredit,
>;

/// Maximal outbound payload size of Millau -> Rialto messages.
pub type ToRialtoMaximalOutboundPayloadSize =
	messages::source::FromThisChainMaximalOutboundPayloadSize<WithRialtoMessageBridge>;

/// Millau <-> Rialto message bridge.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct WithRialtoMessageBridge;

impl MessageBridge for WithRialtoMessageBridge {
	const THIS_CHAIN_ID: ChainId = MILLAU_CHAIN_ID;
	const BRIDGED_CHAIN_ID: ChainId = RIALTO_CHAIN_ID;
	const BRIDGED_MESSAGES_PALLET_NAME: &'static str = bp_millau::WITH_MILLAU_MESSAGES_PALLET_NAME;

	type ThisChain = Millau;
	type BridgedChain = Rialto;
	type BridgedHeaderChain =
		pallet_bridge_grandpa::GrandpaChainHeaders<Runtime, RialtoGrandpaInstance>;
}

/// Millau chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct Millau;

impl messages::UnderlyingChainProvider for Millau {
	type Chain = bp_millau::Millau;
}

impl messages::ThisChainWithMessages for Millau {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;

	fn is_message_accepted(send_origin: &Self::RuntimeOrigin, lane: &LaneId) -> bool {
		let here_location =
			xcm::v3::MultiLocation::from(crate::xcm_config::UniversalLocation::get());
		match send_origin.caller {
			OriginCaller::XcmPallet(pallet_xcm::Origin::Xcm(ref location))
				if *location == here_location =>
			{
				log::trace!(target: "runtime::bridge", "Verifying message sent using XCM pallet to Rialto");
			},
			_ => {
				// keep in mind that in this case all messages are free (in term of fees)
				// => it's just to keep testing bridge on our test deployments until we'll have a
				// better option
				log::trace!(target: "runtime::bridge", "Verifying message sent using messages pallet to Rialto");
			},
		}

		*lane == DEFAULT_XCM_LANE_TO_RIALTO || *lane == [0, 0, 0, 1]
	}

	fn maximal_pending_messages_at_outbound_lane() -> MessageNonce {
		MessageNonce::MAX
	}
}

/// Rialto chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct Rialto;

impl messages::UnderlyingChainProvider for Rialto {
	type Chain = bp_rialto::Rialto;
}

impl messages::BridgedChainWithMessages for Rialto {
	fn verify_dispatch_weight(_message_payload: &[u8]) -> bool {
		true
	}
}

impl TargetHeaderChain<ToRialtoMessagePayload, bp_millau::AccountId> for Rialto {
	type Error = &'static str;
	// The proof is:
	// - hash of the header this proof has been created with;
	// - the storage proof or one or several keys;
	// - id of the lane we prove state of.
	type MessagesDeliveryProof = ToRialtoMessagesDeliveryProof;

	fn verify_message(payload: &ToRialtoMessagePayload) -> Result<(), Self::Error> {
		messages::source::verify_chain_message::<WithRialtoMessageBridge>(payload)
	}

	fn verify_messages_delivery_proof(
		proof: Self::MessagesDeliveryProof,
	) -> Result<(LaneId, InboundLaneData<bp_millau::AccountId>), Self::Error> {
		messages::source::verify_messages_delivery_proof::<WithRialtoMessageBridge>(proof)
	}
}

impl SourceHeaderChain for Rialto {
	type Error = &'static str;
	// The proof is:
	// - hash of the header this proof has been created with;
	// - the storage proof or one or several keys;
	// - id of the lane we prove messages for;
	// - inclusive range of messages nonces that are proved.
	type MessagesProof = FromRialtoMessagesProof;

	fn verify_messages_proof(
		proof: Self::MessagesProof,
		messages_count: u32,
	) -> Result<ProvedMessages<Message>, Self::Error> {
		messages::target::verify_messages_proof::<WithRialtoMessageBridge>(proof, messages_count)
			.map_err(Into::into)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{Runtime, WithRialtoMessagesInstance};

	use bp_runtime::Chain;
	use bridge_runtime_common::{
		assert_complete_bridge_types,
		integrity::{
			assert_complete_bridge_constants, AssertBridgeMessagesPalletConstants,
			AssertBridgePalletNames, AssertChainConstants, AssertCompleteBridgeConstants,
		},
		messages,
	};

	#[test]
	fn ensure_millau_message_lane_weights_are_correct() {
		type Weights = pallet_bridge_messages::weights::BridgeWeight<Runtime>;

		pallet_bridge_messages::ensure_weights_are_correct::<Weights>();

		let max_incoming_message_proof_size = bp_rialto::EXTRA_STORAGE_PROOF_SIZE.saturating_add(
			messages::target::maximal_incoming_message_size(bp_millau::Millau::max_extrinsic_size()),
		);
		pallet_bridge_messages::ensure_able_to_receive_message::<Weights>(
			bp_millau::Millau::max_extrinsic_size(),
			bp_millau::Millau::max_extrinsic_weight(),
			max_incoming_message_proof_size,
			messages::target::maximal_incoming_message_dispatch_weight(
				bp_millau::Millau::max_extrinsic_weight(),
			),
		);

		let max_incoming_inbound_lane_data_proof_size =
			bp_messages::InboundLaneData::<()>::encoded_size_hint_u32(
				bp_millau::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX as _,
				bp_millau::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX as _,
			);
		pallet_bridge_messages::ensure_able_to_receive_confirmation::<Weights>(
			bp_millau::Millau::max_extrinsic_size(),
			bp_millau::Millau::max_extrinsic_weight(),
			max_incoming_inbound_lane_data_proof_size,
			bp_millau::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
			bp_millau::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
		);
	}

	#[test]
	fn ensure_bridge_integrity() {
		assert_complete_bridge_types!(
			runtime: Runtime,
			with_bridged_chain_grandpa_instance: RialtoGrandpaInstance,
			with_bridged_chain_messages_instance: WithRialtoMessagesInstance,
			bridge: WithRialtoMessageBridge,
			this_chain: bp_millau::Millau,
			bridged_chain: bp_rialto::Rialto,
		);

		assert_complete_bridge_constants::<
			Runtime,
			RialtoGrandpaInstance,
			WithRialtoMessagesInstance,
			WithRialtoMessageBridge,
			bp_millau::Millau,
		>(AssertCompleteBridgeConstants {
			this_chain_constants: AssertChainConstants {
				block_length: bp_millau::BlockLength::get(),
				block_weights: bp_millau::BlockWeights::get(),
			},
			messages_pallet_constants: AssertBridgeMessagesPalletConstants {
				max_unrewarded_relayers_in_bridged_confirmation_tx:
					bp_rialto::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
				max_unconfirmed_messages_in_bridged_confirmation_tx:
					bp_rialto::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
				bridged_chain_id: bp_runtime::RIALTO_CHAIN_ID,
			},
			pallet_names: AssertBridgePalletNames {
				with_this_chain_messages_pallet_name: bp_millau::WITH_MILLAU_MESSAGES_PALLET_NAME,
				with_bridged_chain_grandpa_pallet_name: bp_rialto::WITH_RIALTO_GRANDPA_PALLET_NAME,
				with_bridged_chain_messages_pallet_name:
					bp_rialto::WITH_RIALTO_MESSAGES_PALLET_NAME,
			},
		});
	}
}
