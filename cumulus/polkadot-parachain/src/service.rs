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

use codec::{Codec, Decode};
use cumulus_client_cli::CollatorOptions;
use cumulus_client_collator::service::CollatorService;
use cumulus_client_consensus_aura::collators::{
	basic::{self as basic_aura, Params as BasicAuraParams},
	lookahead::{self as aura, Params as AuraParams},
};
use cumulus_client_consensus_common::{
	ParachainBlockImport as TParachainBlockImport, ParachainCandidate, ParachainConsensus,
};
use cumulus_client_consensus_proposer::Proposer;
#[allow(deprecated)]
use cumulus_client_service::old_consensus;
use cumulus_client_service::{
	build_network, build_relay_chain_interface, prepare_node_config, start_relay_chain_tasks,
	BuildNetworkParams, CollatorSybilResistance, DARecoveryProfile, StartRelayChainTasksParams,
};
use cumulus_primitives_core::{
	relay_chain::{Hash as PHash, PersistedValidationData, ValidationCode},
	ParaId,
};
use cumulus_relay_chain_interface::{OverseerHandle, RelayChainInterface};
use sp_core::Pair;

use jsonrpsee::RpcModule;

use crate::{fake_runtime_api::aura::RuntimeApi, rpc};
pub use parachains_common::{AccountId, Balance, Block, Hash, Header, Nonce};

use cumulus_client_consensus_relay_chain::Verifier as RelayChainVerifier;
use futures::{lock::Mutex, prelude::*};
use sc_consensus::{
	import_queue::{BasicQueue, Verifier as VerifierT},
	BlockImportParams, ImportQueue,
};
use sc_executor::{HeapAllocStrategy, WasmExecutor, DEFAULT_HEAP_ALLOC_STRATEGY};
use sc_network::{config::FullNetworkConfiguration, NetworkBlock};
use sc_network_sync::SyncingService;
use sc_service::{Configuration, PartialComponents, TFullBackend, TFullClient, TaskManager};
use sc_telemetry::{Telemetry, TelemetryHandle, TelemetryWorker, TelemetryWorkerHandle};
use sp_api::{ApiExt, ConstructRuntimeApi, ProvideRuntimeApi};
use sp_consensus_aura::AuraApi;
use sp_core::traits::SpawnEssentialNamed;
use sp_keystore::KeystorePtr;
use sp_runtime::{
	app_crypto::AppCrypto,
	traits::{Block as BlockT, Header as HeaderT},
};
use std::{marker::PhantomData, sync::Arc, time::Duration};
use substrate_prometheus_endpoint::Registry;

use polkadot_primitives::CollatorPair;

#[cfg(not(feature = "runtime-benchmarks"))]
type HostFunctions = sp_io::SubstrateHostFunctions;

#[cfg(feature = "runtime-benchmarks")]
type HostFunctions =
	(sp_io::SubstrateHostFunctions, frame_benchmarking::benchmarking::HostFunctions);

type ParachainClient<RuntimeApi> = TFullClient<Block, RuntimeApi, WasmExecutor<HostFunctions>>;

type ParachainBackend = TFullBackend<Block>;

type ParachainBlockImport<RuntimeApi> =
	TParachainBlockImport<Block, Arc<ParachainClient<RuntimeApi>>, ParachainBackend>;

/// Native executor instance.
pub struct ShellRuntimeExecutor;

impl sc_executor::NativeExecutionDispatch for ShellRuntimeExecutor {
	type ExtendHostFunctions = ();

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		shell_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		shell_runtime::native_version()
	}
}

/// Native Asset Hub Westend (Westmint) executor instance.
pub struct AssetHubWestendExecutor;
impl sc_executor::NativeExecutionDispatch for AssetHubWestendExecutor {
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		asset_hub_westend_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		asset_hub_westend_runtime::native_version()
	}
}

/// Native Westend Collectives executor instance.
pub struct CollectivesWestendRuntimeExecutor;

impl sc_executor::NativeExecutionDispatch for CollectivesWestendRuntimeExecutor {
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		collectives_westend_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		collectives_westend_runtime::native_version()
	}
}

/// Native BridgeHubRococo executor instance.
pub struct BridgeHubRococoRuntimeExecutor;
impl sc_executor::NativeExecutionDispatch for BridgeHubRococoRuntimeExecutor {
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		bridge_hub_rococo_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		bridge_hub_rococo_runtime::native_version()
	}
}

/// Native `CoretimeRococo` executor instance.
pub struct CoretimeRococoRuntimeExecutor;
impl sc_executor::NativeExecutionDispatch for CoretimeRococoRuntimeExecutor {
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;
	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		coretime_rococo_runtime::api::dispatch(method, data)
	}
	fn native_version() -> sc_executor::NativeVersion {
		coretime_rococo_runtime::native_version()
	}
}

/// Native `CoretimeWestend` executor instance.
pub struct CoretimeWestendRuntimeExecutor;
impl sc_executor::NativeExecutionDispatch for CoretimeWestendRuntimeExecutor {
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;
	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		coretime_westend_runtime::api::dispatch(method, data)
	}
	fn native_version() -> sc_executor::NativeVersion {
		coretime_westend_runtime::native_version()
	}
}

/// Native contracts executor instance.
pub struct ContractsRococoRuntimeExecutor;
impl sc_executor::NativeExecutionDispatch for ContractsRococoRuntimeExecutor {
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		contracts_rococo_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		contracts_rococo_runtime::native_version()
	}
}

/// Native Westend Glutton executor instance.
pub struct GluttonWestendRuntimeExecutor;

impl sc_executor::NativeExecutionDispatch for GluttonWestendRuntimeExecutor {
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		glutton_westend_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		glutton_westend_runtime::native_version()
	}
}

/// Native `PeopleWestend` executor instance.
pub struct PeopleWestendRuntimeExecutor;
impl sc_executor::NativeExecutionDispatch for PeopleWestendRuntimeExecutor {
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;
	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		people_westend_runtime::api::dispatch(method, data)
	}
	fn native_version() -> sc_executor::NativeVersion {
		people_westend_runtime::native_version()
	}
}

