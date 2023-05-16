// Copyright 2022 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Bridge definitions that are used on Wococo to bridge with Rococo.

use crate::{
	BridgeParachainRococoInstance, ParachainInfo, Runtime, WithBridgeHubRococoMessagesInstance,
	XcmRouter,
};
use bp_messages::LaneId;
use bridge_runtime_common::{
	messages,
	messages::{
		source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
		MessageBridge, ThisChainWithMessages, UnderlyingChainProvider,
	},
	messages_xcm_extension::{XcmBlobHauler, XcmBlobHaulerAdapter},
	refund_relayer_extension::{
		ActualFeeRefund, RefundBridgedParachainMessages, RefundableMessagesLane,
		RefundableParachain,
	},
};
use frame_support::{parameter_types, RuntimeDebug};
use xcm::{
	latest::prelude::*,
	prelude::{InteriorMultiLocation, NetworkId},
};
use xcm_builder::{BridgeBlobDispatcher, HaulBlobExporter};

parameter_types! {
	pub const MaxUnrewardedRelayerEntriesAtInboundLane: bp_messages::MessageNonce =
		bp_bridge_hub_wococo::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	pub const MaxUnconfirmedMessagesAtInboundLane: bp_messages::MessageNonce =
		bp_bridge_hub_wococo::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
	pub const BridgeHubRococoChainId: bp_runtime::ChainId = bp_runtime::BRIDGE_HUB_ROCOCO_CHAIN_ID;
	pub BridgeHubWococoUniversalLocation: InteriorMultiLocation = X2(GlobalConsensus(Wococo), Parachain(ParachainInfo::parachain_id().into()));
	pub RococoGlobalConsensusNetwork: NetworkId = NetworkId::Rococo;
	pub ActiveOutboundLanesToBridgeHubRococo: &'static [bp_messages::LaneId] = &[DEFAULT_XCM_LANE_TO_BRIDGE_HUB_ROCOCO];
	pub PriorityBoostPerMessage: u64 = 921_900_294;
}

/// Proof of messages, coming from Rococo.
pub type FromRococoBridgeHubMessagesProof =
	FromBridgedChainMessagesProof<bp_bridge_hub_rococo::Hash>;
/// Messages delivery proof for Rococo Bridge Hub -> Wococo Bridge Hub messages.
pub type ToRococoBridgeHubMessagesDeliveryProof =
	FromBridgedChainMessagesDeliveryProof<bp_bridge_hub_rococo::Hash>;

/// Dispatches received XCM messages from other bridge
pub type OnBridgeHubWococoBlobDispatcher =
	BridgeBlobDispatcher<XcmRouter, BridgeHubWococoUniversalLocation>;

/// Export XCM messages to be relayed to the otherside
pub type ToBridgeHubRococoHaulBlobExporter = HaulBlobExporter<
	XcmBlobHaulerAdapter<ToBridgeHubRococoXcmBlobHauler>,
	RococoGlobalConsensusNetwork,
	(),
>;
pub struct ToBridgeHubRococoXcmBlobHauler;
impl XcmBlobHauler for ToBridgeHubRococoXcmBlobHauler {
	type MessageSender =
		pallet_bridge_messages::Pallet<Runtime, WithBridgeHubRococoMessagesInstance>;

	type MessageSenderOrigin = super::RuntimeOrigin;

	fn message_sender_origin() -> super::RuntimeOrigin {
		pallet_xcm::Origin::from(MultiLocation::new(1, crate::xcm_config::UniversalLocation::get()))
			.into()
	}

	fn xcm_lane() -> LaneId {
		DEFAULT_XCM_LANE_TO_BRIDGE_HUB_ROCOCO
	}
}
pub const DEFAULT_XCM_LANE_TO_BRIDGE_HUB_ROCOCO: LaneId = LaneId([0, 0, 0, 1]);

/// Messaging Bridge configuration for BridgeHubWococo -> BridgeHubRococo
pub struct WithBridgeHubRococoMessageBridge;
impl MessageBridge for WithBridgeHubRococoMessageBridge {
	const BRIDGED_MESSAGES_PALLET_NAME: &'static str =
		bp_bridge_hub_wococo::WITH_BRIDGE_HUB_WOCOCO_MESSAGES_PALLET_NAME;
	type ThisChain = BridgeHubWococo;
	type BridgedChain = BridgeHubRococo;
	type BridgedHeaderChain = pallet_bridge_parachains::ParachainHeaders<
		Runtime,
		BridgeParachainRococoInstance,
		bp_bridge_hub_rococo::BridgeHubRococo,
	>;
}

