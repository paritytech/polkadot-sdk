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
		parachain::{
			rpc::BuildEmptyRpcExtensions, ParachainBackend, ParachainBlockImport, ParachainClient,
			ParachainNodeSpec, StartConsensus,
		},
		BuildImportQueue,
	},
	fake_runtime_api::aura::RuntimeApi as FakeRuntimeApi,
};
#[allow(deprecated)]
use cumulus_client_service::old_consensus;
use cumulus_client_service::CollatorSybilResistance;
use cumulus_primitives_core::ParaId;
use cumulus_relay_chain_interface::{OverseerHandle, RelayChainInterface};
pub use parachains_common::{Block, Hash};
use polkadot_primitives::CollatorPair;
use prometheus_endpoint::Registry;
use sc_consensus::DefaultImportQueue;
use sc_service::{Configuration, Error, TaskManager};
use sc_telemetry::TelemetryHandle;
use sc_transaction_pool::FullPool;
use sp_keystore::KeystorePtr;
use std::{marker::PhantomData, sync::Arc, time::Duration};

/// Build the import queue for the shell runtime.
pub(crate) struct BuildShellImportQueue<RuntimeApi>(PhantomData<RuntimeApi>);

impl BuildImportQueue<Block, ParachainClient<FakeRuntimeApi>>
	for BuildShellImportQueue<FakeRuntimeApi>
{
	type BlockImport = ParachainBlockImport<FakeRuntimeApi>;

	fn build_import_queue(
		client: Arc<ParachainClient<FakeRuntimeApi>>,
		backend: Arc<ParachainBackend>,
		config: &Configuration,
		_telemetry_handle: Option<TelemetryHandle>,
		task_manager: &TaskManager,
	) -> sc_service::error::Result<(Self::BlockImport, DefaultImportQueue<Block>)> {
		let block_import = ParachainBlockImport::new(client.clone(), backend);
		Ok((
			block_import.clone(),
			cumulus_client_consensus_relay_chain::import_queue(
				client,
				Box::new(block_import),
				|_, _| async { Ok(()) },
				&task_manager.spawn_essential_handle(),
				config.prometheus_registry(),
			)
			.map_err(Into::<Error>::into)?,
		))
	}
}

/// Start relay-chain consensus that is free for all. Everyone can submit a block, the relay-chain
/// decides what is backed and included.
pub(crate) struct StartRelayChainConsensus;

impl StartConsensus<FakeRuntimeApi> for StartRelayChainConsensus {
	fn start_consensus(
		client: Arc<ParachainClient<FakeRuntimeApi>>,
		_backend: Arc<ParachainBackend>,
		block_import: ParachainBlockImport<FakeRuntimeApi>,
		prometheus_registry: Option<&Registry>,
		telemetry: Option<TelemetryHandle>,
		task_manager: &TaskManager,
		relay_chain_interface: Arc<dyn RelayChainInterface>,
		transaction_pool: Arc<FullPool<Block, ParachainClient<FakeRuntimeApi>>>,
		_keystore: KeystorePtr,
		_relay_chain_slot_duration: Duration,
		para_id: ParaId,
		collator_key: CollatorPair,
		overseer_handle: OverseerHandle,
		announce_block: Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
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

impl ParachainNodeSpec for ShellNode {
	type RuntimeApi = FakeRuntimeApi;
	type BuildImportQueue = BuildShellImportQueue<Self::RuntimeApi>;
	type BuildRpcExtensions = BuildEmptyRpcExtensions<Self::RuntimeApi>;
	type StartConsensus = StartRelayChainConsensus;

	const SYBIL_RESISTANCE: CollatorSybilResistance = CollatorSybilResistance::Unresistant;
}
