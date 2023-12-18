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

//! Kusama-to-BridgeHubPolkadot parachains sync entrypoint.

use crate::cli::bridge::{CliBridgeBase, MessagesCliBridge, ParachainToRelayHeadersCliBridge};
use bp_polkadot_core::parachains::{ParaHash, ParaHeadsProof, ParaId};
use relay_substrate_client::{CallOf, HeaderIdOf};
use substrate_relay_helper::parachains::{
	SubmitParachainHeadsCallBuilder, SubstrateParachainsPipeline,
};

/// Kusama-to-BridgeHubPolkadot parachain sync description.
#[derive(Clone, Debug)]
pub struct BridgeHubKusamaToBridgeHubPolkadot;

impl SubstrateParachainsPipeline for BridgeHubKusamaToBridgeHubPolkadot {
	type SourceParachain = relay_bridge_hub_kusama_client::BridgeHubKusama;
	type SourceRelayChain = relay_kusama_client::Kusama;
	type TargetChain = relay_bridge_hub_polkadot_client::BridgeHubPolkadot;

	type SubmitParachainHeadsCallBuilder = BridgeHubKusamaToBridgeHubPolkadotCallBuilder;
}

pub struct BridgeHubKusamaToBridgeHubPolkadotCallBuilder;
impl SubmitParachainHeadsCallBuilder<BridgeHubKusamaToBridgeHubPolkadot>
	for BridgeHubKusamaToBridgeHubPolkadotCallBuilder
{
	fn build_submit_parachain_heads_call(
		at_relay_block: HeaderIdOf<relay_kusama_client::Kusama>,
		parachains: Vec<(ParaId, ParaHash)>,
		parachain_heads_proof: ParaHeadsProof,
	) -> CallOf<relay_bridge_hub_polkadot_client::BridgeHubPolkadot> {
		relay_bridge_hub_polkadot_client::runtime::Call::BridgeKusamaParachain(
			relay_bridge_hub_polkadot_client::runtime::BridgeParachainCall::submit_parachain_heads {
				at_relay_block: (at_relay_block.0, at_relay_block.1),
				parachains,
				parachain_heads_proof,
			},
		)
	}
}

/// Kusama-to-BridgeHubPolkadot parachain sync description for the CLI.
pub struct BridgeHubKusamaToBridgeHubPolkadotCliBridge {}

impl ParachainToRelayHeadersCliBridge for BridgeHubKusamaToBridgeHubPolkadotCliBridge {
	type SourceRelay = relay_kusama_client::Kusama;
	type ParachainFinality = BridgeHubKusamaToBridgeHubPolkadot;
	type RelayFinality =
		crate::bridges::kusama_polkadot::kusama_headers_to_bridge_hub_polkadot::KusamaFinalityToBridgeHubPolkadot;
}

impl CliBridgeBase for BridgeHubKusamaToBridgeHubPolkadotCliBridge {
	type Source = relay_bridge_hub_kusama_client::BridgeHubKusama;
	type Target = relay_bridge_hub_polkadot_client::BridgeHubPolkadot;
}

impl MessagesCliBridge for BridgeHubKusamaToBridgeHubPolkadotCliBridge {
	type MessagesLane =
	crate::bridges::kusama_polkadot::bridge_hub_kusama_messages_to_bridge_hub_polkadot::BridgeHubKusamaMessagesToBridgeHubPolkadotMessageLane;
}
