// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Parachain specific wrapper for the AuRa import queue.

use codec::Codec;
use cumulus_client_consensus_common::ParachainBlockImportMarker;
use sc_client_api::{backend::AuxStore, BlockOf, UsageProvider};
use sc_consensus::{import_queue::DefaultImportQueue, BlockImport};
use sc_consensus_aura::{AuraVerifier, CompatibilityMode};
use sc_consensus_slots::InherentDataProviderExt;
use sc_telemetry::TelemetryHandle;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_blockchain::HeaderBackend;
use sp_consensus::Error as ConsensusError;
use sp_consensus_aura::AuraApi;
use sp_core::crypto::Pair;
use sp_inherents::CreateInherentDataProviders;
use sp_runtime::traits::Block as BlockT;
use std::{fmt::Debug, sync::Arc};
use substrate_prometheus_endpoint::Registry;

/// Parameters for [`import_queue`].
pub struct ImportQueueParams<'a, I, C, CIDP, S> {
	/// The block import to use.
	pub block_import: I,
	/// The client to interact with the chain.
	pub client: Arc<C>,
	/// The inherent data providers, to create the inherent data.
	pub create_inherent_data_providers: CIDP,
	/// The spawner to spawn background tasks.
	pub spawner: &'a S,
	/// The prometheus registry.
	pub registry: Option<&'a Registry>,
	/// The telemetry handle.
	pub telemetry: Option<TelemetryHandle>,
}

/// Start an import queue for the Aura consensus algorithm.
pub fn import_queue<P, Block, I, C, S, CIDP>(
	ImportQueueParams {
		block_import,
		client,
		create_inherent_data_providers,
		spawner,
		registry,
		telemetry,
	}: ImportQueueParams<'_, I, C, CIDP, S>,
) -> Result<DefaultImportQueue<Block>, sp_consensus::Error>
where
	Block: BlockT,
	C::Api: BlockBuilderApi<Block> + AuraApi<Block, P::Public> + ApiExt<Block>,
	C: 'static
		+ ProvideRuntimeApi<Block>
		+ BlockOf
		+ Send
		+ Sync
		+ AuxStore
		+ UsageProvider<Block>
		+ HeaderBackend<Block>,
	I: BlockImport<Block, Error = ConsensusError>
		+ ParachainBlockImportMarker
		+ Send
		+ Sync
		+ 'static,
	P: Pair + 'static,
	P::Public: Debug + Codec,
	P::Signature: Codec,
	S: sp_core::traits::SpawnEssentialNamed,
	CIDP: CreateInherentDataProviders<Block, ()> + Sync + Send + 'static,
	CIDP::InherentDataProviders: InherentDataProviderExt + Send + Sync,
{
	sc_consensus_aura::import_queue::<P, _, _, _, _, _>(sc_consensus_aura::ImportQueueParams {
		block_import,
		justification_import: None,
		client,
		create_inherent_data_providers,
		spawner,
		registry,
		check_for_equivocation: sc_consensus_aura::CheckForEquivocation::No,
		telemetry,
		compatibility_mode: CompatibilityMode::None,
	})
}

/// Parameters of [`build_verifier`].
pub struct BuildVerifierParams<C, CIDP> {
	/// The client to interact with the chain.
	pub client: Arc<C>,
	/// The inherent data providers, to create the inherent data.
	pub create_inherent_data_providers: CIDP,
	/// The telemetry handle.
	pub telemetry: Option<TelemetryHandle>,
}

/// Build the [`AuraVerifier`].
pub fn build_verifier<P, C, CIDP, N>(
	BuildVerifierParams { client, create_inherent_data_providers, telemetry }: BuildVerifierParams<
		C,
		CIDP,
	>,
) -> AuraVerifier<C, P, CIDP, N> {
	sc_consensus_aura::build_verifier(sc_consensus_aura::BuildVerifierParams {
		client,
		create_inherent_data_providers,
		telemetry,
		check_for_equivocation: sc_consensus_aura::CheckForEquivocation::No,
		compatibility_mode: CompatibilityMode::None,
	})
}
