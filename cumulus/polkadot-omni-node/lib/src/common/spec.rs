// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{
	chain_spec::Extensions,
	common::{
		command::NodeCommandRunner,
		rpc::BuildRpcExtensions,
		statement_store::{build_statement_store, new_statement_handler_proto},
		types::{
			ParachainBackend, ParachainBlockImport, ParachainClient, ParachainHostFunctions,
			ParachainService,
		},
		ConstructNodeRuntimeApi, NodeBlock, NodeExtraArgs,
	},
};
use cumulus_client_bootnodes::{start_bootnode_tasks, StartBootnodeTasksParams};
use cumulus_client_cli::CollatorOptions;
use cumulus_client_service::{
	build_network, build_relay_chain_interface, prepare_node_config, start_relay_chain_tasks,
	BuildNetworkParams, CollatorSybilResistance, DARecoveryProfile, StartRelayChainTasksParams,
};
use cumulus_primitives_core::{BlockT, GetParachainInfo, ParaId};
use cumulus_relay_chain_interface::{OverseerHandle, RelayChainInterface};
use futures::FutureExt;
use log::info;
use parachains_common::Hash;
use polkadot_primitives::CollatorPair;
use prometheus_endpoint::Registry;
use sc_client_api::Backend;
use sc_consensus::DefaultImportQueue;
use sc_executor::{HeapAllocStrategy, DEFAULT_HEAP_ALLOC_STRATEGY};
use sc_network::{config::FullNetworkConfiguration, NetworkBackend, NetworkBlock};
use sc_service::{Configuration, ImportQueue, PartialComponents, TaskManager};
use sc_statement_store::Store;
use sc_sysinfo::HwBench;
use sc_telemetry::{TelemetryHandle, TelemetryWorker};
use sc_tracing::tracing::Instrument;
use sc_transaction_pool::TransactionPoolHandle;
use sc_transaction_pool_api::OffchainTransactionPoolFactory;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_keystore::KeystorePtr;
use sp_runtime::traits::AccountIdConversion;
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

pub(crate) trait BuildImportQueue<
	Block: BlockT,
	RuntimeApi,
	BlockImport: sc_consensus::BlockImport<Block>,
>
{
	fn build_import_queue(
		client: Arc<ParachainClient<Block, RuntimeApi>>,
		block_import: ParachainBlockImport<Block, BlockImport>,
		config: &Configuration,
		telemetry_handle: Option<TelemetryHandle>,
		task_manager: &TaskManager,
	) -> sc_service::error::Result<DefaultImportQueue<Block>>;
}

pub(crate) trait StartConsensus<Block: BlockT, RuntimeApi, BI, BIAuxiliaryData>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
{
	fn start_consensus(
		client: Arc<ParachainClient<Block, RuntimeApi>>,
		block_import: ParachainBlockImport<Block, BI>,
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
		block_import_extra_return_value: BIAuxiliaryData,
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

pub(crate) trait InitBlockImport<Block: BlockT, RuntimeApi> {
	type BlockImport: sc_consensus::BlockImport<Block> + Clone + Send + Sync;
	type BlockImportAuxiliaryData;

	fn init_block_import(
		client: Arc<ParachainClient<Block, RuntimeApi>>,
	) -> sc_service::error::Result<(Self::BlockImport, Self::BlockImportAuxiliaryData)>;
}

pub(crate) struct ClientBlockImport;

impl<Block: BlockT, RuntimeApi> InitBlockImport<Block, RuntimeApi> for ClientBlockImport
where
	RuntimeApi: Send + ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
{
	type BlockImport = Arc<ParachainClient<Block, RuntimeApi>>;
	type BlockImportAuxiliaryData = ();

	fn init_block_import(
		client: Arc<ParachainClient<Block, RuntimeApi>>,
	) -> sc_service::error::Result<(Self::BlockImport, Self::BlockImportAuxiliaryData)> {
		Ok((client.clone(), ()))
	}
}

pub(crate) trait BaseNodeSpec {
	type Block: NodeBlock;

	type RuntimeApi: ConstructNodeRuntimeApi<
		Self::Block,
		ParachainClient<Self::Block, Self::RuntimeApi>,
	>;

	type BuildImportQueue: BuildImportQueue<
		Self::Block,
		Self::RuntimeApi,
		<Self::InitBlockImport as InitBlockImport<Self::Block, Self::RuntimeApi>>::BlockImport,
	>;

	type InitBlockImport: self::InitBlockImport<Self::Block, Self::RuntimeApi>;

	/// Retrieves parachain id.
	fn parachain_id(
		client: &ParachainClient<Self::Block, Self::RuntimeApi>,
		parachain_config: &Configuration,
	) -> Option<ParaId> {
		let best_hash = client.chain_info().best_hash;
		let para_id = if client
			.runtime_api()
			.has_api::<dyn GetParachainInfo<Self::Block>>(best_hash)
			.ok()
			.filter(|has_api| *has_api)
			.is_some()
		{
			client
				.runtime_api()
				.parachain_id(best_hash)
				.inspect_err(|err| {
					log::error!(
								"`cumulus_primitives_core::GetParachainInfo` runtime API call errored with {}",
								err
							);
				})
				.ok()?
		} else {
			ParaId::from(
				Extensions::try_get(&*parachain_config.chain_spec).and_then(|ext| ext.para_id())?,
			)
		};

		let parachain_account =
			AccountIdConversion::<polkadot_primitives::AccountId>::into_account_truncating(
				&para_id,
			);

		info!("🪪 Parachain id: {:?}", para_id);
		info!("🧾 Parachain Account: {}", parachain_account);

		Some(para_id)
	}

	/// Starts a `ServiceBuilder` for a full service.
	///
	/// Use this macro if you don't actually need the full service, but just the builder in order to
	/// be able to perform chain operations.
	fn new_partial(
		config: &Configuration,
	) -> sc_service::error::Result<
		ParachainService<
			Self::Block,
			Self::RuntimeApi,
			<Self::InitBlockImport as InitBlockImport<Self::Block, Self::RuntimeApi>>::BlockImport,
			<Self::InitBlockImport as InitBlockImport<Self::Block, Self::RuntimeApi>>::BlockImportAuxiliaryData
		>
	>{
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

		let (block_import, block_import_auxiliary_data) =
			Self::InitBlockImport::init_block_import(client.clone())?;

		let block_import = ParachainBlockImport::new(block_import, backend.clone());

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
			other: (block_import, telemetry, telemetry_worker_handle, block_import_auxiliary_data),
		})
	}
}

