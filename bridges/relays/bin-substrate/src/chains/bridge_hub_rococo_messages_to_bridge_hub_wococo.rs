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
use bp_messages::Weight;
use messages_relay::relay_strategy::MixStrategy;
use relay_bridge_hub_rococo_client::BridgeHubRococo;
use relay_bridge_hub_wococo_client::BridgeHubWococo;
use substrate_relay_helper::messages_lane::SubstrateMessageLane;

pub struct BridgeHubRococoToBridgeHubWococoMessagesCliBridge {}

impl CliBridgeBase for BridgeHubRococoToBridgeHubWococoMessagesCliBridge {
	type Source = BridgeHubRococo;
	type Target = BridgeHubWococo;
}

impl MessagesCliBridge for BridgeHubRococoToBridgeHubWococoMessagesCliBridge {
	const ESTIMATE_MESSAGE_FEE_METHOD: &'static str =
		"TODO: not needed now, used for send_message and estimate_fee CLI";
	type MessagesLane = BridgeHubRococoMessagesToBridgeHubWococoMessageLane;
}

substrate_relay_helper::generate_mocked_receive_message_proof_call_builder!(
	BridgeHubRococoMessagesToBridgeHubWococoMessageLane,
	BridgeHubRococoMessagesToBridgeHubWococoMessageLaneReceiveMessagesProofCallBuilder,
	relay_bridge_hub_wococo_client::runtime::Call::BridgeRococoMessages,
	relay_bridge_hub_wococo_client::runtime::BridgeRococoMessagesCall::receive_messages_proof
);

substrate_relay_helper::generate_mocked_receive_message_delivery_proof_call_builder!(
	BridgeHubRococoMessagesToBridgeHubWococoMessageLane,
	BridgeHubRococoMessagesToBridgeHubWococoMessageLaneReceiveMessagesDeliveryProofCallBuilder,
	relay_bridge_hub_rococo_client::runtime::Call::BridgeWococoMessages,
	relay_bridge_hub_rococo_client::runtime::BridgeWococoMessagesCall::receive_messages_delivery_proof
);

/// Description of BridgeHubRococo -> BridgeHubWococo messages bridge.
#[derive(Clone, Debug)]
pub struct BridgeHubRococoMessagesToBridgeHubWococoMessageLane;

impl SubstrateMessageLane for BridgeHubRococoMessagesToBridgeHubWococoMessageLane {
	const SOURCE_TO_TARGET_CONVERSION_RATE_PARAMETER_NAME: Option<&'static str> = None;
	const TARGET_TO_SOURCE_CONVERSION_RATE_PARAMETER_NAME: Option<&'static str> = None;

	const SOURCE_FEE_MULTIPLIER_PARAMETER_NAME: Option<&'static str> = None;
	const TARGET_FEE_MULTIPLIER_PARAMETER_NAME: Option<&'static str> = None;

	const AT_SOURCE_TRANSACTION_PAYMENT_PALLET_NAME: Option<&'static str> = None;
	const AT_TARGET_TRANSACTION_PAYMENT_PALLET_NAME: Option<&'static str> = None;

	type SourceChain = BridgeHubRococo;
	type TargetChain = BridgeHubWococo;

	type ReceiveMessagesProofCallBuilder =
		BridgeHubRococoMessagesToBridgeHubWococoMessageLaneReceiveMessagesProofCallBuilder;
	type ReceiveMessagesDeliveryProofCallBuilder =
		BridgeHubRococoMessagesToBridgeHubWococoMessageLaneReceiveMessagesDeliveryProofCallBuilder;

	type TargetToSourceChainConversionRateUpdateBuilder = ();

	type RelayStrategy = MixStrategy;
}
