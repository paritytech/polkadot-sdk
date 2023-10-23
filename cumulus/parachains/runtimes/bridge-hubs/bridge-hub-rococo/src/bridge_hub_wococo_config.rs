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

//! Bridge definitions used on BridgeHub with the Wococo flavor.

use crate::{
	bridge_common_config::{BridgeParachainRococoInstance, DeliveryRewardInBalance},
	weights, AccountId, BridgeWococoToRococoMessages, ParachainInfo, Runtime, RuntimeEvent,
	RuntimeOrigin, XcmRouter,
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
		bp_bridge_hub_wococo::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	pub const MaxUnconfirmedMessagesAtInboundLane: bp_messages::MessageNonce =
		bp_bridge_hub_wococo::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
	pub const BridgeHubRococoChainId: bp_runtime::ChainId = bp_runtime::BRIDGE_HUB_ROCOCO_CHAIN_ID;
	pub BridgeHubWococoUniversalLocation: InteriorMultiLocation = X2(GlobalConsensus(Wococo), Parachain(ParachainInfo::parachain_id().into()));
	pub BridgeWococoToRococoMessagesPalletInstance: InteriorMultiLocation = X1(PalletInstance(<BridgeWococoToRococoMessages as PalletInfoAccess>::index() as u8));
	pub RococoGlobalConsensusNetwork: NetworkId = NetworkId::Rococo;
	pub ActiveOutboundLanesToBridgeHubRococo: &'static [bp_messages::LaneId] = &[DEFAULT_XCM_LANE_TO_BRIDGE_HUB_ROCOCO];
	// see the `FEE_BOOST_PER_MESSAGE` constant to get the meaning of this value
	pub PriorityBoostPerMessage: u64 = 182_044_444_444_444;

	pub AssetHubWococoParaId: cumulus_primitives_core::ParaId = bp_asset_hub_wococo::ASSET_HUB_WOCOCO_PARACHAIN_ID.into();

	pub FromAssetHubWococoToAssetHubRococoRoute: SenderAndLane = SenderAndLane::new(
		ParentThen(X1(Parachain(AssetHubWococoParaId::get().into()))).into(),
		DEFAULT_XCM_LANE_TO_BRIDGE_HUB_ROCOCO,
	);

	pub CongestedMessage: Xcm<()> = build_congestion_message(true).into();

	pub UncongestedMessage: Xcm<()> = build_congestion_message(false).into();
}

fn build_congestion_message<Call>(is_congested: bool) -> sp_std::vec::Vec<Instruction<Call>> {
	sp_std::vec![
		UnpaidExecution { weight_limit: Unlimited, check_origin: None },
		Transact {
			origin_kind: OriginKind::Xcm,
			require_weight_at_most:
				bp_asset_hub_wococo::XcmBridgeHubRouterTransactCallMaxWeight::get(),
			call: bp_asset_hub_wococo::Call::ToRococoXcmRouter(
				bp_asset_hub_wococo::XcmBridgeHubRouterCall::report_bridge_status {
					bridge_id: Default::default(),
					is_congested,
				}
			)
			.encode()
			.into(),
		}
	]
}

/// Proof of messages, coming from Rococo.
pub type FromRococoBridgeHubMessagesProof =
	FromBridgedChainMessagesProof<bp_bridge_hub_rococo::Hash>;
/// Messages delivery proof for RococoBridge Hub -> Wococo BridgeHub messages.
pub type ToRococoBridgeHubMessagesDeliveryProof =
	FromBridgedChainMessagesDeliveryProof<bp_bridge_hub_rococo::Hash>;

/// Dispatches received XCM messages from other bridge
pub type OnBridgeHubWococoBlobDispatcher = BridgeBlobDispatcher<
	XcmRouter,
	BridgeHubWococoUniversalLocation,
	BridgeWococoToRococoMessagesPalletInstance,
>;

/// Export XCM messages to be relayed to the other side
pub type ToBridgeHubRococoHaulBlobExporter = HaulBlobExporter<
	XcmBlobHaulerAdapter<ToBridgeHubRococoXcmBlobHauler>,
	RococoGlobalConsensusNetwork,
	(),
>;
pub struct ToBridgeHubRococoXcmBlobHauler;
impl XcmBlobHauler for ToBridgeHubRococoXcmBlobHauler {
	type Runtime = Runtime;
	type MessagesInstance = WithBridgeHubRococoMessagesInstance;
	type SenderAndLane = FromAssetHubWococoToAssetHubRococoRoute;

	type ToSourceChainSender = crate::XcmRouter;
	type CongestedMessage = CongestedMessage;
	type UncongestedMessage = UncongestedMessage;
}
pub const DEFAULT_XCM_LANE_TO_BRIDGE_HUB_ROCOCO: LaneId = LaneId([0, 0, 0, 1]);

/// On messages delivered callback.
pub type OnMessagesDelivered = XcmBlobHaulerAdapter<ToBridgeHubRococoXcmBlobHauler>;

/// Messaging Bridge configuration for BridgeHubWococo -> BridgeHubRococo
pub struct WithBridgeHubRococoMessageBridge;
impl MessageBridge for WithBridgeHubRococoMessageBridge {
	const BRIDGED_MESSAGES_PALLET_NAME: &'static str =
		bp_bridge_hub_wococo::WITH_BRIDGE_HUB_ROCOCO_TO_WOCOCO_MESSAGES_PALLET_NAME;
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
	type RuntimeOrigin = RuntimeOrigin;
}