/// Native `PeopleRococo` executor instance.
pub struct PeopleRococoRuntimeExecutor;
impl sc_executor::NativeExecutionDispatch for PeopleRococoRuntimeExecutor {
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;
	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		people_rococo_runtime::api::dispatch(method, data)
	}
	fn native_version() -> sc_executor::NativeVersion {
		people_rococo_runtime::native_version()
	}
}

/// Assembly of PartialComponents (enough to run chain ops subcommands)
pub type Service<RuntimeApi> = PartialComponents<
	ParachainClient<RuntimeApi>,
	ParachainBackend,
	(),
	sc_consensus::DefaultImportQueue<Block>,
	sc_transaction_pool::FullPool<Block, ParachainClient<RuntimeApi>>,
	(ParachainBlockImport<RuntimeApi>, Option<Telemetry>, Option<TelemetryWorkerHandle>),
>;

/// Starts a `ServiceBuilder` for a full service.
///
/// Use this macro if you don't actually need the full service, but just the builder in order to
/// be able to perform chain operations.
pub fn new_partial<RuntimeApi, BIQ>(
	config: &Configuration,
	build_import_queue: BIQ,
) -> Result<Service<RuntimeApi>, sc_service::Error>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<Block>
		+ sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>,
	BIQ: FnOnce(
		Arc<ParachainClient<RuntimeApi>>,
		ParachainBlockImport<RuntimeApi>,
		&Configuration,
		Option<TelemetryHandle>,
		&TaskManager,
	) -> Result<sc_consensus::DefaultImportQueue<Block>, sc_service::Error>,
{
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

	let heap_pages = config
		.default_heap_pages
		.map_or(DEFAULT_HEAP_ALLOC_STRATEGY, |h| HeapAllocStrategy::Static { extra_pages: h as _ });

	let executor = sc_executor::WasmExecutor::<HostFunctions>::builder()
		.with_execution_method(config.wasm_method)
		.with_max_runtime_instances(config.max_runtime_instances)
		.with_runtime_cache_size(config.runtime_cache_size)
		.with_onchain_heap_alloc_strategy(heap_pages)
		.with_offchain_heap_alloc_strategy(heap_pages)
		.build();

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, _>(
			config,
			telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
			executor,
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

	let import_queue = build_import_queue(
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

/// Start a shell node with the given parachain `Configuration` and relay chain `Configuration`.
///
/// This is the actual implementation that is abstract over the executor and the runtime api for
/// shell nodes.
#[sc_tracing::logging::prefix_logs_with("Parachain")]
async fn start_shell_node_impl<RuntimeApi, RB, BIQ, SC>(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	sybil_resistance_level: CollatorSybilResistance,
	para_id: ParaId,
	rpc_ext_builder: RB,
	build_import_queue: BIQ,
	start_consensus: SC,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi>>)>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<Block>
		+ sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ cumulus_primitives_core::CollectCollationInfo<Block>,
	RB: Fn(Arc<ParachainClient<RuntimeApi>>) -> Result<jsonrpsee::RpcModule<()>, sc_service::Error>
		+ 'static,
	BIQ: FnOnce(
		Arc<ParachainClient<RuntimeApi>>,
		ParachainBlockImport<RuntimeApi>,
		&Configuration,
		Option<TelemetryHandle>,
		&TaskManager,
	) -> Result<sc_consensus::DefaultImportQueue<Block>, sc_service::Error>,
	SC: FnOnce(
		Arc<ParachainClient<RuntimeApi>>,
		ParachainBlockImport<RuntimeApi>,
		Option<&Registry>,
		Option<TelemetryHandle>,
		&TaskManager,
		Arc<dyn RelayChainInterface>,
		Arc<sc_transaction_pool::FullPool<Block, ParachainClient<RuntimeApi>>>,
		Arc<SyncingService<Block>>,
		KeystorePtr,
		Duration,
		ParaId,
		CollatorPair,
		OverseerHandle,
		Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
	) -> Result<(), sc_service::Error>,
{
	let parachain_config = prepare_node_config(parachain_config);

	let params = new_partial::<RuntimeApi, BIQ>(&parachain_config, build_import_queue)?;
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
	let net_config = FullNetworkConfiguration::new(&parachain_config.network);

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
			sybil_resistance_level,
		})
		.await?;

	let rpc_client = client.clone();
	let rpc_builder = Box::new(move |_, _| rpc_ext_builder(rpc_client.clone()));

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
		sync_service: sync_service.clone(),
	})?;

	if validator {
		start_consensus(
			client.clone(),
			block_import,
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|t| t.handle()),
			&task_manager,
			relay_chain_interface.clone(),
			transaction_pool,
			sync_service.clone(),
			params.keystore_container.keystore(),
			relay_chain_slot_duration,
			para_id,
			collator_key.expect("Command line arguments do not allow this. qed"),
			overseer_handle,
			announce_block,
		)?;
	}

	start_network.start_network();

	Ok((task_manager, client))
}

