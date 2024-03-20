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

//! RococoBulletin-to-BridgeHubRococo headers sync entrypoint.

use super::BridgeHubRococoAsBridgeHubPolkadot;

use async_trait::async_trait;
use substrate_relay_helper::{
	equivocation::SubstrateEquivocationDetectionPipeline,
	finality::SubstrateFinalitySyncPipeline,
	finality_base::{engine::Grandpa as GrandpaFinalityEngine, SubstrateFinalityPipeline},
};

use substrate_relay_helper::cli::bridge::{
	CliBridgeBase, MessagesCliBridge, RelayToRelayEquivocationDetectionCliBridge,
	RelayToRelayHeadersCliBridge,
};

/// Description of `RococoBulletin` -> `RococoBridgeHub` finalized headers bridge.
#[derive(Clone, Debug)]
pub struct RococoBulletinFinalityToBridgeHubRococo;

substrate_relay_helper::generate_submit_finality_proof_call_builder!(
	RococoBulletinFinalityToBridgeHubRococo,
	SubmitFinalityProofCallBuilder,
	relay_bridge_hub_rococo_client::RuntimeCall::BridgePolkadotBulletinGrandpa,
	relay_bridge_hub_rococo_client::BridgeBulletinGrandpaCall::submit_finality_proof
);

substrate_relay_helper::generate_report_equivocation_call_builder!(
	RococoBulletinFinalityToBridgeHubRococo,
	ReportEquivocationCallBuilder,
	relay_polkadot_bulletin_client::RuntimeCall::Grandpa,
	relay_polkadot_bulletin_client::GrandpaCall::report_equivocation
);

#[async_trait]
impl SubstrateFinalityPipeline for RococoBulletinFinalityToBridgeHubRococo {
	type SourceChain = relay_polkadot_bulletin_client::PolkadotBulletin;
	type TargetChain = BridgeHubRococoAsBridgeHubPolkadot;

	type FinalityEngine = GrandpaFinalityEngine<Self::SourceChain>;
}

#[async_trait]
impl SubstrateFinalitySyncPipeline for RococoBulletinFinalityToBridgeHubRococo {
	type SubmitFinalityProofCallBuilder = SubmitFinalityProofCallBuilder;
}

#[async_trait]
impl SubstrateEquivocationDetectionPipeline for RococoBulletinFinalityToBridgeHubRococo {
	type ReportEquivocationCallBuilder = ReportEquivocationCallBuilder;
}

/// `RococoBulletin` to BridgeHub `Rococo` bridge definition.
pub struct RococoBulletinToBridgeHubRococoCliBridge {}

impl CliBridgeBase for RococoBulletinToBridgeHubRococoCliBridge {
	type Source = relay_polkadot_bulletin_client::PolkadotBulletin;
	type Target = BridgeHubRococoAsBridgeHubPolkadot;
}

impl RelayToRelayHeadersCliBridge for RococoBulletinToBridgeHubRococoCliBridge {
	type Finality = RococoBulletinFinalityToBridgeHubRococo;
}

impl RelayToRelayEquivocationDetectionCliBridge for RococoBulletinToBridgeHubRococoCliBridge {
	type Equivocation = RococoBulletinFinalityToBridgeHubRococo;
}

impl MessagesCliBridge for RococoBulletinToBridgeHubRococoCliBridge {
	type MessagesLane = crate::bridges::rococo_bulletin::rococo_bulletin_messages_to_bridge_hub_rococo::RococoBulletinMessagesToBridgeHubRococoMessageLane;
}
