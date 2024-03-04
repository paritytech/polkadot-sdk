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

//! PolkadotBulletin-to-BridgeHubPolkadot headers sync entrypoint.

use crate::cli::bridge::{
	CliBridgeBase, MessagesCliBridge, RelayToRelayEquivocationDetectionCliBridge,
	RelayToRelayHeadersCliBridge,
};

use async_trait::async_trait;
use substrate_relay_helper::{
	equivocation::SubstrateEquivocationDetectionPipeline,
	finality::SubstrateFinalitySyncPipeline,
	finality_base::{engine::Grandpa as GrandpaFinalityEngine, SubstrateFinalityPipeline},
};

/// Description of `PolkadotBulletin` -> `PolkadotBridgeHub` finalized headers bridge.
#[derive(Clone, Debug)]
pub struct PolkadotBulletinFinalityToBridgeHubPolkadot;

substrate_relay_helper::generate_submit_finality_proof_call_builder!(
	PolkadotBulletinFinalityToBridgeHubPolkadot,
	SubmitFinalityProofCallBuilder,
	// TODO: https://github.com/paritytech/parity-bridges-common/issues/2547 - use BridgePolkadotBulletinGrandpa
	relay_bridge_hub_polkadot_client::RuntimeCall::BridgeKusamaGrandpa,
	relay_bridge_hub_polkadot_client::BridgePolkadotBulletinGrandpaCall::submit_finality_proof
);

substrate_relay_helper::generate_report_equivocation_call_builder!(
	PolkadotBulletinFinalityToBridgeHubPolkadot,
	ReportEquivocationCallBuilder,
	relay_polkadot_bulletin_client::RuntimeCall::Grandpa,
	relay_polkadot_bulletin_client::GrandpaCall::report_equivocation
);

#[async_trait]
impl SubstrateFinalityPipeline for PolkadotBulletinFinalityToBridgeHubPolkadot {
	type SourceChain = relay_polkadot_bulletin_client::PolkadotBulletin;
	type TargetChain = relay_bridge_hub_polkadot_client::BridgeHubPolkadot;

	type FinalityEngine = GrandpaFinalityEngine<Self::SourceChain>;
}

#[async_trait]
impl SubstrateFinalitySyncPipeline for PolkadotBulletinFinalityToBridgeHubPolkadot {
	type SubmitFinalityProofCallBuilder = SubmitFinalityProofCallBuilder;
}

#[async_trait]
impl SubstrateEquivocationDetectionPipeline for PolkadotBulletinFinalityToBridgeHubPolkadot {
	type ReportEquivocationCallBuilder = ReportEquivocationCallBuilder;
}

/// `PolkadotBulletin` to BridgeHub `Polkadot` bridge definition.
pub struct PolkadotBulletinToBridgeHubPolkadotCliBridge {}

impl CliBridgeBase for PolkadotBulletinToBridgeHubPolkadotCliBridge {
	type Source = relay_polkadot_bulletin_client::PolkadotBulletin;
	type Target = relay_bridge_hub_polkadot_client::BridgeHubPolkadot;
}

impl RelayToRelayHeadersCliBridge for PolkadotBulletinToBridgeHubPolkadotCliBridge {
	type Finality = PolkadotBulletinFinalityToBridgeHubPolkadot;
}

impl RelayToRelayEquivocationDetectionCliBridge for PolkadotBulletinToBridgeHubPolkadotCliBridge {
	type Equivocation = PolkadotBulletinFinalityToBridgeHubPolkadot;
}

impl MessagesCliBridge for PolkadotBulletinToBridgeHubPolkadotCliBridge {
	type MessagesLane = crate::bridges::polkadot_bulletin::polkadot_bulletin_messages_to_bridge_hub_polkadot::PolkadotBulletinMessagesToBridgeHubPolkadotMessageLane;
}