/// Start a node with the given parachain `Configuration` and relay chain `Configuration`.
///
/// This is the actual implementation that is abstract over the executor and the runtime api.
#[sc_tracing::logging::prefix_logs_with("Parachain")]
async fn start_node_impl<RuntimeApi, RB, BIQ, SC>(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	sybil_resistance_level: CollatorSybilResistance,
	para_id: ParaId,
	_rpc_ext_builder: RB,
	build_import_queue: BIQ,
	start_consensus: SC,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi>>)>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<Block>
		+ sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ cumulus_primitives_core::CollectCollationInfo<Block>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	RB: Fn(Arc<ParachainClient<RuntimeApi>>) -> Result<jsonrpsee::RpcModule<()>, sc_service::Error>,
	BIQ: FnOnce(
		Arc<ParachainClient<RuntimeApi>>,
		ParachainBlockImport<RuntimeApi>,
		&Configuration,
		Option<TelemetryHandle>,
		&TaskManager,
	) -> Result<sc_consensus::DefaultImportQueue<Block>, sc_service::Error>,
	SC: FnOnce(
		Arc<ParachainClient<RuntimeApi>>,
		ParachainBlockImport<RuntimeApi>,
		Option<&Registry>,
		Option<TelemetryHandle>,
		&TaskManager,
		Arc<dyn RelayChainInterface>,
		Arc<sc_transaction_pool::FullPool<Block, ParachainClient<RuntimeApi>>>,
		Arc<SyncingService<Block>>,
		KeystorePtr,
		Duration,
		ParaId,
		CollatorPair,
		OverseerHandle,
		Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
		Arc<ParachainBackend>,
	) -> Result<(), sc_service::Error>,
{
	let parachain_config = prepare_node_config(parachain_config);

	let params = new_partial::<RuntimeApi, BIQ>(&parachain_config, build_import_queue)?;
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
	let net_config = FullNetworkConfiguration::new(&parachain_config.network);

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
			sybil_resistance_level,
		})
		.await?;

	let rpc_builder = {
		let client = client.clone();
		let transaction_pool = transaction_pool.clone();

		let backend_for_rpc = backend.clone();
		Box::new(move |deny_unsafe, _| {
			let deps = rpc::FullDeps {
				client: client.clone(),
				pool: transaction_pool.clone(),
				deny_unsafe,
			};

			rpc::create_full(deps, backend_for_rpc.clone()).map_err(Into::into)
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
		sync_service: sync_service.clone(),
	})?;

	if validator {
		start_consensus(
			client.clone(),
			block_import,
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|t| t.handle()),
			&task_manager,
			relay_chain_interface.clone(),
			transaction_pool,
			sync_service.clone(),
			params.keystore_container.keystore(),
			relay_chain_slot_duration,
			para_id,
			collator_key.expect("Command line arguments do not allow this. qed"),
			overseer_handle,
			announce_block,
			backend.clone(),
		)?;
	}

	start_network.start_network();

	Ok((task_manager, client))
}

/// Start a node with the given parachain `Configuration` and relay chain `Configuration`.
///
/// This is the actual implementation that is abstract over the executor and the runtime api.
///
/// This node is basic in the sense that it doesn't support functionality like transaction
/// payment. Intended to replace start_shell_node in use for glutton, shell, and seedling.
#[sc_tracing::logging::prefix_logs_with("Parachain")]
async fn start_basic_lookahead_node_impl<RuntimeApi, RB, BIQ, SC>(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	sybil_resistance_level: CollatorSybilResistance,
	para_id: ParaId,
	rpc_ext_builder: RB,
	build_import_queue: BIQ,
	start_consensus: SC,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi>>)>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<Block>
		+ sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ cumulus_primitives_core::CollectCollationInfo<Block>
		+ frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	RB: Fn(Arc<ParachainClient<RuntimeApi>>) -> Result<jsonrpsee::RpcModule<()>, sc_service::Error>
		+ 'static,
	BIQ: FnOnce(
		Arc<ParachainClient<RuntimeApi>>,
		ParachainBlockImport<RuntimeApi>,
		&Configuration,
		Option<TelemetryHandle>,
		&TaskManager,
	) -> Result<sc_consensus::DefaultImportQueue<Block>, sc_service::Error>,
	SC: FnOnce(
		Arc<ParachainClient<RuntimeApi>>,
		ParachainBlockImport<RuntimeApi>,
		Option<&Registry>,
		Option<TelemetryHandle>,
		&TaskManager,
		Arc<dyn RelayChainInterface>,
		Arc<sc_transaction_pool::FullPool<Block, ParachainClient<RuntimeApi>>>,
		Arc<SyncingService<Block>>,
		KeystorePtr,
		Duration,
		ParaId,
		CollatorPair,
		OverseerHandle,
		Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
		Arc<ParachainBackend>,
	) -> Result<(), sc_service::Error>,
{
	let parachain_config = prepare_node_config(parachain_config);

	let params = new_partial::<RuntimeApi, BIQ>(&parachain_config, build_import_queue)?;
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
	let net_config = FullNetworkConfiguration::new(&parachain_config.network);

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
			sybil_resistance_level,
		})
		.await?;

	let rpc_client = client.clone();
	let rpc_builder = Box::new(move |_, _| rpc_ext_builder(rpc_client.clone()));

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
		sync_service: sync_service.clone(),
	})?;

	if validator {
		start_consensus(
			client.clone(),
			block_import,
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|t| t.handle()),
			&task_manager,
			relay_chain_interface.clone(),
			transaction_pool,
			sync_service.clone(),
			params.keystore_container.keystore(),
			relay_chain_slot_duration,
			para_id,
			collator_key.expect("Command line arguments do not allow this. qed"),
			overseer_handle,
			announce_block,
			backend.clone(),
		)?;
	}

	start_network.start_network();

	Ok((task_manager, client))
}

/// Build the import queue for the rococo parachain runtime.
pub fn rococo_parachain_build_import_queue(
	client: Arc<ParachainClient<RuntimeApi>>,
	block_import: ParachainBlockImport<RuntimeApi>,
	config: &Configuration,
	telemetry: Option<TelemetryHandle>,
	task_manager: &TaskManager,
) -> Result<sc_consensus::DefaultImportQueue<Block>, sc_service::Error> {
	let slot_duration = cumulus_client_consensus_aura::slot_duration(&*client)?;

	cumulus_client_consensus_aura::import_queue::<
		sp_consensus_aura::sr25519::AuthorityPair,
		_,
		_,
		_,
		_,
		_,
	>(cumulus_client_consensus_aura::ImportQueueParams {
		block_import,
		client,
		create_inherent_data_providers: move |_, _| async move {
			let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

			let slot =
				sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
					*timestamp,
					slot_duration,
				);

			Ok((slot, timestamp))
		},
		registry: config.prometheus_registry(),
		spawner: &task_manager.spawn_essential_handle(),
		telemetry,
	})
	.map_err(Into::into)
}

