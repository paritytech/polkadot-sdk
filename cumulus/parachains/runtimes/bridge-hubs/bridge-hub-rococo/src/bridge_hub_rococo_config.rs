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

//! Bridge definitions that are used on Rococo to bridge with Wococo.

use crate::{
	BridgeParachainWococoInstance, BridgeWococoMessages, ParachainInfo, Runtime,
	WithBridgeHubWococoMessagesInstance, XcmRouter,
};
use bp_messages::LaneId;
use bridge_runtime_common::{
	messages,
	messages::{
		source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
		MessageBridge, ThisChainWithMessages, UnderlyingChainProvider,
	},
	messages_xcm_extension::{SenderAndLane, XcmBlobHauler, XcmBlobHaulerAdapter},
	refund_relayer_extension::{
		ActualFeeRefund, RefundBridgedParachainMessages, RefundableMessagesLane,
		RefundableParachain,
	},
};
use frame_support::{parameter_types, traits::PalletInfoAccess, RuntimeDebug};
use xcm::{
	latest::prelude::*,
	prelude::{InteriorMultiLocation, NetworkId},
};
use xcm_builder::{BridgeBlobDispatcher, HaulBlobExporter};

parameter_types! {
	pub const MaxUnrewardedRelayerEntriesAtInboundLane: bp_messages::MessageNonce =
		bp_bridge_hub_rococo::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	pub const MaxUnconfirmedMessagesAtInboundLane: bp_messages::MessageNonce =
		bp_bridge_hub_rococo::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
	pub const BridgeHubWococoChainId: bp_runtime::ChainId = bp_runtime::BRIDGE_HUB_WOCOCO_CHAIN_ID;
	pub BridgeWococoMessagesPalletInstance: InteriorMultiLocation = X1(PalletInstance(<BridgeWococoMessages as PalletInfoAccess>::index() as u8));
	pub BridgeHubRococoUniversalLocation: InteriorMultiLocation = X2(GlobalConsensus(Rococo), Parachain(ParachainInfo::parachain_id().into()));
	pub WococoGlobalConsensusNetwork: NetworkId = NetworkId::Wococo;
	pub ActiveOutboundLanesToBridgeHubWococo: &'static [bp_messages::LaneId] = &[DEFAULT_XCM_LANE_TO_BRIDGE_HUB_WOCOCO];
	pub PriorityBoostPerMessage: u64 = 921_900_294;

	pub FromAssetHubRococoToAssetHubWococoRoute: SenderAndLane = SenderAndLane::new(
		ParentThen(X1(Parachain(1000))).into(),
		DEFAULT_XCM_LANE_TO_BRIDGE_HUB_WOCOCO,
	);
}

/// Proof of messages, coming from Wococo.
pub type FromWococoBridgeHubMessagesProof =
	FromBridgedChainMessagesProof<bp_bridge_hub_wococo::Hash>;
/// Messages delivery proof for Rococo Bridge Hub -> Wococo Bridge Hub messages.
pub type ToWococoBridgeHubMessagesDeliveryProof =
	FromBridgedChainMessagesDeliveryProof<bp_bridge_hub_wococo::Hash>;

/// Dispatches received XCM messages from other bridge
pub type OnBridgeHubRococoBlobDispatcher = BridgeBlobDispatcher<
	XcmRouter,
	BridgeHubRococoUniversalLocation,
	BridgeWococoMessagesPalletInstance,
>;

/// Export XCM messages to be relayed to the otherside
pub type ToBridgeHubWococoHaulBlobExporter = HaulBlobExporter<
	XcmBlobHaulerAdapter<ToBridgeHubWococoXcmBlobHauler>,
	WococoGlobalConsensusNetwork,
	(),
>;
pub struct ToBridgeHubWococoXcmBlobHauler;
impl XcmBlobHauler for ToBridgeHubWococoXcmBlobHauler {
	type Runtime = Runtime;
	type MessagesInstance = WithBridgeHubWococoMessagesInstance;
	type SenderAndLane = FromAssetHubRococoToAssetHubWococoRoute;

	type ToSourceChainSender = crate::XcmRouter;
	type CongestedMessage = ();
	type UncongestedMessage = ();
}
pub const DEFAULT_XCM_LANE_TO_BRIDGE_HUB_WOCOCO: LaneId = LaneId([0, 0, 0, 1]);

/// Messaging Bridge configuration for BridgeHubRococo -> BridgeHubWococo
pub struct WithBridgeHubWococoMessageBridge;
impl MessageBridge for WithBridgeHubWococoMessageBridge {
	const BRIDGED_MESSAGES_PALLET_NAME: &'static str =
		bp_bridge_hub_rococo::WITH_BRIDGE_HUB_ROCOCO_MESSAGES_PALLET_NAME;
	type ThisChain = BridgeHubRococo;
	type BridgedChain = BridgeHubWococo;
	type BridgedHeaderChain = pallet_bridge_parachains::ParachainHeaders<
		Runtime,
		BridgeParachainWococoInstance,
		bp_bridge_hub_wococo::BridgeHubWococo,
	>;
}

