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

//! Westend-to-Rococo bridge hubs headers sync entrypoint.

use crate::cli::bridge::{
	CliBridgeBase, RelayToRelayEquivocationDetectionCliBridge, RelayToRelayHeadersCliBridge,
};

use async_trait::async_trait;
use substrate_relay_helper::{
	equivocation::SubstrateEquivocationDetectionPipeline,
	finality::SubstrateFinalitySyncPipeline,
	finality_base::{engine::Grandpa as GrandpaFinalityEngine, SubstrateFinalityPipeline},
};

/// Description of Westend -> Rococo finalized headers bridge.
#[derive(Clone, Debug)]
pub struct WestendFinalityToBridgeHubRococo;

substrate_relay_helper::generate_submit_finality_proof_ex_call_builder!(
	WestendFinalityToBridgeHubRococo,
	SubmitFinalityProofCallBuilder,
	relay_bridge_hub_rococo_client::RuntimeCall::BridgeWestendGrandpa,
	relay_bridge_hub_rococo_client::BridgeGrandpaCall::submit_finality_proof_ex
);

substrate_relay_helper::generate_report_equivocation_call_builder!(
	WestendFinalityToBridgeHubRococo,
	ReportEquivocationCallBuilder,
	relay_westend_client::RuntimeCall::Grandpa,
	relay_westend_client::GrandpaCall::report_equivocation
);

#[async_trait]
impl SubstrateFinalityPipeline for WestendFinalityToBridgeHubRococo {
	type SourceChain = relay_westend_client::Westend;
	type TargetChain = relay_bridge_hub_rococo_client::BridgeHubRococo;

	type FinalityEngine = GrandpaFinalityEngine<Self::SourceChain>;
}

#[async_trait]
impl SubstrateFinalitySyncPipeline for WestendFinalityToBridgeHubRococo {
	type SubmitFinalityProofCallBuilder = SubmitFinalityProofCallBuilder;
}

#[async_trait]
impl SubstrateEquivocationDetectionPipeline for WestendFinalityToBridgeHubRococo {
	type ReportEquivocationCallBuilder = ReportEquivocationCallBuilder;
}

/// `Westend` to BridgeHub `Rococo` bridge definition.
pub struct WestendToBridgeHubRococoCliBridge {}

impl CliBridgeBase for WestendToBridgeHubRococoCliBridge {
	type Source = relay_westend_client::Westend;
	type Target = relay_bridge_hub_rococo_client::BridgeHubRococo;
}

impl RelayToRelayHeadersCliBridge for WestendToBridgeHubRococoCliBridge {
	type Finality = WestendFinalityToBridgeHubRococo;
}

impl RelayToRelayEquivocationDetectionCliBridge for WestendToBridgeHubRococoCliBridge {
	type Equivocation = WestendFinalityToBridgeHubRococo;
}