/// Start a rococo parachain node.
pub async fn start_rococo_parachain_node(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	para_id: ParaId,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi>>)> {
	start_node_impl::<RuntimeApi, _, _, _>(
		parachain_config,
		polkadot_config,
		collator_options,
		CollatorSybilResistance::Resistant, // Aura
		para_id,
		|_| Ok(RpcModule::new(())),
		rococo_parachain_build_import_queue,
		|client,
		 block_import,
		 prometheus_registry,
		 telemetry,
		 task_manager,
		 relay_chain_interface,
		 transaction_pool,
		 sync_oracle,
		 keystore,
		 relay_chain_slot_duration,
		 para_id,
		 collator_key,
		 overseer_handle,
		 announce_block,
		 backend| {
			let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
				task_manager.spawn_handle(),
				client.clone(),
				transaction_pool,
				prometheus_registry,
				telemetry.clone(),
			);
			let proposer = Proposer::new(proposer_factory);

			let collator_service = CollatorService::new(
				client.clone(),
				Arc::new(task_manager.spawn_handle()),
				announce_block,
				client.clone(),
			);

			let params = AuraParams {
				create_inherent_data_providers: move |_, ()| async move { Ok(()) },
				block_import,
				para_client: client.clone(),
				para_backend: backend.clone(),
				relay_client: relay_chain_interface,
				code_hash_provider: move |block_hash| {
					client.code_at(block_hash).ok().map(|c| ValidationCode::from(c).hash())
				},
				sync_oracle,
				keystore,
				collator_key,
				para_id,
				overseer_handle,
				relay_chain_slot_duration,
				proposer,
				collator_service,
				authoring_duration: Duration::from_millis(1500),
				reinitialize: false,
			};

			let fut = aura::run::<
				Block,
				sp_consensus_aura::sr25519::AuthorityPair,
				_,
				_,
				_,
				_,
				_,
				_,
				_,
				_,
				_,
			>(params);
			task_manager.spawn_essential_handle().spawn("aura", None, fut);

			Ok(())
		},
		hwbench,
	)
	.await
}

/// Build the import queue for the shell runtime.
pub fn shell_build_import_queue<RuntimeApi>(
	client: Arc<ParachainClient<RuntimeApi>>,
	block_import: ParachainBlockImport<RuntimeApi>,
	config: &Configuration,
	_: Option<TelemetryHandle>,
	task_manager: &TaskManager,
) -> Result<sc_consensus::DefaultImportQueue<Block>, sc_service::Error>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<Block>
		+ sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>,
{
	cumulus_client_consensus_relay_chain::import_queue(
		client,
		block_import,
		|_, _| async { Ok(()) },
		&task_manager.spawn_essential_handle(),
		config.prometheus_registry(),
	)
	.map_err(Into::into)
}

/// Start a polkadot-shell parachain node.
pub async fn start_shell_node<RuntimeApi>(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	para_id: ParaId,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi>>)>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<Block>
		+ sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ cumulus_primitives_core::CollectCollationInfo<Block>,
{
	start_shell_node_impl::<RuntimeApi, _, _, _>(
		parachain_config,
		polkadot_config,
		collator_options,
		CollatorSybilResistance::Unresistant, // free-for-all consensus
		para_id,
		|_| Ok(RpcModule::new(())),
		shell_build_import_queue,
		|client,
		 block_import,
		 prometheus_registry,
		 telemetry,
		 task_manager,
		 relay_chain_interface,
		 transaction_pool,
		 _sync_oracle,
		 _keystore,
		 _relay_chain_slot_duration,
		 para_id,
		 collator_key,
		 overseer_handle,
		 announce_block| {
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
		},
		hwbench,
	)
	.await
}

enum BuildOnAccess<R> {
	Uninitialized(Option<Box<dyn FnOnce() -> R + Send + Sync>>),
	Initialized(R),
}

impl<R> BuildOnAccess<R> {
	fn get_mut(&mut self) -> &mut R {
		loop {
			match self {
				Self::Uninitialized(f) => {
					*self = Self::Initialized((f.take().unwrap())());
				},
				Self::Initialized(ref mut r) => return r,
			}
		}
	}
}

/// Special [`ParachainConsensus`] implementation that waits for the upgrade from
/// shell to a parachain runtime that implements Aura.
struct WaitForAuraConsensus<Client, AuraId> {
	client: Arc<Client>,
	aura_consensus: Arc<Mutex<BuildOnAccess<Box<dyn ParachainConsensus<Block>>>>>,
	relay_chain_consensus: Arc<Mutex<Box<dyn ParachainConsensus<Block>>>>,
	_phantom: PhantomData<AuraId>,
}

impl<Client, AuraId> Clone for WaitForAuraConsensus<Client, AuraId> {
	fn clone(&self) -> Self {
		Self {
			client: self.client.clone(),
			aura_consensus: self.aura_consensus.clone(),
			relay_chain_consensus: self.relay_chain_consensus.clone(),
			_phantom: PhantomData,
		}
	}
}

#[async_trait::async_trait]
impl<Client, AuraId> ParachainConsensus<Block> for WaitForAuraConsensus<Client, AuraId>
where
	Client: sp_api::ProvideRuntimeApi<Block> + Send + Sync,
	Client::Api: AuraApi<Block, AuraId>,
	AuraId: Send + Codec + Sync,
{
	async fn produce_candidate(
		&mut self,
		parent: &Header,
		relay_parent: PHash,
		validation_data: &PersistedValidationData,
	) -> Option<ParachainCandidate<Block>> {
		if self
			.client
			.runtime_api()
			.has_api::<dyn AuraApi<Block, AuraId>>(parent.hash())
			.unwrap_or(false)
		{
			self.aura_consensus
				.lock()
				.await
				.get_mut()
				.produce_candidate(parent, relay_parent, validation_data)
				.await
		} else {
			self.relay_chain_consensus
				.lock()
				.await
				.produce_candidate(parent, relay_parent, validation_data)
				.await
		}
	}
}

struct Verifier<Client, AuraId> {
	client: Arc<Client>,
	aura_verifier: BuildOnAccess<Box<dyn VerifierT<Block>>>,
	relay_chain_verifier: Box<dyn VerifierT<Block>>,
	_phantom: PhantomData<AuraId>,
}

