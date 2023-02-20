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

//! BridgeHubWococo-to-BridgeHubRococo messages sync entrypoint.

use crate::cli::bridge::{CliBridgeBase, MessagesCliBridge};
use relay_bridge_hub_rococo_client::BridgeHubRococo;
use relay_bridge_hub_wococo_client::BridgeHubWococo;
use substrate_relay_helper::{messages_lane::SubstrateMessageLane, UtilityPalletBatchCallBuilder};

pub struct BridgeHubWococoToBridgeHubRococoMessagesCliBridge {}

impl CliBridgeBase for BridgeHubWococoToBridgeHubRococoMessagesCliBridge {
	type Source = BridgeHubWococo;
	type Target = BridgeHubRococo;
}

impl MessagesCliBridge for BridgeHubWococoToBridgeHubRococoMessagesCliBridge {
	type MessagesLane = BridgeHubWococoMessagesToBridgeHubRococoMessageLane;
}

substrate_relay_helper::generate_receive_message_proof_call_builder!(
	BridgeHubWococoMessagesToBridgeHubRococoMessageLane,
	BridgeHubWococoMessagesToBridgeHubRococoMessageLaneReceiveMessagesProofCallBuilder,
	relay_bridge_hub_rococo_client::runtime::Call::BridgeWococoMessages,
	relay_bridge_hub_rococo_client::runtime::BridgeWococoMessagesCall::receive_messages_proof
);

substrate_relay_helper::generate_receive_message_delivery_proof_call_builder!(
	BridgeHubWococoMessagesToBridgeHubRococoMessageLane,
	BridgeHubWococoMessagesToBridgeHubRococoMessageLaneReceiveMessagesDeliveryProofCallBuilder,
	relay_bridge_hub_wococo_client::runtime::Call::BridgeRococoMessages,
	relay_bridge_hub_wococo_client::runtime::BridgeRococoMessagesCall::receive_messages_delivery_proof
);

/// Description of BridgeHubWococo -> BridgeHubRococo messages bridge.
#[derive(Clone, Debug)]
pub struct BridgeHubWococoMessagesToBridgeHubRococoMessageLane;

impl SubstrateMessageLane for BridgeHubWococoMessagesToBridgeHubRococoMessageLane {
	type SourceChain = BridgeHubWococo;
	type TargetChain = BridgeHubRococo;

	type ReceiveMessagesProofCallBuilder =
		BridgeHubWococoMessagesToBridgeHubRococoMessageLaneReceiveMessagesProofCallBuilder;
	type ReceiveMessagesDeliveryProofCallBuilder =
		BridgeHubWococoMessagesToBridgeHubRococoMessageLaneReceiveMessagesDeliveryProofCallBuilder;

	type SourceBatchCallBuilder = UtilityPalletBatchCallBuilder<BridgeHubWococo>;
	type TargetBatchCallBuilder = UtilityPalletBatchCallBuilder<BridgeHubRococo>;
}
