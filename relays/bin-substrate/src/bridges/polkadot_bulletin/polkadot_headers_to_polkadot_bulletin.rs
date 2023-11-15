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

//! Polkadot-to-PolkadotBulletin headers sync entrypoint.

use crate::cli::bridge::{
	CliBridgeBase, RelayToRelayEquivocationDetectionCliBridge, RelayToRelayHeadersCliBridge,
};

use async_trait::async_trait;
use substrate_relay_helper::{
	equivocation::SubstrateEquivocationDetectionPipeline,
	finality::SubstrateFinalitySyncPipeline,
	finality_base::{engine::Grandpa as GrandpaFinalityEngine, SubstrateFinalityPipeline},
};

/// Description of Polkadot -> `PolkadotBulletin` finalized headers bridge.
#[derive(Clone, Debug)]
pub struct PolkadotFinalityToPolkadotBulletin;

substrate_relay_helper::generate_submit_finality_proof_call_builder!(
	PolkadotFinalityToPolkadotBulletin,
	SubmitFinalityProofCallBuilder,
	relay_polkadot_bulletin_client::RuntimeCall::BridgePolkadotGrandpa,
	relay_polkadot_bulletin_client::BridgePolkadotGrandpaCall::submit_finality_proof
);

substrate_relay_helper::generate_report_equivocation_call_builder!(
	PolkadotFinalityToPolkadotBulletin,
	ReportEquivocationCallBuilder,
	relay_polkadot_client::RuntimeCall::Grandpa,
	relay_polkadot_client::GrandpaCall::report_equivocation
);

#[async_trait]
impl SubstrateFinalityPipeline for PolkadotFinalityToPolkadotBulletin {
	type SourceChain = relay_polkadot_client::Polkadot;
	type TargetChain = relay_polkadot_bulletin_client::PolkadotBulletin;

	type FinalityEngine = GrandpaFinalityEngine<Self::SourceChain>;
}

#[async_trait]
impl SubstrateFinalitySyncPipeline for PolkadotFinalityToPolkadotBulletin {
	type SubmitFinalityProofCallBuilder = SubmitFinalityProofCallBuilder;
}

#[async_trait]
impl SubstrateEquivocationDetectionPipeline for PolkadotFinalityToPolkadotBulletin {
	type ReportEquivocationCallBuilder = ReportEquivocationCallBuilder;
}

/// `Polkadot` to BridgeHub `PolkadotBulletin` bridge definition.
pub struct PolkadotToPolkadotBulletinCliBridge {}

impl CliBridgeBase for PolkadotToPolkadotBulletinCliBridge {
	type Source = relay_polkadot_client::Polkadot;
	type Target = relay_polkadot_bulletin_client::PolkadotBulletin;
}

impl RelayToRelayHeadersCliBridge for PolkadotToPolkadotBulletinCliBridge {
	type Finality = PolkadotFinalityToPolkadotBulletin;
}

impl RelayToRelayEquivocationDetectionCliBridge for PolkadotToPolkadotBulletinCliBridge {
	type Equivocation = PolkadotFinalityToPolkadotBulletin;
}