#[async_trait::async_trait]
impl<Client, AuraId> VerifierT<Block> for Verifier<Client, AuraId>
where
	Client: sp_api::ProvideRuntimeApi<Block> + Send + Sync,
	Client::Api: AuraApi<Block, AuraId>,
	AuraId: Send + Sync + Codec,
{
	async fn verify(
		&mut self,
		block_import: BlockImportParams<Block>,
	) -> Result<BlockImportParams<Block>, String> {
		if self
			.client
			.runtime_api()
			.has_api::<dyn AuraApi<Block, AuraId>>(*block_import.header.parent_hash())
			.unwrap_or(false)
		{
			self.aura_verifier.get_mut().verify(block_import).await
		} else {
			self.relay_chain_verifier.verify(block_import).await
		}
	}
}

/// Build the import queue for Aura-based runtimes.
pub fn aura_build_import_queue<RuntimeApi, AuraId: AppCrypto>(
	client: Arc<ParachainClient<RuntimeApi>>,
	block_import: ParachainBlockImport<RuntimeApi>,
	config: &Configuration,
	telemetry_handle: Option<TelemetryHandle>,
	task_manager: &TaskManager,
) -> Result<sc_consensus::DefaultImportQueue<Block>, sc_service::Error>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<Block>
		+ sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ sp_consensus_aura::AuraApi<Block, <<AuraId as AppCrypto>::Pair as Pair>::Public>,
	<<AuraId as AppCrypto>::Pair as Pair>::Signature:
		TryFrom<Vec<u8>> + std::hash::Hash + sp_runtime::traits::Member + Codec,
{
	let client2 = client.clone();

	let aura_verifier = move || {
		let slot_duration = cumulus_client_consensus_aura::slot_duration(&*client2).unwrap();

		Box::new(cumulus_client_consensus_aura::build_verifier::<
			<AuraId as AppCrypto>::Pair,
			_,
			_,
			_,
		>(cumulus_client_consensus_aura::BuildVerifierParams {
			client: client2.clone(),
			create_inherent_data_providers: move |_, _| async move {
				let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

				let slot =
							sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
								*timestamp,
								slot_duration,
							);

				Ok((slot, timestamp))
			},
			telemetry: telemetry_handle,
		})) as Box<_>
	};

	let relay_chain_verifier =
		Box::new(RelayChainVerifier::new(client.clone(), |_, _| async { Ok(()) })) as Box<_>;

	let verifier = Verifier {
		client,
		relay_chain_verifier,
		aura_verifier: BuildOnAccess::Uninitialized(Some(Box::new(aura_verifier))),
		_phantom: PhantomData,
	};

	let registry = config.prometheus_registry();
	let spawner = task_manager.spawn_essential_handle();

	Ok(BasicQueue::new(verifier, Box::new(block_import), None, &spawner, registry))
}

/// Start an aura powered parachain node. Some system chains use this.
pub async fn start_generic_aura_node<RuntimeApi, AuraId: AppCrypto>(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	para_id: ParaId,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi>>)>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<Block>
		+ sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ cumulus_primitives_core::CollectCollationInfo<Block>
		+ sp_consensus_aura::AuraApi<Block, <<AuraId as AppCrypto>::Pair as Pair>::Public>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	<<AuraId as AppCrypto>::Pair as Pair>::Signature:
		TryFrom<Vec<u8>> + std::hash::Hash + sp_runtime::traits::Member + Codec,
{
	start_node_impl::<RuntimeApi, _, _, _>(
		parachain_config,
		polkadot_config,
		collator_options,
		CollatorSybilResistance::Resistant, // Aura
		para_id,
		|_| Ok(RpcModule::new(())),
		aura_build_import_queue::<_, AuraId>,
		|client,
		 block_import,
		 prometheus_registry,
		 telemetry,
		 task_manager,
		 relay_chain_interface,
		 transaction_pool,
		 sync_oracle,
		 keystore,
		 relay_chain_slot_duration,
		 para_id,
		 collator_key,
		 overseer_handle,
		 announce_block,
		 _backend| {
			let slot_duration = cumulus_client_consensus_aura::slot_duration(&*client)?;

			let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
				task_manager.spawn_handle(),
				client.clone(),
				transaction_pool,
				prometheus_registry,
				telemetry.clone(),
			);
			let proposer = Proposer::new(proposer_factory);

			let collator_service = CollatorService::new(
				client.clone(),
				Arc::new(task_manager.spawn_handle()),
				announce_block,
				client.clone(),
			);

			let params = BasicAuraParams {
				create_inherent_data_providers: move |_, ()| async move { Ok(()) },
				block_import,
				para_client: client,
				relay_client: relay_chain_interface,
				sync_oracle,
				keystore,
				collator_key,
				para_id,
				overseer_handle,
				slot_duration,
				relay_chain_slot_duration,
				proposer,
				collator_service,
				// Very limited proposal time.
				authoring_duration: Duration::from_millis(500),
				collation_request_receiver: None,
			};

			let fut =
				basic_aura::run::<Block, <AuraId as AppCrypto>::Pair, _, _, _, _, _, _, _>(params);
			task_manager.spawn_essential_handle().spawn("aura", None, fut);

			Ok(())
		},
		hwbench,
	)
	.await
}

