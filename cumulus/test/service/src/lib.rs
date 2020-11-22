// Copyright 2019 Parity Technologies (UK) Ltd.
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

//! Crate used for testing with Cumulus.

#![warn(missing_docs)]

mod chain_spec;
mod genesis;

pub use chain_spec::*;
pub use cumulus_test_runtime as runtime;
pub use genesis::*;

use core::future::Future;
use cumulus_network::BlockAnnounceValidator;
use cumulus_primitives::ParaId;
use cumulus_service::{
	prepare_node_config, start_collator, start_full_node, StartCollatorParams, StartFullNodeParams,
};
use cumulus_test_runtime::{NodeBlock as Block, RuntimeApi};
use polkadot_primitives::v1::CollatorPair;
use sc_client_api::execution_extensions::ExecutionStrategies;
use sc_executor::native_executor_instance;
pub use sc_executor::NativeExecutor;
use sc_network::{config::TransportConfig, multiaddr, NetworkService};
use sc_service::{
	config::{
		DatabaseConfig, KeystoreConfig, MultiaddrWithPeerId, NetworkConfiguration,
		OffchainWorkerConfig, PruningMode, WasmExecutionMethod,
	},
	BasePath, ChainSpec, Configuration, Error as ServiceError, PartialComponents, Role,
	RpcHandlers, TFullBackend, TFullClient, TaskExecutor, TaskManager,
};
use sp_core::{Pair, H256};
use sp_keyring::Sr25519Keyring;
use sp_runtime::traits::BlakeTwo256;
use sp_state_machine::BasicExternalities;
use sp_trie::PrefixedMemoryDB;
use std::sync::Arc;
use substrate_test_client::BlockchainEventsExt;

// Native executor instance.
native_executor_instance!(
	pub RuntimeExecutor,
	cumulus_test_runtime::api::dispatch,
	cumulus_test_runtime::native_version,
);

/// Starts a `ServiceBuilder` for a full service.
///
/// Use this macro if you don't actually need the full service, but just the builder in order to
/// be able to perform chain operations.
pub fn new_partial(
	config: &mut Configuration,
) -> Result<
	PartialComponents<
		TFullClient<Block, RuntimeApi, RuntimeExecutor>,
		TFullBackend<Block>,
		(),
		sp_consensus::import_queue::BasicQueue<Block, PrefixedMemoryDB<BlakeTwo256>>,
		sc_transaction_pool::FullPool<Block, TFullClient<Block, RuntimeApi, RuntimeExecutor>>,
		(),
	>,
	sc_service::Error,
> {
	let inherent_data_providers = sp_inherents::InherentDataProviders::new();

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, RuntimeExecutor>(&config)?;
	let client = Arc::new(client);

	let registry = config.prometheus_registry();

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		config.prometheus_registry(),
		task_manager.spawn_handle(),
		client.clone(),
	);

	let import_queue = cumulus_consensus::import_queue::import_queue(
		client.clone(),
		client.clone(),
		inherent_data_providers.clone(),
		&task_manager.spawn_handle(),
		registry.clone(),
	)?;

	let params = PartialComponents {
		backend,
		client,
		import_queue,
		keystore_container,
		task_manager,
		transaction_pool,
		inherent_data_providers,
		select_chain: (),
		other: (),
	};

	Ok(params)
}