/// Message verifier for BridgeHubRococo messages sent from BridgeHubWococo
pub type ToBridgeHubRococoMessageVerifier =
	messages::source::FromThisChainMessageVerifier<WithBridgeHubRococoMessageBridge>;

/// Maximal outbound payload size of BridgeHubWococo -> BridgeHubRococo messages.
pub type ToBridgeHubRococoMaximalOutboundPayloadSize =
	messages::source::FromThisChainMaximalOutboundPayloadSize<WithBridgeHubRococoMessageBridge>;

/// BridgeHubRococo chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct BridgeHubRococo;

impl UnderlyingChainProvider for BridgeHubRococo {
	type Chain = bp_bridge_hub_rococo::BridgeHubRococo;
}

impl messages::BridgedChainWithMessages for BridgeHubRococo {}

/// BridgeHubWococo chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct BridgeHubWococo;

impl UnderlyingChainProvider for BridgeHubWococo {
	type Chain = bp_bridge_hub_wococo::BridgeHubWococo;
}

impl ThisChainWithMessages for BridgeHubWococo {
	type RuntimeOrigin = crate::RuntimeOrigin;
}

/// Signed extension that refunds relayers that are delivering messages from the Rococo parachain.
pub type BridgeRefundBridgeHubRococoMessages = RefundBridgedParachainMessages<
	Runtime,
	RefundableParachain<BridgeParachainRococoInstance, bp_bridge_hub_rococo::BridgeHubRococo>,
	RefundableMessagesLane<WithBridgeHubRococoMessagesInstance, BridgeHubRococoMessagesLane>,
	ActualFeeRefund<Runtime>,
	PriorityBoostPerMessage,
	StrBridgeRefundBridgeHubRococoMessages,
>;
bp_runtime::generate_static_str_provider!(BridgeRefundBridgeHubRococoMessages);

parameter_types! {
	pub const BridgeHubRococoMessagesLane: bp_messages::LaneId = DEFAULT_XCM_LANE_TO_BRIDGE_HUB_ROCOCO;
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::BridgeGrandpaRococoInstance;
	use bridge_runtime_common::{
		assert_complete_bridge_types,
		integrity::{
			assert_complete_bridge_constants, check_message_lane_weights,
			AssertBridgeMessagesPalletConstants, AssertBridgePalletNames, AssertChainConstants,
			AssertCompleteBridgeConstants,
		},
	};

	#[test]
	fn ensure_bridge_hub_wococo_message_lane_weights_are_correct() {
		check_message_lane_weights::<
			bp_bridge_hub_wococo::BridgeHubWococo,
			Runtime,
			WithBridgeHubRococoMessagesInstance,
		>(
			bp_bridge_hub_rococo::EXTRA_STORAGE_PROOF_SIZE,
			bp_bridge_hub_wococo::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
			bp_bridge_hub_wococo::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
			true,
		);
	}

	#[test]
	fn ensure_bridge_integrity() {
		assert_complete_bridge_types!(
			runtime: Runtime,
			with_bridged_chain_grandpa_instance: BridgeGrandpaRococoInstance,
			with_bridged_chain_messages_instance: WithBridgeHubRococoMessagesInstance,
			bridge: WithBridgeHubRococoMessageBridge,
			this_chain: bp_wococo::Wococo,
			bridged_chain: bp_rococo::Rococo,
		);

		assert_complete_bridge_constants::<
			Runtime,
			BridgeGrandpaRococoInstance,
			WithBridgeHubRococoMessagesInstance,
			WithBridgeHubRococoMessageBridge,
		>(AssertCompleteBridgeConstants {
			this_chain_constants: AssertChainConstants {
				block_length: bp_bridge_hub_wococo::BlockLength::get(),
				block_weights: bp_bridge_hub_wococo::BlockWeights::get(),
			},
			messages_pallet_constants: AssertBridgeMessagesPalletConstants {
				max_unrewarded_relayers_in_bridged_confirmation_tx:
					bp_bridge_hub_rococo::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
				max_unconfirmed_messages_in_bridged_confirmation_tx:
					bp_bridge_hub_rococo::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
				bridged_chain_id: bp_runtime::BRIDGE_HUB_ROCOCO_CHAIN_ID,
			},
			pallet_names: AssertBridgePalletNames {
				with_this_chain_messages_pallet_name:
					bp_bridge_hub_wococo::WITH_BRIDGE_HUB_WOCOCO_MESSAGES_PALLET_NAME,
				with_bridged_chain_grandpa_pallet_name: bp_rococo::WITH_ROCOCO_GRANDPA_PALLET_NAME,
				with_bridged_chain_messages_pallet_name:
					bp_bridge_hub_rococo::WITH_BRIDGE_HUB_ROCOCO_MESSAGES_PALLET_NAME,
			},
		});
	}
}
