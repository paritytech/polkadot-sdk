// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{
	build_network_future, build_system_rpc_future,
	client::{Client, ClientConfig},
	config::{Configuration, ExecutorConfiguration, KeystoreConfig, Multiaddr, PrometheusConfig},
	error::Error,
	metrics::MetricsService,
	start_rpc_servers, BuildGenesisBlock, GenesisBlockBuilder, RpcHandlers, SpawnTaskHandle,
	TaskManager, TransactionPoolAdapter,
};
use futures::{select, FutureExt, StreamExt};
use jsonrpsee::RpcModule;
use log::info;
use prometheus_endpoint::Registry;
use sc_chain_spec::{get_extension, ChainSpec};
use sc_client_api::{
	execution_extensions::ExecutionExtensions, proof_provider::ProofProvider, BadBlocks,
	BlockBackend, BlockchainEvents, ExecutorProvider, ForkBlocks, StorageProvider, UsageProvider,
};
use sc_client_db::{Backend, BlocksPruning, DatabaseSettings, PruningMode};
use sc_consensus::import_queue::{ImportQueue, ImportQueueService};
use sc_executor::{
	sp_wasm_interface::HostFunctions, HeapAllocStrategy, NativeExecutionDispatch, RuntimeVersionOf,
	WasmExecutor, DEFAULT_HEAP_ALLOC_STRATEGY,
};
use sc_keystore::LocalKeystore;
use sc_network::{
	config::{FullNetworkConfiguration, ProtocolId, SyncMode},
	multiaddr::Protocol,
	service::{
		traits::{PeerStore, RequestResponseConfig},
		NotificationMetrics,
	},
	NetworkBackend, NetworkStateInfo,
};
use sc_network_common::role::{Role, Roles};
use sc_network_light::light_client_requests::handler::LightClientRequestHandler;
use sc_network_sync::{
	block_relay_protocol::{BlockDownloader, BlockRelayParams},
	block_request_handler::BlockRequestHandler,
	engine::SyncingEngine,
	service::network::{NetworkServiceHandle, NetworkServiceProvider},
	state_request_handler::StateRequestHandler,
	strategy::{
		polkadot::{PolkadotSyncingStrategy, PolkadotSyncingStrategyConfig},
		SyncingStrategy,
	},
	warp_request_handler::RequestHandler as WarpSyncRequestHandler,
	SyncingService, WarpSyncConfig,
};
use sc_rpc::{
	author::AuthorApiServer,
	chain::ChainApiServer,
	offchain::OffchainApiServer,
	state::{ChildStateApiServer, StateApiServer},
	system::SystemApiServer,
	DenyUnsafe, SubscriptionTaskExecutor,
};
use sc_rpc_spec_v2::{
	archive::ArchiveApiServer,
	chain_head::ChainHeadApiServer,
	chain_spec::ChainSpecApiServer,
	transaction::{TransactionApiServer, TransactionBroadcastApiServer},
};
use sc_telemetry::{telemetry, ConnectionMessage, Telemetry, TelemetryHandle, SUBSTRATE_INFO};
use sc_transaction_pool_api::{MaintainedTransactionPool, TransactionPool};
use sc_utils::mpsc::{tracing_unbounded, TracingUnboundedSender};
use sp_api::{CallApiAt, ProvideRuntimeApi};
use sp_blockchain::{HeaderBackend, HeaderMetadata};
use sp_consensus::block_validation::{
	BlockAnnounceValidator, Chain, DefaultBlockAnnounceValidator,
};
use sp_core::traits::{CodeExecutor, SpawnNamed};
use sp_keystore::KeystorePtr;
use sp_runtime::traits::{Block as BlockT, BlockIdTo, NumberFor, Zero};
use std::{
	str::FromStr,
	sync::Arc,
	time::{Duration, SystemTime},
};

/// Full client type.
pub type TFullClient<TBl, TRtApi, TExec> =
	Client<TFullBackend<TBl>, TFullCallExecutor<TBl, TExec>, TBl, TRtApi>;

/// Full client backend type.
pub type TFullBackend<TBl> = Backend<TBl>;

/// Full client call executor type.
pub type TFullCallExecutor<TBl, TExec> = crate::client::LocalCallExecutor<TBl, Backend<TBl>, TExec>;

type TFullParts<TBl, TRtApi, TExec> =
	(TFullClient<TBl, TRtApi, TExec>, Arc<TFullBackend<TBl>>, KeystoreContainer, TaskManager);

/// Construct a local keystore shareable container
pub struct KeystoreContainer(Arc<LocalKeystore>);

impl KeystoreContainer {
	/// Construct KeystoreContainer
	pub fn new(config: &KeystoreConfig) -> Result<Self, Error> {
		let keystore = Arc::new(match config {
			KeystoreConfig::Path { path, password } =>
				LocalKeystore::open(path.clone(), password.clone())?,
			KeystoreConfig::InMemory => LocalKeystore::in_memory(),
		});

		Ok(Self(keystore))
	}

	/// Returns a shared reference to a dynamic `Keystore` trait implementation.
	pub fn keystore(&self) -> KeystorePtr {
		self.0.clone()
	}

	/// Returns a shared reference to the local keystore .
	pub fn local_keystore(&self) -> Arc<LocalKeystore> {
		self.0.clone()
	}
}

/// Creates a new full client for the given config.
pub fn new_full_client<TBl, TRtApi, TExec>(
	config: &Configuration,
	telemetry: Option<TelemetryHandle>,
	executor: TExec,
) -> Result<TFullClient<TBl, TRtApi, TExec>, Error>
where
	TBl: BlockT,
	TExec: CodeExecutor + RuntimeVersionOf + Clone,
{
	new_full_parts(config, telemetry, executor).map(|parts| parts.0)
}

