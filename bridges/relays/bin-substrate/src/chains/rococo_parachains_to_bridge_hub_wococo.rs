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

//! Wococo-to-Rococo parachains sync entrypoint.

use crate::cli::bridge::{CliBridgeBase, ParachainToRelayHeadersCliBridge};
use bp_polkadot_core::parachains::{ParaHash, ParaHeadsProof, ParaId};
use parachains_relay::ParachainsPipeline;
use relay_substrate_client::{CallOf, HeaderIdOf};
use substrate_relay_helper::parachains::{
	SubmitParachainHeadsCallBuilder, SubstrateParachainsPipeline,
};

/// BridgeHub-to-BridgeHub parachain sync description.
#[derive(Clone, Debug)]
pub struct BridgeHubRococoToBridgeHubWococo;

impl ParachainsPipeline for BridgeHubRococoToBridgeHubWococo {
	type SourceChain = relay_rococo_client::Rococo;
	type TargetChain = relay_bridge_hub_wococo_client::BridgeHubWococo;
}

impl SubstrateParachainsPipeline for BridgeHubRococoToBridgeHubWococo {
	type SourceParachain = relay_bridge_hub_rococo_client::BridgeHubRococo;
	type SourceRelayChain = relay_rococo_client::Rococo;
	type TargetChain = relay_bridge_hub_wococo_client::BridgeHubWococo;

	type SubmitParachainHeadsCallBuilder = BridgeHubRococoToBridgeHubWococoCallBuilder;

	const SOURCE_PARACHAIN_PARA_ID: u32 = bp_bridge_hub_rococo::BRIDGE_HUB_ROCOCO_PARACHAIN_ID;
}

pub struct BridgeHubRococoToBridgeHubWococoCallBuilder;
impl SubmitParachainHeadsCallBuilder<BridgeHubRococoToBridgeHubWococo>
	for BridgeHubRococoToBridgeHubWococoCallBuilder
{
	fn build_submit_parachain_heads_call(
		at_relay_block: HeaderIdOf<relay_rococo_client::Rococo>,
		parachains: Vec<(ParaId, ParaHash)>,
		parachain_heads_proof: ParaHeadsProof,
	) -> CallOf<relay_bridge_hub_wococo_client::BridgeHubWococo> {
		relay_bridge_hub_wococo_client::runtime::Call::BridgeRococoParachain(
			relay_bridge_hub_wococo_client::runtime::BridgeParachainCall::submit_parachain_heads(
				(at_relay_block.0, at_relay_block.1),
				parachains,
				parachain_heads_proof,
			),
		)
	}
}

/// `BridgeHubParachain` to `BridgeHubParachain` bridge definition.
pub struct BridgeHubRococoToBridgeHubWococoCliBridge {}

impl ParachainToRelayHeadersCliBridge for BridgeHubRococoToBridgeHubWococoCliBridge {
	type SourceRelay = relay_rococo_client::Rococo;
	type ParachainFinality = BridgeHubRococoToBridgeHubWococo;
	type RelayFinality =
		crate::chains::rococo_headers_to_bridge_hub_wococo::RococoFinalityToBridgeHubWococo;
}

impl CliBridgeBase for BridgeHubRococoToBridgeHubWococoCliBridge {
	type Source = relay_bridge_hub_rococo_client::BridgeHubRococo;
	type Target = relay_bridge_hub_wococo_client::BridgeHubWococo;
}
