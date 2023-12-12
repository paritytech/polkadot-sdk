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

//! Bridge definitions used on BridgeHubRococo for bridging to Rococo Bulletin.
//!
//! Rococo Bulletin chain will be the 1:1 copy of the Polkadot Bulletin, so we
//! are reusing Polkadot Bulletin chain primitives everywhere here.

use crate::{
	bridge_common_config::{
		BridgeGrandpaRococoBulletinInstance, BridgeHubRococo, BridgeHubRococoUniversalLocation,
	},
	xcm_config::UniversalLocation,
	weights, AccountId, BridgeRococoBulletinGrandpa, BridgeRococoBulletinMessages, Runtime,
	RuntimeEvent, XcmOverRococoBulletin, XcmRouter,
};
use bp_messages::LaneId;
use bridge_runtime_common::{
	messages,
	messages::{
		source::{FromBridgedChainMessagesDeliveryProof, TargetHeaderChainAdapter},
		target::{FromBridgedChainMessagesProof, SourceHeaderChainAdapter},
		MessageBridge, UnderlyingChainProvider,
	},
	messages_xcm_extension::{
		SenderAndLane, XcmAsPlainPayload, XcmBlobHauler,
		XcmBlobMessageDispatch,
	},
	refund_relayer_extension::{
		ActualFeeRefund, RefundBridgedGrandpaMessages, RefundSignedExtensionAdapter,
		RefundableMessagesLane,
	},
};

use frame_support::{parameter_types, traits::PalletInfoAccess};
use sp_runtime::RuntimeDebug;
use xcm::{
	latest::prelude::*,
	prelude::{InteriorMultiLocation, NetworkId},
};
use xcm_builder::BridgeBlobDispatcher;

parameter_types! {
	/// Maximal number of entries in the unrewarded relayers vector at the Rococo Bridge Hub. It matches the
	/// maximal number of unrewarded relayers that the single confirmation transaction at Rococo Bulletin Chain
	/// may process.
	pub const MaxUnrewardedRelayerEntriesAtInboundLane: bp_messages::MessageNonce =
		bp_polkadot_bulletin::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	/// Maximal number of unconfirmed messages at the Rococo Bridge Hub. It matches the maximal number of
	/// uncinfirmed messages that the single confirmation transaction at Rococo Bulletin Chain may process.
	pub const MaxUnconfirmedMessagesAtInboundLane: bp_messages::MessageNonce =
		bp_polkadot_bulletin::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
	/// Bridge specific chain (network) identifier of the Rococo Bulletin Chain.
	pub const RococoBulletinChainId: bp_runtime::ChainId = bp_runtime::POLKADOT_BULLETIN_CHAIN_ID;
	/// Interior location (relative to this runtime) of the with-RococoBulletin messages pallet.
	pub BridgeRococoToRococoBulletinMessagesPalletInstance: InteriorMultiLocation = X1(
		PalletInstance(<BridgeRococoBulletinMessages as PalletInfoAccess>::index() as u8)
	);
	/// Rococo Bulletin Network identifier.
	pub RococoBulletinGlobalConsensusNetwork: NetworkId = NetworkId::ByGenesis([42u8; 32]); // TODO
	/// All active lanes that the current bridge supports.
	pub ActiveOutboundLanesToRococoBulletin: &'static [bp_messages::LaneId]
		= &[XCM_LANE_FOR_ROCOCO_PEOPLE_TO_ROCOCO_BULLETIN];
	/// Lane identifier, used to connect Rococo People and Rococo Bulletin chain.
	pub const RococoPeopleToRococoBulletinMessagesLane: bp_messages::LaneId
		= XCM_LANE_FOR_ROCOCO_PEOPLE_TO_ROCOCO_BULLETIN;
	/// Priority boost that the registered relayer receives for every additional message in the message
	/// delivery transaction.
	///
	/// It is determined semi-automatically - see `FEE_BOOST_PER_MESSAGE` constant to get the
	/// meaning of this value.
	pub PriorityBoostPerMessage: u64 = 182_044_444_444_444;

	/// Identifier of the sibling Rococo People parachain.
	pub RococoPeopleParaId: cumulus_primitives_core::ParaId = 1004.into(); // TODO: confirm
	/// A route (XCM location and bridge lane) that the Rococo People Chain -> Rococo Bulletin Chain
	/// message is following.
	pub FromRococoPeopleToRococoBulletinRoute: SenderAndLane = SenderAndLane::new(
		ParentThen(X1(Parachain(RococoPeopleParaId::get().into()))).into(),
		XCM_LANE_FOR_ROCOCO_PEOPLE_TO_ROCOCO_BULLETIN,
	);
	/// All active routes and their XCM destinations.
	pub ActiveLanes: sp_std::vec::Vec<(SenderAndLane, (NetworkId, InteriorMultiLocation))> = sp_std::vec![
			(
				FromRococoPeopleToRococoBulletinRoute::get(),
				(RococoBulletinGlobalConsensusNetwork::get(), Here)
			)
	];

	/// XCM message that is never sent.
	pub NeverSentMessage: Option<Xcm<()>> = None;
}
pub const XCM_LANE_FOR_ROCOCO_PEOPLE_TO_ROCOCO_BULLETIN: LaneId = LaneId([0, 0, 0, 0]);