/// Uses the lookahead collator to support async backing.
///
/// Start an aura powered parachain node. Some system chains use this.
pub async fn start_generic_aura_lookahead_node<RuntimeApi, AuraId: AppCrypto>(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	para_id: ParaId,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi>>)>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<Block>
		+ sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ cumulus_primitives_core::CollectCollationInfo<Block>
		+ sp_consensus_aura::AuraApi<Block, <<AuraId as AppCrypto>::Pair as Pair>::Public>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>
		+ cumulus_primitives_aura::AuraUnincludedSegmentApi<Block>,
	<<AuraId as AppCrypto>::Pair as Pair>::Signature:
		TryFrom<Vec<u8>> + std::hash::Hash + sp_runtime::traits::Member + Codec,
{
	start_node_impl::<RuntimeApi, _, _, _>(
		parachain_config,
		polkadot_config,
		collator_options,
		CollatorSybilResistance::Resistant, // Aura
		para_id,
		|_| Ok(RpcModule::new(())),
		aura_build_import_queue::<_, AuraId>,
		|client,
		 block_import,
		 prometheus_registry,
		 telemetry,
		 task_manager,
		 relay_chain_interface,
		 transaction_pool,
		 sync_oracle,
		 keystore,
		 relay_chain_slot_duration,
		 para_id,
		 collator_key,
		 overseer_handle,
		 announce_block,
		 backend| {
			let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
				task_manager.spawn_handle(),
				client.clone(),
				transaction_pool,
				prometheus_registry,
				telemetry.clone(),
			);
			let proposer = Proposer::new(proposer_factory);

			let collator_service = CollatorService::new(
				client.clone(),
				Arc::new(task_manager.spawn_handle()),
				announce_block,
				client.clone(),
			);

			let params = AuraParams {
				create_inherent_data_providers: move |_, ()| async move { Ok(()) },
				block_import,
				para_client: client.clone(),
				para_backend: backend,
				relay_client: relay_chain_interface,
				code_hash_provider: move |block_hash| {
					client.code_at(block_hash).ok().map(|c| ValidationCode::from(c).hash())
				},
				sync_oracle,
				keystore,
				collator_key,
				para_id,
				overseer_handle,
				relay_chain_slot_duration,
				proposer,
				collator_service,
				authoring_duration: Duration::from_millis(1500),
				reinitialize: false,
			};

			let fut =
				aura::run::<Block, <AuraId as AppCrypto>::Pair, _, _, _, _, _, _, _, _, _>(params);
			task_manager.spawn_essential_handle().spawn("aura", None, fut);

			Ok(())
		},
		hwbench,
	)
	.await
}

/// Start a shell node which should later transition into an Aura powered parachain node. Asset Hub
/// uses this because at genesis, Asset Hub was on the `shell` runtime which didn't have Aura and
/// needs to sync and upgrade before it can run `AuraApi` functions.
pub async fn start_asset_hub_node<RuntimeApi, AuraId: AppCrypto + Send + Codec + Sync>(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	para_id: ParaId,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi>>)>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<Block>
		+ sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ cumulus_primitives_core::CollectCollationInfo<Block>
		+ sp_consensus_aura::AuraApi<Block, <<AuraId as AppCrypto>::Pair as Pair>::Public>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	<<AuraId as AppCrypto>::Pair as Pair>::Signature:
		TryFrom<Vec<u8>> + std::hash::Hash + sp_runtime::traits::Member + Codec,
{
	start_node_impl::<RuntimeApi, _, _, _>(
		parachain_config,
		polkadot_config,
		collator_options,
		CollatorSybilResistance::Resistant, // Aura
		para_id,
		|_| Ok(RpcModule::new(())),
		aura_build_import_queue::<_, AuraId>,
		|client,
		 block_import,
		 prometheus_registry,
		 telemetry,
		 task_manager,
		 relay_chain_interface,
		 transaction_pool,
		 sync_oracle,
		 keystore,
		 relay_chain_slot_duration,
		 para_id,
		 collator_key,
		 overseer_handle,
		 announce_block,
		 _backend| {
			let relay_chain_interface2 = relay_chain_interface.clone();

			let collator_service = CollatorService::new(
				client.clone(),
				Arc::new(task_manager.spawn_handle()),
				announce_block,
				client.clone(),
			);

			let spawner = task_manager.spawn_handle();

			let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
				spawner,
				client.clone(),
				transaction_pool,
				prometheus_registry,
				telemetry.clone(),
			);

			let collation_future = Box::pin(async move {
				// Start collating with the `shell` runtime while waiting for an upgrade to an Aura
				// compatible runtime.
				let mut request_stream = cumulus_client_collator::relay_chain_driven::init(
					collator_key.clone(),
					para_id,
					overseer_handle.clone(),
				)
				.await;
				while let Some(request) = request_stream.next().await {
					let pvd = request.persisted_validation_data().clone();
					let last_head_hash =
						match <Block as BlockT>::Header::decode(&mut &pvd.parent_head.0[..]) {
							Ok(header) => header.hash(),
							Err(e) => {
								log::error!("Could not decode the head data: {e}");
								request.complete(None);
								continue
							},
						};

					// Check if we have upgraded to an Aura compatible runtime and transition if
					// necessary.
					if client
						.runtime_api()
						.has_api::<dyn AuraApi<Block, AuraId>>(last_head_hash)
						.unwrap_or(false)
					{
						// Respond to this request before transitioning to Aura.
						request.complete(None);
						break
					}
				}

				// Move to Aura consensus.
				let slot_duration = match cumulus_client_consensus_aura::slot_duration(&*client) {
					Ok(d) => d,
					Err(e) => {
						log::error!("Could not get Aura slot duration: {e}");
						return
					},
				};

				let proposer = Proposer::new(proposer_factory);

				let params = BasicAuraParams {
					create_inherent_data_providers: move |_, ()| async move { Ok(()) },
					block_import,
					para_client: client,
					relay_client: relay_chain_interface2,
					sync_oracle,
					keystore,
					collator_key,
					para_id,
					overseer_handle,
					slot_duration,
					relay_chain_slot_duration,
					proposer,
					collator_service,
					// Very limited proposal time.
					authoring_duration: Duration::from_millis(500),
					collation_request_receiver: Some(request_stream),
				};

				basic_aura::run::<Block, <AuraId as AppCrypto>::Pair, _, _, _, _, _, _, _>(params)
					.await
			});

			let spawner = task_manager.spawn_essential_handle();
			spawner.spawn_essential("cumulus-asset-hub-collator", None, collation_future);

			Ok(())
		},
		hwbench,
	)
	.await
}

