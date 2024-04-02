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

//! Rococo-to-RococoBulletin headers sync entrypoint.

use super::RococoAsPolkadot;

use async_trait::async_trait;
use substrate_relay_helper::{
	equivocation::SubstrateEquivocationDetectionPipeline,
	finality::SubstrateFinalitySyncPipeline,
	finality_base::{engine::Grandpa as GrandpaFinalityEngine, SubstrateFinalityPipeline},
};

use substrate_relay_helper::cli::bridge::{
	CliBridgeBase, RelayToRelayEquivocationDetectionCliBridge, RelayToRelayHeadersCliBridge,
};

/// Description of Rococo -> `RococoBulletin` finalized headers bridge.
#[derive(Clone, Debug)]
pub struct RococoFinalityToRococoBulletin;

substrate_relay_helper::generate_submit_finality_proof_call_builder!(
	RococoFinalityToRococoBulletin,
	SubmitFinalityProofCallBuilder,
	relay_polkadot_bulletin_client::RuntimeCall::BridgePolkadotGrandpa,
	relay_polkadot_bulletin_client::BridgePolkadotGrandpaCall::submit_finality_proof
);

substrate_relay_helper::generate_report_equivocation_call_builder!(
	RococoFinalityToRococoBulletin,
	ReportEquivocationCallBuilder,
	relay_rococo_client::RuntimeCall::Grandpa,
	relay_rococo_client::GrandpaCall::report_equivocation
);

#[async_trait]
impl SubstrateFinalityPipeline for RococoFinalityToRococoBulletin {
	type SourceChain = RococoAsPolkadot;
	type TargetChain = relay_polkadot_bulletin_client::PolkadotBulletin;

	type FinalityEngine = GrandpaFinalityEngine<Self::SourceChain>;
}

#[async_trait]
impl SubstrateFinalitySyncPipeline for RococoFinalityToRococoBulletin {
	type SubmitFinalityProofCallBuilder = SubmitFinalityProofCallBuilder;
}

#[async_trait]
impl SubstrateEquivocationDetectionPipeline for RococoFinalityToRococoBulletin {
	type ReportEquivocationCallBuilder = ReportEquivocationCallBuilder;
}

/// `Rococo` to BridgeHub `RococoBulletin` bridge definition.
pub struct RococoToRococoBulletinCliBridge {}

impl CliBridgeBase for RococoToRococoBulletinCliBridge {
	type Source = RococoAsPolkadot;
	type Target = relay_polkadot_bulletin_client::PolkadotBulletin;
}

impl RelayToRelayHeadersCliBridge for RococoToRococoBulletinCliBridge {
	type Finality = RococoFinalityToRococoBulletin;
}

impl RelayToRelayEquivocationDetectionCliBridge for RococoToRococoBulletinCliBridge {
	type Equivocation = RococoFinalityToRococoBulletin;
}
