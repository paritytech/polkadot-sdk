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

//! Kusama-to-BridgeHubPolkadot headers sync entrypoint.

use crate::cli::bridge::{
	CliBridgeBase, RelayToRelayEquivocationDetectionCliBridge, RelayToRelayHeadersCliBridge,
};

use async_trait::async_trait;
use substrate_relay_helper::{
	equivocation::SubstrateEquivocationDetectionPipeline,
	finality::SubstrateFinalitySyncPipeline,
	finality_base::{engine::Grandpa as GrandpaFinalityEngine, SubstrateFinalityPipeline},
};

/// Description of Kusama -> PolkadotBridgeHub finalized headers bridge.
#[derive(Clone, Debug)]
pub struct KusamaFinalityToBridgeHubPolkadot;

substrate_relay_helper::generate_submit_finality_proof_call_builder!(
	KusamaFinalityToBridgeHubPolkadot,
	SubmitFinalityProofCallBuilder,
	relay_bridge_hub_polkadot_client::RuntimeCall::BridgeKusamaGrandpa,
	relay_bridge_hub_polkadot_client::BridgeKusamaGrandpaCall::submit_finality_proof
);

substrate_relay_helper::generate_report_equivocation_call_builder!(
	KusamaFinalityToBridgeHubPolkadot,
	ReportEquivocationCallBuilder,
	relay_kusama_client::RuntimeCall::Grandpa,
	relay_kusama_client::GrandpaCall::report_equivocation
);

#[async_trait]
impl SubstrateFinalityPipeline for KusamaFinalityToBridgeHubPolkadot {
	type SourceChain = relay_kusama_client::Kusama;
	type TargetChain = relay_bridge_hub_polkadot_client::BridgeHubPolkadot;

	type FinalityEngine = GrandpaFinalityEngine<Self::SourceChain>;
}

#[async_trait]
impl SubstrateFinalitySyncPipeline for KusamaFinalityToBridgeHubPolkadot {
	type SubmitFinalityProofCallBuilder = SubmitFinalityProofCallBuilder;
}

#[async_trait]
impl SubstrateEquivocationDetectionPipeline for KusamaFinalityToBridgeHubPolkadot {
	type ReportEquivocationCallBuilder = ReportEquivocationCallBuilder;
}

/// `Kusama` to BridgeHub `Polkadot` bridge definition.
pub struct KusamaToBridgeHubPolkadotCliBridge {}

impl CliBridgeBase for KusamaToBridgeHubPolkadotCliBridge {
	type Source = relay_kusama_client::Kusama;
	type Target = relay_bridge_hub_polkadot_client::BridgeHubPolkadot;
}

impl RelayToRelayHeadersCliBridge for KusamaToBridgeHubPolkadotCliBridge {
	type Finality = KusamaFinalityToBridgeHubPolkadot;
}

impl RelayToRelayEquivocationDetectionCliBridge for KusamaToBridgeHubPolkadotCliBridge {
	type Equivocation = KusamaFinalityToBridgeHubPolkadot;
}
