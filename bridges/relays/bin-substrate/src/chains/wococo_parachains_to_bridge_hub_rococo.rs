// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

//! Rococo-to-Wococo parachains sync entrypoint.

use crate::cli::bridge::{CliBridgeBase, MessagesCliBridge, ParachainToRelayHeadersCliBridge};
use bp_polkadot_core::parachains::{ParaHash, ParaHeadsProof, ParaId};
use parachains_relay::ParachainsPipeline;
use relay_substrate_client::{CallOf, HeaderIdOf};
use substrate_relay_helper::parachains::{
	SubmitParachainHeadsCallBuilder, SubstrateParachainsPipeline,
};

/// BridgeHub-to-BridgeHub parachain sync description.
#[derive(Clone, Debug)]
pub struct BridgeHubWococoToBridgeHubRococo;

impl ParachainsPipeline for BridgeHubWococoToBridgeHubRococo {
	type SourceChain = relay_wococo_client::Wococo;
	type TargetChain = relay_bridge_hub_rococo_client::BridgeHubRococo;
}

impl SubstrateParachainsPipeline for BridgeHubWococoToBridgeHubRococo {
	type SourceParachain = relay_bridge_hub_wococo_client::BridgeHubWococo;
	type SourceRelayChain = relay_wococo_client::Wococo;
	type TargetChain = relay_bridge_hub_rococo_client::BridgeHubRococo;

	type SubmitParachainHeadsCallBuilder = BridgeHubWococoToBridgeHubRococoCallBuilder;

	const SOURCE_PARACHAIN_PARA_ID: u32 = bp_bridge_hub_wococo::BRIDGE_HUB_WOCOCO_PARACHAIN_ID;
}

pub struct BridgeHubWococoToBridgeHubRococoCallBuilder;
impl SubmitParachainHeadsCallBuilder<BridgeHubWococoToBridgeHubRococo>
	for BridgeHubWococoToBridgeHubRococoCallBuilder
{
	fn build_submit_parachain_heads_call(
		at_relay_block: HeaderIdOf<relay_wococo_client::Wococo>,
		parachains: Vec<(ParaId, ParaHash)>,
		parachain_heads_proof: ParaHeadsProof,
	) -> CallOf<relay_bridge_hub_rococo_client::BridgeHubRococo> {
		relay_bridge_hub_rococo_client::runtime::Call::BridgeWococoParachain(
			relay_bridge_hub_rococo_client::runtime::BridgeParachainCall::submit_parachain_heads(
				(at_relay_block.0, at_relay_block.1),
				parachains,
				parachain_heads_proof,
			),
		)
	}
}

/// `BridgeHubParachain` to `BridgeHubParachain` bridge definition.
pub struct BridgeHubWococoToBridgeHubRococoCliBridge {}

impl ParachainToRelayHeadersCliBridge for BridgeHubWococoToBridgeHubRococoCliBridge {
	type SourceRelay = relay_wococo_client::Wococo;
	type ParachainFinality = BridgeHubWococoToBridgeHubRococo;
	type RelayFinality =
		crate::chains::wococo_headers_to_bridge_hub_rococo::WococoFinalityToBridgeHubRococo;
}

impl CliBridgeBase for BridgeHubWococoToBridgeHubRococoCliBridge {
	type Source = relay_bridge_hub_wococo_client::BridgeHubWococo;
	type Target = relay_bridge_hub_rococo_client::BridgeHubRococo;
}

impl MessagesCliBridge for BridgeHubWococoToBridgeHubRococoCliBridge {
	const ESTIMATE_MESSAGE_FEE_METHOD: &'static str =
		bp_bridge_hub_rococo::TO_BRIDGE_HUB_ROCOCO_ESTIMATE_MESSAGE_FEE_METHOD;
	type MessagesLane =
	crate::chains::bridge_hub_wococo_messages_to_bridge_hub_rococo::BridgeHubWococoMessagesToBridgeHubRococoMessageLane;
}
