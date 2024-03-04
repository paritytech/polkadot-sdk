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

//! PolkadotBulletin-to-BridgeHubPolkadot messages sync entrypoint.

use crate::cli::bridge::{CliBridgeBase, MessagesCliBridge};
use relay_bridge_hub_polkadot_client::BridgeHubPolkadot;
use relay_polkadot_bulletin_client::PolkadotBulletin;
use substrate_relay_helper::{messages_lane::SubstrateMessageLane, UtilityPalletBatchCallBuilder};

/// PolkadotBulletin-to-BridgeHubPolkadot messages bridge.
pub struct PolkadotBulletinToBridgeHubPolkadotMessagesCliBridge {}

impl CliBridgeBase for PolkadotBulletinToBridgeHubPolkadotMessagesCliBridge {
	type Source = PolkadotBulletin;
	type Target = BridgeHubPolkadot;
}

impl MessagesCliBridge for PolkadotBulletinToBridgeHubPolkadotMessagesCliBridge {
	type MessagesLane = PolkadotBulletinMessagesToBridgeHubPolkadotMessageLane;
}

substrate_relay_helper::generate_receive_message_proof_call_builder!(
	PolkadotBulletinMessagesToBridgeHubPolkadotMessageLane,
	PolkadotBulletinMessagesToBridgeHubPolkadotMessageLaneReceiveMessagesProofCallBuilder,
	// TODO: https://github.com/paritytech/parity-bridges-common/issues/2547 - use BridgePolkadotBulletinMessages
	relay_bridge_hub_polkadot_client::RuntimeCall::BridgeKusamaMessages,
	relay_bridge_hub_polkadot_client::BridgePolkadotBulletinMessagesCall::receive_messages_proof
);

substrate_relay_helper::generate_receive_message_delivery_proof_call_builder!(
	PolkadotBulletinMessagesToBridgeHubPolkadotMessageLane,
	PolkadotBulletinMessagesToBridgeHubPolkadotMessageLaneReceiveMessagesDeliveryProofCallBuilder,
	relay_polkadot_bulletin_client::RuntimeCall::BridgePolkadotMessages,
	relay_polkadot_bulletin_client::BridgePolkadotMessagesCall::receive_messages_delivery_proof
);

/// PolkadotBulletin-to-BridgeHubPolkadot messages lane.
#[derive(Clone, Debug)]
pub struct PolkadotBulletinMessagesToBridgeHubPolkadotMessageLane;

impl SubstrateMessageLane for PolkadotBulletinMessagesToBridgeHubPolkadotMessageLane {
	type SourceChain = PolkadotBulletin;
	type TargetChain = BridgeHubPolkadot;

	type ReceiveMessagesProofCallBuilder =
		PolkadotBulletinMessagesToBridgeHubPolkadotMessageLaneReceiveMessagesProofCallBuilder;
	type ReceiveMessagesDeliveryProofCallBuilder =
		PolkadotBulletinMessagesToBridgeHubPolkadotMessageLaneReceiveMessagesDeliveryProofCallBuilder;

	type SourceBatchCallBuilder = ();
	type TargetBatchCallBuilder = UtilityPalletBatchCallBuilder<BridgeHubPolkadot>;
}
