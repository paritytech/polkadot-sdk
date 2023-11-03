// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Bridge definitions used on BridgeHub with the Rococo flavor for bridging to BridgeHubWococo.

use crate::{
	bridge_common_config::{BridgeParachainWococoInstance, DeliveryRewardInBalance},
	weights, AccountId, BridgeWococoMessages, ParachainInfo, Runtime, RuntimeEvent, RuntimeOrigin,
	XcmRouter,
};
use bp_messages::LaneId;
use bridge_runtime_common::{
	messages,
	messages::{
		source::{FromBridgedChainMessagesDeliveryProof, TargetHeaderChainAdapter},
		target::{FromBridgedChainMessagesProof, SourceHeaderChainAdapter},
		MessageBridge, ThisChainWithMessages, UnderlyingChainProvider,
	},
	messages_xcm_extension::{
		SenderAndLane, XcmAsPlainPayload, XcmBlobHauler, XcmBlobHaulerAdapter,
		XcmBlobMessageDispatch,
	},
	refund_relayer_extension::{
		ActualFeeRefund, RefundBridgedParachainMessages, RefundSignedExtensionAdapter,
		RefundableMessagesLane, RefundableParachain,
	},
};

use codec::Encode;
use frame_support::{parameter_types, traits::PalletInfoAccess};
use sp_runtime::RuntimeDebug;
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
	pub BridgeRococoToWococoMessagesPalletInstance: InteriorMultiLocation = X1(PalletInstance(<BridgeWococoMessages as PalletInfoAccess>::index() as u8));
	pub BridgeHubRococoUniversalLocation: InteriorMultiLocation = X2(GlobalConsensus(Rococo), Parachain(ParachainInfo::parachain_id().into()));
	pub WococoGlobalConsensusNetwork: NetworkId = NetworkId::Wococo;
	pub ActiveOutboundLanesToBridgeHubWococo: &'static [bp_messages::LaneId] = &[XCM_LANE_FOR_ASSET_HUB_ROCOCO_TO_ASSET_HUB_WOCOCO];
	pub const AssetHubRococoToAssetHubWococoMessagesLane: bp_messages::LaneId = XCM_LANE_FOR_ASSET_HUB_ROCOCO_TO_ASSET_HUB_WOCOCO;
	// see the `FEE_BOOST_PER_MESSAGE` constant to get the meaning of this value
	pub PriorityBoostPerMessage: u64 = 182_044_444_444_444;

	pub AssetHubRococoParaId: cumulus_primitives_core::ParaId = bp_asset_hub_rococo::ASSET_HUB_ROCOCO_PARACHAIN_ID.into();
	pub AssetHubWococoParaId: cumulus_primitives_core::ParaId = bp_asset_hub_wococo::ASSET_HUB_WOCOCO_PARACHAIN_ID.into();

	pub FromAssetHubRococoToAssetHubWococoRoute: SenderAndLane = SenderAndLane::new(
		ParentThen(X1(Parachain(AssetHubRococoParaId::get().into()))).into(),
		XCM_LANE_FOR_ASSET_HUB_ROCOCO_TO_ASSET_HUB_WOCOCO,
	);

	pub CongestedMessage: Xcm<()> = build_congestion_message(true).into();

	pub UncongestedMessage: Xcm<()> = build_congestion_message(false).into();
}
pub const XCM_LANE_FOR_ASSET_HUB_ROCOCO_TO_ASSET_HUB_WOCOCO: LaneId = LaneId([0, 0, 0, 1]);

fn build_congestion_message<Call>(is_congested: bool) -> sp_std::vec::Vec<Instruction<Call>> {
	sp_std::vec![
		UnpaidExecution { weight_limit: Unlimited, check_origin: None },
		Transact {
			origin_kind: OriginKind::Xcm,
			require_weight_at_most:
				bp_asset_hub_rococo::XcmBridgeHubRouterTransactCallMaxWeight::get(),
			call: bp_asset_hub_rococo::Call::ToWococoXcmRouter(
				bp_asset_hub_rococo::XcmBridgeHubRouterCall::report_bridge_status {
					bridge_id: Default::default(),
					is_congested,
				}
			)
			.encode()
			.into(),
		}
	]
}

/// Proof of messages, coming from Wococo.
pub type FromWococoBridgeHubMessagesProof =
	FromBridgedChainMessagesProof<bp_bridge_hub_wococo::Hash>;
/// Messages delivery proof for Rococo Bridge Hub -> Wococo Bridge Hub messages.
pub type ToWococoBridgeHubMessagesDeliveryProof =
	FromBridgedChainMessagesDeliveryProof<bp_bridge_hub_wococo::Hash>;

/// Dispatches received XCM messages from other bridge
type FromWococoMessageBlobDispatcher = BridgeBlobDispatcher<
	XcmRouter,
	BridgeHubRococoUniversalLocation,
	BridgeRococoToWococoMessagesPalletInstance,
