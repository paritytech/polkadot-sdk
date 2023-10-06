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

//! Polkadot-to-BridgeHubKusama parachains sync entrypoint.

use crate::cli::bridge::{CliBridgeBase, MessagesCliBridge, ParachainToRelayHeadersCliBridge};
use bp_polkadot_core::parachains::{ParaHash, ParaHeadsProof, ParaId};
use relay_substrate_client::{CallOf, HeaderIdOf};
use substrate_relay_helper::parachains::{
	SubmitParachainHeadsCallBuilder, SubstrateParachainsPipeline,
};

/// Polkadot-to-BridgeHubKusama parachain sync description.
#[derive(Clone, Debug)]
pub struct BridgeHubPolkadotToBridgeHubKusama;

impl SubstrateParachainsPipeline for BridgeHubPolkadotToBridgeHubKusama {
	type SourceParachain = relay_bridge_hub_polkadot_client::BridgeHubPolkadot;
	type SourceRelayChain = relay_polkadot_client::Polkadot;
	type TargetChain = relay_bridge_hub_kusama_client::BridgeHubKusama;

	type SubmitParachainHeadsCallBuilder = BridgeHubPolkadotToBridgeHubKusamaCallBuilder;
}

pub struct BridgeHubPolkadotToBridgeHubKusamaCallBuilder;
impl SubmitParachainHeadsCallBuilder<BridgeHubPolkadotToBridgeHubKusama>
	for BridgeHubPolkadotToBridgeHubKusamaCallBuilder
{
	fn build_submit_parachain_heads_call(
		at_relay_block: HeaderIdOf<relay_polkadot_client::Polkadot>,
		parachains: Vec<(ParaId, ParaHash)>,
		parachain_heads_proof: ParaHeadsProof,
	) -> CallOf<relay_bridge_hub_kusama_client::BridgeHubKusama> {
		relay_bridge_hub_kusama_client::runtime::Call::BridgePolkadotParachain(
			bp_parachains::BridgeParachainCall::submit_parachain_heads {
				at_relay_block: (at_relay_block.0, at_relay_block.1),
				parachains,
				parachain_heads_proof,
			},
		)
	}
}

/// Polkadot-to-BridgeHubKusama parachain sync description for the CLI.
pub struct BridgeHubPolkadotToBridgeHubKusamaCliBridge {}

impl ParachainToRelayHeadersCliBridge for BridgeHubPolkadotToBridgeHubKusamaCliBridge {
	type SourceRelay = relay_polkadot_client::Polkadot;
	type ParachainFinality = BridgeHubPolkadotToBridgeHubKusama;
	type RelayFinality =
		crate::bridges::kusama_polkadot::polkadot_headers_to_bridge_hub_kusama::PolkadotFinalityToBridgeHubKusama;
}

impl CliBridgeBase for BridgeHubPolkadotToBridgeHubKusamaCliBridge {
	type Source = relay_bridge_hub_polkadot_client::BridgeHubPolkadot;
	type Target = relay_bridge_hub_kusama_client::BridgeHubKusama;
}

impl MessagesCliBridge for BridgeHubPolkadotToBridgeHubKusamaCliBridge {
	type MessagesLane =
	crate::bridges::kusama_polkadot::bridge_hub_polkadot_messages_to_bridge_hub_kusama::BridgeHubPolkadotMessagesToBridgeHubKusamaMessageLane;
}
