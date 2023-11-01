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

//! BridgeHubRococo-to-BridgeHubWococo messages sync entrypoint.

use crate::cli::bridge::{CliBridgeBase, MessagesCliBridge};
use relay_bridge_hub_rococo_client::BridgeHubRococo;
use relay_bridge_hub_wococo_client::BridgeHubWococo;
use substrate_relay_helper::{messages_lane::SubstrateMessageLane, UtilityPalletBatchCallBuilder};

pub struct BridgeHubRococoToBridgeHubWococoMessagesCliBridge {}

impl CliBridgeBase for BridgeHubRococoToBridgeHubWococoMessagesCliBridge {
	type Source = BridgeHubRococo;
	type Target = BridgeHubWococo;
}

impl MessagesCliBridge for BridgeHubRococoToBridgeHubWococoMessagesCliBridge {
	type MessagesLane = BridgeHubRococoMessagesToBridgeHubWococoMessageLane;
}

substrate_relay_helper::generate_receive_message_proof_call_builder!(
	BridgeHubRococoMessagesToBridgeHubWococoMessageLane,
	BridgeHubRococoMessagesToBridgeHubWococoMessageLaneReceiveMessagesProofCallBuilder,
	relay_bridge_hub_wococo_client::RuntimeCall::BridgeRococoMessages,
	relay_bridge_hub_wococo_client::BridgeMessagesCall::receive_messages_proof
);

substrate_relay_helper::generate_receive_message_delivery_proof_call_builder!(
	BridgeHubRococoMessagesToBridgeHubWococoMessageLane,
	BridgeHubRococoMessagesToBridgeHubWococoMessageLaneReceiveMessagesDeliveryProofCallBuilder,
	relay_bridge_hub_rococo_client::RuntimeCall::BridgeWococoMessages,
	relay_bridge_hub_rococo_client::BridgeMessagesCall::receive_messages_delivery_proof
);

/// Description of BridgeHubRococo -> BridgeHubWococo messages bridge.
#[derive(Clone, Debug)]
pub struct BridgeHubRococoMessagesToBridgeHubWococoMessageLane;

impl SubstrateMessageLane for BridgeHubRococoMessagesToBridgeHubWococoMessageLane {
	type SourceChain = BridgeHubRococo;
	type TargetChain = BridgeHubWococo;

	type ReceiveMessagesProofCallBuilder =
		BridgeHubRococoMessagesToBridgeHubWococoMessageLaneReceiveMessagesProofCallBuilder;
	type ReceiveMessagesDeliveryProofCallBuilder =
		BridgeHubRococoMessagesToBridgeHubWococoMessageLaneReceiveMessagesDeliveryProofCallBuilder;

	type SourceBatchCallBuilder = UtilityPalletBatchCallBuilder<BridgeHubRococo>;
	type TargetBatchCallBuilder = UtilityPalletBatchCallBuilder<BridgeHubWococo>;
}