/// Start a node with the given parachain `Configuration` and relay chain `Configuration`.
///
/// This is the actual implementation that is abstract over the executor and the runtime api.
#[sc_cli::prefix_logs_with(parachain_config.network.node_name.as_str())]
async fn start_node_impl<RB>(
	parachain_config: Configuration,
	collator_key: CollatorPair,
	polkadot_config: Configuration,
	para_id: ParaId,
	is_collator: bool,
	rpc_ext_builder: RB,
) -> sc_service::error::Result<(
	TaskManager,
	Arc<TFullClient<Block, RuntimeApi, RuntimeExecutor>>,
	Arc<NetworkService<Block, H256>>,
	RpcHandlers,
)>
where
	RB: Fn(
			Arc<TFullClient<Block, RuntimeApi, RuntimeExecutor>>,
		) -> jsonrpc_core::IoHandler<sc_rpc::Metadata>
		+ Send
		+ 'static,
{
	if matches!(parachain_config.role, Role::Light) {
		return Err("Light client not supported!".into());
	}

	let mut parachain_config = prepare_node_config(parachain_config);

	let params = new_partial(&mut parachain_config)?;
	params
		.inherent_data_providers
		.register_provider(sp_timestamp::InherentDataProvider)
		.expect("Registers timestamp inherent data provider.");

	let transaction_pool = params.transaction_pool.clone();
	let mut task_manager = params.task_manager;

	let polkadot_full_node = polkadot_test_service::new_full(
		polkadot_config,
		polkadot_service::IsCollator::Yes(collator_key.public()),
	)?;

	let client = params.client.clone();
	let backend = params.backend.clone();
	let block_announce_validator = BlockAnnounceValidator::new(
		polkadot_full_node.client.clone(),
		para_id,
		Box::new(polkadot_full_node.network.clone()),
		polkadot_full_node.backend.clone(),
		polkadot_full_node.client.clone(),
	);
	let block_announce_validator_builder = move |_| Box::new(block_announce_validator) as Box<_>;

	let prometheus_registry = parachain_config.prometheus_registry().cloned();
	let import_queue = params.import_queue;
	let (network, network_status_sinks, system_rpc_tx, start_network) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &parachain_config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			on_demand: None,
			block_announce_validator_builder: Some(Box::new(block_announce_validator_builder)),
			finality_proof_request_builder: None,
			finality_proof_provider: None,
		})?;

	let rpc_extensions_builder = {
		let client = client.clone();

		Box::new(move |_, _| rpc_ext_builder(client.clone()))
	};

	let rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		on_demand: None,
		remote_blockchain: None,
		rpc_extensions_builder,
		client: client.clone(),
		transaction_pool: transaction_pool.clone(),
		task_manager: &mut task_manager,
		telemetry_connection_sinks: Default::default(),
		config: parachain_config,
		keystore: params.keystore_container.sync_keystore(),
		backend,
		network: network.clone(),
		network_status_sinks,
		system_rpc_tx,
	})?;

	let announce_block = {
		let network = network.clone();
		Arc::new(move |hash, data| network.announce_block(hash, data))
	};

	let polkadot_full_node = polkadot_full_node.with_client(polkadot_test_service::TestClient);
	if is_collator {
		let proposer_factory = sc_basic_authorship::ProposerFactory::new(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool,
			prometheus_registry.as_ref(),
		);

		let params = StartCollatorParams {
			proposer_factory,
			inherent_data_providers: params.inherent_data_providers,
			backend: params.backend,
			block_import: client.clone(),
			block_status: client.clone(),
			announce_block,
			client: client.clone(),
			spawner: task_manager.spawn_handle(),
			task_manager: &mut task_manager,
			para_id,
			collator_key,
			polkadot_full_node,
		};

		start_collator(params).await?;
	} else {
		let params = StartFullNodeParams {
			client: client.clone(),
			announce_block,
			task_manager: &mut task_manager,
			para_id,
			polkadot_full_node,
		};

		start_full_node(params)?;
	}

	start_network.start_network();

	Ok((task_manager, client, network, rpc_handlers))
}

/// A Cumulus test node instance used for testing.
pub struct CumulusTestNode {
	/// TaskManager's instance.
	pub task_manager: TaskManager,
	/// Client's instance.
	pub client: Arc<TFullClient<Block, RuntimeApi, RuntimeExecutor>>,
	/// Node's network.
	pub network: Arc<NetworkService<Block, H256>>,
	/// The `MultiaddrWithPeerId` to this node. This is useful if you want to pass it as "boot node"
	/// to other nodes.
	pub addr: MultiaddrWithPeerId,
	/// RPCHandlers to make RPC queries.
	pub rpc_handlers: RpcHandlers,
}

/// Run a Cumulus test node using the Cumulus test runtime. The node will be using an in-memory
/// socket, therefore you need to provide boot nodes if you want it to be connected to other nodes.
/// The `storage_update_func` can be used to make adjustements to the runtime before the node
/// starts.
pub async fn run_test_node(
	task_executor: TaskExecutor,
	key: Sr25519Keyring,
	parachain_storage_update_func: impl Fn(),
	polkadot_storage_update_func: impl Fn(),
	parachain_boot_nodes: Vec<MultiaddrWithPeerId>,
	polkadot_boot_nodes: Vec<MultiaddrWithPeerId>,
	para_id: ParaId,
	is_collator: bool,
) -> CumulusTestNode {
	let collator_key = CollatorPair::generate().0;
	let parachain_config = node_config(
		parachain_storage_update_func,
		task_executor.clone(),
		key,
		parachain_boot_nodes,
		para_id,
		is_collator,
	)
	.expect("could not generate Configuration");
	let mut polkadot_config = polkadot_test_service::node_config(
		polkadot_storage_update_func,
		task_executor.clone(),
		key,
		polkadot_boot_nodes,
		false,
	);

	polkadot_config.network.node_name =
		format!("{} (relay chain)", polkadot_config.network.node_name);

	let multiaddr = parachain_config.network.listen_addresses[0].clone();
	let (task_manager, client, network, rpc_handlers) = start_node_impl(
		parachain_config,
		collator_key,
		polkadot_config,
		para_id,
		is_collator,
		|_| Default::default(),
	)
	.await
	.expect("could not create Cumulus test service");

	let peer_id = network.local_peer_id().clone();
	let addr = MultiaddrWithPeerId { multiaddr, peer_id };

	CumulusTestNode {
		task_manager,
		client,
		network,
		addr,
		rpc_handlers,
	}
}

