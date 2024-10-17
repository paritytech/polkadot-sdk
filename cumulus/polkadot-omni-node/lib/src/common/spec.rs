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

use crate::common::{
	command::NodeCommandRunner,
	rpc::BuildRpcExtensions,
	types::{
		ParachainBackend, ParachainBlockImport, ParachainClient, ParachainHostFunctions,
		ParachainService,
	},
	ConstructNodeRuntimeApi, NodeBlock, NodeExtraArgs,
};
use cumulus_client_cli::CollatorOptions;
use cumulus_client_service::{
	build_network, build_relay_chain_interface, prepare_node_config, start_relay_chain_tasks,
	BuildNetworkParams, CollatorSybilResistance, DARecoveryProfile, StartRelayChainTasksParams,
};
use cumulus_primitives_core::{BlockT, ParaId};
use cumulus_relay_chain_interface::{OverseerHandle, RelayChainInterface};
use parachains_common::Hash;
use polkadot_primitives::CollatorPair;
use prometheus_endpoint::Registry;
use sc_consensus::DefaultImportQueue;
use sc_executor::{HeapAllocStrategy, DEFAULT_HEAP_ALLOC_STRATEGY};
use sc_network::{config::FullNetworkConfiguration, NetworkBackend, NetworkBlock};
use sc_service::{Configuration, ImportQueue, PartialComponents, TaskManager};
use sc_sysinfo::HwBench;
use sc_telemetry::{TelemetryHandle, TelemetryWorker};
use sc_tracing::tracing::Instrument;
use sc_transaction_pool::TransactionPoolHandle;
use sp_keystore::KeystorePtr;
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

pub(crate) trait BuildImportQueue<Block: BlockT, RuntimeApi> {
	fn build_import_queue(
		client: Arc<ParachainClient<Block, RuntimeApi>>,
		block_import: ParachainBlockImport<Block, RuntimeApi>,
		config: &Configuration,
		telemetry_handle: Option<TelemetryHandle>,
		task_manager: &TaskManager,
	) -> sc_service::error::Result<DefaultImportQueue<Block>>;
}

pub(crate) trait StartConsensus<Block: BlockT, RuntimeApi>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
{
	fn start_consensus(
		client: Arc<ParachainClient<Block, RuntimeApi>>,
		block_import: ParachainBlockImport<Block, RuntimeApi>,
		prometheus_registry: Option<&Registry>,
		telemetry: Option<TelemetryHandle>,
		task_manager: &TaskManager,
		relay_chain_interface: Arc<dyn RelayChainInterface>,
		transaction_pool: Arc<TransactionPoolHandle<Block, ParachainClient<Block, RuntimeApi>>>,
		keystore: KeystorePtr,
		relay_chain_slot_duration: Duration,
		para_id: ParaId,
		collator_key: CollatorPair,
		overseer_handle: OverseerHandle,
		announce_block: Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
		backend: Arc<ParachainBackend<Block>>,
		node_extra_args: NodeExtraArgs,
	) -> Result<(), sc_service::Error>;
}

/// Checks that the hardware meets the requirements and print a warning otherwise.
fn warn_if_slow_hardware(hwbench: &sc_sysinfo::HwBench) {
	// Polkadot para-chains should generally use these requirements to ensure that the relay-chain
	// will not take longer than expected to import its blocks.
	if let Err(err) =
		frame_benchmarking_cli::SUBSTRATE_REFERENCE_HARDWARE.check_hardware(hwbench, false)
	{
		log::warn!(
			"⚠️  The hardware does not meet the minimal requirements {} for role 'Authority' find out more at:\n\
			https://wiki.polkadot.network/docs/maintain-guides-how-to-validate-polkadot#reference-hardware",
			err
		);
	}
}

pub(crate) trait BaseNodeSpec {
	type Block: NodeBlock;

	type RuntimeApi: ConstructNodeRuntimeApi<
		Self::Block,
		ParachainClient<Self::Block, Self::RuntimeApi>,
	>;

	type BuildImportQueue: BuildImportQueue<Self::Block, Self::RuntimeApi>;