/// Create the initial parts of a full node with the default genesis block builder.
pub fn new_full_parts_record_import<TBl, TRtApi, TExec>(
	config: &Configuration,
	telemetry: Option<TelemetryHandle>,
	executor: TExec,
	enable_import_proof_recording: bool,
) -> Result<TFullParts<TBl, TRtApi, TExec>, Error>
where
	TBl: BlockT,
	TExec: CodeExecutor + RuntimeVersionOf + Clone,
{
	let backend = new_db_backend(config.db_config())?;

	let genesis_block_builder = GenesisBlockBuilder::new(
		config.chain_spec.as_storage_builder(),
		!config.no_genesis(),
		backend.clone(),
		executor.clone(),
	)?;

	new_full_parts_with_genesis_builder(
		config,
		telemetry,
		executor,
		backend,
		genesis_block_builder,
		enable_import_proof_recording,
	)
}
/// Create the initial parts of a full node with the default genesis block builder.
pub fn new_full_parts<TBl, TRtApi, TExec>(
	config: &Configuration,
	telemetry: Option<TelemetryHandle>,
	executor: TExec,
) -> Result<TFullParts<TBl, TRtApi, TExec>, Error>
where
	TBl: BlockT,
	TExec: CodeExecutor + RuntimeVersionOf + Clone,
{
	new_full_parts_record_import(config, telemetry, executor, false)
}

/// Create the initial parts of a full node.
pub fn new_full_parts_with_genesis_builder<TBl, TRtApi, TExec, TBuildGenesisBlock>(
	config: &Configuration,
	telemetry: Option<TelemetryHandle>,
	executor: TExec,
	backend: Arc<TFullBackend<TBl>>,
	genesis_block_builder: TBuildGenesisBlock,
	enable_import_proof_recording: bool,
) -> Result<TFullParts<TBl, TRtApi, TExec>, Error>
where
	TBl: BlockT,
	TExec: CodeExecutor + RuntimeVersionOf + Clone,
	TBuildGenesisBlock: BuildGenesisBlock<
		TBl,
		BlockImportOperation = <Backend<TBl> as sc_client_api::backend::Backend<TBl>>::BlockImportOperation
	>,
{
	let keystore_container = KeystoreContainer::new(&config.keystore)?;

	let task_manager = {
		let registry = config.prometheus_config.as_ref().map(|cfg| &cfg.registry);
		TaskManager::new(config.tokio_handle.clone(), registry)?
	};

	let chain_spec = &config.chain_spec;
	let fork_blocks = get_extension::<ForkBlocks<TBl>>(chain_spec.extensions())
		.cloned()
		.unwrap_or_default();

	let bad_blocks = get_extension::<BadBlocks<TBl>>(chain_spec.extensions())
		.cloned()
		.unwrap_or_default();

	let client = {
		let extensions = ExecutionExtensions::new(None, Arc::new(executor.clone()));

		let wasm_runtime_substitutes = config
			.chain_spec
			.code_substitutes()
			.into_iter()
			.map(|(n, c)| {
				let number = NumberFor::<TBl>::from_str(&n).map_err(|_| {
					Error::Application(Box::from(format!(
						"Failed to parse `{}` as block number for code substitutes. \
						 In an old version the key for code substitute was a block hash. \
						 Please update the chain spec to a version that is compatible with your node.",
						n
					)))
				})?;
				Ok((number, c))
			})
			.collect::<Result<std::collections::HashMap<_, _>, Error>>()?;

		let client = new_client(
			backend.clone(),
			executor,
			genesis_block_builder,
			fork_blocks,
			bad_blocks,
			extensions,
			Box::new(task_manager.spawn_handle()),
			config.prometheus_config.as_ref().map(|config| config.registry.clone()),
			telemetry,
			ClientConfig {
				offchain_worker_enabled: config.offchain_worker.enabled,
				offchain_indexing_api: config.offchain_worker.indexing_enabled,
				wasm_runtime_overrides: config.wasm_runtime_overrides.clone(),
				no_genesis: config.no_genesis(),
				wasm_runtime_substitutes,
				enable_import_proof_recording,
			},
		)?;

		client
	};

	Ok((client, backend, keystore_container, task_manager))
}

/// Creates a [`NativeElseWasmExecutor`](sc_executor::NativeElseWasmExecutor) according to
/// [`Configuration`].
#[deprecated(note = "Please switch to `new_wasm_executor`. Will be removed at end of 2024.")]
#[allow(deprecated)]
pub fn new_native_or_wasm_executor<D: NativeExecutionDispatch>(
	config: &Configuration,
) -> sc_executor::NativeElseWasmExecutor<D> {
	#[allow(deprecated)]
	sc_executor::NativeElseWasmExecutor::new_with_wasm_executor(new_wasm_executor(&config.executor))
}

/// Creates a [`WasmExecutor`] according to [`ExecutorConfiguration`].
pub fn new_wasm_executor<H: HostFunctions>(config: &ExecutorConfiguration) -> WasmExecutor<H> {
	let strategy = config
		.default_heap_pages
		.map_or(DEFAULT_HEAP_ALLOC_STRATEGY, |p| HeapAllocStrategy::Static { extra_pages: p as _ });
	WasmExecutor::<H>::builder()
		.with_execution_method(config.wasm_method)
		.with_onchain_heap_alloc_strategy(strategy)
		.with_offchain_heap_alloc_strategy(strategy)
		.with_max_runtime_instances(config.max_runtime_instances)
		.with_runtime_cache_size(config.runtime_cache_size)
		.build()
}

/// Create an instance of default DB-backend backend.
pub fn new_db_backend<Block>(
	settings: DatabaseSettings,
) -> Result<Arc<Backend<Block>>, sp_blockchain::Error>
where
	Block: BlockT,
{
	const CANONICALIZATION_DELAY: u64 = 4096;

	Ok(Arc::new(Backend::new(settings, CANONICALIZATION_DELAY)?))
}

/// Create an instance of client backed by given backend.
pub fn new_client<E, Block, RA, G>(
	backend: Arc<Backend<Block>>,
	executor: E,
	genesis_block_builder: G,
	fork_blocks: ForkBlocks<Block>,
	bad_blocks: BadBlocks<Block>,
	execution_extensions: ExecutionExtensions<Block>,
	spawn_handle: Box<dyn SpawnNamed>,
	prometheus_registry: Option<Registry>,
	telemetry: Option<TelemetryHandle>,
	config: ClientConfig<Block>,
) -> Result<
	Client<
		Backend<Block>,
		crate::client::LocalCallExecutor<Block, Backend<Block>, E>,
		Block,
		RA,
	>,
	sp_blockchain::Error,