/// Proof of messages, coming from Rococo Bulletin chain.
pub type FromRococoBulletinMessagesProof =
	FromBridgedChainMessagesProof<bp_polkadot_bulletin::Hash>;
/// Messages delivery proof for Rococo Bridge Hub -> Rococo Bulletin messages.
pub type ToRococoBulletinMessagesDeliveryProof =
	FromBridgedChainMessagesDeliveryProof<bp_polkadot_bulletin::Hash>;

/// Dispatches received XCM messages from other bridge.
type FromRococoBulletinMessageBlobDispatcher = BridgeBlobDispatcher<
	XcmRouter,
	BridgeHubRococoUniversalLocation,
	BridgeRococoToRococoBulletinMessagesPalletInstance,
>;

/// Export XCM messages to be relayed to the other side.
pub type ToRococoBulletinHaulBlobExporter = XcmOverRococoBulletin;

pub struct ToRococoBulletinXcmBlobHauler;
impl XcmBlobHauler for ToRococoBulletinXcmBlobHauler {
	type Runtime = Runtime;
	type MessagesInstance = WithRococoBulletinMessagesInstance;

	type ToSourceChainSender = XcmRouter;
	type CongestedMessage = NeverSentMessage;
	type UncongestedMessage = NeverSentMessage;
}

/// Messaging Bridge configuration for BridgeHubRococo -> Rococo Bulletin.
pub struct WithRococoBulletinMessageBridge;
impl MessageBridge for WithRococoBulletinMessageBridge {
	// Bulletin chain assumes it is bridged with Polkadot Bridge Hub
	const BRIDGED_MESSAGES_PALLET_NAME: &'static str =
		bp_bridge_hub_polkadot::WITH_BRIDGE_HUB_POLKADOT_MESSAGES_PALLET_NAME;
	type ThisChain = BridgeHubRococo;
	type BridgedChain = RococoBulletin;
	type BridgedHeaderChain = BridgeRococoBulletinGrandpa;
}

/// Message verifier for RococoBulletin messages sent from BridgeHubRococo.
pub type ToRococoBulletinMessageVerifier =
	messages::source::FromThisChainMessageVerifier<WithRococoBulletinMessageBridge>;

/// Maximal outbound payload size of BridgeHubRococo -> RococoBulletin messages.
pub type ToRococoBulletinMaximalOutboundPayloadSize =
	messages::source::FromThisChainMaximalOutboundPayloadSize<WithRococoBulletinMessageBridge>;

/// RococoBulletin chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct RococoBulletin;

impl UnderlyingChainProvider for RococoBulletin {
	type Chain = bp_polkadot_bulletin::PolkadotBulletin;
}

impl messages::BridgedChainWithMessages for RococoBulletin {}

/// Signed extension that refunds relayers that are delivering messages from the Rococo Bulletin
/// chain.
pub type OnBridgeHubRococoRefundRococoBulletinMessages = RefundSignedExtensionAdapter<
	RefundBridgedGrandpaMessages<
		Runtime,
		BridgeGrandpaRococoBulletinInstance,
		RefundableMessagesLane<
			WithRococoBulletinMessagesInstance,
			RococoPeopleToRococoBulletinMessagesLane,
		>,
		ActualFeeRefund<Runtime>,
		PriorityBoostPerMessage,
		StrOnBridgeHubRococoRefundRococoBulletinMessages,
	>,
>;
bp_runtime::generate_static_str_provider!(OnBridgeHubRococoRefundRococoBulletinMessages);

