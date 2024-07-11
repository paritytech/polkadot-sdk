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

use crate::common::{BuildImportQueue, BuildRpcExtensions, ConstructNodeRuntimeApi};
use cumulus_client_cli::CollatorOptions;
use cumulus_client_consensus_common::ParachainBlockImport as TParachainBlockImport;
use cumulus_client_service::{
	build_network, build_relay_chain_interface, prepare_node_config, start_relay_chain_tasks,
	BuildNetworkParams, CollatorSybilResistance, DARecoveryProfile, StartRelayChainTasksParams,
};
use cumulus_primitives_core::ParaId;
use cumulus_relay_chain_interface::{OverseerHandle, RelayChainInterface};
use futures::prelude::*;
use parachains_common::{Block, Hash};
use polkadot_primitives::CollatorPair;
use prometheus_endpoint::Registry;
use sc_consensus::{DefaultImportQueue, ImportQueue};
use sc_executor::WasmExecutor;
use sc_network::{config::FullNetworkConfiguration, service::traits::NetworkBackend, NetworkBlock};
use sc_service::{Configuration, PartialComponents, TFullBackend, TFullClient, TaskManager};
use sc_telemetry::{Telemetry, TelemetryHandle, TelemetryWorker, TelemetryWorkerHandle};
use sc_transaction_pool::FullPool;
use sp_keystore::KeystorePtr;
use std::{pin::Pin, sync::Arc, time::Duration};

pub mod aura;

#[cfg(not(feature = "runtime-benchmarks"))]
pub type HostFunctions = cumulus_client_service::ParachainHostFunctions;

#[cfg(feature = "runtime-benchmarks")]
pub type HostFunctions = (
	cumulus_client_service::ParachainHostFunctions,
	frame_benchmarking::benchmarking::HostFunctions,
);

pub type ParachainClient<RuntimeApi> = TFullClient<Block, RuntimeApi, WasmExecutor<HostFunctions>>;

pub type ParachainBackend = TFullBackend<Block>;

pub type ParachainBlockImport<RuntimeApi> =
	TParachainBlockImport<Block, Arc<ParachainClient<RuntimeApi>>, ParachainBackend>;

/// Assembly of PartialComponents (enough to run chain ops subcommands)
pub type Service<RuntimeApi> = PartialComponents<
	ParachainClient<RuntimeApi>,
	ParachainBackend,
	(),
	DefaultImportQueue<Block>,
	FullPool<Block, ParachainClient<RuntimeApi>>,
	(ParachainBlockImport<RuntimeApi>, Option<Telemetry>, Option<TelemetryWorkerHandle>),
>;

/// Checks that the hardware meets the requirements and print a warning otherwise.
fn warn_if_slow_hardware(hwbench: &sc_sysinfo::HwBench) {
	// Polkadot para-chains should generally use these requirements to ensure that the relay-chain
	// will not take longer than expected to import its blocks.
	if let Err(err) = frame_benchmarking_cli::SUBSTRATE_REFERENCE_HARDWARE.check_hardware(hwbench) {
		log::warn!(
			"⚠️  The hardware does not meet the minimal requirements {} for role 'Authority' find out more at:\n\
			https://wiki.polkadot.network/docs/maintain-guides-how-to-validate-polkadot#reference-hardware",
			err
		);
	}
}

pub trait StartConsensus<RuntimeApi>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<RuntimeApi>>,
{
	fn start_consensus(
		client: Arc<ParachainClient<RuntimeApi>>,
		backend: Arc<ParachainBackend>,
		block_import: ParachainBlockImport<RuntimeApi>,
		prometheus_registry: Option<&Registry>,
		telemetry: Option<TelemetryHandle>,
		task_manager: &TaskManager,
		relay_chain_interface: Arc<dyn RelayChainInterface>,
		transaction_pool: Arc<FullPool<Block, ParachainClient<RuntimeApi>>>,
		keystore: KeystorePtr,
		relay_chain_slot_duration: Duration,
		para_id: ParaId,
		collator_key: CollatorPair,
		overseer_handle: OverseerHandle,
		announce_block: Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
	) -> Result<(), sc_service::Error>;
}