/// Start a shell node which should later transition into an Aura powered parachain node. Asset Hub
/// uses this because at genesis, Asset Hub was on the `shell` runtime which didn't have Aura and
/// needs to sync and upgrade before it can run `AuraApi` functions.
///
/// Uses the lookahead collator to support async backing.
#[sc_tracing::logging::prefix_logs_with("Parachain")]
pub async fn start_asset_hub_lookahead_node<RuntimeApi, AuraId: AppCrypto + Send + Codec + Sync>(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	para_id: ParaId,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi>>)>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<Block>
		+ sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ cumulus_primitives_core::CollectCollationInfo<Block>
		+ sp_consensus_aura::AuraApi<Block, <<AuraId as AppCrypto>::Pair as Pair>::Public>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>
		+ cumulus_primitives_aura::AuraUnincludedSegmentApi<Block>,
	<<AuraId as AppCrypto>::Pair as Pair>::Signature:
		TryFrom<Vec<u8>> + std::hash::Hash + sp_runtime::traits::Member + Codec,
{
	start_node_impl::<RuntimeApi, _, _, _>(
		parachain_config,
		polkadot_config,
		collator_options,
		CollatorSybilResistance::Resistant, // Aura
		para_id,
		|_| Ok(RpcModule::new(())),
		aura_build_import_queue::<_, AuraId>,
		|client,
		 block_import,
		 prometheus_registry,
		 telemetry,
		 task_manager,
		 relay_chain_interface,
		 transaction_pool,
		 sync_oracle,
		 keystore,
		 relay_chain_slot_duration,
		 para_id,
		 collator_key,
		 overseer_handle,
		 announce_block,
		 backend| {
			let relay_chain_interface2 = relay_chain_interface.clone();

			let collator_service = CollatorService::new(
				client.clone(),
				Arc::new(task_manager.spawn_handle()),
				announce_block,
				client.clone(),
			);

			let spawner = task_manager.spawn_handle();

			let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
				spawner,
				client.clone(),
				transaction_pool,
				prometheus_registry,
				telemetry.clone(),
			);

			let collation_future = Box::pin(async move {
				// Start collating with the `shell` runtime while waiting for an upgrade to an Aura
				// compatible runtime.
				let mut request_stream = cumulus_client_collator::relay_chain_driven::init(
					collator_key.clone(),
					para_id,
					overseer_handle.clone(),
				)
				.await;
				while let Some(request) = request_stream.next().await {
					let pvd = request.persisted_validation_data().clone();
					let last_head_hash =
						match <Block as BlockT>::Header::decode(&mut &pvd.parent_head.0[..]) {
							Ok(header) => header.hash(),
							Err(e) => {
								log::error!("Could not decode the head data: {e}");
								request.complete(None);
								continue
							},
						};

					// Check if we have upgraded to an Aura compatible runtime and transition if
					// necessary.
					if client
						.runtime_api()
						.has_api::<dyn AuraApi<Block, AuraId>>(last_head_hash)
						.unwrap_or(false)
					{
						// Respond to this request before transitioning to Aura.
						request.complete(None);
						break
					}
				}

				// Move to Aura consensus.
				let proposer = Proposer::new(proposer_factory);

				let params = AuraParams {
					create_inherent_data_providers: move |_, ()| async move { Ok(()) },
					block_import,
					para_client: client.clone(),
					para_backend: backend,
					relay_client: relay_chain_interface2,
					code_hash_provider: move |block_hash| {
						client.code_at(block_hash).ok().map(|c| ValidationCode::from(c).hash())
					},
					sync_oracle,
					keystore,
					collator_key,
					para_id,
					overseer_handle,
					relay_chain_slot_duration,
					proposer,
					collator_service,
					authoring_duration: Duration::from_millis(1500),
					reinitialize: true, /* we need to always re-initialize for asset-hub moving
					                     * to aura */
				};

				aura::run::<Block, <AuraId as AppCrypto>::Pair, _, _, _, _, _, _, _, _, _>(params)
					.await
			});

			let spawner = task_manager.spawn_essential_handle();
			spawner.spawn_essential("cumulus-asset-hub-collator", None, collation_future);

			Ok(())
		},
		hwbench,
	)
	.await
}

/// Start an aura powered parachain node which uses the lookahead collator to support async backing.
/// This node is basic in the sense that its runtime api doesn't include common contents such as
/// transaction payment. Used for aura glutton.
pub async fn start_basic_lookahead_node<RuntimeApi, AuraId: AppCrypto>(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	para_id: ParaId,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi>>)>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<Block>
		+ sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ cumulus_primitives_core::CollectCollationInfo<Block>
		+ sp_consensus_aura::AuraApi<Block, <<AuraId as AppCrypto>::Pair as Pair>::Public>
		+ frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>
		+ cumulus_primitives_aura::AuraUnincludedSegmentApi<Block>,
	<<AuraId as AppCrypto>::Pair as Pair>::Signature:
		TryFrom<Vec<u8>> + std::hash::Hash + sp_runtime::traits::Member + Codec,
{
	start_basic_lookahead_node_impl::<RuntimeApi, _, _, _>(
		parachain_config,
		polkadot_config,
		collator_options,
		CollatorSybilResistance::Resistant, // Aura
		para_id,
		|_| Ok(RpcModule::new(())),
		aura_build_import_queue::<_, AuraId>,
		|client,
		 block_import,
		 prometheus_registry,
		 telemetry,
		 task_manager,
		 relay_chain_interface,
		 transaction_pool,
		 sync_oracle,
		 keystore,
		 relay_chain_slot_duration,
		 para_id,
		 collator_key,
		 overseer_handle,
		 announce_block,
		 backend| {
			let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
				task_manager.spawn_handle(),
				client.clone(),
				transaction_pool,
				prometheus_registry,
				telemetry.clone(),
			);
			let proposer = Proposer::new(proposer_factory);

			let collator_service = CollatorService::new(
				client.clone(),
				Arc::new(task_manager.spawn_handle()),
				announce_block,
				client.clone(),
			);

			let params = AuraParams {
				create_inherent_data_providers: move |_, ()| async move { Ok(()) },
				block_import,
				para_client: client.clone(),
				para_backend: backend.clone(),
				relay_client: relay_chain_interface,
				code_hash_provider: move |block_hash| {
					client.code_at(block_hash).ok().map(|c| ValidationCode::from(c).hash())
				},
				sync_oracle,
				keystore,
				collator_key,
				para_id,
				overseer_handle,
				relay_chain_slot_duration,
				proposer,
				collator_service,
				authoring_duration: Duration::from_millis(1500),
				reinitialize: false,
			};

			let fut =
				aura::run::<Block, <AuraId as AppCrypto>::Pair, _, _, _, _, _, _, _, _, _>(params);
			task_manager.spawn_essential_handle().spawn("aura", None, fut);

			Ok(())
		},
		hwbench,
	)
	.await
}