/// Message verifier for BridgeHubWococo messages sent from BridgeHubRococo
pub type ToBridgeHubWococoMessageVerifier =
	messages::source::FromThisChainMessageVerifier<WithBridgeHubWococoMessageBridge>;

/// Maximal outbound payload size of BridgeHubRococo -> BridgeHubWococo messages.
pub type ToBridgeHubWococoMaximalOutboundPayloadSize =
	messages::source::FromThisChainMaximalOutboundPayloadSize<WithBridgeHubWococoMessageBridge>;

/// BridgeHubWococo chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct BridgeHubWococo;

impl UnderlyingChainProvider for BridgeHubWococo {
	type Chain = bp_bridge_hub_wococo::BridgeHubWococo;
}

impl messages::BridgedChainWithMessages for BridgeHubWococo {}

/// BridgeHubRococo chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct BridgeHubRococo;

impl UnderlyingChainProvider for BridgeHubRococo {
	type Chain = bp_bridge_hub_rococo::BridgeHubRococo;
}

impl ThisChainWithMessages for BridgeHubRococo {
	type RuntimeOrigin = crate::RuntimeOrigin;
}

/// Signed extension that refunds relayers that are delivering messages from the Wococo parachain.
pub type BridgeRefundBridgeHubWococoMessages = RefundBridgedParachainMessages<
	Runtime,
	RefundableParachain<BridgeParachainWococoInstance, bp_bridge_hub_wococo::BridgeHubWococo>,
	RefundableMessagesLane<WithBridgeHubWococoMessagesInstance, BridgeHubWococoMessagesLane>,
	ActualFeeRefund<Runtime>,
	PriorityBoostPerMessage,
	StrBridgeRefundBridgeHubWococoMessages,
>;
bp_runtime::generate_static_str_provider!(BridgeRefundBridgeHubWococoMessages);

parameter_types! {
	pub const BridgeHubWococoMessagesLane: bp_messages::LaneId = DEFAULT_XCM_LANE_TO_BRIDGE_HUB_WOCOCO;
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::BridgeGrandpaWococoInstance;
	use bridge_runtime_common::{
		assert_complete_bridge_types,
		integrity::{
			assert_complete_bridge_constants, check_message_lane_weights,
			AssertBridgeMessagesPalletConstants, AssertBridgePalletNames, AssertChainConstants,
			AssertCompleteBridgeConstants,
		},
	};

	#[test]
	fn ensure_bridge_hub_rococo_message_lane_weights_are_correct() {
		check_message_lane_weights::<
			bp_bridge_hub_rococo::BridgeHubRococo,
			Runtime,
			WithBridgeHubWococoMessagesInstance,
		>(
			bp_bridge_hub_wococo::EXTRA_STORAGE_PROOF_SIZE,
			bp_bridge_hub_rococo::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
			bp_bridge_hub_rococo::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
			true,
		);
	}

	#[test]
	fn ensure_bridge_integrity() {
		assert_complete_bridge_types!(
			runtime: Runtime,
			with_bridged_chain_grandpa_instance: BridgeGrandpaWococoInstance,
			with_bridged_chain_messages_instance: WithBridgeHubWococoMessagesInstance,
			bridge: WithBridgeHubWococoMessageBridge,
			this_chain: bp_rococo::Rococo,
			bridged_chain: bp_wococo::Wococo,
		);

		assert_complete_bridge_constants::<
			Runtime,
			BridgeGrandpaWococoInstance,
			WithBridgeHubWococoMessagesInstance,
			WithBridgeHubWococoMessageBridge,
		>(AssertCompleteBridgeConstants {
			this_chain_constants: AssertChainConstants {
				block_length: bp_bridge_hub_rococo::BlockLength::get(),
				block_weights: bp_bridge_hub_rococo::BlockWeights::get(),
			},
			messages_pallet_constants: AssertBridgeMessagesPalletConstants {
				max_unrewarded_relayers_in_bridged_confirmation_tx:
					bp_bridge_hub_wococo::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
				max_unconfirmed_messages_in_bridged_confirmation_tx:
					bp_bridge_hub_wococo::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
				bridged_chain_id: bp_runtime::BRIDGE_HUB_WOCOCO_CHAIN_ID,
			},
			pallet_names: AssertBridgePalletNames {
				with_this_chain_messages_pallet_name:
					bp_bridge_hub_rococo::WITH_BRIDGE_HUB_ROCOCO_MESSAGES_PALLET_NAME,
				with_bridged_chain_grandpa_pallet_name: bp_wococo::WITH_WOCOCO_GRANDPA_PALLET_NAME,
				with_bridged_chain_messages_pallet_name:
					bp_bridge_hub_wococo::WITH_BRIDGE_HUB_WOCOCO_MESSAGES_PALLET_NAME,
			},
		});
	}
}