pub(crate) trait NodeSpec: BaseNodeSpec {
	type BuildRpcExtensions: BuildRpcExtensions<
		ParachainClient<Self::Block, Self::RuntimeApi>,
		ParachainBackend<Self::Block>,
		TransactionPoolHandle<Self::Block, ParachainClient<Self::Block, Self::RuntimeApi>>,
		Store,
	>;

	type StartConsensus: StartConsensus<
		Self::Block,
		Self::RuntimeApi,
		<Self::InitBlockImport as InitBlockImport<Self::Block, Self::RuntimeApi>>::BlockImport,
		<Self::InitBlockImport as InitBlockImport<Self::Block, Self::RuntimeApi>>::BlockImportAuxiliaryData,
	>;

	const SYBIL_RESISTANCE: CollatorSybilResistance;

	fn start_manual_seal_node(
		_config: Configuration,
		_block_time: u64,
	) -> sc_service::error::Result<TaskManager> {
		Err(sc_service::Error::Other("Manual seal not supported for this node type".into()))
	}

	/// Start a node with the given parachain spec.
	///
	/// This is the actual implementation that is abstract over the executor and the runtime api.
	fn start_node<Net>(
		parachain_config: Configuration,
		polkadot_config: Configuration,
		collator_options: CollatorOptions,
		hwbench: Option<sc_sysinfo::HwBench>,
		node_extra_args: NodeExtraArgs,
	) -> Pin<Box<dyn Future<Output = sc_service::error::Result<TaskManager>>>>
	where
		Net: NetworkBackend<Self::Block, Hash>,
	{
		let fut = async move {
			let parachain_config = prepare_node_config(parachain_config);
			let parachain_public_addresses = parachain_config.network.public_addresses.clone();
			let parachain_fork_id = parachain_config.chain_spec.fork_id().map(ToString::to_string);
			let advertise_non_global_ips = parachain_config.network.allow_non_globals_in_dht;
			let params = Self::new_partial(&parachain_config)?;
			let (block_import, mut telemetry, telemetry_worker_handle, block_import_auxiliary_data) =
				params.other;
			let client = params.client.clone();
			let backend = params.backend.clone();
			let mut task_manager = params.task_manager;

			// Resolve parachain id based on runtime, or based on chain spec.
			let para_id = Self::parachain_id(&client, &parachain_config)
				.ok_or("Failed to retrieve the parachain id")?;
			let relay_chain_fork_id = polkadot_config.chain_spec.fork_id().map(ToString::to_string);
			let (relay_chain_interface, collator_key, relay_chain_network, paranode_rx) =
				build_relay_chain_interface(
					polkadot_config,
					&parachain_config,
					telemetry_worker_handle,
					&mut task_manager,
					collator_options.clone(),
					hwbench.clone(),
				)
				.await
				.map_err(|e| sc_service::Error::Application(Box::new(e)))?;

			let validator = parachain_config.role.is_authority();
			let prometheus_registry = parachain_config.prometheus_registry().cloned();
			let transaction_pool = params.transaction_pool.clone();
			let import_queue_service = params.import_queue.service();
			let mut net_config = FullNetworkConfiguration::<_, _, Net>::new(
				&parachain_config.network,
				prometheus_registry.clone(),
			);

			let metrics = Net::register_notification_metrics(
				parachain_config.prometheus_config.as_ref().map(|config| &config.registry),
			);

			let statement_handler_proto = node_extra_args.enable_statement_store.then(|| {
				new_statement_handler_proto(&*client, &parachain_config, &metrics, &mut net_config)
			});

			let (network, system_rpc_tx, tx_handler_controller, sync_service) =
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
					metrics,
				})
				.await?;

