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

//! Bridge definitions used on BridgeHub with the Westend flavor.

use crate::{
	bridge_common_config::DeliveryRewardInBalance, weights, xcm_config::UniversalLocation,
	AccountId, BridgeRococoMessages, PolkadotXcm, Runtime, RuntimeEvent, RuntimeOrigin,
	XcmOverBridgeHubRococo, XcmRouter,
};
use bp_messages::LaneId;
use bp_parachains::SingleParaStoredHeaderDataBuilder;
use bp_runtime::Chain;
use bridge_runtime_common::{
	messages,
	messages::{
		source::{FromBridgedChainMessagesDeliveryProof, TargetHeaderChainAdapter},
		target::{FromBridgedChainMessagesProof, SourceHeaderChainAdapter},
		MessageBridge, ThisChainWithMessages, UnderlyingChainProvider,
	},
	messages_xcm_extension::{
		SenderAndLane, XcmAsPlainPayload, XcmBlobHauler, XcmBlobHaulerAdapter,
		XcmBlobMessageDispatch, XcmVersionOfDestAndRemoteBridge,
	},
	refund_relayer_extension::{
		ActualFeeRefund, RefundBridgedParachainMessages, RefundSignedExtensionAdapter,
		RefundableMessagesLane, RefundableParachain,
	},
};
use codec::Encode;
use frame_support::{
	parameter_types,
	traits::{ConstU32, PalletInfoAccess},
};
use sp_runtime::RuntimeDebug;
use xcm::{
	latest::prelude::*,
	prelude::{InteriorLocation, NetworkId},
};
use xcm_builder::BridgeBlobDispatcher;

parameter_types! {
	pub const RelayChainHeadersToKeep: u32 = 1024;
	pub const ParachainHeadsToKeep: u32 = 64;

	pub const RococoBridgeParachainPalletName: &'static str = "Paras";
	pub const MaxRococoParaHeadDataSize: u32 = bp_rococo::MAX_NESTED_PARACHAIN_HEAD_DATA_SIZE;

	pub const MaxUnrewardedRelayerEntriesAtInboundLane: bp_messages::MessageNonce =
		bp_bridge_hub_westend::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	pub const MaxUnconfirmedMessagesAtInboundLane: bp_messages::MessageNonce =
		bp_bridge_hub_westend::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
	pub const BridgeHubRococoChainId: bp_runtime::ChainId = BridgeHubRococo::ID;
	pub BridgeWestendToRococoMessagesPalletInstance: InteriorLocation = [PalletInstance(<BridgeRococoMessages as PalletInfoAccess>::index() as u8)].into();
	pub RococoGlobalConsensusNetwork: NetworkId = NetworkId::Rococo;
	pub RococoGlobalConsensusNetworkLocation: Location = Location::new(
		2,
		[GlobalConsensus(RococoGlobalConsensusNetwork::get())]
	);
	// see the `FEE_BOOST_PER_MESSAGE` constant to get the meaning of this value
	pub PriorityBoostPerMessage: u64 = 182_044_444_444_444;

	pub AssetHubWestendParaId: cumulus_primitives_core::ParaId = bp_asset_hub_westend::ASSET_HUB_WESTEND_PARACHAIN_ID.into();
	pub AssetHubRococoParaId: cumulus_primitives_core::ParaId = bp_asset_hub_rococo::ASSET_HUB_ROCOCO_PARACHAIN_ID.into();

	// Lanes
	pub ActiveOutboundLanesToBridgeHubRococo: &'static [bp_messages::LaneId] = &[XCM_LANE_FOR_ASSET_HUB_WESTEND_TO_ASSET_HUB_ROCOCO];
	pub const AssetHubWestendToAssetHubRococoMessagesLane: bp_messages::LaneId = XCM_LANE_FOR_ASSET_HUB_WESTEND_TO_ASSET_HUB_ROCOCO;
	pub FromAssetHubWestendToAssetHubRococoRoute: SenderAndLane = SenderAndLane::new(
		ParentThen([Parachain(AssetHubWestendParaId::get().into())].into()).into(),
		XCM_LANE_FOR_ASSET_HUB_WESTEND_TO_ASSET_HUB_ROCOCO,
	);
	pub ActiveLanes: sp_std::vec::Vec<(SenderAndLane, (NetworkId, InteriorLocation))> = sp_std::vec![
			(
				FromAssetHubWestendToAssetHubRococoRoute::get(),
				(RococoGlobalConsensusNetwork::get(), [Parachain(AssetHubRococoParaId::get().into())].into())
			)
	];

	pub CongestedMessage: Xcm<()> = build_congestion_message(true).into();
	pub UncongestedMessage: Xcm<()> = build_congestion_message(false).into();

	pub BridgeHubRococoLocation: Location = Location::new(
		2,
		[
			GlobalConsensus(RococoGlobalConsensusNetwork::get()),
			Parachain(<bp_bridge_hub_rococo::BridgeHubRococo as bp_runtime::Parachain>::PARACHAIN_ID)
		]
	);
}
pub const XCM_LANE_FOR_ASSET_HUB_WESTEND_TO_ASSET_HUB_ROCOCO: LaneId = LaneId([0, 0, 0, 2]);