#[sc_tracing::logging::prefix_logs_with("Parachain")]
async fn start_contracts_rococo_node_impl<RuntimeApi, RB, BIQ, SC>(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	sybil_resistance_level: CollatorSybilResistance,
	para_id: ParaId,
	_rpc_ext_builder: RB,
	build_import_queue: BIQ,
	start_consensus: SC,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi>>)>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<Block>
		+ sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ cumulus_primitives_core::CollectCollationInfo<Block>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>
		+ cumulus_primitives_aura::AuraUnincludedSegmentApi<Block>,
	RB: Fn(Arc<ParachainClient<RuntimeApi>>) -> Result<jsonrpsee::RpcModule<()>, sc_service::Error>,
	BIQ: FnOnce(
		Arc<ParachainClient<RuntimeApi>>,
		ParachainBlockImport<RuntimeApi>,
		&Configuration,
		Option<TelemetryHandle>,
		&TaskManager,
	) -> Result<sc_consensus::DefaultImportQueue<Block>, sc_service::Error>,
	SC: FnOnce(
		Arc<ParachainClient<RuntimeApi>>,
		ParachainBlockImport<RuntimeApi>,
		Option<&Registry>,
		Option<TelemetryHandle>,
		&TaskManager,
		Arc<dyn RelayChainInterface>,
		Arc<sc_transaction_pool::FullPool<Block, ParachainClient<RuntimeApi>>>,
		Arc<SyncingService<Block>>,
		KeystorePtr,
		Duration,
		ParaId,
		CollatorPair,
		OverseerHandle,
		Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
		Arc<ParachainBackend>,
	) -> Result<(), sc_service::Error>,
{
	let parachain_config = prepare_node_config(parachain_config);

	let params = new_partial::<RuntimeApi, BIQ>(&parachain_config, build_import_queue)?;
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
	let net_config = FullNetworkConfiguration::new(&parachain_config.network);

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
			sybil_resistance_level,
		})
		.await?;

	let rpc_builder = {
		let client = client.clone();
		let transaction_pool = transaction_pool.clone();

		Box::new(move |deny_unsafe, _| {
			let deps = crate::rpc::FullDeps {
				client: client.clone(),
				pool: transaction_pool.clone(),
				deny_unsafe,
			};

			crate::rpc::create_contracts_rococo(deps).map_err(Into::into)
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
		sync_service: sync_service.clone(),
	})?;

	if validator {
		start_consensus(
			client.clone(),
			block_import,
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|t| t.handle()),
			&task_manager,
			relay_chain_interface.clone(),
			transaction_pool,
			sync_service.clone(),
			params.keystore_container.keystore(),
			relay_chain_slot_duration,
			para_id,
			collator_key.expect("Command line arguments do not allow this. qed"),
			overseer_handle,
			announce_block,
			backend.clone(),
		)?;
	}

	start_network.start_network();

	Ok((task_manager, client))
}

#[allow(clippy::type_complexity)]
pub fn contracts_rococo_build_import_queue(
	client: Arc<ParachainClient<RuntimeApi>>,
	block_import: ParachainBlockImport<RuntimeApi>,
	config: &Configuration,
	telemetry: Option<TelemetryHandle>,
	task_manager: &TaskManager,
) -> Result<sc_consensus::DefaultImportQueue<Block>, sc_service::Error> {
	let slot_duration = cumulus_client_consensus_aura::slot_duration(&*client)?;

	cumulus_client_consensus_aura::import_queue::<
		sp_consensus_aura::sr25519::AuthorityPair,
		_,
		_,
		_,
		_,
		_,
	>(cumulus_client_consensus_aura::ImportQueueParams {
		block_import,
		client,
		create_inherent_data_providers: move |_, _| async move {
			let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

			let slot =
				sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
					*timestamp,
					slot_duration,
				);

			Ok((slot, timestamp))
		},
		registry: config.prometheus_registry(),
		spawner: &task_manager.spawn_essential_handle(),
		telemetry,
	})
	.map_err(Into::into)
}

/// Start a parachain node.
pub async fn start_contracts_rococo_node(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	para_id: ParaId,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi>>)> {
	start_contracts_rococo_node_impl::<RuntimeApi, _, _, _>(
		parachain_config,
		polkadot_config,
		collator_options,
		CollatorSybilResistance::Resistant, // Aura
		para_id,
		|_| Ok(RpcModule::new(())),
		contracts_rococo_build_import_queue,
		|client,
		 block_import,
		 prometheus_registry,
		 telemetry,
		 task_manager,
		 relay_chain_interface,
		 transaction_pool,
		 sync_oracle,
		 keystore,
		 relay_chain_slot_duration,
		 para_id,
		 collator_key,
		 overseer_handle,
		 announce_block,
		 backend| {
			let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
				task_manager.spawn_handle(),
				client.clone(),
				transaction_pool,
				prometheus_registry,
				telemetry.clone(),
			);
			let proposer = Proposer::new(proposer_factory);

			let collator_service = CollatorService::new(
				client.clone(),
				Arc::new(task_manager.spawn_handle()),
				announce_block,
				client.clone(),
			);

			let params = AuraParams {
				create_inherent_data_providers: move |_, ()| async move { Ok(()) },
				block_import,
				para_client: client.clone(),
				para_backend: backend.clone(),
				relay_client: relay_chain_interface,
				code_hash_provider: move |block_hash| {
					client.code_at(block_hash).ok().map(|c| ValidationCode::from(c).hash())
				},
				sync_oracle,
				keystore,
				collator_key,
				para_id,
				overseer_handle,
				relay_chain_slot_duration,
				proposer,
				collator_service,
				// Very limited proposal time.
				authoring_duration: Duration::from_millis(1500),
				reinitialize: false,
			};

			let fut = aura::run::<
				Block,
				sp_consensus_aura::sr25519::AuthorityPair,
				_,
				_,
				_,
				_,
				_,
				_,
				_,
				_,
				_,
			>(params);
			task_manager.spawn_essential_handle().spawn("aura", None, fut);

			Ok(())
		},
		hwbench,
	)
	.await
}

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
