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

//! Wococo-to-Rococo bridge hubs headers sync entrypoint.

use crate::cli::bridge::{CliBridgeBase, RelayToRelayHeadersCliBridge};

use async_trait::async_trait;
use relay_substrate_client::{AccountKeyPairOf, Client};
use substrate_relay_helper::{
	finality::{engine::Grandpa as GrandpaFinalityEngine, SubstrateFinalitySyncPipeline},
	TransactionParams,
};

/// Description of Wococo -> Rococo finalized headers bridge.
#[derive(Clone, Debug)]
pub struct WococoFinalityToBridgeHubRococo;

substrate_relay_helper::generate_submit_finality_proof_call_builder!(
	WococoFinalityToBridgeHubRococo,
	WococoFinalityToBridgeHubRococoCallBuilder,
	relay_bridge_hub_rococo_client::RuntimeCall::BridgeWococoGrandpa,
	relay_bridge_hub_rococo_client::BridgeGrandpaCall::submit_finality_proof
);

#[async_trait]
impl SubstrateFinalitySyncPipeline for WococoFinalityToBridgeHubRococo {
	type SourceChain = relay_wococo_client::Wococo;
	type TargetChain = relay_bridge_hub_rococo_client::BridgeHubRococo;

	type FinalityEngine = GrandpaFinalityEngine<Self::SourceChain>;
	type SubmitFinalityProofCallBuilder = WococoFinalityToBridgeHubRococoCallBuilder;

	async fn start_relay_guards(
		target_client: &Client<Self::TargetChain>,
		_transaction_params: &TransactionParams<AccountKeyPairOf<Self::TargetChain>>,
		enable_version_guard: bool,
	) -> relay_substrate_client::Result<()> {
		if enable_version_guard {
			relay_substrate_client::guard::abort_on_spec_version_change(
				target_client.clone(),
				target_client.simple_runtime_version().await?.spec_version,
			);
		}
		Ok(())
	}
}

/// `Wococo` to BridgeHub `Rococo` bridge definition.
pub struct WococoToBridgeHubRococoCliBridge {}

impl CliBridgeBase for WococoToBridgeHubRococoCliBridge {
	type Source = relay_wococo_client::Wococo;
	type Target = relay_bridge_hub_rococo_client::BridgeHubRococo;
}

impl RelayToRelayHeadersCliBridge for WococoToBridgeHubRococoCliBridge {
	type Finality = WococoFinalityToBridgeHubRococo;
}
