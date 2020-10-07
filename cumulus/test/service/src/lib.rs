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
pub use genesis::*;

use ansi_term::Color;
use core::future::Future;
use cumulus_collator::CollatorBuilder;
use cumulus_network::DelayedBlockAnnounceValidator;
use cumulus_primitives::ParaId;
use cumulus_service::{
	prepare_node_config, start_full_node, StartCollatorParams, StartFullNodeParams,
};
use cumulus_test_runtime::{NodeBlock as Block, RuntimeApi};
use polkadot_primitives::v0::CollatorPair;
use sc_client_api::execution_extensions::ExecutionStrategies;
use sc_client_api::BlockBackend;
use sc_executor::native_executor_instance;
pub use sc_executor::NativeExecutor;
use sc_informant::OutputFormat;
use sc_network::{config::TransportConfig, multiaddr, NetworkService};
use sc_service::{
	config::{
		DatabaseConfig, KeystoreConfig, MultiaddrWithPeerId, NetworkConfiguration,
		OffchainWorkerConfig, PruningMode, WasmExecutionMethod,
	},
	BasePath, ChainSpec, Configuration, Error as ServiceError, PartialComponents, Role,
	RpcHandlers, TFullBackend, TFullClient, TaskExecutor, TaskManager,
};
use sp_consensus::{BlockImport, Environment, Error as ConsensusError, Proposer};
use sp_core::{crypto::Pair, H256};
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

	let (client, backend, keystore, task_manager) =
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
		keystore,
		task_manager,
		transaction_pool,
		inherent_data_providers,
		select_chain: (),
		other: (),
	};

	Ok(params)
}

/// Start a test collator node for a parachain.
///
/// A collator is similar to a validator in a normal blockchain.
/// It is responsible for producing blocks and sending the blocks to a
/// parachain validator for validation and inclusion into the relay chain.
pub fn start_test_collator<'a, PF, BI, BS>(
	StartCollatorParams {
		para_id,
		proposer_factory,
		inherent_data_providers,
		block_import,
		block_status,
		announce_block,
		client,
		block_announce_validator,
		task_manager,
		polkadot_config,
		collator_key,
	}: StartCollatorParams<'a, Block, PF, BI, BS, TFullClient<Block, RuntimeApi, RuntimeExecutor>>,
) -> sc_service::error::Result<()>
where
	PF: Environment<Block> + Send + 'static,
	BI: BlockImport<
			Block,
			Error = ConsensusError,
			Transaction = <PF::Proposer as Proposer<Block>>::Transaction,
		> + Send
		+ Sync
		+ 'static,
	BS: BlockBackend<Block> + Send + Sync + 'static,
{
	let builder = CollatorBuilder::new(
		proposer_factory,
		inherent_data_providers,
		block_import,
		block_status,
		para_id,
		client,
		announce_block,
		block_announce_validator,
	);

	let (polkadot_future, polkadot_task_manager) = {
		let (task_manager, client, handles, _network, _rpc_handlers) =
			polkadot_test_service::polkadot_test_new_full(
				polkadot_config,
				Some((collator_key.public(), para_id)),
				None,
				false,
				6000,
			)?;

		let test_client = polkadot_test_service::TestClient(client);

		let future = polkadot_collator::build_collator_service(
			task_manager.spawn_handle(),
			handles,
			test_client,
			para_id,
			collator_key,
			builder,
		)?;

		(future, task_manager)
	};

	task_manager
		.spawn_essential_handle()
		.spawn("polkadot", polkadot_future);

	task_manager.add_child(polkadot_task_manager);

	Ok(())
}