>;

/// Export XCM messages to be relayed to the other side
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

	type ToSourceChainSender = XcmRouter;
	type CongestedMessage = CongestedMessage;
	type UncongestedMessage = UncongestedMessage;
}

/// On messages delivered callback.
type OnMessagesDeliveredFromWococo = XcmBlobHaulerAdapter<ToBridgeHubWococoXcmBlobHauler>;

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
	type RuntimeOrigin = RuntimeOrigin;
}

/// Signed extension that refunds relayers that are delivering messages from the Wococo parachain.
pub type OnBridgeHubRococoRefundBridgeHubWococoMessages = RefundSignedExtensionAdapter<
	RefundBridgedParachainMessages<
		Runtime,
		RefundableParachain<BridgeParachainWococoInstance, bp_bridge_hub_wococo::BridgeHubWococo>,
		RefundableMessagesLane<
			WithBridgeHubWococoMessagesInstance,
			AssetHubRococoToAssetHubWococoMessagesLane,
		>,
		ActualFeeRefund<Runtime>,
		PriorityBoostPerMessage,
		StrOnBridgeHubRococoRefundBridgeHubWococoMessages,
	>,
>;
bp_runtime::generate_static_str_provider!(OnBridgeHubRococoRefundBridgeHubWococoMessages);

/// Add XCM messages support for BridgeHubRococo to support Rococo->Wococo XCM messages
pub type WithBridgeHubWococoMessagesInstance = pallet_bridge_messages::Instance1;
impl pallet_bridge_messages::Config<WithBridgeHubWococoMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_messages_rococo_to_wococo::WeightInfo<Runtime>;
	type BridgedChainId = BridgeHubWococoChainId;
	type ActiveOutboundLanes = ActiveOutboundLanesToBridgeHubWococo;
	type MaxUnrewardedRelayerEntriesAtInboundLane = MaxUnrewardedRelayerEntriesAtInboundLane;
	type MaxUnconfirmedMessagesAtInboundLane = MaxUnconfirmedMessagesAtInboundLane;

	type MaximalOutboundPayloadSize = ToBridgeHubWococoMaximalOutboundPayloadSize;
	type OutboundPayload = XcmAsPlainPayload;

	type InboundPayload = XcmAsPlainPayload;
	type InboundRelayer = AccountId;
	type DeliveryPayments = ();

	type TargetHeaderChain = TargetHeaderChainAdapter<WithBridgeHubWococoMessageBridge>;
	type LaneMessageVerifier = ToBridgeHubWococoMessageVerifier;
	type DeliveryConfirmationPayments = pallet_bridge_relayers::DeliveryConfirmationPaymentsAdapter<
		Runtime,
		WithBridgeHubWococoMessagesInstance,
		DeliveryRewardInBalance,
	>;

	type SourceHeaderChain = SourceHeaderChainAdapter<WithBridgeHubWococoMessageBridge>;
	type MessageDispatch = XcmBlobMessageDispatch<
		FromWococoMessageBlobDispatcher,
		Self::WeightInfo,
		cumulus_pallet_xcmp_queue::bridging::OutXcmpChannelStatusProvider<
			AssetHubRococoParaId,
			Runtime,
		>,
	>;
	type OnMessagesDelivered = OnMessagesDeliveredFromWococo;
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::bridge_common_config::BridgeGrandpaWococoInstance;
	use bridge_runtime_common::{
		assert_complete_bridge_types,
		integrity::{
			assert_complete_bridge_constants, check_message_lane_weights,
			AssertBridgeMessagesPalletConstants, AssertBridgePalletNames, AssertChainConstants,
			AssertCompleteBridgeConstants,
		},
	};
	use parachains_common::{rococo, Balance};

	/// Every additional message in the message delivery transaction boosts its priority.
	/// So the priority of transaction with `N+1` messages is larger than priority of
	/// transaction with `N` messages by the `PriorityBoostPerMessage`.
	///
	/// Economically, it is an equivalent of adding tip to the transaction with `N` messages.
	/// The `FEE_BOOST_PER_MESSAGE` constant is the value of this tip.
	///
	/// We want this tip to be large enough (delivery transactions with more messages = less
	/// operational costs and a faster bridge), so this value should be significant.
	const FEE_BOOST_PER_MESSAGE: Balance = 2 * rococo::currency::UNITS;

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

		bridge_runtime_common::priority_calculator::ensure_priority_boost_is_sane::<
			Runtime,
			WithBridgeHubWococoMessagesInstance,
			PriorityBoostPerMessage,
		>(FEE_BOOST_PER_MESSAGE);

		assert_eq!(
			BridgeRococoToWococoMessagesPalletInstance::get(),
			X1(PalletInstance(
				bp_bridge_hub_rococo::WITH_BRIDGE_ROCOCO_TO_WOCOCO_MESSAGES_PALLET_INDEX
			))
		);
	}
}