>
where
	Block: BlockT,
	E: CodeExecutor + RuntimeVersionOf,
	G: BuildGenesisBlock<
		Block,
		BlockImportOperation = <Backend<Block> as sc_client_api::backend::Backend<Block>>::BlockImportOperation
	>,
{
	let executor = crate::client::LocalCallExecutor::new(
		backend.clone(),
		executor,
		config.clone(),
		execution_extensions,
	)?;

	Client::new(
		backend,
		executor,
		spawn_handle,
		genesis_block_builder,
		fork_blocks,
		bad_blocks,
		prometheus_registry,
		telemetry,
		config,
	)
}

/// Parameters to pass into `build`.
pub struct SpawnTasksParams<'a, TBl: BlockT, TCl, TExPool, TRpc, Backend> {
	/// The service configuration.
	pub config: Configuration,
	/// A shared client returned by `new_full_parts`.
	pub client: Arc<TCl>,
	/// A shared backend returned by `new_full_parts`.
	pub backend: Arc<Backend>,
	/// A task manager returned by `new_full_parts`.
	pub task_manager: &'a mut TaskManager,
	/// A shared keystore returned by `new_full_parts`.
	pub keystore: KeystorePtr,
	/// A shared transaction pool.
	pub transaction_pool: Arc<TExPool>,
	/// Builds additional [`RpcModule`]s that should be added to the server
	pub rpc_builder: Box<dyn Fn(SubscriptionTaskExecutor) -> Result<RpcModule<TRpc>, Error>>,
	/// A shared network instance.
	pub network: Arc<dyn sc_network::service::traits::NetworkService>,
	/// A Sender for RPC requests.
	pub system_rpc_tx: TracingUnboundedSender<sc_rpc::system::Request<TBl>>,
	/// Controller for transactions handlers
	pub tx_handler_controller:
		sc_network_transactions::TransactionsHandlerController<<TBl as BlockT>::Hash>,
	/// Syncing service.
	pub sync_service: Arc<SyncingService<TBl>>,
	/// Telemetry instance for this node.
	pub telemetry: Option<&'a mut Telemetry>,
}

/// Spawn the tasks that are required to run a node.
pub fn spawn_tasks<TBl, TBackend, TExPool, TRpc, TCl>(
	params: SpawnTasksParams<TBl, TCl, TExPool, TRpc, TBackend>,
) -> Result<RpcHandlers, Error>
where
	TCl: ProvideRuntimeApi<TBl>
		+ HeaderMetadata<TBl, Error = sp_blockchain::Error>
		+ Chain<TBl>
		+ BlockBackend<TBl>
		+ BlockIdTo<TBl, Error = sp_blockchain::Error>
		+ ProofProvider<TBl>
		+ HeaderBackend<TBl>
		+ BlockchainEvents<TBl>
		+ ExecutorProvider<TBl>
		+ UsageProvider<TBl>
		+ StorageProvider<TBl, TBackend>
		+ CallApiAt<TBl>
		+ Send
		+ 'static,
	<TCl as ProvideRuntimeApi<TBl>>::Api: sp_api::Metadata<TBl>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<TBl>
		+ sp_session::SessionKeys<TBl>
		+ sp_api::ApiExt<TBl>,
	TBl: BlockT,
	TBl::Hash: Unpin,
	TBl::Header: Unpin,
	TBackend: 'static + sc_client_api::backend::Backend<TBl> + Send,
	TExPool: MaintainedTransactionPool<Block = TBl, Hash = <TBl as BlockT>::Hash> + 'static,
{
	let SpawnTasksParams {
		mut config,
		task_manager,
		client,
		backend,
		keystore,
		transaction_pool,
		rpc_builder,
		network,
		system_rpc_tx,
		tx_handler_controller,
		sync_service,
		telemetry,
	} = params;

	let chain_info = client.usage_info().chain;

	sp_session::generate_initial_session_keys(
		client.clone(),
		chain_info.best_hash,
		config.dev_key_seed.clone().map(|s| vec![s]).unwrap_or_default(),
		keystore.clone(),
	)
	.map_err(|e| Error::Application(Box::new(e)))?;

	let sysinfo = sc_sysinfo::gather_sysinfo();
	sc_sysinfo::print_sysinfo(&sysinfo);

	let telemetry = telemetry
		.map(|telemetry| {
			init_telemetry(
				config.network.node_name.clone(),
				config.impl_name.clone(),
				config.impl_version.clone(),
				config.chain_spec.name().to_string(),
				config.role.is_authority(),
				network.clone(),
				client.clone(),
				telemetry,
				Some(sysinfo),
			)
		})
		.transpose()?;

	info!("📦 Highest known block at #{}", chain_info.best_number);

	let spawn_handle = task_manager.spawn_handle();

	// Inform the tx pool about imported and finalized blocks.
	spawn_handle.spawn(
		"txpool-notifications",
		Some("transaction-pool"),
		sc_transaction_pool::notification_future(client.clone(), transaction_pool.clone()),
	);

	spawn_handle.spawn(
		"on-transaction-imported",
		Some("transaction-pool"),
		propagate_transaction_notifications(
			transaction_pool.clone(),
			tx_handler_controller,
			telemetry.clone(),
		),
	);

	// Prometheus metrics.
	let metrics_service =
		if let Some(PrometheusConfig { port, registry }) = config.prometheus_config.clone() {
			// Set static metrics.
			let metrics = MetricsService::with_prometheus(
				telemetry,
				&registry,
				config.role,
				&config.network.node_name,
				&config.impl_version,
			)?;
			spawn_handle.spawn(
				"prometheus-endpoint",
				None,
				prometheus_endpoint::init_prometheus(port, registry).map(drop),
			);

			metrics
		} else {
			MetricsService::new(telemetry)
		};

	// Periodically updated metrics and telemetry updates.
	spawn_handle.spawn(
		"telemetry-periodic-send",
		None,
		metrics_service.run(
			client.clone(),
			transaction_pool.clone(),
			network.clone(),
			sync_service.clone(),
		),
	);

	let rpc_id_provider = config.rpc.id_provider.take();

	// jsonrpsee RPC
	let gen_rpc_module = || {
		gen_rpc_module(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool.clone(),
			keystore.clone(),
			system_rpc_tx.clone(),
			config.impl_name.clone(),
			config.impl_version.clone(),
			config.chain_spec.as_ref(),
			&config.state_pruning,
			config.blocks_pruning,
			backend.clone(),
			&*rpc_builder,
		)
	};

	let rpc_server_handle = start_rpc_servers(
		&config.rpc,
		config.prometheus_registry(),
		&config.tokio_handle,
		gen_rpc_module,
		rpc_id_provider,
	)?;

	let listen_addrs = rpc_server_handle
		.listen_addrs()
		.into_iter()
		.map(|socket_addr| {
			let mut multiaddr: Multiaddr = socket_addr.ip().into();
			multiaddr.push(Protocol::Tcp(socket_addr.port()));
			multiaddr
		})
		.collect();

	let in_memory_rpc = {
		let mut module = gen_rpc_module()?;
		module.extensions_mut().insert(DenyUnsafe::No);
		module
	};

	let in_memory_rpc_handle = RpcHandlers::new(Arc::new(in_memory_rpc), listen_addrs);

	// Spawn informant task
	spawn_handle.spawn(
		"informant",
		None,
		sc_informant::build(client.clone(), network, sync_service.clone()),
	);

	task_manager.keep_alive((config.base_path, rpc_server_handle));

	Ok(in_memory_rpc_handle)
}

