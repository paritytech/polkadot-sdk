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

use crate::{RialtoGrandpaInstance, Runtime, RuntimeOrigin, WithRialtoMessagesInstance};

use bp_messages::LaneId;
use bridge_runtime_common::{
	messages::{
		self, source::TargetHeaderChainAdapter, target::SourceHeaderChainAdapter, MessageBridge,
	},
	messages_xcm_extension::{SenderAndLane, XcmBlobHauler, XcmBlobHaulerAdapter},
};
use frame_support::{parameter_types, weights::Weight, RuntimeDebug};
use pallet_bridge_relayers::WeightInfoExt as _;
use xcm::latest::prelude::*;
use xcm_builder::HaulBlobExporter;

/// Default lane that is used to send messages to Rialto.
pub const XCM_LANE: LaneId = LaneId([0, 0, 0, 0]);
/// Weight of 2 XCM instructions is for simple `Trap(42)` program, coming through bridge
/// (it is prepended with `UniversalOrigin` instruction). It is used just for simplest manual
/// tests, confirming that we don't break encoding somewhere between.
pub const BASE_XCM_WEIGHT_TWICE: Weight = crate::xcm_config::BaseXcmWeight::get().saturating_mul(2);

parameter_types! {
	/// Weight credit for our test messages.
	///
	/// 2 XCM instructions is for simple `Trap(42)` program, coming through bridge
	/// (it is prepended with `UniversalOrigin` instruction).
	pub const WeightCredit: Weight = BASE_XCM_WEIGHT_TWICE;
	/// Lane used by the with-Rialto bridge.
	pub RialtoSenderAndLane: SenderAndLane = SenderAndLane::new(Here.into(), XCM_LANE);

	/// Dummy message used in configuration.
	pub DummyXcmMessage: Xcm<()> = Xcm::new();
}

/// Message payload for Millau -> Rialto messages.
pub type ToRialtoMessagePayload = messages::source::FromThisChainMessagePayload;

/// Message verifier for Millau -> Rialto messages.
pub type ToRialtoMessageVerifier =
	messages::source::FromThisChainMessageVerifier<WithRialtoMessageBridge>;

/// Message payload for Rialto -> Millau messages.
pub type FromRialtoMessagePayload = messages::target::FromBridgedChainMessagePayload;

/// Messages proof for Rialto -> Millau messages.
pub type FromRialtoMessagesProof = messages::target::FromBridgedChainMessagesProof<bp_rialto::Hash>;

/// Messages delivery proof for Millau -> Rialto messages.
pub type ToRialtoMessagesDeliveryProof =
	messages::source::FromBridgedChainMessagesDeliveryProof<bp_rialto::Hash>;

/// Call-dispatch based message dispatch for Rialto -> Millau messages.
pub type FromRialtoMessageDispatch =
	bridge_runtime_common::messages_xcm_extension::XcmBlobMessageDispatch<
		crate::xcm_config::OnMillauBlobDispatcher,
		(),
		(),
	>;

/// Maximal outbound payload size of Millau -> Rialto messages.
pub type ToRialtoMaximalOutboundPayloadSize =
	messages::source::FromThisChainMaximalOutboundPayloadSize<WithRialtoMessageBridge>;

/// Millau <-> Rialto message bridge.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct WithRialtoMessageBridge;

impl MessageBridge for WithRialtoMessageBridge {
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
}

/// Rialto chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct Rialto;
/// Rialto as source header chain.
pub type RialtoAsSourceHeaderChain = SourceHeaderChainAdapter<WithRialtoMessageBridge>;
/// Rialto as target header chain.
pub type RialtoAsTargetHeaderChain = TargetHeaderChainAdapter<WithRialtoMessageBridge>;

impl messages::UnderlyingChainProvider for Rialto {
	type Chain = bp_rialto::Rialto;
}

impl messages::BridgedChainWithMessages for Rialto {}

/// Export XCM messages to be relayed to Rialto.
pub type ToRialtoBlobExporter = HaulBlobExporter<
	XcmBlobHaulerAdapter<ToRialtoXcmBlobHauler>,
	crate::xcm_config::RialtoNetwork,
	(),
>;

/// To-Rialto XCM hauler.
pub struct ToRialtoXcmBlobHauler;

impl XcmBlobHauler for ToRialtoXcmBlobHauler {
	type Runtime = Runtime;
	type MessagesInstance = WithRialtoMessagesInstance;
	type SenderAndLane = RialtoSenderAndLane;

	type ToSourceChainSender = crate::xcm_config::XcmRouter;
	type CongestedMessage = DummyXcmMessage;
	type UncongestedMessage = DummyXcmMessage;
}

impl pallet_bridge_messages::WeightInfoExt for crate::weights::RialtoMessagesWeightInfo<Runtime> {
	fn expected_extra_storage_proof_size() -> u32 {
		bp_rialto::EXTRA_STORAGE_PROOF_SIZE
	}

	fn receive_messages_proof_overhead_from_runtime() -> Weight {
		pallet_bridge_relayers::weights::BridgeWeight::<Runtime>::receive_messages_proof_overhead_from_runtime()
	}

	fn receive_messages_delivery_proof_overhead_from_runtime() -> Weight {
		pallet_bridge_relayers::weights::BridgeWeight::<Runtime>::receive_messages_delivery_proof_overhead_from_runtime()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{Runtime, WithRialtoMessagesInstance};

	use bridge_runtime_common::{
		assert_complete_bridge_types,
		integrity::{
			assert_complete_bridge_constants, check_message_lane_weights,
			AssertBridgeMessagesPalletConstants, AssertBridgePalletNames, AssertChainConstants,
			AssertCompleteBridgeConstants,
		},
	};

	#[test]
	fn ensure_millau_message_lane_weights_are_correct() {
		check_message_lane_weights::<bp_millau::Millau, Runtime, WithRialtoMessagesInstance>(
			bp_rialto::EXTRA_STORAGE_PROOF_SIZE,
			bp_millau::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
			bp_millau::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
			false,
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
