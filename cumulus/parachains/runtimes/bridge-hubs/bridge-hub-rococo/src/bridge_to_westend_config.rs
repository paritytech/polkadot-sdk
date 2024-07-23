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

//! Bridge definitions used on BridgeHubRococo for bridging to BridgeHubWestend.

use crate::{
	bridge_common_config::{BridgeParachainWestendInstance, DeliveryRewardInBalance},
	weights,
	xcm_config::UniversalLocation,
	BridgeWestendMessages, PolkadotXcm, Runtime, RuntimeEvent, XcmOverBridgeHubWestend, XcmRouter,
};
use bp_messages::{
	source_chain::FromBridgedChainMessagesDeliveryProof,
	target_chain::FromBridgedChainMessagesProof, LaneId,
};
use bp_runtime::Chain;
use bridge_runtime_common::{
	extensions::refund_relayer_extension::{
		ActualFeeRefund, RefundBridgedMessages, RefundSignedExtensionAdapter,
		RefundableMessagesLane,
	},
	messages_xcm_extension::{
		SenderAndLane, XcmAsPlainPayload, XcmBlobHauler, XcmBlobHaulerAdapter,
		XcmBlobMessageDispatch, XcmVersionOfDestAndRemoteBridge,
	},
};

use codec::Encode;
use frame_support::{parameter_types, traits::PalletInfoAccess};
use xcm::{
	latest::prelude::*,
	prelude::{InteriorLocation, NetworkId},
};
use xcm_builder::BridgeBlobDispatcher;

parameter_types! {
	pub const BridgeHubWestendChainId: bp_runtime::ChainId = bp_bridge_hub_westend::BridgeHubWestend::ID;
	pub BridgeRococoToWestendMessagesPalletInstance: InteriorLocation = [PalletInstance(<BridgeWestendMessages as PalletInfoAccess>::index() as u8)].into();
	pub WestendGlobalConsensusNetwork: NetworkId = NetworkId::Westend;
	pub WestendGlobalConsensusNetworkLocation: Location = Location::new(
		2,
		[GlobalConsensus(WestendGlobalConsensusNetwork::get())]
	);
	// see the `FEE_BOOST_PER_RELAY_HEADER` constant get the meaning of this value
	pub PriorityBoostPerRelayHeader: u64 = 32_007_814_407_814;
	// see the `FEE_BOOST_PER_PARACHAIN_HEADER` constant get the meaning of this value
	pub PriorityBoostPerParachainHeader: u64 = 1_396_340_903_540_903;
	// see the `FEE_BOOST_PER_MESSAGE` constant to get the meaning of this value
	pub PriorityBoostPerMessage: u64 = 182_044_444_444_444;

	pub AssetHubRococoParaId: cumulus_primitives_core::ParaId = bp_asset_hub_rococo::ASSET_HUB_ROCOCO_PARACHAIN_ID.into();
	pub AssetHubWestendParaId: cumulus_primitives_core::ParaId = bp_asset_hub_westend::ASSET_HUB_WESTEND_PARACHAIN_ID.into();

	// Lanes
	pub ActiveOutboundLanesToBridgeHubWestend: &'static [bp_messages::LaneId] = &[XCM_LANE_FOR_ASSET_HUB_ROCOCO_TO_ASSET_HUB_WESTEND];
	pub const AssetHubRococoToAssetHubWestendMessagesLane: bp_messages::LaneId = XCM_LANE_FOR_ASSET_HUB_ROCOCO_TO_ASSET_HUB_WESTEND;
	pub FromAssetHubRococoToAssetHubWestendRoute: SenderAndLane = SenderAndLane::new(
		ParentThen([Parachain(AssetHubRococoParaId::get().into())].into()).into(),
		XCM_LANE_FOR_ASSET_HUB_ROCOCO_TO_ASSET_HUB_WESTEND,
	);
	pub ActiveLanes: alloc::vec::Vec<(SenderAndLane, (NetworkId, InteriorLocation))> = alloc::vec![
			(
				FromAssetHubRococoToAssetHubWestendRoute::get(),
				(WestendGlobalConsensusNetwork::get(), [Parachain(AssetHubWestendParaId::get().into())].into())
			)
	];

	pub CongestedMessage: Xcm<()> = build_congestion_message(true).into();
	pub UncongestedMessage: Xcm<()> = build_congestion_message(false).into();

	pub BridgeHubWestendLocation: Location = Location::new(
		2,
		[
			GlobalConsensus(WestendGlobalConsensusNetwork::get()),
			Parachain(<bp_bridge_hub_westend::BridgeHubWestend as bp_runtime::Parachain>::PARACHAIN_ID)
		]
	);
}
pub const XCM_LANE_FOR_ASSET_HUB_ROCOCO_TO_ASSET_HUB_WESTEND: LaneId = LaneId([0, 0, 0, 2]);

