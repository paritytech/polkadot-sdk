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

//! Rococo-to-Westend parachains sync entrypoint.

use crate::cli::bridge::{CliBridgeBase, MessagesCliBridge, ParachainToRelayHeadersCliBridge};
use bp_polkadot_core::parachains::{ParaHash, ParaHeadsProof, ParaId};
use relay_substrate_client::{CallOf, HeaderIdOf};
use substrate_relay_helper::parachains::{
	SubmitParachainHeadsCallBuilder, SubstrateParachainsPipeline,
};

/// BridgeHub-to-BridgeHub parachain sync description.
#[derive(Clone, Debug)]
pub struct BridgeHubWestendToBridgeHubRococo;

impl SubstrateParachainsPipeline for BridgeHubWestendToBridgeHubRococo {
	type SourceParachain = relay_bridge_hub_westend_client::BridgeHubWestend;
	type SourceRelayChain = relay_westend_client::Westend;
	type TargetChain = relay_bridge_hub_rococo_client::BridgeHubRococo;

	type SubmitParachainHeadsCallBuilder = BridgeHubWestendToBridgeHubRococoCallBuilder;
}

pub struct BridgeHubWestendToBridgeHubRococoCallBuilder;
impl SubmitParachainHeadsCallBuilder<BridgeHubWestendToBridgeHubRococo>
	for BridgeHubWestendToBridgeHubRococoCallBuilder
{
	fn build_submit_parachain_heads_call(
		at_relay_block: HeaderIdOf<relay_westend_client::Westend>,
		parachains: Vec<(ParaId, ParaHash)>,
		parachain_heads_proof: ParaHeadsProof,
	) -> CallOf<relay_bridge_hub_rococo_client::BridgeHubRococo> {
		relay_bridge_hub_rococo_client::RuntimeCall::BridgeWestendParachains(
			relay_bridge_hub_rococo_client::BridgeParachainCall::submit_parachain_heads {
				at_relay_block: (at_relay_block.0, at_relay_block.1),
				parachains,
				parachain_heads_proof,
			},
		)
	}
}

/// `BridgeHubParachain` to `BridgeHubParachain` bridge definition.
pub struct BridgeHubWestendToBridgeHubRococoCliBridge {}

impl ParachainToRelayHeadersCliBridge for BridgeHubWestendToBridgeHubRococoCliBridge {
	type SourceRelay = relay_westend_client::Westend;
	type ParachainFinality = BridgeHubWestendToBridgeHubRococo;
	type RelayFinality =
		crate::bridges::rococo_westend::westend_headers_to_bridge_hub_rococo::WestendFinalityToBridgeHubRococo;
}

impl CliBridgeBase for BridgeHubWestendToBridgeHubRococoCliBridge {
	type Source = relay_bridge_hub_westend_client::BridgeHubWestend;
	type Target = relay_bridge_hub_rococo_client::BridgeHubRococo;
}

impl MessagesCliBridge for BridgeHubWestendToBridgeHubRococoCliBridge {
	type MessagesLane =
	crate::bridges::rococo_westend::bridge_hub_westend_messages_to_bridge_hub_rococo::BridgeHubWestendMessagesToBridgeHubRococoMessageLane;
}
