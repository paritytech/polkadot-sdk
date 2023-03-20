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

//! BridgeHubPolkadot-to-BridgeHubKusama messages sync entrypoint.

use crate::cli::bridge::{CliBridgeBase, MessagesCliBridge};
use relay_bridge_hub_kusama_client::BridgeHubKusama;
use relay_bridge_hub_polkadot_client::BridgeHubPolkadot;
use substrate_relay_helper::{messages_lane::SubstrateMessageLane, UtilityPalletBatchCallBuilder};

/// BridgeHubPolkadot-to-BridgeHubKusama messages bridge.
pub struct BridgeHubPolkadotToBridgeHubKusamaMessagesCliBridge {}

impl CliBridgeBase for BridgeHubPolkadotToBridgeHubKusamaMessagesCliBridge {
	type Source = BridgeHubPolkadot;
	type Target = BridgeHubKusama;
}

impl MessagesCliBridge for BridgeHubPolkadotToBridgeHubKusamaMessagesCliBridge {
	type MessagesLane = BridgeHubPolkadotMessagesToBridgeHubKusamaMessageLane;
}

substrate_relay_helper::generate_receive_message_proof_call_builder!(
	BridgeHubPolkadotMessagesToBridgeHubKusamaMessageLane,
	BridgeHubPolkadotMessagesToBridgeHubKusamaMessageLaneReceiveMessagesProofCallBuilder,
	relay_bridge_hub_kusama_client::runtime::Call::BridgePolkadotMessages,
	relay_bridge_hub_kusama_client::runtime::BridgePolkadotMessagesCall::receive_messages_proof
);

substrate_relay_helper::generate_receive_message_delivery_proof_call_builder!(
	BridgeHubPolkadotMessagesToBridgeHubKusamaMessageLane,
	BridgeHubPolkadotMessagesToBridgeHubKusamaMessageLaneReceiveMessagesDeliveryProofCallBuilder,
	relay_bridge_hub_polkadot_client::runtime::Call::BridgeKusamaMessages,
	relay_bridge_hub_polkadot_client::runtime::BridgeKusamaMessagesCall::receive_messages_delivery_proof
);

/// BridgeHubPolkadot-to-BridgeHubKusama messages lane.
#[derive(Clone, Debug)]
pub struct BridgeHubPolkadotMessagesToBridgeHubKusamaMessageLane;

impl SubstrateMessageLane for BridgeHubPolkadotMessagesToBridgeHubKusamaMessageLane {
	type SourceChain = BridgeHubPolkadot;
	type TargetChain = BridgeHubKusama;

	type ReceiveMessagesProofCallBuilder =
		BridgeHubPolkadotMessagesToBridgeHubKusamaMessageLaneReceiveMessagesProofCallBuilder;
	type ReceiveMessagesDeliveryProofCallBuilder =
		BridgeHubPolkadotMessagesToBridgeHubKusamaMessageLaneReceiveMessagesDeliveryProofCallBuilder;

	type SourceBatchCallBuilder = UtilityPalletBatchCallBuilder<BridgeHubPolkadot>;
	type TargetBatchCallBuilder = UtilityPalletBatchCallBuilder<BridgeHubKusama>;
}