fn build_congestion_message<Call>(is_congested: bool) -> alloc::vec::Vec<Instruction<Call>> {
	alloc::vec![
		UnpaidExecution { weight_limit: Unlimited, check_origin: None },
		Transact {
			origin_kind: OriginKind::Xcm,
			require_weight_at_most:
				bp_asset_hub_rococo::XcmBridgeHubRouterTransactCallMaxWeight::get(),
			call: bp_asset_hub_rococo::Call::ToWestendXcmRouter(
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

/// Proof of messages, coming from Westend.
pub type FromWestendBridgeHubMessagesProof =
	FromBridgedChainMessagesProof<bp_bridge_hub_westend::Hash>;
/// Messages delivery proof for Rococo Bridge Hub -> Westend Bridge Hub messages.
pub type ToWestendBridgeHubMessagesDeliveryProof =
	FromBridgedChainMessagesDeliveryProof<bp_bridge_hub_westend::Hash>;

/// Dispatches received XCM messages from other bridge
type FromWestendMessageBlobDispatcher =
	BridgeBlobDispatcher<XcmRouter, UniversalLocation, BridgeRococoToWestendMessagesPalletInstance>;

/// Export XCM messages to be relayed to the other side
pub type ToBridgeHubWestendHaulBlobExporter = XcmOverBridgeHubWestend;

pub struct ToBridgeHubWestendXcmBlobHauler;
impl XcmBlobHauler for ToBridgeHubWestendXcmBlobHauler {
	type Runtime = Runtime;
	type MessagesInstance = WithBridgeHubWestendMessagesInstance;
	type ToSourceChainSender = XcmRouter;
	type CongestedMessage = CongestedMessage;
	type UncongestedMessage = UncongestedMessage;
}

/// On messages delivered callback.
type OnMessagesDeliveredFromWestend =
	XcmBlobHaulerAdapter<ToBridgeHubWestendXcmBlobHauler, ActiveLanes>;

/// Signed extension that refunds relayers that are delivering messages from the Westend parachain.
pub type OnBridgeHubRococoRefundBridgeHubWestendMessages = RefundSignedExtensionAdapter<
	RefundBridgedMessages<
		Runtime,
		RefundableMessagesLane<
			WithBridgeHubWestendMessagesInstance,
			AssetHubRococoToAssetHubWestendMessagesLane,
		>,
		ActualFeeRefund<Runtime>,
		PriorityBoostPerMessage,
		StrOnBridgeHubRococoRefundBridgeHubWestendMessages,
	>,
>;
bp_runtime::generate_static_str_provider!(OnBridgeHubRococoRefundBridgeHubWestendMessages);

/// Add XCM messages support for BridgeHubRococo to support Rococo->Westend XCM messages
pub type WithBridgeHubWestendMessagesInstance = pallet_bridge_messages::Instance3;
impl pallet_bridge_messages::Config<WithBridgeHubWestendMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::pallet_bridge_messages_rococo_to_westend::WeightInfo<Runtime>;

	type ThisChain = bp_bridge_hub_rococo::BridgeHubRococo;
	type BridgedChain = bp_bridge_hub_westend::BridgeHubWestend;
	type BridgedHeaderChain = pallet_bridge_parachains::ParachainHeaders<
		Runtime,
		BridgeParachainWestendInstance,
		bp_bridge_hub_westend::BridgeHubWestend,
	>;

	type ActiveOutboundLanes = ActiveOutboundLanesToBridgeHubWestend;

	type OutboundPayload = XcmAsPlainPayload;

	type InboundPayload = XcmAsPlainPayload;
	type DeliveryPayments = ();

	type DeliveryConfirmationPayments = pallet_bridge_relayers::DeliveryConfirmationPaymentsAdapter<
		Runtime,
		WithBridgeHubWestendMessagesInstance,
		DeliveryRewardInBalance,
	>;

	type MessageDispatch = XcmBlobMessageDispatch<
		FromWestendMessageBlobDispatcher,
		Self::WeightInfo,
		cumulus_pallet_xcmp_queue::bridging::OutXcmpChannelStatusProvider<
			AssetHubRococoParaId,
			Runtime,
		>,
	>;
	type OnMessagesDelivered = OnMessagesDeliveredFromWestend;
}

/// Add support for the export and dispatch of XCM programs.
pub type XcmOverBridgeHubWestendInstance = pallet_xcm_bridge_hub::Instance1;
impl pallet_xcm_bridge_hub::Config<XcmOverBridgeHubWestendInstance> for Runtime {
	type UniversalLocation = UniversalLocation;
	type BridgedNetwork = WestendGlobalConsensusNetworkLocation;
	type BridgeMessagesPalletInstance = WithBridgeHubWestendMessagesInstance;
	type MessageExportPrice = ();
	type DestinationVersion =
		XcmVersionOfDestAndRemoteBridge<PolkadotXcm, BridgeHubWestendLocation>;
	type Lanes = ActiveLanes;
	type LanesSupport = ToBridgeHubWestendXcmBlobHauler;
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::bridge_common_config::BridgeGrandpaWestendInstance;
	use bridge_runtime_common::{
		assert_complete_bridge_types,
		extensions::refund_relayer_extension::RefundableParachain,
		integrity::{
			assert_complete_with_parachain_bridge_constants, check_message_lane_weights,
			AssertChainConstants, AssertCompleteBridgeConstants,
		},
	};
	use parachains_common::Balance;
	use testnet_parachains_constants::rococo;

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

	// see `FEE_BOOST_PER_MESSAGE` comment
	const FEE_BOOST_PER_RELAY_HEADER: Balance = 2 * rococo::currency::UNITS;
	// see `FEE_BOOST_PER_MESSAGE` comment
	const FEE_BOOST_PER_PARACHAIN_HEADER: Balance = 2 * rococo::currency::UNITS;

	#[test]
	fn ensure_bridge_hub_rococo_message_lane_weights_are_correct() {
		check_message_lane_weights::<
			bp_bridge_hub_rococo::BridgeHubRococo,
			Runtime,
			WithBridgeHubWestendMessagesInstance,
		>(
			bp_bridge_hub_westend::EXTRA_STORAGE_PROOF_SIZE,
			bp_bridge_hub_rococo::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
			bp_bridge_hub_rococo::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
			true,
		);
	}

	#[test]
	fn ensure_bridge_integrity() {
		assert_complete_bridge_types!(
			runtime: Runtime,
			with_bridged_chain_grandpa_instance: BridgeGrandpaWestendInstance,
			with_bridged_chain_messages_instance: WithBridgeHubWestendMessagesInstance,
			this_chain: bp_bridge_hub_rococo::BridgeHubRococo,
			bridged_chain: bp_bridge_hub_westend::BridgeHubWestend,
		);

		assert_complete_with_parachain_bridge_constants::<
			Runtime,
			BridgeGrandpaWestendInstance,
			WithBridgeHubWestendMessagesInstance,
			bp_westend::Westend,
		>(AssertCompleteBridgeConstants {
			this_chain_constants: AssertChainConstants {
				block_length: bp_bridge_hub_rococo::BlockLength::get(),
				block_weights: bp_bridge_hub_rococo::BlockWeightsForAsyncBacking::get(),
			},
		});

		bridge_runtime_common::extensions::priority_calculator::per_relay_header::ensure_priority_boost_is_sane::<
			Runtime,
			BridgeGrandpaWestendInstance,
			PriorityBoostPerRelayHeader,
		>(FEE_BOOST_PER_RELAY_HEADER);

		bridge_runtime_common::extensions::priority_calculator::per_parachain_header::ensure_priority_boost_is_sane::<
			Runtime,
			RefundableParachain<WithBridgeHubWestendMessagesInstance, bp_bridge_hub_westend::BridgeHubWestend>,
			PriorityBoostPerParachainHeader,
		>(FEE_BOOST_PER_PARACHAIN_HEADER);

		bridge_runtime_common::extensions::priority_calculator::per_message::ensure_priority_boost_is_sane::<
			Runtime,
			WithBridgeHubWestendMessagesInstance,
			PriorityBoostPerMessage,
		>(FEE_BOOST_PER_MESSAGE);

		let expected: InteriorLocation = [PalletInstance(
			bp_bridge_hub_rococo::WITH_BRIDGE_ROCOCO_TO_WESTEND_MESSAGES_PALLET_INDEX,
		)]
		.into();

		assert_eq!(BridgeRococoToWestendMessagesPalletInstance::get(), expected,);
	}
}