/// Start a node with the given parachain `Configuration` and relay chain `Configuration`.
///
/// This is the actual implementation that is abstract over the executor and the runtime api.
fn start_node_impl<RB>(
	parachain_config: Configuration,
	collator_key: Arc<CollatorPair>,
	mut polkadot_config: polkadot_collator::Configuration,
	para_id: ParaId,
	validator: bool,
	rpc_ext_builder: RB,
) -> sc_service::error::Result<(
	TaskManager,
	Arc<TFullClient<Block, RuntimeApi, RuntimeExecutor>>,
	Arc<NetworkService<Block, H256>>,
	Arc<RpcHandlers>,
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

	parachain_config.informant_output_format = OutputFormat {
		enable_color: true,
		prefix: format!("[{}] ", Color::Yellow.bold().paint("Parachain")),
	};
	polkadot_config.informant_output_format = OutputFormat {
		enable_color: true,
		prefix: format!("[{}] ", Color::Blue.bold().paint("Relaychain")),
	};

	let params = new_partial(&mut parachain_config)?;
	params
		.inherent_data_providers
		.register_provider(sp_timestamp::InherentDataProvider)
		.unwrap();

	let client = params.client.clone();
	let backend = params.backend.clone();
	let block_announce_validator = DelayedBlockAnnounceValidator::new();
	let block_announce_validator_builder = {
		let block_announce_validator = block_announce_validator.clone();
		move |_| Box::new(block_announce_validator) as Box<_>
	};

	let prometheus_registry = parachain_config.prometheus_registry().cloned();
	let transaction_pool = params.transaction_pool.clone();
	let mut task_manager = params.task_manager;
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

		Box::new(move |_deny_unsafe| rpc_ext_builder(client.clone()))
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
		keystore: params.keystore,
		backend,
		network: network.clone(),
		network_status_sinks,
		system_rpc_tx,
	})?;

	let announce_block = {
		let network = network.clone();
		Arc::new(move |hash, data| network.announce_block(hash, data))
	};

	if validator {
		let proposer_factory = sc_basic_authorship::ProposerFactory::new(
			client.clone(),
			transaction_pool,
			prometheus_registry.as_ref(),
		);

		let params = StartCollatorParams {
			para_id,
			block_import: client.clone(),
			proposer_factory,
			inherent_data_providers: params.inherent_data_providers,
			block_status: client.clone(),
			announce_block,
			client: client.clone(),
			block_announce_validator,
			task_manager: &mut task_manager,
			polkadot_config,
			collator_key,
		};

		start_test_collator(params)?;
	} else {
		let params = StartFullNodeParams {
			client: client.clone(),
			announce_block,
			polkadot_config,
			collator_key,
			block_announce_validator,
			task_manager: &mut task_manager,
			para_id,
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
	pub rpc_handlers: Arc<RpcHandlers>,
}

/// Run a Cumulus test node using the Cumulus test runtime. The node will be using an in-memory
/// socket, therefore you need to provide boot nodes if you want it to be connected to other nodes.
/// The `storage_update_func` can be used to make adjustements to the runtime before the node
/// starts.
pub fn run_test_node(
	task_executor: TaskExecutor,
	key: Sr25519Keyring,
	parachain_storage_update_func: impl Fn(),
	polkadot_storage_update_func: impl Fn(),
	parachain_boot_nodes: Vec<MultiaddrWithPeerId>,
	polkadot_boot_nodes: Vec<MultiaddrWithPeerId>,
	para_id: ParaId,
	validator: bool,
) -> CumulusTestNode {
	let collator_key = Arc::new(sp_core::Pair::generate().0);
	let parachain_config = node_config(
		parachain_storage_update_func,
		task_executor.clone(),
		key,
		parachain_boot_nodes,
		para_id,
	)
	.expect("could not generate Configuration");
	let polkadot_config = polkadot_test_service::node_config(
		polkadot_storage_update_func,
		task_executor.clone(),
		key,
		polkadot_boot_nodes,
	);
	let multiaddr = parachain_config.network.listen_addresses[0].clone();
	let (task_manager, client, network, rpc_handlers) = start_node_impl::<_>(
		parachain_config,
		collator_key,
		polkadot_config,
		para_id,
		validator,
		|_| Default::default(),
	)
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
) -> Result<Configuration, ServiceError> {
	let base_path = BasePath::new_temp_dir()?;
	let root = base_path.path().to_path_buf();
	let role = Role::Authority {
		sentry_nodes: Vec::new(),
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
		format!("Cumulus Test Node for: {}", key_seed),
		"network/test/0.1",
		Default::default(),
		None,
	);
	let informant_output_format = OutputFormat {
		enable_color: false,
		prefix: format!("[{}] ", key_seed),
	};

	network_config.boot_nodes = boot_nodes;

	network_config.allow_non_globals_in_dht = false;

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
		keystore: KeystoreConfig::Path {
			path: root.join("key"),
			password: None,
		},
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
		informant_output_format,
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