/// Signed extension that refunds relayers that are delivering messages from the Rococo parachain.
pub type BridgeRefundBridgeHubRococoMessages = RefundSignedExtensionAdapter<
	RefundBridgedParachainMessages<
		Runtime,
		RefundableParachain<BridgeParachainRococoInstance, bp_bridge_hub_rococo::BridgeHubRococo>,
		RefundableMessagesLane<WithBridgeHubRococoMessagesInstance, BridgeHubRococoMessagesLane>,
		ActualFeeRefund<Runtime>,
		PriorityBoostPerMessage,
		StrBridgeRefundBridgeHubRococoMessages,
	>,
>;
bp_runtime::generate_static_str_provider!(BridgeRefundBridgeHubRococoMessages);

parameter_types! {
	pub const BridgeHubRococoMessagesLane: bp_messages::LaneId = DEFAULT_XCM_LANE_TO_BRIDGE_HUB_ROCOCO;
}

/// Add XCM messages support for BridgeHubWococo to support Wococo->Rococo XCM messages
pub type WithBridgeHubRococoMessagesInstance = pallet_bridge_messages::Instance2;
impl pallet_bridge_messages::Config<WithBridgeHubRococoMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_messages_wococo_to_rococo::WeightInfo<Runtime>;
	type BridgedChainId = BridgeHubRococoChainId;
	type ActiveOutboundLanes = ActiveOutboundLanesToBridgeHubRococo;
	type MaxUnrewardedRelayerEntriesAtInboundLane = MaxUnrewardedRelayerEntriesAtInboundLane;
	type MaxUnconfirmedMessagesAtInboundLane = MaxUnconfirmedMessagesAtInboundLane;

	type MaximalOutboundPayloadSize = ToBridgeHubRococoMaximalOutboundPayloadSize;
	type OutboundPayload = XcmAsPlainPayload;

	type InboundPayload = XcmAsPlainPayload;
	type InboundRelayer = AccountId;
	type DeliveryPayments = ();

	type TargetHeaderChain = TargetHeaderChainAdapter<WithBridgeHubRococoMessageBridge>;
	type LaneMessageVerifier = ToBridgeHubRococoMessageVerifier;
	type DeliveryConfirmationPayments = pallet_bridge_relayers::DeliveryConfirmationPaymentsAdapter<
		Runtime,
		WithBridgeHubRococoMessagesInstance,
		DeliveryRewardInBalance,
	>;

	type SourceHeaderChain = SourceHeaderChainAdapter<WithBridgeHubRococoMessageBridge>;
	type MessageDispatch = XcmBlobMessageDispatch<
		OnBridgeHubWococoBlobDispatcher,
		Self::WeightInfo,
		cumulus_pallet_xcmp_queue::bridging::OutXcmpChannelStatusProvider<
			AssetHubWococoParaId,
			Runtime,
		>,
	>;
	type OnMessagesDelivered = OnMessagesDelivered;
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::bridge_common_config::BridgeGrandpaRococoInstance;
	use bridge_runtime_common::{
		assert_complete_bridge_types,
		integrity::{
			assert_complete_bridge_constants, check_message_lane_weights,
			AssertBridgeMessagesPalletConstants, AssertBridgePalletNames, AssertChainConstants,
			AssertCompleteBridgeConstants,
		},
	};
	use parachains_common::{wococo, Balance};

	/// Every additional message in the message delivery transaction boosts its priority.
	/// So the priority of transaction with `N+1` messages is larger than priority of
	/// transaction with `N` messages by the `PriorityBoostPerMessage`.
	///
	/// Economically, it is an equivalent of adding tip to the transaction with `N` messages.
	/// The `FEE_BOOST_PER_MESSAGE` constant is the value of this tip.
	///
	/// We want this tip to be large enough (delivery transactions with more messages = less
	/// operational costs and a faster bridge), so this value should be significant.
	const FEE_BOOST_PER_MESSAGE: Balance = 2 * wococo::currency::UNITS;

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
					bp_bridge_hub_wococo::WITH_BRIDGE_HUB_ROCOCO_TO_WOCOCO_MESSAGES_PALLET_NAME,
				with_bridged_chain_grandpa_pallet_name: bp_rococo::WITH_ROCOCO_GRANDPA_PALLET_NAME,
				with_bridged_chain_messages_pallet_name:
					bp_bridge_hub_rococo::WITH_BRIDGE_HUB_WOCOCO_TO_ROCOCO_MESSAGES_PALLET_NAME,
			},
		});

		bridge_runtime_common::priority_calculator::ensure_priority_boost_is_sane::<
			Runtime,
			WithBridgeHubRococoMessagesInstance,
			PriorityBoostPerMessage,
		>(FEE_BOOST_PER_MESSAGE);

		assert_eq!(
			BridgeWococoToRococoMessagesPalletInstance::get(),
			X1(PalletInstance(
				bp_bridge_hub_wococo::WITH_BRIDGE_WOCOCO_TO_ROCOCO_MESSAGES_PALLET_INDEX
			))
		);
	}
}