/// Returns a future that forwards imported transactions to the transaction networking protocol.
pub async fn propagate_transaction_notifications<Block, ExPool>(
	transaction_pool: Arc<ExPool>,
	tx_handler_controller: sc_network_transactions::TransactionsHandlerController<
		<Block as BlockT>::Hash,
	>,
	telemetry: Option<TelemetryHandle>,
) where
	Block: BlockT,
	ExPool: MaintainedTransactionPool<Block = Block, Hash = <Block as BlockT>::Hash>,
{
	const TELEMETRY_INTERVAL: Duration = Duration::from_secs(1);

	// transaction notifications
	let mut notifications = transaction_pool.import_notification_stream().fuse();
	let mut timer = futures_timer::Delay::new(TELEMETRY_INTERVAL).fuse();
	let mut tx_imported = false;

	loop {
		select! {
			notification = notifications.next() => {
				let Some(hash) = notification else { return };

				tx_handler_controller.propagate_transaction(hash);

				tx_imported = true;
			},
			_ = timer => {
				timer = futures_timer::Delay::new(TELEMETRY_INTERVAL).fuse();

				if !tx_imported {
					continue;
				}

				tx_imported = false;
				let status = transaction_pool.status();

				telemetry!(
					telemetry;
					SUBSTRATE_INFO;
					"txpool.import";
					"ready" => status.ready,
					"future" => status.future,
				);
			}
		}
	}
}

/// Initialize telemetry with provided configuration and return telemetry handle
pub fn init_telemetry<Block, Client, Network>(
	name: String,
	implementation: String,
	version: String,
	chain: String,
	authority: bool,
	network: Network,
	client: Arc<Client>,
	telemetry: &mut Telemetry,
	sysinfo: Option<sc_telemetry::SysInfo>,
) -> sc_telemetry::Result<TelemetryHandle>
where
	Block: BlockT,
	Client: BlockBackend<Block>,
	Network: NetworkStateInfo,
{
	let genesis_hash = client.block_hash(Zero::zero()).ok().flatten().unwrap_or_default();
	let connection_message = ConnectionMessage {
		name,
		implementation,
		version,
		target_os: sc_sysinfo::TARGET_OS.into(),
		target_arch: sc_sysinfo::TARGET_ARCH.into(),
		target_env: sc_sysinfo::TARGET_ENV.into(),
		config: String::new(),
		chain,
		genesis_hash: format!("{:?}", genesis_hash),
		authority,
		startup_time: SystemTime::UNIX_EPOCH
			.elapsed()
			.map(|dur| dur.as_millis())
			.unwrap_or(0)
			.to_string(),
		network_id: network.local_peer_id().to_base58(),
		sysinfo,
	};

	telemetry.start_telemetry(connection_message)?;

	Ok(telemetry.handle())
}

