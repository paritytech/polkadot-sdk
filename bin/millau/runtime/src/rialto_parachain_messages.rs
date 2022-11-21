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

//! Everything required to serve Millau <-> RialtoParachain messages.

use crate::{Runtime, RuntimeCall, RuntimeOrigin, WithRialtoParachainsInstance};

use bp_messages::{
	source_chain::TargetHeaderChain,
	target_chain::{ProvedMessages, SourceHeaderChain},
	InboundLaneData, LaneId, Message, MessageNonce,
};
use bp_runtime::{ChainId, MILLAU_CHAIN_ID, RIALTO_PARACHAIN_CHAIN_ID};
use bridge_runtime_common::messages::{self, MessageBridge};
use frame_support::{parameter_types, weights::Weight, RuntimeDebug};

/// Default lane that is used to send messages to Rialto parachain.
pub const XCM_LANE: LaneId = [0, 0, 0, 0];
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

/// Message payload for Millau -> RialtoParachain messages.
pub type ToRialtoParachainMessagePayload = messages::source::FromThisChainMessagePayload;

/// Message verifier for Millau -> RialtoParachain messages.
pub type ToRialtoParachainMessageVerifier =
	messages::source::FromThisChainMessageVerifier<WithRialtoParachainMessageBridge>;

/// Message payload for RialtoParachain -> Millau messages.
pub type FromRialtoParachainMessagePayload =
	messages::target::FromBridgedChainMessagePayload<RuntimeCall>;

/// Messages proof for RialtoParachain -> Millau messages.
type FromRialtoParachainMessagesProof =
	messages::target::FromBridgedChainMessagesProof<bp_rialto_parachain::Hash>;

/// Messages delivery proof for Millau -> RialtoParachain messages.
type ToRialtoParachainMessagesDeliveryProof =
	messages::source::FromBridgedChainMessagesDeliveryProof<bp_rialto_parachain::Hash>;

/// Call-dispatch based message dispatch for RialtoParachain -> Millau messages.
pub type FromRialtoParachainMessageDispatch = messages::target::FromBridgedChainMessageDispatch<
	WithRialtoParachainMessageBridge,
	xcm_executor::XcmExecutor<crate::xcm_config::XcmConfig>,
	crate::xcm_config::XcmWeigher,
	WeightCredit,
>;

/// Maximal outbound payload size of Millau -> RialtoParachain messages.
pub type ToRialtoParachainMaximalOutboundPayloadSize =
	messages::source::FromThisChainMaximalOutboundPayloadSize<WithRialtoParachainMessageBridge>;

/// Millau <-> RialtoParachain message bridge.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct WithRialtoParachainMessageBridge;

impl MessageBridge for WithRialtoParachainMessageBridge {
	const THIS_CHAIN_ID: ChainId = MILLAU_CHAIN_ID;
	const BRIDGED_CHAIN_ID: ChainId = RIALTO_PARACHAIN_CHAIN_ID;
	const BRIDGED_MESSAGES_PALLET_NAME: &'static str = bp_millau::WITH_MILLAU_MESSAGES_PALLET_NAME;

	type ThisChain = Millau;
	type BridgedChain = RialtoParachain;
	type BridgedHeaderChain = pallet_bridge_parachains::ParachainHeaders<
		Runtime,
		WithRialtoParachainsInstance,
		bp_rialto_parachain::RialtoParachain,
	>;
}

/// Millau chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct Millau;

impl messages::UnderlyingChainProvider for Millau {
	type Chain = bp_millau::Millau;
}

impl messages::ThisChainWithMessages for Millau {
	type RuntimeCall = RuntimeCall;
	type RuntimeOrigin = RuntimeOrigin;

	fn is_message_accepted(_send_origin: &Self::RuntimeOrigin, _lane: &LaneId) -> bool {
		true
	}

	fn maximal_pending_messages_at_outbound_lane() -> MessageNonce {
		MessageNonce::MAX
	}
}

/// RialtoParachain chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct RialtoParachain;

impl messages::UnderlyingChainProvider for RialtoParachain {
	type Chain = bp_rialto_parachain::RialtoParachain;
}

impl messages::BridgedChainWithMessages for RialtoParachain {
	fn verify_dispatch_weight(_message_payload: &[u8]) -> bool {
		true
	}
}

impl TargetHeaderChain<ToRialtoParachainMessagePayload, bp_millau::AccountId> for RialtoParachain {
	type Error = &'static str;
	// The proof is:
	// - hash of the header this proof has been created with;
	// - the storage proof or one or several keys;
	// - id of the lane we prove state of.
	type MessagesDeliveryProof = ToRialtoParachainMessagesDeliveryProof;

	fn verify_message(payload: &ToRialtoParachainMessagePayload) -> Result<(), Self::Error> {
		messages::source::verify_chain_message::<WithRialtoParachainMessageBridge>(payload)
	}

	fn verify_messages_delivery_proof(
		proof: Self::MessagesDeliveryProof,
	) -> Result<(LaneId, InboundLaneData<bp_millau::AccountId>), Self::Error> {
		messages::source::verify_messages_delivery_proof::<WithRialtoParachainMessageBridge>(proof)
	}
}

impl SourceHeaderChain for RialtoParachain {
	type Error = &'static str;
	// The proof is:
	// - hash of the header this proof has been created with;
	// - the storage proof or one or several keys;
	// - id of the lane we prove messages for;
	// - inclusive range of messages nonces that are proved.
	type MessagesProof = FromRialtoParachainMessagesProof;

	fn verify_messages_proof(
		proof: Self::MessagesProof,
		messages_count: u32,
	) -> Result<ProvedMessages<Message>, Self::Error> {
		messages::target::verify_messages_proof::<WithRialtoParachainMessageBridge>(
			proof,
			messages_count,
		)
		.map_err(Into::into)
	}
}
