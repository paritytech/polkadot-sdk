// Copyright Parity Technologies (UK) Ltd.
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

//! Definitions, related to bridge with Polkadot Bulletin Chain.

// TODO: this file assumes that there'll be sibling chain (called Kawabunga) that will
// be sending messages to Polkadot Bulletin chain. We'll need to change that once it
// is decidec.

use crate::{
	bridge_kusama_config::PriorityBoostPerMessage, BridgeGrandpaBulletinInstance,
	BridgePolkadotBulletinGrandpa, BridgePolkadotBulletinMessages, Runtime,
	WithPolkadotBulletinMessagesInstance, XcmRouter,
};

use bp_messages::LaneId;
use bp_runtime::UnderlyingChainProvider;
use bridge_runtime_common::{
	messages::{self, MessageBridge, ThisChainWithMessages},
	messages_xcm_extension::{SenderAndLane, XcmBlobHauler, XcmBlobHaulerAdapter},
	refund_relayer_extension::{
		ActualFeeRefund, RefundBridgedGrandpaMessages, RefundSignedExtensionAdapter,
		RefundableMessagesLane,
	},
};
use frame_support::{parameter_types, traits::PalletInfoAccess};
use sp_runtime::RuntimeDebug;
use xcm::{latest::prelude::*, prelude::NetworkId};
use xcm_builder::{BridgeBlobDispatcher, HaulBlobExporter};

/// The only lane we are using to bridge with Polkadot Bulletin Chain.
pub const WITH_POLKADOT_BULLETIN_LANE: LaneId = LaneId([0, 0, 0, 0]);

parameter_types! {
	/// Network identifier of the Polkadot Bulletin chain.
	pub PolkadotBulletinGlobalConsensusNetwork: NetworkId = NetworkId::ByGenesis([42u8; 32]); // TODO
	/// Identifier of the Kawabunga parachain.
	pub KawabungaParaId: cumulus_primitives_core::ParaId = 42.into();

	/// Interior location of the with-Polakdot Bulletin messages pallet within this runtime.
	pub WithPolkadotBulletinMessagesPalletLocation: InteriorMultiLocation = X1(PalletInstance(
		<BridgePolkadotBulletinMessages as PalletInfoAccess>::index() as u8,
	));

	/// Maximal number of unrewarded relayer entries.
	pub const MaxUnrewardedRelayerEntriesAtInboundLane: bp_messages::MessageNonce =
		bp_polkadot_bulletin::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	/// Maximal number of unconfirmed messages.
	pub const MaxUnconfirmedMessagesAtInboundLane: bp_messages::MessageNonce =
		bp_polkadot_bulletin::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
	/// Chain identifier of Polkadot Bulletin Chain.
	pub const PolkadotBulletinChainId: bp_runtime::ChainId =
		bp_runtime::POLKADOT_BULLETIN_CHAIN_ID;

	/// The only lane we are using to bridge with Polkadot Bulletin Chain.
	pub const WithPolkadotBulletinLane: LaneId = LaneId([0, 0, 0, 0]);
	/// All active lanes in the with Polkadot Bulletin Chain  bridge.
	pub ActiveOutboundLanesToPolkadotBulletin: &'static [LaneId] = &[WITH_POLKADOT_BULLETIN_LANE];

	/// Sending chain location and lane used to communicate with Polkadot Bulletin chain.
	pub FromKawabungaToPolkadotBulletinRoute: SenderAndLane = SenderAndLane::new(
		ParentThen(X1(Parachain(KawabungaParaId::get().into()))).into(),
		WITH_POLKADOT_BULLETIN_LANE,
	);

	// Following constants are set to `None` assuming that the communication with Polkadot Bulletin
	// chain is the "system-to-system" communication and noone pays any fees anywhere. So we don't
	// need any congestion/uncongestion mechanisms here. If it will ever change, we'll need to
	// support that.

	/// Message that is sent to Kawabunga when the bridge becomes congested.
	pub CongestedMessage: Option<Xcm<()>> = None;
	/// Message that is sent to Kawabunga when the bridge becomes uncongested.
	pub UncongestedMessage: Option<Xcm<()>> = None;
}

