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

use crate::BridgePolkadotBulletinGrandpa;

use bp_messages::LaneId;
use bp_runtime::{ChainId, UnderlyingChainProvider};
use bridge_runtime_common::messages::{self, MessageBridge, ThisChainWithMessages};
use frame_support::parameter_types;
use sp_runtime::RuntimeDebug;

/// A chain identifier of the Polkadot Bulletin Chain.
///
/// This type (and the constant) will be removed in the future versions, so it is here.
pub const POLKADOT_BULLETIN_CHAIN_ID: ChainId = *b"pbch";
/// The only lane we are using to bridge with Polkadot Bulletin Chain.
pub const WITH_POLKADOT_BULLETIN_LANE: LaneId = LaneId([0, 0, 0, 0]);

parameter_types! {
	/// Maximal number of unrewarded relayer entries.
	pub const MaxUnrewardedRelayerEntriesAtInboundLane: bp_messages::MessageNonce =
		bp_polkadot_bulletin::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	/// Maximal number of unconfirmed messages.
	pub const MaxUnconfirmedMessagesAtInboundLane: bp_messages::MessageNonce =
		bp_polkadot_bulletin::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
	/// Chain identifier of Polkadot Bulletin Chain.
	pub const PolkadotBulletinChainId: bp_runtime::ChainId = POLKADOT_BULLETIN_CHAIN_ID;

	/// All active lanes in the with Polkadot Bulletin Chain  bridge.
	pub ActiveOutboundLanesToPolkadotBulletin: &'static [LaneId] = &[WITH_POLKADOT_BULLETIN_LANE];
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