pub trait NodeSpec {
	type RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Self::RuntimeApi>>;

	type BuildImportQueue: BuildImportQueue<Block, ParachainClient<Self::RuntimeApi>> + 'static;

	type BuildRpcExtensions: BuildRpcExtensions<
			ParachainClient<Self::RuntimeApi>,
			ParachainBackend,
			FullPool<Block, ParachainClient<Self::RuntimeApi>>,
		> + 'static;

	type StartConsensus: StartConsensus<Self::RuntimeApi> + 'static;

	const SYBIL_RESISTANCE: CollatorSybilResistance;

	/// Starts a `ServiceBuilder` for a full service.
	///
	/// Use this macro if you don't actually need the full service, but just the builder in order to
	/// be able to perform chain operations.
	fn new_partial(config: &Configuration) -> sc_service::error::Result<Service<Self::RuntimeApi>> {
		let telemetry = config
			.telemetry_endpoints
			.clone()
			.filter(|x| !x.is_empty())
			.map(|endpoints| -> Result<_, sc_telemetry::Error> {
				let worker = TelemetryWorker::new(16)?;
				let telemetry = worker.handle().new_telemetry(endpoints);
				Ok((worker, telemetry))
			})
			.transpose()?;

		let executor = sc_service::new_wasm_executor(config);

		let (client, backend, keystore_container, task_manager) =
			sc_service::new_full_parts_record_import::<Block, Self::RuntimeApi, _>(
				config,
				telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
				executor,
				true,
			)?;
		let client = Arc::new(client);

		let telemetry_worker_handle = telemetry.as_ref().map(|(worker, _)| worker.handle());
		let telemetry = telemetry.map(|(worker, telemetry)| {
			task_manager.spawn_handle().spawn("telemetry", None, worker.run());
			telemetry
		});

		let transaction_pool = sc_transaction_pool::BasicPool::new_full(
			config.transaction_pool.clone(),
			config.role.is_authority().into(),
			config.prometheus_registry(),
			task_manager.spawn_essential_handle(),
			client.clone(),
		);

		let block_import = ParachainBlockImport::new(client.clone(), backend.clone());
		let import_queue = Self::BuildImportQueue::build_import_queue(
			client.clone(),
			Box::new(block_import.clone()),
			config,
			telemetry.as_ref().map(|telemetry| telemetry.handle()),
			&task_manager,
		)?;

		Ok(PartialComponents {
			client,
			backend,
			task_manager,
			keystore_container,
			select_chain: (),
			import_queue,
			transaction_pool,
			other: (block_import, telemetry, telemetry_worker_handle),
		})
	}