/// Message verifier for PolkadotBulletin messages sent from ThisChain
pub type ToPolkadotBulletinMessageVerifier =
	messages::source::FromThisChainMessageVerifier<WithPolkadotBulletinMessageBridge>;

/// Maximal outbound payload size of ThisChain -> PolkadotBulletin messages.
pub type ToPolkadotBulletinMaximalOutboundPayloadSize =
	messages::source::FromThisChainMaximalOutboundPayloadSize<WithPolkadotBulletinMessageBridge>;

/// Messaging Bridge configuration for ThisChain -> PolkadotBulletin
pub struct WithPolkadotBulletinMessageBridge;

impl MessageBridge for WithPolkadotBulletinMessageBridge {
	const BRIDGED_MESSAGES_PALLET_NAME: &'static str =
		bp_bridge_hub_polkadot::WITH_BRIDGE_HUB_POLKADOT_MESSAGES_PALLET_NAME;
	type ThisChain = ThisChain;
	type BridgedChain = PolkadotBulletin;
	type BridgedHeaderChain = BridgePolkadotBulletinGrandpa;
}

/// PolkadotBulletin chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct PolkadotBulletin;

impl UnderlyingChainProvider for PolkadotBulletin {
	type Chain = bp_polkadot_bulletin::PolkadotBulletin;
}

impl messages::BridgedChainWithMessages for PolkadotBulletin {}

/// ThisChain chain from message lane point of view.
#[derive(RuntimeDebug, Clone, Copy)]
pub struct ThisChain;

impl UnderlyingChainProvider for ThisChain {
	type Chain = bp_bridge_hub_polkadot::BridgeHubPolkadot;
}

impl ThisChainWithMessages for ThisChain {
	type RuntimeOrigin = crate::RuntimeOrigin;
}

/// Dispatches received XCM messages from the Polkadot Bulletin chain.
pub type FromPolkadotBulletinBlobDispatcher<UniversalLocation> =
	BridgeBlobDispatcher<XcmRouter, UniversalLocation, WithPolkadotBulletinMessagesPalletLocation>;

/// Export XCM messages to be relayed to the Polkadot Bulletin chain.
pub type ToPolkadotBulletinHaulBlobExporter = HaulBlobExporter<
	XcmBlobHaulerAdapter<ToPolkadotBulletinXcmBlobHauler>,
	PolkadotBulletinGlobalConsensusNetwork,
	(),
>;
pub struct ToPolkadotBulletinXcmBlobHauler;
impl XcmBlobHauler for ToPolkadotBulletinXcmBlobHauler {
	type Runtime = Runtime;
	type MessagesInstance = WithPolkadotBulletinMessagesInstance;
	type SenderAndLane = FromKawabungaToPolkadotBulletinRoute;

	type ToSourceChainSender = crate::XcmRouter;
	type CongestedMessage = CongestedMessage;
	type UncongestedMessage = UncongestedMessage;
}

/// Signed extension that refunds relayers that are delivering messages from the Bulletin chain.
pub type BridgeRefundPolkadotBulletinMessages = RefundSignedExtensionAdapter<
	RefundBridgedGrandpaMessages<
		Runtime,
		BridgeGrandpaBulletinInstance,
		RefundableMessagesLane<WithPolkadotBulletinMessagesInstance, WithPolkadotBulletinLane>,
		ActualFeeRefund<Runtime>,
		// we could reuse the same priority boost as we do for with-Kusama bridge
		PriorityBoostPerMessage,
		StrBridgeRefundPolkadotBulletinMessages,
	>,
>;
bp_runtime::generate_static_str_provider!(BridgeRefundPolkadotBulletinMessages);
