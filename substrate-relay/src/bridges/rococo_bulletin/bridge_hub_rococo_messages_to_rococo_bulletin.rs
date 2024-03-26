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

//! BridgeHubRococo-to-RococoBulletin messages sync entrypoint.

use super::BridgeHubRococoAsBridgeHubPolkadot;
use relay_polkadot_bulletin_client::PolkadotBulletin as RococoBulletin;
use substrate_relay_helper::{
	cli::bridge::{CliBridgeBase, MessagesCliBridge},
	messages_lane::SubstrateMessageLane,
	UtilityPalletBatchCallBuilder,
};

/// BridgeHubRococo-to-RococoBulletin messages bridge.
pub struct BridgeHubRococoToRococoBulletinMessagesCliBridge {}

impl CliBridgeBase for BridgeHubRococoToRococoBulletinMessagesCliBridge {
	type Source = BridgeHubRococoAsBridgeHubPolkadot;
	type Target = RococoBulletin;
}

impl MessagesCliBridge for BridgeHubRococoToRococoBulletinMessagesCliBridge {
	type MessagesLane = BridgeHubRococoMessagesToRococoBulletinMessageLane;
}

substrate_relay_helper::generate_receive_message_proof_call_builder!(
	BridgeHubRococoMessagesToRococoBulletinMessageLane,
	BridgeHubRococoMessagesToRococoBulletinMessageLaneReceiveMessagesProofCallBuilder,
	relay_polkadot_bulletin_client::RuntimeCall::BridgePolkadotMessages,
	relay_polkadot_bulletin_client::BridgePolkadotMessagesCall::receive_messages_proof
);

substrate_relay_helper::generate_receive_message_delivery_proof_call_builder!(
	BridgeHubRococoMessagesToRococoBulletinMessageLane,
	BridgeHubRococoMessagesToRococoBulletinMessageLaneReceiveMessagesDeliveryProofCallBuilder,
	relay_bridge_hub_rococo_client::RuntimeCall::BridgePolkadotBulletinMessages,
	relay_bridge_hub_rococo_client::BridgeBulletinMessagesCall::receive_messages_delivery_proof
);

/// BridgeHubRococo-to-RococoBulletin messages lane.
#[derive(Clone, Debug)]
pub struct BridgeHubRococoMessagesToRococoBulletinMessageLane;

impl SubstrateMessageLane for BridgeHubRococoMessagesToRococoBulletinMessageLane {
	type SourceChain = BridgeHubRococoAsBridgeHubPolkadot;
	type TargetChain = RococoBulletin;

	type ReceiveMessagesProofCallBuilder =
		BridgeHubRococoMessagesToRococoBulletinMessageLaneReceiveMessagesProofCallBuilder;
	type ReceiveMessagesDeliveryProofCallBuilder =
		BridgeHubRococoMessagesToRococoBulletinMessageLaneReceiveMessagesDeliveryProofCallBuilder;

	type SourceBatchCallBuilder = UtilityPalletBatchCallBuilder<BridgeHubRococoAsBridgeHubPolkadot>;
	type TargetBatchCallBuilder = ();
}
