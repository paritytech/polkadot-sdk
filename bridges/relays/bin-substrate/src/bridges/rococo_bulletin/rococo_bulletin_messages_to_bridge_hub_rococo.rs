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

//! RococoBulletin-to-BridgeHubRococo messages sync entrypoint.

use super::BridgeHubRococoAsBridgeHubPolkadot;
use crate::cli::bridge::{CliBridgeBase, MessagesCliBridge};
use relay_polkadot_bulletin_client::PolkadotBulletin as RococoBulletin;
use substrate_relay_helper::{messages_lane::SubstrateMessageLane, UtilityPalletBatchCallBuilder};

/// RococoBulletin-to-BridgeHubRococo messages bridge.
pub struct RococoBulletinToBridgeHubRococoMessagesCliBridge {}

impl CliBridgeBase for RococoBulletinToBridgeHubRococoMessagesCliBridge {
	type Source = RococoBulletin;
	type Target = BridgeHubRococoAsBridgeHubPolkadot;
}

impl MessagesCliBridge for RococoBulletinToBridgeHubRococoMessagesCliBridge {
	type MessagesLane = RococoBulletinMessagesToBridgeHubRococoMessageLane;
}

substrate_relay_helper::generate_receive_message_proof_call_builder!(
	RococoBulletinMessagesToBridgeHubRococoMessageLane,
	RococoBulletinMessagesToBridgeHubRococoMessageLaneReceiveMessagesProofCallBuilder,
	relay_bridge_hub_rococo_client::RuntimeCall::BridgePolkadotBulletinMessages,
	relay_bridge_hub_rococo_client::BridgeBulletinMessagesCall::receive_messages_proof
);

substrate_relay_helper::generate_receive_message_delivery_proof_call_builder!(
	RococoBulletinMessagesToBridgeHubRococoMessageLane,
	RococoBulletinMessagesToBridgeHubRococoMessageLaneReceiveMessagesDeliveryProofCallBuilder,
	relay_polkadot_bulletin_client::RuntimeCall::BridgePolkadotMessages,
	relay_polkadot_bulletin_client::BridgePolkadotMessagesCall::receive_messages_delivery_proof
);

/// RococoBulletin-to-BridgeHubRococo messages lane.
#[derive(Clone, Debug)]
pub struct RococoBulletinMessagesToBridgeHubRococoMessageLane;

impl SubstrateMessageLane for RococoBulletinMessagesToBridgeHubRococoMessageLane {
	type SourceChain = RococoBulletin;
	type TargetChain = BridgeHubRococoAsBridgeHubPolkadot;

	type ReceiveMessagesProofCallBuilder =
		RococoBulletinMessagesToBridgeHubRococoMessageLaneReceiveMessagesProofCallBuilder;
	type ReceiveMessagesDeliveryProofCallBuilder =
		RococoBulletinMessagesToBridgeHubRococoMessageLaneReceiveMessagesDeliveryProofCallBuilder;

	type SourceBatchCallBuilder = ();
	type TargetBatchCallBuilder = UtilityPalletBatchCallBuilder<BridgeHubRococoAsBridgeHubPolkadot>;
}
