// Copyright 2022 Parity Technologies (UK) Ltd.
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

//! BridgeHubPolkadot-to-PolkadotBulletin messages sync entrypoint.

use crate::cli::bridge::{CliBridgeBase, MessagesCliBridge};
use relay_bridge_hub_polkadot_client::BridgeHubPolkadot;
use relay_polkadot_bulletin_client::PolkadotBulletin;
use substrate_relay_helper::{messages_lane::SubstrateMessageLane, UtilityPalletBatchCallBuilder};

/// BridgeHubPolkadot-to-PolkadotBulletin messages bridge.
pub struct BridgeHubPolkadotToPolkadotBulletinMessagesCliBridge {}

impl CliBridgeBase for BridgeHubPolkadotToPolkadotBulletinMessagesCliBridge {
	type Source = BridgeHubPolkadot;
	type Target = PolkadotBulletin;
}

impl MessagesCliBridge for BridgeHubPolkadotToPolkadotBulletinMessagesCliBridge {
	type MessagesLane = BridgeHubPolkadotMessagesToPolkadotBulletinMessageLane;
}

substrate_relay_helper::generate_receive_message_proof_call_builder!(
	BridgeHubPolkadotMessagesToPolkadotBulletinMessageLane,
	BridgeHubPolkadotMessagesToPolkadotBulletinMessageLaneReceiveMessagesProofCallBuilder,
	relay_polkadot_bulletin_client::RuntimeCall::BridgePolkadotBridgeHubMessages,
	relay_polkadot_bulletin_client::BridgePolkadotBridgeHubMessagesCall::receive_messages_proof
);

substrate_relay_helper::generate_receive_message_delivery_proof_call_builder!(
	BridgeHubPolkadotMessagesToPolkadotBulletinMessageLane,
	BridgeHubPolkadotMessagesToPolkadotBulletinMessageLaneReceiveMessagesDeliveryProofCallBuilder,
	relay_bridge_hub_polkadot_client::runtime::Call::BridgePolkadotBulletinMessages,
	relay_bridge_hub_polkadot_client::runtime::BridgePolkadotBulletinMessagesCall::receive_messages_delivery_proof
);

/// BridgeHubPolkadot-to-PolkadotBulletin messages lane.
#[derive(Clone, Debug)]
pub struct BridgeHubPolkadotMessagesToPolkadotBulletinMessageLane;

impl SubstrateMessageLane for BridgeHubPolkadotMessagesToPolkadotBulletinMessageLane {
	type SourceChain = BridgeHubPolkadot;
	type TargetChain = PolkadotBulletin;

	type ReceiveMessagesProofCallBuilder =
		BridgeHubPolkadotMessagesToPolkadotBulletinMessageLaneReceiveMessagesProofCallBuilder;
	type ReceiveMessagesDeliveryProofCallBuilder =
		BridgeHubPolkadotMessagesToPolkadotBulletinMessageLaneReceiveMessagesDeliveryProofCallBuilder;

	type SourceBatchCallBuilder = UtilityPalletBatchCallBuilder<BridgeHubPolkadot>;
	type TargetBatchCallBuilder = ();
}
