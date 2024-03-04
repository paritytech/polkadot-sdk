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

//! Polkadot-to-KusamaBridgeHub headers sync entrypoint.

use crate::cli::bridge::{
	CliBridgeBase, RelayToRelayEquivocationDetectionCliBridge, RelayToRelayHeadersCliBridge,
};

use async_trait::async_trait;
use substrate_relay_helper::{
	equivocation::SubstrateEquivocationDetectionPipeline,
	finality::SubstrateFinalitySyncPipeline,
	finality_base::{engine::Grandpa as GrandpaFinalityEngine, SubstrateFinalityPipeline},
};

/// Description of Polkadot -> KusamaBridgeHub finalized headers bridge.
#[derive(Clone, Debug)]
pub struct PolkadotFinalityToBridgeHubKusama;

substrate_relay_helper::generate_submit_finality_proof_call_builder!(
	PolkadotFinalityToBridgeHubKusama,
	SubmitFinalityProofCallBuilder,
	relay_bridge_hub_kusama_client::RuntimeCall::BridgePolkadotGrandpa,
	relay_bridge_hub_kusama_client::BridgeGrandpaCall::submit_finality_proof
);

substrate_relay_helper::generate_report_equivocation_call_builder!(
	PolkadotFinalityToBridgeHubKusama,
	ReportEquivocationCallBuilder,
	relay_polkadot_client::RuntimeCall::Grandpa,
	relay_polkadot_client::GrandpaCall::report_equivocation
);

#[async_trait]
impl SubstrateFinalityPipeline for PolkadotFinalityToBridgeHubKusama {
	type SourceChain = relay_polkadot_client::Polkadot;
	type TargetChain = relay_bridge_hub_kusama_client::BridgeHubKusama;

	type FinalityEngine = GrandpaFinalityEngine<Self::SourceChain>;
}

#[async_trait]
impl SubstrateFinalitySyncPipeline for PolkadotFinalityToBridgeHubKusama {
	type SubmitFinalityProofCallBuilder = SubmitFinalityProofCallBuilder;
}

#[async_trait]
impl SubstrateEquivocationDetectionPipeline for PolkadotFinalityToBridgeHubKusama {
	type ReportEquivocationCallBuilder = ReportEquivocationCallBuilder;
}

/// `Polkadot` to BridgeHub `Kusama` bridge definition.
pub struct PolkadotToBridgeHubKusamaCliBridge {}

impl CliBridgeBase for PolkadotToBridgeHubKusamaCliBridge {
	type Source = relay_polkadot_client::Polkadot;
	type Target = relay_bridge_hub_kusama_client::BridgeHubKusama;
}

impl RelayToRelayHeadersCliBridge for PolkadotToBridgeHubKusamaCliBridge {
	type Finality = PolkadotFinalityToBridgeHubKusama;
}

impl RelayToRelayEquivocationDetectionCliBridge for PolkadotToBridgeHubKusamaCliBridge {
	type Equivocation = PolkadotFinalityToBridgeHubKusama;
}