/// Generate RPC module using provided configuration
pub fn gen_rpc_module<TBl, TBackend, TCl, TRpc, TExPool>(
	spawn_handle: SpawnTaskHandle,
	client: Arc<TCl>,
	transaction_pool: Arc<TExPool>,
	keystore: KeystorePtr,
	system_rpc_tx: TracingUnboundedSender<sc_rpc::system::Request<TBl>>,
	impl_name: String,
	impl_version: String,
	chain_spec: &dyn ChainSpec,
	state_pruning: &Option<PruningMode>,
	blocks_pruning: BlocksPruning,
	backend: Arc<TBackend>,
	rpc_builder: &(dyn Fn(SubscriptionTaskExecutor) -> Result<RpcModule<TRpc>, Error>),
) -> Result<RpcModule<()>, Error>
where
	TBl: BlockT,
	TCl: ProvideRuntimeApi<TBl>
		+ BlockchainEvents<TBl>
		+ HeaderBackend<TBl>
		+ HeaderMetadata<TBl, Error = sp_blockchain::Error>
		+ ExecutorProvider<TBl>
		+ CallApiAt<TBl>
		+ ProofProvider<TBl>
		+ StorageProvider<TBl, TBackend>
		+ BlockBackend<TBl>
		+ Send
		+ Sync
		+ 'static,
	TBackend: sc_client_api::backend::Backend<TBl> + 'static,
	<TCl as ProvideRuntimeApi<TBl>>::Api: sp_session::SessionKeys<TBl> + sp_api::Metadata<TBl>,
	TExPool: MaintainedTransactionPool<Block = TBl, Hash = <TBl as BlockT>::Hash> + 'static,
	TBl::Hash: Unpin,
	TBl::Header: Unpin,
{
	let system_info = sc_rpc::system::SystemInfo {
		chain_name: chain_spec.name().into(),
		impl_name,
		impl_version,
		properties: chain_spec.properties(),
		chain_type: chain_spec.chain_type(),
	};

	let mut rpc_api = RpcModule::new(());
	let task_executor = Arc::new(spawn_handle);

	let (chain, state, child_state) = {
		let chain = sc_rpc::chain::new_full(client.clone(), task_executor.clone()).into_rpc();
		let (state, child_state) = sc_rpc::state::new_full(client.clone(), task_executor.clone());
		let state = state.into_rpc();
		let child_state = child_state.into_rpc();

		(chain, state, child_state)
	};

	const MAX_TRANSACTION_PER_CONNECTION: usize = 16;

	let transaction_broadcast_rpc_v2 = sc_rpc_spec_v2::transaction::TransactionBroadcast::new(
		client.clone(),
		transaction_pool.clone(),
		task_executor.clone(),
		MAX_TRANSACTION_PER_CONNECTION,
	)
	.into_rpc();

	let transaction_v2 = sc_rpc_spec_v2::transaction::Transaction::new(
		client.clone(),
		transaction_pool.clone(),
		task_executor.clone(),
	)
	.into_rpc();

	let chain_head_v2 = sc_rpc_spec_v2::chain_head::ChainHead::new(
		client.clone(),
		backend.clone(),
		task_executor.clone(),
		// Defaults to sensible limits for the `ChainHead`.
		sc_rpc_spec_v2::chain_head::ChainHeadConfig::default(),
	)
	.into_rpc();

	// Part of the RPC v2 spec.
	// An archive node that can respond to the `archive` RPC-v2 queries is a node with:
	// - state pruning in archive mode: The storage of blocks is kept around
	// - block pruning in archive mode: The block's body is kept around
	let is_archive_node = state_pruning.as_ref().map(|sp| sp.is_archive()).unwrap_or(false) &&
		blocks_pruning.is_archive();
	let genesis_hash = client.hash(Zero::zero()).ok().flatten().expect("Genesis block exists; qed");
	if is_archive_node {
		let archive_v2 = sc_rpc_spec_v2::archive::Archive::new(
			client.clone(),
			backend.clone(),
			genesis_hash,
			task_executor.clone(),
		)
		.into_rpc();
		rpc_api.merge(archive_v2).map_err(|e| Error::Application(e.into()))?;
	}

	// ChainSpec RPC-v2.
	let chain_spec_v2 = sc_rpc_spec_v2::chain_spec::ChainSpec::new(
		chain_spec.name().into(),
		genesis_hash,
		chain_spec.properties(),
	)
	.into_rpc();

	let author = sc_rpc::author::Author::new(
		client.clone(),
		transaction_pool,
		keystore,
		task_executor.clone(),
	)
	.into_rpc();

	let system = sc_rpc::system::System::new(system_info, system_rpc_tx).into_rpc();

	if let Some(storage) = backend.offchain_storage() {
		let offchain = sc_rpc::offchain::Offchain::new(storage).into_rpc();

		rpc_api.merge(offchain).map_err(|e| Error::Application(e.into()))?;
	}

	// Part of the RPC v2 spec.
	rpc_api.merge(transaction_v2).map_err(|e| Error::Application(e.into()))?;
	rpc_api
		.merge(transaction_broadcast_rpc_v2)
		.map_err(|e| Error::Application(e.into()))?;
	rpc_api.merge(chain_head_v2).map_err(|e| Error::Application(e.into()))?;
	rpc_api.merge(chain_spec_v2).map_err(|e| Error::Application(e.into()))?;

	// Part of the old RPC spec.
	rpc_api.merge(chain).map_err(|e| Error::Application(e.into()))?;
	rpc_api.merge(author).map_err(|e| Error::Application(e.into()))?;
	rpc_api.merge(system).map_err(|e| Error::Application(e.into()))?;
	rpc_api.merge(state).map_err(|e| Error::Application(e.into()))?;
	rpc_api.merge(child_state).map_err(|e| Error::Application(e.into()))?;
	// Additional [`RpcModule`]s defined in the node to fit the specific blockchain
	let extra_rpcs = rpc_builder(task_executor.clone())?;
	rpc_api.merge(extra_rpcs).map_err(|e| Error::Application(e.into()))?;

	Ok(rpc_api)
}

/// Parameters to pass into [`build_network`].
pub struct BuildNetworkParams<'a, Block, Net, TxPool, IQ, Client>
where
	Block: BlockT,
	Net: NetworkBackend<Block, <Block as BlockT>::Hash>,
{
	/// The service configuration.
	pub config: &'a Configuration,
	/// Full network configuration.
	pub net_config: FullNetworkConfiguration<Block, <Block as BlockT>::Hash, Net>,
	/// A shared client returned by `new_full_parts`.
	pub client: Arc<Client>,
	/// A shared transaction pool.
	pub transaction_pool: Arc<TxPool>,
	/// A handle for spawning tasks.
	pub spawn_handle: SpawnTaskHandle,
	/// An import queue.
	pub import_queue: IQ,
	/// A block announce validator builder.
	pub block_announce_validator_builder: Option<
		Box<dyn FnOnce(Arc<Client>) -> Box<dyn BlockAnnounceValidator<Block> + Send> + Send>,
	>,
	/// Optional warp sync config.
	pub warp_sync_config: Option<WarpSyncConfig<Block>>,
	/// User specified block relay params. If not specified, the default
	/// block request handler will be used.
	pub block_relay: Option<BlockRelayParams<Block, Net>>,
	/// Metrics.
	pub metrics: NotificationMetrics,
}

/// Build the network service, the network status sinks and an RPC sender.
pub fn build_network<Block, Net, TxPool, IQ, Client>(
	params: BuildNetworkParams<Block, Net, TxPool, IQ, Client>,
) -> Result<
	(
		Arc<dyn sc_network::service::traits::NetworkService>,
		TracingUnboundedSender<sc_rpc::system::Request<Block>>,
		sc_network_transactions::TransactionsHandlerController<<Block as BlockT>::Hash>,
		Arc<SyncingService<Block>>,
	),
	Error,