	/// Start a node with the given parachain spec.
	///
	/// This is the actual implementation that is abstract over the executor and the runtime api.
	fn start_node<Net>(
		parachain_config: Configuration,
		polkadot_config: Configuration,
		collator_options: CollatorOptions,
		para_id: ParaId,
		hwbench: Option<sc_sysinfo::HwBench>,
	) -> Pin<Box<dyn Future<Output = sc_service::error::Result<TaskManager>>>>
	where
		Net: NetworkBackend<Block, Hash>,
	{
		Box::pin(async move {
			let parachain_config = prepare_node_config(parachain_config);

			let params = Self::new_partial(&parachain_config)?;
			let (block_import, mut telemetry, telemetry_worker_handle) = params.other;

			let client = params.client.clone();
			let backend = params.backend.clone();

			let mut task_manager = params.task_manager;
			let (relay_chain_interface, collator_key) = build_relay_chain_interface(
				polkadot_config,
				&parachain_config,
				telemetry_worker_handle,
				&mut task_manager,
				collator_options.clone(),
				hwbench.clone(),
			)
			.await
			.map_err(|e| sc_service::Error::Application(Box::new(e) as Box<_>))?;

			let validator = parachain_config.role.is_authority();
			let prometheus_registry = parachain_config.prometheus_registry().cloned();
			let transaction_pool = params.transaction_pool.clone();
			let import_queue_service = params.import_queue.service();
			let net_config = FullNetworkConfiguration::<_, _, Net>::new(&parachain_config.network);

			let (network, system_rpc_tx, tx_handler_controller, start_network, sync_service) =
				build_network(BuildNetworkParams {
					parachain_config: &parachain_config,
					net_config,
					client: client.clone(),
					transaction_pool: transaction_pool.clone(),
					para_id,
					spawn_handle: task_manager.spawn_handle(),
					relay_chain_interface: relay_chain_interface.clone(),
					import_queue: params.import_queue,
					sybil_resistance_level: Self::SYBIL_RESISTANCE,
				})
				.await?;

			let rpc_builder = {
				let client = client.clone();
				let transaction_pool = transaction_pool.clone();
				let backend_for_rpc = backend.clone();

				Box::new(move |deny_unsafe, _| {
					Self::BuildRpcExtensions::build_rpc_extensions(
						deny_unsafe,
						client.clone(),
						backend_for_rpc.clone(),
						transaction_pool.clone(),
					)
				})
			};

			sc_service::spawn_tasks(sc_service::SpawnTasksParams {
				rpc_builder,
				client: client.clone(),
				transaction_pool: transaction_pool.clone(),
				task_manager: &mut task_manager,
				config: parachain_config,
				keystore: params.keystore_container.keystore(),
				backend: backend.clone(),
				network: network.clone(),
				sync_service: sync_service.clone(),
				system_rpc_tx,
				tx_handler_controller,
				telemetry: telemetry.as_mut(),
			})?;

			if let Some(hwbench) = hwbench {
				sc_sysinfo::print_hwbench(&hwbench);
				if validator {
					warn_if_slow_hardware(&hwbench);
				}

				if let Some(ref mut telemetry) = telemetry {
					let telemetry_handle = telemetry.handle();
					task_manager.spawn_handle().spawn(
						"telemetry_hwbench",
						None,
						sc_sysinfo::initialize_hwbench_telemetry(telemetry_handle, hwbench),
					);
				}
			}

			let announce_block = {
				let sync_service = sync_service.clone();
				Arc::new(move |hash, data| sync_service.announce_block(hash, data))
			};

			let relay_chain_slot_duration = Duration::from_secs(6);

			let overseer_handle = relay_chain_interface
				.overseer_handle()
				.map_err(|e| sc_service::Error::Application(Box::new(e)))?;

			start_relay_chain_tasks(StartRelayChainTasksParams {
				client: client.clone(),
				announce_block: announce_block.clone(),
				para_id,
				relay_chain_interface: relay_chain_interface.clone(),
				task_manager: &mut task_manager,
				da_recovery_profile: if validator {
					DARecoveryProfile::Collator
				} else {
					DARecoveryProfile::FullNode
				},
				import_queue: import_queue_service,
				relay_chain_slot_duration,
				recovery_handle: Box::new(overseer_handle.clone()),
				sync_service,
			})?;

			if validator {
				Self::StartConsensus::start_consensus(
					client.clone(),
					backend.clone(),
					block_import,
					prometheus_registry.as_ref(),
					telemetry.as_ref().map(|t| t.handle()),
					&task_manager,
					relay_chain_interface.clone(),
					transaction_pool,
					params.keystore_container.keystore(),
					relay_chain_slot_duration,
					para_id,
					collator_key.expect("Command line arguments do not allow this. qed"),
					overseer_handle,
					announce_block,
				)?;
			}

			start_network.start_network();

			Ok(task_manager)
		})
	}
}