			let statement_store = statement_handler_proto
				.map(|statement_handler_proto| {
					build_statement_store(
						&parachain_config,
						&mut task_manager,
						client.clone(),
						network.clone(),
						sync_service.clone(),
						params.keystore_container.local_keystore(),
						statement_handler_proto,
					)
				})
				.transpose()?;

			if parachain_config.offchain_worker.enabled {
				let custom_extensions = {
					let statement_store = statement_store.clone();
					move |_hash| {
						if let Some(statement_store) = &statement_store {
							vec![Box::new(statement_store.clone().as_statement_store_ext())
								as Box<_>]
						} else {
							vec![]
						}
					}
				};

				let offchain_workers =
					sc_offchain::OffchainWorkers::new(sc_offchain::OffchainWorkerOptions {
						runtime_api_provider: client.clone(),
						keystore: Some(params.keystore_container.keystore()),
						offchain_db: backend.offchain_storage(),
						transaction_pool: Some(OffchainTransactionPoolFactory::new(
							transaction_pool.clone(),
						)),
						network_provider: Arc::new(network.clone()),
						is_validator: parachain_config.role.is_authority(),
						enable_http_requests: true,
						custom_extensions,
					})?;
				task_manager.spawn_handle().spawn(
					"offchain-workers-runner",
					"offchain-work",
					offchain_workers.run(client.clone(), task_manager.spawn_handle()).boxed(),
				);
			}

			let rpc_builder = {
				let client = client.clone();
				let transaction_pool = transaction_pool.clone();
				let backend_for_rpc = backend.clone();
				let statement_store = statement_store.clone();

				Box::new(move |_| {
					Self::BuildRpcExtensions::build_rpc_extensions(
						client.clone(),
						backend_for_rpc.clone(),
						transaction_pool.clone(),
						statement_store.clone(),
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
				prometheus_registry: prometheus_registry.as_ref(),
			})?;

			start_bootnode_tasks(StartBootnodeTasksParams {
				embedded_dht_bootnode: collator_options.embedded_dht_bootnode,
				dht_bootnode_discovery: collator_options.dht_bootnode_discovery,
				para_id,
				task_manager: &mut task_manager,
				relay_chain_interface: relay_chain_interface.clone(),
				relay_chain_fork_id,
				relay_chain_network,
				request_receiver: paranode_rx,
				parachain_network: network,
				advertise_non_global_ips,
				parachain_genesis_hash: client.chain_info().genesis_hash,
				parachain_fork_id,
				parachain_public_addresses,
			});

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
					block_import_auxiliary_data,
				)?;
			}

			Ok(task_manager)
		};

		Box::pin(Instrument::instrument(
			fut,
			sc_tracing::tracing::info_span!(
				sc_tracing::logging::PREFIX_LOG_SPAN,
				name = "Parachain"
			),
		))
	}
}

pub(crate) trait DynNodeSpec: NodeCommandRunner {
	/// Start node with manual-seal consensus.
	fn start_manual_seal_node(
		self: Box<Self>,
		config: Configuration,
		block_time: u64,
	) -> sc_service::error::Result<TaskManager>;

	/// Start the node.
	fn start_node(
		self: Box<Self>,
		parachain_config: Configuration,
		polkadot_config: Configuration,
		collator_options: CollatorOptions,
		hwbench: Option<HwBench>,
		node_extra_args: NodeExtraArgs,
	) -> Pin<Box<dyn Future<Output = sc_service::error::Result<TaskManager>>>>;
}

impl<T> DynNodeSpec for T
where
	T: NodeSpec + NodeCommandRunner,
{
	fn start_manual_seal_node(
		self: Box<Self>,
		config: Configuration,
		block_time: u64,
	) -> sc_service::error::Result<TaskManager> {
		<Self as NodeSpec>::start_manual_seal_node(config, block_time)
	}

	fn start_node(
		self: Box<Self>,
		parachain_config: Configuration,
		polkadot_config: Configuration,
		collator_options: CollatorOptions,
		hwbench: Option<HwBench>,
		node_extra_args: NodeExtraArgs,
	) -> Pin<Box<dyn Future<Output = sc_service::error::Result<TaskManager>>>> {
		match parachain_config.network.network_backend {
			sc_network::config::NetworkBackendType::Libp2p =>
				<Self as NodeSpec>::start_node::<sc_network::NetworkWorker<_, _>>(
					parachain_config,
					polkadot_config,
					collator_options,
					hwbench,
					node_extra_args,
				),
			sc_network::config::NetworkBackendType::Litep2p =>
				<Self as NodeSpec>::start_node::<sc_network::Litep2pNetworkBackend>(
					parachain_config,
					polkadot_config,
					collator_options,
					hwbench,
					node_extra_args,
				),
		}
	}
}