>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ Chain<Block>
		+ BlockBackend<Block>
		+ BlockIdTo<Block, Error = sp_blockchain::Error>
		+ ProofProvider<Block>
		+ HeaderBackend<Block>
		+ BlockchainEvents<Block>
		+ 'static,
	TxPool: TransactionPool<Block = Block, Hash = <Block as BlockT>::Hash> + 'static,
	IQ: ImportQueue<Block> + 'static,
	Net: NetworkBackend<Block, <Block as BlockT>::Hash>,
{
	let BuildNetworkParams {
		config,
		mut net_config,
		client,
		transaction_pool,
		spawn_handle,
		import_queue,
		block_announce_validator_builder,
		warp_sync_config,
		block_relay,
		metrics,
	} = params;

	let block_announce_validator = if let Some(f) = block_announce_validator_builder {
		f(client.clone())
	} else {
		Box::new(DefaultBlockAnnounceValidator)
	};

	let network_service_provider = NetworkServiceProvider::new();
	let protocol_id = config.protocol_id();
	let fork_id = config.chain_spec.fork_id();
	let metrics_registry = config.prometheus_config.as_ref().map(|config| &config.registry);

	let block_downloader = match block_relay {
		Some(params) => {
			let BlockRelayParams { mut server, downloader, request_response_config } = params;

			net_config.add_request_response_protocol(request_response_config);

			spawn_handle.spawn("block-request-handler", Some("networking"), async move {
				server.run().await;
			});

			downloader
		},
		None => build_default_block_downloader(
			&protocol_id,
			fork_id,
			&mut net_config,
			network_service_provider.handle(),
			Arc::clone(&client),
			config.network.default_peers_set.in_peers as usize +
				config.network.default_peers_set.out_peers as usize,
			&spawn_handle,
		),
	};

	let syncing_strategy = build_polkadot_syncing_strategy(
		protocol_id.clone(),
		fork_id,
		&mut net_config,
		warp_sync_config,
		block_downloader,
		client.clone(),
		&spawn_handle,
		metrics_registry,
	)?;

	let (syncing_engine, sync_service, block_announce_config) = SyncingEngine::new(
		Roles::from(&config.role),
		Arc::clone(&client),
		metrics_registry,
		metrics.clone(),
		&net_config,
		protocol_id.clone(),
		fork_id,
		block_announce_validator,
		syncing_strategy,
		network_service_provider.handle(),
		import_queue.service(),
		net_config.peer_store_handle(),
	)?;

	spawn_handle.spawn_blocking("syncing", None, syncing_engine.run());

	build_network_advanced(BuildNetworkAdvancedParams {
		role: config.role,
		protocol_id,
		fork_id,
		ipfs_server: config.network.ipfs_server,
		announce_block: config.announce_block,
		net_config,
		client,
		transaction_pool,
		spawn_handle,
		import_queue,
		sync_service,
		block_announce_config,
		network_service_provider,
		metrics_registry,
		metrics,
	})
}

/// Parameters to pass into [`build_network_advanced`].
pub struct BuildNetworkAdvancedParams<'a, Block, Net, TxPool, IQ, Client>
where
	Block: BlockT,
	Net: NetworkBackend<Block, <Block as BlockT>::Hash>,
{
	/// Role of the local node.
	pub role: Role,
	/// Protocol name prefix.
	pub protocol_id: ProtocolId,
	/// Fork ID.
	pub fork_id: Option<&'a str>,
	/// Enable serving block data over IPFS bitswap.
	pub ipfs_server: bool,
	/// Announce block automatically after they have been imported.
	pub announce_block: bool,
	/// Full network configuration.
	pub net_config: FullNetworkConfiguration<Block, <Block as BlockT>::Hash, Net>,
	/// A shared client returned by `new_full_parts`.
	pub client: Arc<Client>,
	/// A shared transaction pool.
	pub transaction_pool: Arc<TxPool>,
	/// A handle for spawning tasks.
	pub spawn_handle: SpawnTaskHandle,
	/// An import queue.
	pub import_queue: IQ,
	/// Syncing service to communicate with syncing engine.
	pub sync_service: SyncingService<Block>,
	/// Block announce config.
	pub block_announce_config: Net::NotificationProtocolConfig,
	/// Network service provider to drive with network internally.
	pub network_service_provider: NetworkServiceProvider,
	/// Prometheus metrics registry.
	pub metrics_registry: Option<&'a Registry>,
	/// Metrics.
	pub metrics: NotificationMetrics,
}

/// Build the network service, the network status sinks and an RPC sender, this is a lower-level
/// version of [`build_network`] for those needing more control.
pub fn build_network_advanced<Block, Net, TxPool, IQ, Client>(
	params: BuildNetworkAdvancedParams<Block, Net, TxPool, IQ, Client>,
) -> Result<
	(
		Arc<dyn sc_network::service::traits::NetworkService>,
		TracingUnboundedSender<sc_rpc::system::Request<Block>>,
		sc_network_transactions::TransactionsHandlerController<<Block as BlockT>::Hash>,
		Arc<SyncingService<Block>>,
	),
	Error,
