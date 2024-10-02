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

use crate::{
	common::{
		rpc::BuildEmptyRpcExtensions,
		spec::{BaseNodeSpec, BuildImportQueue, NodeSpec, StartConsensus},
		types::{Block, Hash, ParachainBackend, ParachainBlockImport, ParachainClient},
		NodeExtraArgs,
	},
	fake_runtime_api::aura_sr25519::RuntimeApi as FakeRuntimeApi,
};
#[docify::export(slot_based_colator_import)]
#[allow(deprecated)]
use cumulus_client_service::old_consensus;
use cumulus_client_service::CollatorSybilResistance;
use cumulus_primitives_core::ParaId;
use cumulus_relay_chain_interface::{OverseerHandle, RelayChainInterface};
use polkadot_primitives::CollatorPair;
use prometheus_endpoint::Registry;
use sc_consensus::DefaultImportQueue;
use sc_service::{Configuration, Error, TaskManager};
use sc_telemetry::TelemetryHandle;
use sc_transaction_pool::FullPool;
use sp_keystore::KeystorePtr;
use std::{sync::Arc, time::Duration};

/// Build the import queue for the shell runtime.
pub(crate) struct BuildShellImportQueue;

impl BuildImportQueue<Block<u32>, FakeRuntimeApi> for BuildShellImportQueue {
	fn build_import_queue(
		client: Arc<ParachainClient<Block<u32>, FakeRuntimeApi>>,
		block_import: ParachainBlockImport<Block<u32>, FakeRuntimeApi>,
		config: &Configuration,
		_telemetry_handle: Option<TelemetryHandle>,
		task_manager: &TaskManager,
	) -> sc_service::error::Result<DefaultImportQueue<Block<u32>>> {
		cumulus_client_consensus_relay_chain::import_queue(
			client,
			block_import,
			|_, _| async { Ok(()) },
			&task_manager.spawn_essential_handle(),
			config.prometheus_registry(),
		)
		.map_err(Into::into)
	}
}

/// Start relay-chain consensus that is free for all. Everyone can submit a block, the relay-chain
/// decides what is backed and included.
pub(crate) struct StartRelayChainConsensus;

impl StartConsensus<Block<u32>, FakeRuntimeApi> for StartRelayChainConsensus {
	fn start_consensus(
		client: Arc<ParachainClient<Block<u32>, FakeRuntimeApi>>,
		block_import: ParachainBlockImport<Block<u32>, FakeRuntimeApi>,
		prometheus_registry: Option<&Registry>,
		telemetry: Option<TelemetryHandle>,
		task_manager: &TaskManager,
		relay_chain_interface: Arc<dyn RelayChainInterface>,
		transaction_pool: Arc<FullPool<Block<u32>, ParachainClient<Block<u32>, FakeRuntimeApi>>>,
		_keystore: KeystorePtr,
		_relay_chain_slot_duration: Duration,
		para_id: ParaId,
		collator_key: CollatorPair,
		overseer_handle: OverseerHandle,
		announce_block: Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
		_backend: Arc<ParachainBackend<Block<u32>>>,
		_node_extra_args: NodeExtraArgs,
	) -> Result<(), Error> {
		let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool,
			prometheus_registry,
			telemetry,
		);

		let free_for_all = cumulus_client_consensus_relay_chain::build_relay_chain_consensus(
			cumulus_client_consensus_relay_chain::BuildRelayChainConsensusParams {
				para_id,
				proposer_factory,
				block_import,
				relay_chain_interface: relay_chain_interface.clone(),
				create_inherent_data_providers: move |_, (relay_parent, validation_data)| {
					let relay_chain_interface = relay_chain_interface.clone();
					async move {
						let parachain_inherent =
                            cumulus_client_parachain_inherent::ParachainInherentDataProvider::create_at(
                                relay_parent,
                                &relay_chain_interface,
                                &validation_data,
                                para_id,
                            ).await;
						let parachain_inherent = parachain_inherent.ok_or_else(|| {
							Box::<dyn std::error::Error + Send + Sync>::from(
								"Failed to create parachain inherent",
							)
						})?;
						Ok(parachain_inherent)
					}
				},
			},
		);

		let spawner = task_manager.spawn_handle();

		// Required for free-for-all consensus
		#[allow(deprecated)]
		old_consensus::start_collator_sync(old_consensus::StartCollatorParams {
			para_id,
			block_status: client.clone(),
			announce_block,
			overseer_handle,
			spawner,
			key: collator_key,
			parachain_consensus: free_for_all,
			runtime_api: client.clone(),
		});

		Ok(())
	}
}

pub(crate) struct ShellNode;

impl BaseNodeSpec for ShellNode {
	type Block = Block<u32>;
	type RuntimeApi = FakeRuntimeApi;
	type BuildImportQueue = BuildShellImportQueue;
}

impl NodeSpec for ShellNode {
	type BuildRpcExtensions = BuildEmptyRpcExtensions<Block<u32>, Self::RuntimeApi>;
	type StartConsensus = StartRelayChainConsensus;

	const SYBIL_RESISTANCE: CollatorSybilResistance = CollatorSybilResistance::Unresistant;
}