/// Create a Cumulus `Configuration`. By default an in-memory socket will be used, therefore you
/// need to provide boot nodes if you want the future node to be connected to other nodes. The
/// `storage_update_func` can be used to make adjustments to the runtime before the node starts.
pub fn node_config(
	storage_update_func: impl Fn(),
	task_executor: TaskExecutor,
	key: Sr25519Keyring,
	boot_nodes: Vec<MultiaddrWithPeerId>,
	para_id: ParaId,
	is_collator: bool,
) -> Result<Configuration, ServiceError> {
	let base_path = BasePath::new_temp_dir()?;
	let root = base_path.path().to_path_buf();
	let role = if is_collator {
		Role::Authority {
			sentry_nodes: Vec::new(),
		}
	} else {
		Role::Full
	};
	let key_seed = key.to_seed();
	let mut spec = Box::new(chain_spec::get_chain_spec(para_id));

	let mut storage = spec
		.as_storage_builder()
		.build_storage()
		.expect("could not build storage");

	BasicExternalities::execute_with_storage(&mut storage, storage_update_func);
	spec.set_storage(storage);

	let mut network_config = NetworkConfiguration::new(
		format!("{} (parachain)", key_seed.to_string()),
		"network/test/0.1",
		Default::default(),
		None,
	);

	network_config.boot_nodes = boot_nodes;

	network_config.allow_non_globals_in_dht = true;

	network_config
		.listen_addresses
		.push(multiaddr::Protocol::Memory(rand::random()).into());

	network_config.transport = TransportConfig::MemoryOnly;

	Ok(Configuration {
		impl_name: "cumulus-test-node".to_string(),
		impl_version: "0.1".to_string(),
		role,
		task_executor,
		transaction_pool: Default::default(),
		network: network_config,
		keystore: KeystoreConfig::InMemory,
		database: DatabaseConfig::RocksDb {
			path: root.join("db"),
			cache_size: 128,
		},
		state_cache_size: 67108864,
		state_cache_child_ratio: None,
		pruning: PruningMode::ArchiveAll,
		chain_spec: spec,
		wasm_method: WasmExecutionMethod::Interpreted,
		// NOTE: we enforce the use of the native runtime to make the errors more debuggable
		execution_strategies: ExecutionStrategies {
			syncing: sc_client_api::ExecutionStrategy::NativeWhenPossible,
			importing: sc_client_api::ExecutionStrategy::NativeWhenPossible,
			block_construction: sc_client_api::ExecutionStrategy::NativeWhenPossible,
			offchain_worker: sc_client_api::ExecutionStrategy::NativeWhenPossible,
			other: sc_client_api::ExecutionStrategy::NativeWhenPossible,
		},
		rpc_http: None,
		rpc_ws: None,
		rpc_ipc: None,
		rpc_ws_max_connections: None,
		rpc_cors: None,
		rpc_methods: Default::default(),
		prometheus_config: None,
		telemetry_endpoints: None,
		telemetry_external_transport: None,
		default_heap_pages: None,
		offchain_worker: OffchainWorkerConfig {
			enabled: true,
			indexing_enabled: false,
		},
		force_authoring: false,
		disable_grandpa: false,
		dev_key_seed: Some(key_seed),
		tracing_targets: None,
		tracing_receiver: Default::default(),
		max_runtime_instances: 8,
		announce_block: true,
		base_path: Some(base_path),
		informant_output_format: Default::default(),
		wasm_runtime_overrides: None,
	})
}

impl CumulusTestNode {
	/// Wait for `count` blocks to be imported in the node and then exit. This function will not
	/// return if no blocks are ever created, thus you should restrict the maximum amount of time of
	/// the test execution.
	pub fn wait_for_blocks(&self, count: usize) -> impl Future<Output = ()> {
		self.client.wait_for_blocks(count)
	}
}