>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ Chain<Block>
		+ BlockBackend<Block>
		+ BlockIdTo<Block, Error = sp_blockchain::Error>
		+ ProofProvider<Block>
		+ HeaderBackend<Block>
		+ BlockchainEvents<Block>
		+ 'static,
	TxPool: TransactionPool<Block = Block, Hash = <Block as BlockT>::Hash> + 'static,
	IQ: ImportQueue<Block> + 'static,
	Net: NetworkBackend<Block, <Block as BlockT>::Hash>,
{
	let BuildNetworkAdvancedParams {
		role,
		protocol_id,
		fork_id,
		ipfs_server,
		announce_block,
		mut net_config,
		client,
		transaction_pool,
		spawn_handle,
		import_queue,
		sync_service,
		block_announce_config,
		network_service_provider,
		metrics_registry,
		metrics,
	} = params;

	let genesis_hash = client.info().genesis_hash;

	let light_client_request_protocol_config = {
		// Allow both outgoing and incoming requests.
		let (handler, protocol_config) =
			LightClientRequestHandler::new::<Net>(&protocol_id, fork_id, client.clone());
		spawn_handle.spawn("light-client-request-handler", Some("networking"), handler.run());
		protocol_config
	};

	// install request handlers to `FullNetworkConfiguration`
	net_config.add_request_response_protocol(light_client_request_protocol_config);

	let bitswap_config = ipfs_server.then(|| {
		let (handler, config) = Net::bitswap_server(client.clone());
		spawn_handle.spawn("bitswap-request-handler", Some("networking"), handler);

		config
	});

	// Create transactions protocol and add it to the list of supported protocols of
	let (transactions_handler_proto, transactions_config) =
		sc_network_transactions::TransactionsHandlerPrototype::new::<_, Block, Net>(
			protocol_id.clone(),
			genesis_hash,
			fork_id,
			metrics.clone(),
			net_config.peer_store_handle(),
		);
	net_config.add_notification_protocol(transactions_config);

	// Start task for `PeerStore`
	let peer_store = net_config.take_peer_store();
	spawn_handle.spawn("peer-store", Some("networking"), peer_store.run());

	let sync_service = Arc::new(sync_service);

	let network_params = sc_network::config::Params::<Block, <Block as BlockT>::Hash, Net> {
		role,
		executor: {
			let spawn_handle = Clone::clone(&spawn_handle);
			Box::new(move |fut| {
				spawn_handle.spawn("libp2p-node", Some("networking"), fut);
			})
		},
		network_config: net_config,
		genesis_hash,
		protocol_id,
		fork_id: fork_id.map(ToOwned::to_owned),
		metrics_registry: metrics_registry.cloned(),
		block_announce_config,
		bitswap_config,
		notification_metrics: metrics,
	};

	let has_bootnodes = !network_params.network_config.network_config.boot_nodes.is_empty();
	let network_mut = Net::new(network_params)?;
	let network = network_mut.network_service().clone();

	let (tx_handler, tx_handler_controller) = transactions_handler_proto.build(
		network.clone(),
		sync_service.clone(),
		Arc::new(TransactionPoolAdapter { pool: transaction_pool, client: client.clone() }),
		metrics_registry,
	)?;
	spawn_handle.spawn_blocking(
		"network-transactions-handler",
		Some("networking"),
		tx_handler.run(),
	);

	spawn_handle.spawn_blocking(
		"chain-sync-network-service-provider",
		Some("networking"),
		network_service_provider.run(Arc::new(network.clone())),
	);
	spawn_handle.spawn("import-queue", None, {
		let sync_service = sync_service.clone();

		async move { import_queue.run(sync_service.as_ref()).await }
	});

	let (system_rpc_tx, system_rpc_rx) = tracing_unbounded("mpsc_system_rpc", 10_000);
	spawn_handle.spawn(
		"system-rpc-handler",
		Some("networking"),
		build_system_rpc_future::<_, _, <Block as BlockT>::Hash>(
			role,
			network_mut.network_service(),
			sync_service.clone(),
			client.clone(),
			system_rpc_rx,
			has_bootnodes,
		),
	);

	let future = build_network_future::<_, _, <Block as BlockT>::Hash, _>(
		network_mut,
		client,
		sync_service.clone(),
		announce_block,
	);

	// The network worker is responsible for gathering all network messages and processing
	// them. This is quite a heavy task, and at the time of the writing of this comment it
	// frequently happens that this future takes several seconds or in some situations
	// even more than a minute until it has processed its entire queue. This is clearly an
	// issue, and ideally we would like to fix the network future to take as little time as
	// possible, but we also take the extra harm-prevention measure to execute the networking
	// future using `spawn_blocking`.
	spawn_handle.spawn_blocking("network-worker", Some("networking"), future);

	Ok((network, system_rpc_tx, tx_handler_controller, sync_service.clone()))
}

/// Configuration for [`build_default_syncing_engine`].
pub struct DefaultSyncingEngineConfig<'a, Block, Client, Net>
where
	Block: BlockT,
	Net: NetworkBackend<Block, <Block as BlockT>::Hash>,
{
	/// Role of the local node.
	pub role: Role,
	/// Protocol name prefix.
	pub protocol_id: ProtocolId,
	/// Fork ID.
	pub fork_id: Option<&'a str>,
	/// Full network configuration.
	pub net_config: &'a mut FullNetworkConfiguration<Block, <Block as BlockT>::Hash, Net>,
	/// Validator for incoming block announcements.
	pub block_announce_validator: Box<dyn BlockAnnounceValidator<Block> + Send>,
	/// Handle to communicate with `NetworkService`.
	pub network_service_handle: NetworkServiceHandle,
	/// Warp sync configuration (when used).
	pub warp_sync_config: Option<WarpSyncConfig<Block>>,
	/// A shared client returned by `new_full_parts`.
	pub client: Arc<Client>,
	/// Blocks import queue API.
	pub import_queue_service: Box<dyn ImportQueueService<Block>>,
	/// Expected max total number of peer connections (in + out).
	pub num_peers_hint: usize,
	/// A handle for spawning tasks.
	pub spawn_handle: &'a SpawnTaskHandle,
	/// Prometheus metrics registry.
	pub metrics_registry: Option<&'a Registry>,
	/// Metrics.
	pub metrics: NotificationMetrics,
}