	/// Starts a `ServiceBuilder` for a full service.
	///
	/// Use this macro if you don't actually need the full service, but just the builder in order to
	/// be able to perform chain operations.
	fn new_partial(
		config: &Configuration,
	) -> sc_service::error::Result<ParachainService<Self::Block, Self::RuntimeApi>> {
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

		let heap_pages =
			config.executor.default_heap_pages.map_or(DEFAULT_HEAP_ALLOC_STRATEGY, |h| {
				HeapAllocStrategy::Static { extra_pages: h as _ }
			});

		let executor = sc_executor::WasmExecutor::<ParachainHostFunctions>::builder()
			.with_execution_method(config.executor.wasm_method)
			.with_max_runtime_instances(config.executor.max_runtime_instances)
			.with_runtime_cache_size(config.executor.runtime_cache_size)
			.with_onchain_heap_alloc_strategy(heap_pages)
			.with_offchain_heap_alloc_strategy(heap_pages)
			.build();

		let (client, backend, keystore_container, task_manager) =
			sc_service::new_full_parts_record_import::<Self::Block, Self::RuntimeApi, _>(
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

		let transaction_pool = Arc::from(
			sc_transaction_pool::Builder::new(
				task_manager.spawn_essential_handle(),
				client.clone(),
				config.role.is_authority().into(),
			)
			.with_options(config.transaction_pool.clone())
			.with_prometheus(config.prometheus_registry())
			.build(),
		);

		let block_import = ParachainBlockImport::new(client.clone(), backend.clone());

		let import_queue = Self::BuildImportQueue::build_import_queue(
			client.clone(),
			block_import.clone(),
			config,
			telemetry.as_ref().map(|telemetry| telemetry.handle()),
			&task_manager,
		)?;

		Ok(PartialComponents {
			backend,
			client,
			import_queue,
			keystore_container,
			task_manager,
			transaction_pool,
			select_chain: (),
			other: (block_import, telemetry, telemetry_worker_handle),
		})
	}
}

pub(crate) trait NodeSpec: BaseNodeSpec {
	type BuildRpcExtensions: BuildRpcExtensions<
		ParachainClient<Self::Block, Self::RuntimeApi>,
		ParachainBackend<Self::Block>,
		TransactionPoolHandle<Self::Block, ParachainClient<Self::Block, Self::RuntimeApi>>,
	>;

	type StartConsensus: StartConsensus<Self::Block, Self::RuntimeApi>;

	const SYBIL_RESISTANCE: CollatorSybilResistance;

	/// Start a node with the given parachain spec.
	///
	/// This is the actual implementation that is abstract over the executor and the runtime api.
	fn start_node<Net>(
		parachain_config: Configuration,
		polkadot_config: Configuration,
		collator_options: CollatorOptions,
		para_id: ParaId,
		hwbench: Option<sc_sysinfo::HwBench>,
		node_extra_args: NodeExtraArgs,
	) -> Pin<Box<dyn Future<Output = sc_service::error::Result<TaskManager>>>>
	where
		Net: NetworkBackend<Self::Block, Hash>,
	{
		Box::pin(
			async move {
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
				let net_config = FullNetworkConfiguration::<_, _, Net>::new(
					&parachain_config.network,
					prometheus_registry.clone(),
				);

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

					Box::new(move |_| {
						Self::BuildRpcExtensions::build_rpc_extensions(
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
						backend.clone(),
						node_extra_args,
					)?;
				}

				start_network.start_network();

				Ok(task_manager)
			}
			.instrument(sc_tracing::tracing::info_span!(
				sc_tracing::logging::PREFIX_LOG_SPAN,
				name = "Parachain",
			)),
		)
	}
}

pub(crate) trait DynNodeSpec: NodeCommandRunner {
	fn start_node(
		self: Box<Self>,
		parachain_config: Configuration,
		polkadot_config: Configuration,
		collator_options: CollatorOptions,
		para_id: ParaId,
		hwbench: Option<HwBench>,
		node_extra_args: NodeExtraArgs,
	) -> Pin<Box<dyn Future<Output = sc_service::error::Result<TaskManager>>>>;
}

impl<T> DynNodeSpec for T
where
	T: NodeSpec + NodeCommandRunner,
{
	fn start_node(
		self: Box<Self>,
		parachain_config: Configuration,
		polkadot_config: Configuration,
		collator_options: CollatorOptions,
		para_id: ParaId,
		hwbench: Option<HwBench>,
		node_extra_args: NodeExtraArgs,
	) -> Pin<Box<dyn Future<Output = sc_service::error::Result<TaskManager>>>> {
		match parachain_config.network.network_backend {
			sc_network::config::NetworkBackendType::Libp2p =>
				<Self as NodeSpec>::start_node::<sc_network::NetworkWorker<_, _>>(
					parachain_config,
					polkadot_config,
					collator_options,
					para_id,
					hwbench,
					node_extra_args,
				),
			sc_network::config::NetworkBackendType::Litep2p =>
				<Self as NodeSpec>::start_node::<sc_network::Litep2pNetworkBackend>(
					parachain_config,
					polkadot_config,
					collator_options,
					para_id,
					hwbench,
					node_extra_args,
				),
		}
	}
}