fn build_congestion_message<Call>(is_congested: bool) -> sp_std::vec::Vec<Instruction<Call>> {
	sp_std::vec![
		UnpaidExecution { weight_limit: Unlimited, check_origin: None },
		Transact {
			origin_kind: OriginKind::Xcm,
			require_weight_at_most:
				bp_asset_hub_westend::XcmBridgeHubRouterTransactCallMaxWeight::get(),
			call: bp_asset_hub_westend::Call::ToRococoXcmRouter(
				bp_asset_hub_westend::XcmBridgeHubRouterCall::report_bridge_status {
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
/// Messages delivery proof for Rococo Bridge Hub -> Westend Bridge Hub messages.
pub type ToRococoBridgeHubMessagesDeliveryProof =
	FromBridgedChainMessagesDeliveryProof<bp_bridge_hub_rococo::Hash>;

/// Dispatches received XCM messages from other bridge
type FromRococoMessageBlobDispatcher =
	BridgeBlobDispatcher<XcmRouter, UniversalLocation, BridgeWestendToRococoMessagesPalletInstance>;

/// Export XCM messages to be relayed to the other side
pub type ToBridgeHubRococoHaulBlobExporter = XcmOverBridgeHubRococo;

pub struct ToBridgeHubRococoXcmBlobHauler;
impl XcmBlobHauler for ToBridgeHubRococoXcmBlobHauler {
	type Runtime = Runtime;
	type MessagesInstance = WithBridgeHubRococoMessagesInstance;

	type ToSourceChainSender = XcmRouter;
	type CongestedMessage = CongestedMessage;
	type UncongestedMessage = UncongestedMessage;
}

/// On messages delivered callback.
type OnMessagesDelivered = XcmBlobHaulerAdapter<ToBridgeHubRococoXcmBlobHauler, ActiveLanes>;

/// Messaging Bridge configuration for BridgeHubWestend -> BridgeHubRococo
pub struct WithBridgeHubRococoMessageBridge;
impl MessageBridge for WithBridgeHubRococoMessageBridge {
	const BRIDGED_MESSAGES_PALLET_NAME: &'static str =
		bp_bridge_hub_westend::WITH_BRIDGE_HUB_WESTEND_MESSAGES_PALLET_NAME;
	type ThisChain = BridgeHubWestend;
	type BridgedChain = BridgeHubRococo;
	type BridgedHeaderChain = pallet_bridge_parachains::ParachainHeaders<
		Runtime,
		BridgeParachainRococoInstance,
		bp_bridge_hub_rococo::BridgeHubRococo,
	>;
}

/// Maximal outbound payload size of BridgeHubWestend -> BridgeHubRococo messages.
type ToBridgeHubRococoMaximalOutboundPayloadSize =
	messages::source::FromThisChainMaximalOutboundPayloadSize<WithBridgeHubRococoMessageBridge>;

/// BridgeHubRococo chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct BridgeHubRococo;

impl UnderlyingChainProvider for BridgeHubRococo {
	type Chain = bp_bridge_hub_rococo::BridgeHubRococo;
}

impl messages::BridgedChainWithMessages for BridgeHubRococo {}

/// BridgeHubWestend chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct BridgeHubWestend;

impl UnderlyingChainProvider for BridgeHubWestend {
	type Chain = bp_bridge_hub_westend::BridgeHubWestend;
}

impl ThisChainWithMessages for BridgeHubWestend {
	type RuntimeOrigin = RuntimeOrigin;
}

/// Signed extension that refunds relayers that are delivering messages from the Rococo parachain.
pub type OnBridgeHubWestendRefundBridgeHubRococoMessages = RefundSignedExtensionAdapter<
	RefundBridgedParachainMessages<
		Runtime,
		RefundableParachain<BridgeParachainRococoInstance, bp_bridge_hub_rococo::BridgeHubRococo>,
		RefundableMessagesLane<
			WithBridgeHubRococoMessagesInstance,
			AssetHubWestendToAssetHubRococoMessagesLane,
		>,
		ActualFeeRefund<Runtime>,
		PriorityBoostPerMessage,
		StrOnBridgeHubWestendRefundBridgeHubRococoMessages,
	>,
>;
bp_runtime::generate_static_str_provider!(OnBridgeHubWestendRefundBridgeHubRococoMessages);

/// Add GRANDPA bridge pallet to track Rococo relay chain.
pub type BridgeGrandpaRococoInstance = pallet_bridge_grandpa::Instance1;
impl pallet_bridge_grandpa::Config<BridgeGrandpaRococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type BridgedChain = bp_rococo::Rococo;
	type MaxFreeMandatoryHeadersPerBlock = ConstU32<4>;
	type HeadersToKeep = RelayChainHeadersToKeep;
	type WeightInfo = weights::pallet_bridge_grandpa::WeightInfo<Runtime>;
}

/// Add parachain bridge pallet to track Rococo BridgeHub parachain
pub type BridgeParachainRococoInstance = pallet_bridge_parachains::Instance1;
impl pallet_bridge_parachains::Config<BridgeParachainRococoInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_parachains::WeightInfo<Runtime>;
	type BridgesGrandpaPalletInstance = BridgeGrandpaRococoInstance;
	type ParasPalletName = RococoBridgeParachainPalletName;
	type ParaStoredHeaderDataBuilder =
		SingleParaStoredHeaderDataBuilder<bp_bridge_hub_rococo::BridgeHubRococo>;
	type HeadsToKeep = ParachainHeadsToKeep;
	type MaxParaHeadDataSize = MaxRococoParaHeadDataSize;
}

/// Add XCM messages support for BridgeHubWestend to support Westend->Rococo XCM messages
pub type WithBridgeHubRococoMessagesInstance = pallet_bridge_messages::Instance1;
impl pallet_bridge_messages::Config<WithBridgeHubRococoMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_messages::WeightInfo<Runtime>;
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
	type DeliveryConfirmationPayments = pallet_bridge_relayers::DeliveryConfirmationPaymentsAdapter<
		Runtime,
		WithBridgeHubRococoMessagesInstance,
		DeliveryRewardInBalance,
	>;

	type SourceHeaderChain = SourceHeaderChainAdapter<WithBridgeHubRococoMessageBridge>;
	type MessageDispatch = XcmBlobMessageDispatch<
		FromRococoMessageBlobDispatcher,
		Self::WeightInfo,
		cumulus_pallet_xcmp_queue::bridging::OutXcmpChannelStatusProvider<
			AssetHubWestendParaId,
			Runtime,
		>,
	>;
	type OnMessagesDelivered = OnMessagesDelivered;
}

/// Add support for the export and dispatch of XCM programs.
pub type XcmOverBridgeHubRococoInstance = pallet_xcm_bridge_hub::Instance1;
impl pallet_xcm_bridge_hub::Config<XcmOverBridgeHubRococoInstance> for Runtime {
	type UniversalLocation = UniversalLocation;
	type BridgedNetwork = RococoGlobalConsensusNetworkLocation;
	type BridgeMessagesPalletInstance = WithBridgeHubRococoMessagesInstance;
	type MessageExportPrice = ();
	type DestinationVersion = XcmVersionOfDestAndRemoteBridge<PolkadotXcm, BridgeHubRococoLocation>;
	type Lanes = ActiveLanes;
	type LanesSupport = ToBridgeHubRococoXcmBlobHauler;
}

#[cfg(test)]
mod tests {
	use super::*;
	use bridge_runtime_common::{
		assert_complete_bridge_types,
		integrity::{
			assert_complete_bridge_constants, check_message_lane_weights,
			AssertBridgeMessagesPalletConstants, AssertBridgePalletNames, AssertChainConstants,
			AssertCompleteBridgeConstants,
		},
	};
	use parachains_common::Balance;
	use testnet_parachains_constants::westend;

	/// Every additional message in the message delivery transaction boosts its priority.
	/// So the priority of transaction with `N+1` messages is larger than priority of
	/// transaction with `N` messages by the `PriorityBoostPerMessage`.
	///
	/// Economically, it is an equivalent of adding tip to the transaction with `N` messages.
	/// The `FEE_BOOST_PER_MESSAGE` constant is the value of this tip.
	///
	/// We want this tip to be large enough (delivery transactions with more messages = less
	/// operational costs and a faster bridge), so this value should be significant.
	const FEE_BOOST_PER_MESSAGE: Balance = 2 * westend::currency::UNITS;

	#[test]
	fn ensure_bridge_hub_westend_message_lane_weights_are_correct() {
		check_message_lane_weights::<
			bp_bridge_hub_westend::BridgeHubWestend,
			Runtime,
			WithBridgeHubRococoMessagesInstance,
		>(
			bp_bridge_hub_rococo::EXTRA_STORAGE_PROOF_SIZE,
			bp_bridge_hub_westend::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
			bp_bridge_hub_westend::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
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
			this_chain: bp_westend::Westend,
			bridged_chain: bp_rococo::Rococo,
		);

		assert_complete_bridge_constants::<
			Runtime,
			BridgeGrandpaRococoInstance,
			WithBridgeHubRococoMessagesInstance,
			WithBridgeHubRococoMessageBridge,
		>(AssertCompleteBridgeConstants {
			this_chain_constants: AssertChainConstants {
				block_length: bp_bridge_hub_westend::BlockLength::get(),
				block_weights: bp_bridge_hub_westend::BlockWeightsForAsyncBacking::get(),
			},
			messages_pallet_constants: AssertBridgeMessagesPalletConstants {
				max_unrewarded_relayers_in_bridged_confirmation_tx:
					bp_bridge_hub_rococo::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
				max_unconfirmed_messages_in_bridged_confirmation_tx:
					bp_bridge_hub_rococo::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
				bridged_chain_id: BridgeHubRococo::ID,
			},
			pallet_names: AssertBridgePalletNames {
				with_this_chain_messages_pallet_name:
					bp_bridge_hub_westend::WITH_BRIDGE_HUB_WESTEND_MESSAGES_PALLET_NAME,
				with_bridged_chain_grandpa_pallet_name: bp_rococo::WITH_ROCOCO_GRANDPA_PALLET_NAME,
				with_bridged_chain_messages_pallet_name:
					bp_bridge_hub_rococo::WITH_BRIDGE_HUB_ROCOCO_MESSAGES_PALLET_NAME,
			},
		});

		bridge_runtime_common::priority_calculator::ensure_priority_boost_is_sane::<
			Runtime,
			WithBridgeHubRococoMessagesInstance,
			PriorityBoostPerMessage,
		>(FEE_BOOST_PER_MESSAGE);

		assert_eq!(
			BridgeWestendToRococoMessagesPalletInstance::get(),
			[PalletInstance(
				bp_bridge_hub_westend::WITH_BRIDGE_WESTEND_TO_ROCOCO_MESSAGES_PALLET_INDEX
			)]
		);
	}
}