/// Build default syncing engine using [`build_default_block_downloader`] and
/// [`build_polkadot_syncing_strategy`] internally.
pub fn build_default_syncing_engine<Block, Client, Net>(
	config: DefaultSyncingEngineConfig<Block, Client, Net>,
) -> Result<(SyncingService<Block>, Net::NotificationProtocolConfig), Error>
where
	Block: BlockT,
	Client: HeaderBackend<Block>
		+ BlockBackend<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ ProofProvider<Block>
		+ Send
		+ Sync
		+ 'static,
	Net: NetworkBackend<Block, <Block as BlockT>::Hash>,
{
	let DefaultSyncingEngineConfig {
		role,
		protocol_id,
		fork_id,
		net_config,
		block_announce_validator,
		network_service_handle,
		warp_sync_config,
		client,
		import_queue_service,
		num_peers_hint,
		spawn_handle,
		metrics_registry,
		metrics,
	} = config;

	let block_downloader = build_default_block_downloader(
		&protocol_id,
		fork_id,
		net_config,
		network_service_handle.clone(),
		client.clone(),
		num_peers_hint,
		spawn_handle,
	);
	let syncing_strategy = build_polkadot_syncing_strategy(
		protocol_id.clone(),
		fork_id,
		net_config,
		warp_sync_config,
		block_downloader,
		client.clone(),
		spawn_handle,
		metrics_registry,
	)?;

	let (syncing_engine, sync_service, block_announce_config) = SyncingEngine::new(
		Roles::from(&role),
		client,
		metrics_registry,
		metrics,
		&net_config,
		protocol_id,
		fork_id,
		block_announce_validator,
		syncing_strategy,
		network_service_handle,
		import_queue_service,
		net_config.peer_store_handle(),
	)?;

	spawn_handle.spawn_blocking("syncing", None, syncing_engine.run());

	Ok((sync_service, block_announce_config))
}

/// Build default block downloader
pub fn build_default_block_downloader<Block, Client, Net>(
	protocol_id: &ProtocolId,
	fork_id: Option<&str>,
	net_config: &mut FullNetworkConfiguration<Block, <Block as BlockT>::Hash, Net>,
	network_service_handle: NetworkServiceHandle,
	client: Arc<Client>,
	num_peers_hint: usize,
	spawn_handle: &SpawnTaskHandle,
) -> Arc<dyn BlockDownloader<Block>>
where
	Block: BlockT,
	Client: HeaderBackend<Block> + BlockBackend<Block> + Send + Sync + 'static,
	Net: NetworkBackend<Block, <Block as BlockT>::Hash>,
{
	// Custom protocol was not specified, use the default block handler.
	// Allow both outgoing and incoming requests.
	let BlockRelayParams { mut server, downloader, request_response_config } =
		BlockRequestHandler::new::<Net>(
			network_service_handle,
			&protocol_id,
			fork_id,
			client.clone(),
			num_peers_hint,
		);

	spawn_handle.spawn("block-request-handler", Some("networking"), async move {
		server.run().await;
	});

	net_config.add_request_response_protocol(request_response_config);

	downloader
}

/// Build standard polkadot syncing strategy
pub fn build_polkadot_syncing_strategy<Block, Client, Net>(
	protocol_id: ProtocolId,
	fork_id: Option<&str>,
	net_config: &mut FullNetworkConfiguration<Block, <Block as BlockT>::Hash, Net>,
	warp_sync_config: Option<WarpSyncConfig<Block>>,
	block_downloader: Arc<dyn BlockDownloader<Block>>,
	client: Arc<Client>,
	spawn_handle: &SpawnTaskHandle,
	metrics_registry: Option<&Registry>,
) -> Result<Box<dyn SyncingStrategy<Block>>, Error>
where
	Block: BlockT,
	Client: HeaderBackend<Block>
		+ BlockBackend<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ ProofProvider<Block>
		+ Send
		+ Sync
		+ 'static,
	Net: NetworkBackend<Block, <Block as BlockT>::Hash>,
{
	if warp_sync_config.is_none() && net_config.network_config.sync_mode.is_warp() {
		return Err("Warp sync enabled, but no warp sync provider configured.".into())
	}

	if client.requires_full_sync() {
		match net_config.network_config.sync_mode {
			SyncMode::LightState { .. } =>
				return Err("Fast sync doesn't work for archive nodes".into()),
			SyncMode::Warp => return Err("Warp sync doesn't work for archive nodes".into()),
			SyncMode::Full => {},
		}
	}

	let genesis_hash = client.info().genesis_hash;

	let (state_request_protocol_config, state_request_protocol_name) = {
		let num_peer_hint = net_config.network_config.default_peers_set_num_full as usize +
			net_config.network_config.default_peers_set.reserved_nodes.len();
		// Allow both outgoing and incoming requests.
		let (handler, protocol_config) =
			StateRequestHandler::new::<Net>(&protocol_id, fork_id, client.clone(), num_peer_hint);
		let config_name = protocol_config.protocol_name().clone();

		spawn_handle.spawn("state-request-handler", Some("networking"), handler.run());
		(protocol_config, config_name)
	};
	net_config.add_request_response_protocol(state_request_protocol_config);

	let (warp_sync_protocol_config, warp_sync_protocol_name) = match warp_sync_config.as_ref() {
		Some(WarpSyncConfig::WithProvider(warp_with_provider)) => {
			// Allow both outgoing and incoming requests.
			let (handler, protocol_config) = WarpSyncRequestHandler::new::<_, Net>(
				protocol_id,
				genesis_hash,
				fork_id,
				warp_with_provider.clone(),
			);
			let config_name = protocol_config.protocol_name().clone();

			spawn_handle.spawn("warp-sync-request-handler", Some("networking"), handler.run());
			(Some(protocol_config), Some(config_name))
		},
		_ => (None, None),
	};
	if let Some(config) = warp_sync_protocol_config {
		net_config.add_request_response_protocol(config);
	}

	let syncing_config = PolkadotSyncingStrategyConfig {
		mode: net_config.network_config.sync_mode,
		max_parallel_downloads: net_config.network_config.max_parallel_downloads,
		max_blocks_per_request: net_config.network_config.max_blocks_per_request,
		min_peers_to_start_warp_sync: net_config.network_config.min_peers_to_start_warp_sync,
		metrics_registry: metrics_registry.cloned(),
		state_request_protocol_name,
		block_downloader,
	};
	Ok(Box::new(PolkadotSyncingStrategy::new(
		syncing_config,
		client,
		warp_sync_config,
		warp_sync_protocol_name,
	)?))
}