/// Add XCM messages support for BridgeHubRococo to support Rococo->Rococo Bulletin XCM messages.
pub type WithRococoBulletinMessagesInstance = pallet_bridge_messages::Instance4;
impl pallet_bridge_messages::Config<WithRococoBulletinMessagesInstance> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo =
		weights::pallet_bridge_messages_rococo_to_rococo_bulletin::WeightInfo<Runtime>;
	type BridgedChainId = RococoBulletinChainId;
	type ActiveOutboundLanes = ActiveOutboundLanesToRococoBulletin;
	type MaxUnrewardedRelayerEntriesAtInboundLane = MaxUnrewardedRelayerEntriesAtInboundLane;
	type MaxUnconfirmedMessagesAtInboundLane = MaxUnconfirmedMessagesAtInboundLane;

	type MaximalOutboundPayloadSize = ToRococoBulletinMaximalOutboundPayloadSize;
	type OutboundPayload = XcmAsPlainPayload;

	type InboundPayload = XcmAsPlainPayload;
	type InboundRelayer = AccountId;
	type DeliveryPayments = ();

	type TargetHeaderChain = TargetHeaderChainAdapter<WithRococoBulletinMessageBridge>;
	type LaneMessageVerifier = ToRococoBulletinMessageVerifier;
	type DeliveryConfirmationPayments = ();

	type SourceHeaderChain = SourceHeaderChainAdapter<WithRococoBulletinMessageBridge>;
	type MessageDispatch =
		XcmBlobMessageDispatch<FromRococoBulletinMessageBlobDispatcher, Self::WeightInfo, ()>;
	type OnMessagesDelivered = ();
}

/// Add support for the export and dispatch of XCM programs.
pub type XcmOverRococoBulletinInstance = pallet_xcm_bridge_hub::Instance2;
impl pallet_xcm_bridge_hub::Config<XcmOverRococoBulletinInstance> for Runtime {
	type UniversalLocation = UniversalLocation;
	type BridgedNetworkId = RococoBulletinGlobalConsensusNetwork;
	type BridgeMessagesPalletInstance = WithRococoBulletinMessagesInstance;
	type MessageExportPrice = ();
	type Lanes = ActiveLanes;
	type LanesSupport = ToRococoBulletinXcmBlobHauler;
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::bridge_common_config::BridgeGrandpaRococoBulletinInstance;
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
			WithRococoBulletinMessagesInstance,
		>(
			bp_polkadot_bulletin::EXTRA_STORAGE_PROOF_SIZE,
			bp_bridge_hub_rococo::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
			bp_bridge_hub_rococo::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
			true,
		);
	}

	#[test]
	fn ensure_bridge_integrity() {
		assert_complete_bridge_types!(
			runtime: Runtime,
			with_bridged_chain_grandpa_instance: BridgeGrandpaRococoBulletinInstance,
			with_bridged_chain_messages_instance: WithRococoBulletinMessagesInstance,
			bridge: WithRococoBulletinMessageBridge,
			this_chain: bp_rococo::Rococo,
			bridged_chain: bp_polkadot_bulletin::PolkadotBulletin,
		);

		assert_complete_bridge_constants::<
			Runtime,
			BridgeGrandpaRococoBulletinInstance,
			WithRococoBulletinMessagesInstance,
			WithRococoBulletinMessageBridge,
		>(AssertCompleteBridgeConstants {
			this_chain_constants: AssertChainConstants {
				block_length: bp_bridge_hub_rococo::BlockLength::get(),
				block_weights: bp_bridge_hub_rococo::BlockWeights::get(),
			},
			messages_pallet_constants: AssertBridgeMessagesPalletConstants {
				max_unrewarded_relayers_in_bridged_confirmation_tx:
					bp_polkadot_bulletin::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
				max_unconfirmed_messages_in_bridged_confirmation_tx:
					bp_polkadot_bulletin::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
				bridged_chain_id: bp_runtime::POLKADOT_BULLETIN_CHAIN_ID,
			},
			pallet_names: AssertBridgePalletNames {
				// we emulate Pollkadot Bridge Hub here
				with_this_chain_messages_pallet_name:
					bp_bridge_hub_polkadot::WITH_BRIDGE_HUB_POLKADOT_MESSAGES_PALLET_NAME,
				with_bridged_chain_grandpa_pallet_name:
					bp_polkadot_bulletin::WITH_POLKADOT_BULLETIN_GRANDPA_PALLET_NAME,
				with_bridged_chain_messages_pallet_name:
					bp_polkadot_bulletin::WITH_POLKADOT_BULLETIN_MESSAGES_PALLET_NAME,
			},
		});

		bridge_runtime_common::priority_calculator::ensure_priority_boost_is_sane::<
			Runtime,
			WithRococoBulletinMessagesInstance,
			PriorityBoostPerMessage,
		>(FEE_BOOST_PER_MESSAGE);

		assert_eq!(
			BridgeRococoToRococoBulletinMessagesPalletInstance::get(),
			X1(PalletInstance(
				53 /* TODO: bp_bridge_hub_rococo::WITH_BRIDGE_ROCOCO_TO_ROCOCO_BULLETIN_MESSAGES_PALLET_INDEX */
			))
		);
	}
}
