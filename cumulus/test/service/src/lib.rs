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

//! Crate used for testing with Cumulus.

#![warn(missing_docs)]

/// Utilities used for benchmarking
pub mod bench_utils;

pub mod chain_spec;
mod genesis;

use runtime::AccountId;
use sc_executor::{HeapAllocStrategy, WasmExecutor, DEFAULT_HEAP_ALLOC_STRATEGY};
use std::{
	future::Future,
	net::{IpAddr, Ipv4Addr, SocketAddr},
	time::Duration,
};
use url::Url;

use crate::runtime::Weight;
use cumulus_client_cli::{CollatorOptions, RelayChainMode};
use cumulus_client_consensus_common::{
	ParachainBlockImport as TParachainBlockImport, ParachainCandidate, ParachainConsensus,
};
use cumulus_client_pov_recovery::RecoveryHandle;
#[allow(deprecated)]
use cumulus_client_service::old_consensus;
use cumulus_client_service::{
	build_network, prepare_node_config, start_relay_chain_tasks, BuildNetworkParams,
	CollatorSybilResistance, DARecoveryProfile, StartRelayChainTasksParams,
};
use cumulus_primitives_core::ParaId;
use cumulus_relay_chain_inprocess_interface::RelayChainInProcessInterface;
use cumulus_relay_chain_interface::{RelayChainError, RelayChainInterface, RelayChainResult};
use cumulus_relay_chain_minimal_node::{
	build_minimal_relay_chain_node_light_client, build_minimal_relay_chain_node_with_rpc,
};

use cumulus_test_runtime::{Hash, Header, NodeBlock as Block, RuntimeApi};

use frame_system_rpc_runtime_api::AccountNonceApi;
use polkadot_node_subsystem::{errors::RecoveryError, messages::AvailabilityRecoveryMessage};
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{CollatorPair, Hash as PHash, PersistedValidationData};
use polkadot_service::ProvideRuntimeApi;
use sc_consensus::ImportQueue;
use sc_network::{
	config::{FullNetworkConfiguration, TransportConfig},
	multiaddr, NetworkBlock, NetworkService, NetworkStateInfo,
};
use sc_service::{
	config::{
		BlocksPruning, DatabaseSource, KeystoreConfig, MultiaddrWithPeerId, NetworkConfiguration,
		OffchainWorkerConfig, PruningMode, WasmExecutionMethod,
	},
	BasePath, ChainSpec as ChainSpecService, Configuration, Error as ServiceError,
	PartialComponents, Role, RpcHandlers, TFullBackend, TFullClient, TaskManager,
};
use sp_arithmetic::traits::SaturatedConversion;
use sp_blockchain::HeaderBackend;
use sp_core::{Pair, H256};
use sp_keyring::Sr25519Keyring;
use sp_runtime::{codec::Encode, generic};
use sp_state_machine::BasicExternalities;
use std::sync::Arc;
use substrate_test_client::{
	BlockchainEventsExt, RpcHandlersExt, RpcTransactionError, RpcTransactionOutput,
};

pub use chain_spec::*;
pub use cumulus_test_runtime as runtime;
pub use genesis::*;
pub use sp_keyring::Sr25519Keyring as Keyring;

const LOG_TARGET: &str = "cumulus-test-service";

/// A consensus that will never produce any block.
#[derive(Clone)]
struct NullConsensus;

#[async_trait::async_trait]
impl ParachainConsensus<Block> for NullConsensus {
	async fn produce_candidate(
		&mut self,
		_: &Header,
		_: PHash,
		_: &PersistedValidationData,
	) -> Option<ParachainCandidate<Block>> {
		None
	}
}

/// The signature of the announce block fn.
pub type AnnounceBlockFn = Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>;

/// Native executor instance.
pub struct RuntimeExecutor;

impl sc_executor::NativeExecutionDispatch for RuntimeExecutor {
	type ExtendHostFunctions = ();

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		cumulus_test_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		cumulus_test_runtime::native_version()
	}
}

/// The client type being used by the test service.
pub type Client = TFullClient<
	runtime::NodeBlock,
	runtime::RuntimeApi,
	sc_executor::NativeElseWasmExecutor<RuntimeExecutor>,
>;

/// The backend type being used by the test service.
pub type Backend = TFullBackend<Block>;

/// The block-import type being used by the test service.
pub type ParachainBlockImport = TParachainBlockImport<Block, Arc<Client>, Backend>;

/// Transaction pool type used by the test service
pub type TransactionPool = Arc<sc_transaction_pool::FullPool<Block, Client>>;

/// Recovery handle that fails regularly to simulate unavailable povs.
pub struct FailingRecoveryHandle {
	overseer_handle: OverseerHandle,
	counter: u32,
}

impl FailingRecoveryHandle {
	/// Create a new FailingRecoveryHandle
	pub fn new(overseer_handle: OverseerHandle) -> Self {
		Self { overseer_handle, counter: 0 }
	}
}

#[async_trait::async_trait]
impl RecoveryHandle for FailingRecoveryHandle {
	async fn send_recovery_msg(
		&mut self,
		message: AvailabilityRecoveryMessage,
		origin: &'static str,
	) {
		// For every 5th block we immediately signal unavailability to trigger
		// a retry.
		if self.counter % 5 == 0 {
			let AvailabilityRecoveryMessage::RecoverAvailableData(_, _, _, back_sender) = message;
			tracing::info!(target: LOG_TARGET, "Failing pov recovery.");
			back_sender
				.send(Err(RecoveryError::Unavailable))
				.expect("Return channel should work here.");
		} else {
			self.overseer_handle.send_msg(message, origin).await;
		}
		self.counter += 1;
	}
}

/// Starts a `ServiceBuilder` for a full service.
///
/// Use this macro if you don't actually need the full service, but just the builder in order to
/// be able to perform chain operations.
pub fn new_partial(
	config: &mut Configuration,
) -> Result<
	PartialComponents<
		Client,
		Backend,
		(),
		sc_consensus::import_queue::BasicQueue<Block>,
		sc_transaction_pool::FullPool<Block, Client>,
		ParachainBlockImport,
	>,
	sc_service::Error,
> {
	let heap_pages = config
		.default_heap_pages
		.map_or(DEFAULT_HEAP_ALLOC_STRATEGY, |h| HeapAllocStrategy::Static { extra_pages: h as _ });

	let wasm = WasmExecutor::builder()
		.with_execution_method(config.wasm_method)
		.with_onchain_heap_alloc_strategy(heap_pages)
		.with_offchain_heap_alloc_strategy(heap_pages)
		.with_max_runtime_instances(config.max_runtime_instances)
		.with_runtime_cache_size(config.runtime_cache_size)
		.build();

	let executor =
		sc_executor::NativeElseWasmExecutor::<RuntimeExecutor>::new_with_wasm_executor(wasm);

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, _>(config, None, executor)?;
	let client = Arc::new(client);

	let block_import = ParachainBlockImport::new(client.clone(), backend.clone());

	let registry = config.prometheus_registry();

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		config.role.is_authority().into(),
		config.prometheus_registry(),
		task_manager.spawn_essential_handle(),
		client.clone(),
	);

	let import_queue = cumulus_client_consensus_relay_chain::import_queue(
		client.clone(),
		block_import.clone(),
		|_, _| async { Ok(sp_timestamp::InherentDataProvider::from_system_time()) },
		&task_manager.spawn_essential_handle(),
		registry,
	)?;

	let params = PartialComponents {
		backend,
		client,
		import_queue,
		keystore_container,
		task_manager,
		transaction_pool,
		select_chain: (),
		other: block_import,
	};

	Ok(params)
}

async fn build_relay_chain_interface(
	relay_chain_config: Configuration,
	collator_key: Option<CollatorPair>,
	collator_options: CollatorOptions,
	task_manager: &mut TaskManager,
) -> RelayChainResult<Arc<dyn RelayChainInterface + 'static>> {
	let relay_chain_full_node = match collator_options.relay_chain_mode {
		cumulus_client_cli::RelayChainMode::Embedded => polkadot_test_service::new_full(
			relay_chain_config,
			if let Some(ref key) = collator_key {
				polkadot_service::IsParachainNode::Collator(key.clone())
			} else {
				polkadot_service::IsParachainNode::Collator(CollatorPair::generate().0)
			},
			None,
		)
		.map_err(|e| RelayChainError::Application(Box::new(e) as Box<_>))?,
		cumulus_client_cli::RelayChainMode::ExternalRpc(rpc_target_urls) =>
			return build_minimal_relay_chain_node_with_rpc(
				relay_chain_config,
				task_manager,
				rpc_target_urls,
			)
			.await
			.map(|r| r.0),
		cumulus_client_cli::RelayChainMode::LightClient =>
			return build_minimal_relay_chain_node_light_client(relay_chain_config, task_manager)
				.await
				.map(|r| r.0),
	};

	task_manager.add_child(relay_chain_full_node.task_manager);
	tracing::info!("Using inprocess node.");
	Ok(Arc::new(RelayChainInProcessInterface::new(
		relay_chain_full_node.client.clone(),
		relay_chain_full_node.backend.clone(),
		relay_chain_full_node.sync_service.clone(),
		relay_chain_full_node.overseer_handle.ok_or(RelayChainError::GenericError(
			"Overseer should be running in full node.".to_string(),
		))?,
	)))
}

/// Start a node with the given parachain `Configuration` and relay chain `Configuration`.
///
/// This is the actual implementation that is abstract over the executor and the runtime api.
#[sc_tracing::logging::prefix_logs_with(parachain_config.network.node_name.as_str())]
pub async fn start_node_impl<RB>(
	parachain_config: Configuration,
	collator_key: Option<CollatorPair>,
	relay_chain_config: Configuration,
	para_id: ParaId,
	wrap_announce_block: Option<Box<dyn FnOnce(AnnounceBlockFn) -> AnnounceBlockFn>>,
	fail_pov_recovery: bool,
	rpc_ext_builder: RB,
	consensus: Consensus,
	collator_options: CollatorOptions,
) -> sc_service::error::Result<(
	TaskManager,
	Arc<Client>,
	Arc<NetworkService<Block, H256>>,
	RpcHandlers,
	TransactionPool,
)>
where
	RB: Fn(Arc<Client>) -> Result<jsonrpsee::RpcModule<()>, sc_service::Error> + Send + 'static,
{
	let mut parachain_config = prepare_node_config(parachain_config);

	let params = new_partial(&mut parachain_config)?;

	let transaction_pool = params.transaction_pool.clone();
	let mut task_manager = params.task_manager;

	let client = params.client.clone();
	let backend = params.backend.clone();

	let block_import = params.other;

	let relay_chain_interface = build_relay_chain_interface(
		relay_chain_config,
		collator_key.clone(),
		collator_options.clone(),
		&mut task_manager,
	)
	.await
	.map_err(|e| sc_service::Error::Application(Box::new(e) as Box<_>))?;

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
			sybil_resistance_level: CollatorSybilResistance::Unresistant, // no consensus
		})
		.await?;

	let prometheus_registry = parachain_config.prometheus_registry().cloned();

	let rpc_builder = {
		let client = client.clone();
		Box::new(move |_, _| rpc_ext_builder(client.clone()))
	};

	let rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
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
		telemetry: None,
	})?;

	let announce_block = {
		let sync_service = sync_service.clone();
		Arc::new(move |hash, data| sync_service.announce_block(hash, data))
	};

	let announce_block = wrap_announce_block
		.map(|w| (w)(announce_block.clone()))
		.unwrap_or_else(|| announce_block);

	let relay_chain_interface_for_closure = relay_chain_interface.clone();

	let overseer_handle = relay_chain_interface
		.overseer_handle()
		.map_err(|e| sc_service::Error::Application(Box::new(e)))?;

	let recovery_handle: Box<dyn RecoveryHandle> = if fail_pov_recovery {
		Box::new(FailingRecoveryHandle::new(overseer_handle.clone()))
	} else {
		Box::new(overseer_handle.clone())
	};
	let is_collator = collator_key.is_some();
	let relay_chain_slot_duration = Duration::from_secs(6);

	start_relay_chain_tasks(StartRelayChainTasksParams {
		client: client.clone(),
		announce_block: announce_block.clone(),
		para_id,
		relay_chain_interface: relay_chain_interface.clone(),
		task_manager: &mut task_manager,
		da_recovery_profile: if is_collator {
			DARecoveryProfile::Collator
		} else {
			DARecoveryProfile::FullNode
		},
		import_queue: import_queue_service,
		relay_chain_slot_duration,
		recovery_handle,
		sync_service: sync_service.clone(),
	})?;

	if let Some(collator_key) = collator_key {
		let parachain_consensus: Box<dyn ParachainConsensus<Block>> = match consensus {
			Consensus::RelayChain => {
				let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
					task_manager.spawn_handle(),
					client.clone(),
					transaction_pool.clone(),
					prometheus_registry.as_ref(),
					None,
				);
				let relay_chain_interface2 = relay_chain_interface_for_closure.clone();
				Box::new(cumulus_client_consensus_relay_chain::RelayChainConsensus::new(
					para_id,
					proposer_factory,
					move |_, (relay_parent, validation_data)| {
						let relay_chain_interface = relay_chain_interface_for_closure.clone();
						async move {
							let parachain_inherent =
							cumulus_primitives_parachain_inherent::ParachainInherentData::create_at(
								relay_parent,
								&relay_chain_interface,
								&validation_data,
								para_id,
							).await;

							let time = sp_timestamp::InherentDataProvider::from_system_time();

							let parachain_inherent = parachain_inherent.ok_or_else(|| {
								Box::<dyn std::error::Error + Send + Sync>::from(String::from(
									"error",
								))
							})?;
							Ok((time, parachain_inherent))
						}
					},
					block_import,
					relay_chain_interface2,
				))
			},
			Consensus::Null => Box::new(NullConsensus),
		};

		#[allow(deprecated)]
		old_consensus::start_collator(old_consensus::StartCollatorParams {
			block_status: client.clone(),
			announce_block,
			runtime_api: client.clone(),
			spawner: task_manager.spawn_handle(),
			para_id,
			parachain_consensus,
			key: collator_key,
			overseer_handle,
		})
		.await;
	}

	start_network.start_network();

	Ok((task_manager, client, network, rpc_handlers, transaction_pool))
}

/// A Cumulus test node instance used for testing.
pub struct TestNode {
	/// TaskManager's instance.
	pub task_manager: TaskManager,
	/// Client's instance.
	pub client: Arc<Client>,
	/// Node's network.
	pub network: Arc<NetworkService<Block, H256>>,
	/// The `MultiaddrWithPeerId` to this node. This is useful if you want to pass it as "boot
	/// node" to other nodes.
	pub addr: MultiaddrWithPeerId,
	/// RPCHandlers to make RPC queries.
	pub rpc_handlers: RpcHandlers,
	/// Node's transaction pool
	pub transaction_pool: TransactionPool,
}

#[allow(missing_docs)]
pub enum Consensus {
	/// Use the relay-chain provided consensus.
	RelayChain,
	/// Use the null consensus that will never produce any block.
	Null,
}

/// A builder to create a [`TestNode`].
pub struct TestNodeBuilder {
	para_id: ParaId,
	tokio_handle: tokio::runtime::Handle,
	key: Sr25519Keyring,
	collator_key: Option<CollatorPair>,
	parachain_nodes: Vec<MultiaddrWithPeerId>,
	parachain_nodes_exclusive: bool,
	relay_chain_nodes: Vec<MultiaddrWithPeerId>,
	wrap_announce_block: Option<Box<dyn FnOnce(AnnounceBlockFn) -> AnnounceBlockFn>>,
	storage_update_func_parachain: Option<Box<dyn Fn()>>,
	storage_update_func_relay_chain: Option<Box<dyn Fn()>>,
	consensus: Consensus,
	relay_chain_mode: RelayChainMode,
	endowed_accounts: Vec<AccountId>,
}

impl TestNodeBuilder {
	/// Create a new instance of `Self`.
	///
	/// `para_id` - The parachain id this node is running for.
	/// `tokio_handle` - The tokio handler to use.
	/// `key` - The key that will be used to generate the name and that will be passed as
	/// `dev_seed`.
	pub fn new(para_id: ParaId, tokio_handle: tokio::runtime::Handle, key: Sr25519Keyring) -> Self {
		TestNodeBuilder {
			key,
			para_id,
			tokio_handle,
			collator_key: None,
			parachain_nodes: Vec::new(),
			parachain_nodes_exclusive: false,
			relay_chain_nodes: Vec::new(),
			wrap_announce_block: None,
			storage_update_func_parachain: None,
			storage_update_func_relay_chain: None,
			consensus: Consensus::RelayChain,
			endowed_accounts: Default::default(),
			relay_chain_mode: RelayChainMode::Embedded,
		}
	}

	/// Enable collator for this node.
	pub fn enable_collator(mut self) -> Self {
		let collator_key = CollatorPair::generate().0;
		self.collator_key = Some(collator_key);
		self
	}

	/// Instruct the node to exclusively connect to registered parachain nodes.
	///
	/// Parachain nodes can be registered using [`Self::connect_to_parachain_node`] and
	/// [`Self::connect_to_parachain_nodes`].
	pub fn exclusively_connect_to_registered_parachain_nodes(mut self) -> Self {
		self.parachain_nodes_exclusive = true;
		self
	}

	/// Make the node connect to the given parachain node.
	///
	/// By default the node will not be connected to any node or will be able to discover any other
	/// node.
	pub fn connect_to_parachain_node(mut self, node: &TestNode) -> Self {
		self.parachain_nodes.push(node.addr.clone());
		self
	}

	/// Make the node connect to the given parachain nodes.
	///
	/// By default the node will not be connected to any node or will be able to discover any other
	/// node.
	pub fn connect_to_parachain_nodes<'a>(
		mut self,
		nodes: impl IntoIterator<Item = &'a TestNode>,
	) -> Self {
		self.parachain_nodes.extend(nodes.into_iter().map(|n| n.addr.clone()));
		self
	}

	/// Make the node connect to the given relay chain node.
	///
	/// By default the node will not be connected to any node or will be able to discover any other
	/// node.
	pub fn connect_to_relay_chain_node(
		mut self,
		node: &polkadot_test_service::PolkadotTestNode,
	) -> Self {
		self.relay_chain_nodes.push(node.addr.clone());
		self
	}

	/// Make the node connect to the given relay chain nodes.
	///
	/// By default the node will not be connected to any node or will be able to discover any other
	/// node.
	pub fn connect_to_relay_chain_nodes<'a>(
		mut self,
		nodes: impl IntoIterator<Item = &'a polkadot_test_service::PolkadotTestNode>,
	) -> Self {
		self.relay_chain_nodes.extend(nodes.into_iter().map(|n| n.addr.clone()));
		self
	}

	/// Wrap the announce block function of this node.
	pub fn wrap_announce_block(
		mut self,
		wrap: impl FnOnce(AnnounceBlockFn) -> AnnounceBlockFn + 'static,
	) -> Self {
		self.wrap_announce_block = Some(Box::new(wrap));
		self
	}

	/// Allows accessing the parachain storage before the test node is built.
	pub fn update_storage_parachain(mut self, updater: impl Fn() + 'static) -> Self {
		self.storage_update_func_parachain = Some(Box::new(updater));
		self
	}

	/// Allows accessing the relay chain storage before the test node is built.
	pub fn update_storage_relay_chain(mut self, updater: impl Fn() + 'static) -> Self {
		self.storage_update_func_relay_chain = Some(Box::new(updater));
		self
	}

	/// Use the null consensus that will never author any block.
	pub fn use_null_consensus(mut self) -> Self {
		self.consensus = Consensus::Null;
		self
	}

	/// Connect to full node via RPC.
	pub fn use_external_relay_chain_node_at_url(mut self, network_address: Url) -> Self {
		self.relay_chain_mode = RelayChainMode::ExternalRpc(vec![network_address]);
		self
	}

	/// Connect to full node via RPC.
	pub fn use_external_relay_chain_node_at_port(mut self, port: u16) -> Self {
		let mut localhost_url =
			Url::parse("ws://localhost").expect("Should be able to parse localhost Url");
		localhost_url.set_port(Some(port)).expect("Should be able to set port");
		self.relay_chain_mode = RelayChainMode::ExternalRpc(vec![localhost_url]);
		self
	}

	/// Accounts which will have an initial balance.
	pub fn endowed_accounts(mut self, accounts: Vec<AccountId>) -> TestNodeBuilder {
		self.endowed_accounts = accounts;
		self
	}

	/// Build the [`TestNode`].
	pub async fn build(self) -> TestNode {
		let parachain_config = node_config(
			self.storage_update_func_parachain.unwrap_or_else(|| Box::new(|| ())),
			self.tokio_handle.clone(),
			self.key,
			self.parachain_nodes,
			self.parachain_nodes_exclusive,
			self.para_id,
			self.collator_key.is_some(),
			self.endowed_accounts,
		)
		.expect("could not generate Configuration");

		let mut relay_chain_config = polkadot_test_service::node_config(
			self.storage_update_func_relay_chain.unwrap_or_else(|| Box::new(|| ())),
			self.tokio_handle,
			self.key,
			self.relay_chain_nodes,
			false,
		);

		let collator_options = CollatorOptions { relay_chain_mode: self.relay_chain_mode };

		relay_chain_config.network.node_name =
			format!("{} (relay chain)", relay_chain_config.network.node_name);

		let multiaddr = parachain_config.network.listen_addresses[0].clone();
		let (task_manager, client, network, rpc_handlers, transaction_pool) = start_node_impl(
			parachain_config,
			self.collator_key,
			relay_chain_config,
			self.para_id,
			self.wrap_announce_block,
			false,
			|_| Ok(jsonrpsee::RpcModule::new(())),
			self.consensus,
			collator_options,
		)
		.await
		.expect("could not create Cumulus test service");

		let peer_id = network.local_peer_id();
		let addr = MultiaddrWithPeerId { multiaddr, peer_id };

		TestNode { task_manager, client, network, addr, rpc_handlers, transaction_pool }
	}
}

/// Create a Cumulus `Configuration`.
///
/// By default an in-memory socket will be used, therefore you need to provide nodes if you want the
/// node to be connected to other nodes. If `nodes_exclusive` is `true`, the node will only connect
/// to the given `nodes` and not to any other node. The `storage_update_func` can be used to make
/// adjustments to the runtime genesis.
pub fn node_config(
	storage_update_func: impl Fn(),
	tokio_handle: tokio::runtime::Handle,
	key: Sr25519Keyring,
	nodes: Vec<MultiaddrWithPeerId>,
	nodes_exlusive: bool,
	para_id: ParaId,
	is_collator: bool,
	endowed_accounts: Vec<AccountId>,
) -> Result<Configuration, ServiceError> {
	let base_path = BasePath::new_temp_dir()?;
	let root = base_path.path().join(format!("cumulus_test_service_{}", key));
	let role = if is_collator { Role::Authority } else { Role::Full };
	let key_seed = key.to_seed();
	let mut spec =
		Box::new(chain_spec::get_chain_spec_with_extra_endowed(para_id, endowed_accounts));

	let mut storage = spec.as_storage_builder().build_storage().expect("could not build storage");

	BasicExternalities::execute_with_storage(&mut storage, storage_update_func);
	spec.set_storage(storage);

	let mut network_config = NetworkConfiguration::new(
		format!("{} (parachain)", key_seed),
		"network/test/0.1",
		Default::default(),
		None,
	);

	if nodes_exlusive {
		network_config.default_peers_set.reserved_nodes = nodes;
		network_config.default_peers_set.non_reserved_mode =
			sc_network::config::NonReservedPeerMode::Deny;
	} else {
		network_config.boot_nodes = nodes;
	}

	network_config.allow_non_globals_in_dht = true;

	network_config
		.listen_addresses
		.push(multiaddr::Protocol::Memory(rand::random()).into());

	network_config.transport = TransportConfig::MemoryOnly;

	Ok(Configuration {
		impl_name: "cumulus-test-node".to_string(),
		impl_version: "0.1".to_string(),
		role,
		tokio_handle,
		transaction_pool: Default::default(),
		network: network_config,
		keystore: KeystoreConfig::InMemory,
		database: DatabaseSource::RocksDb { path: root.join("db"), cache_size: 128 },
		trie_cache_maximum_size: Some(64 * 1024 * 1024),
		state_pruning: Some(PruningMode::ArchiveAll),
		blocks_pruning: BlocksPruning::KeepAll,
		chain_spec: spec,
		wasm_method: WasmExecutionMethod::Compiled {
			instantiation_strategy: sc_executor_wasmtime::InstantiationStrategy::PoolingCopyOnWrite,
		},
		rpc_addr: None,
		rpc_max_connections: Default::default(),
		rpc_cors: None,
		rpc_methods: Default::default(),
		rpc_max_request_size: Default::default(),
		rpc_max_response_size: Default::default(),
		rpc_id_provider: None,
		rpc_max_subs_per_conn: Default::default(),
		rpc_port: 9945,
		prometheus_config: None,
		telemetry_endpoints: None,
		default_heap_pages: None,
		offchain_worker: OffchainWorkerConfig { enabled: true, indexing_enabled: false },
		force_authoring: false,
		disable_grandpa: false,
		dev_key_seed: Some(key_seed),
		tracing_targets: None,
		tracing_receiver: Default::default(),
		max_runtime_instances: 8,
		announce_block: true,
		data_path: root,
		base_path,
		informant_output_format: Default::default(),
		wasm_runtime_overrides: None,
		runtime_cache_size: 2,
	})
}

impl TestNode {
	/// Wait for `count` blocks to be imported in the node and then exit. This function will not
	/// return if no blocks are ever created, thus you should restrict the maximum amount of time of
	/// the test execution.
	pub fn wait_for_blocks(&self, count: usize) -> impl Future<Output = ()> {
		self.client.wait_for_blocks(count)
	}

	/// Send an extrinsic to this node.
	pub async fn send_extrinsic(
		&self,
		function: impl Into<runtime::RuntimeCall>,
		caller: Sr25519Keyring,
	) -> Result<RpcTransactionOutput, RpcTransactionError> {
		let extrinsic = construct_extrinsic(&self.client, function, caller.pair(), Some(0));

		self.rpc_handlers.send_transaction(extrinsic.into()).await
	}

	/// Register a parachain at this relay chain.
	pub async fn schedule_upgrade(&self, validation: Vec<u8>) -> Result<(), RpcTransactionError> {
		let call = frame_system::Call::set_code { code: validation };

		self.send_extrinsic(
			runtime::SudoCall::sudo_unchecked_weight {
				call: Box::new(call.into()),
				weight: Weight::from_parts(1_000, 0),
			},
			Sr25519Keyring::Alice,
		)
		.await
		.map(drop)
	}
}

/// Fetch account nonce for key pair
pub fn fetch_nonce(client: &Client, account: sp_core::sr25519::Public) -> u32 {
	let best_hash = client.chain_info().best_hash;
	client
		.runtime_api()
		.account_nonce(best_hash, account.into())
		.expect("Fetching account nonce works; qed")
}

/// Construct an extrinsic that can be applied to the test runtime.
pub fn construct_extrinsic(
	client: &Client,
	function: impl Into<runtime::RuntimeCall>,
	caller: sp_core::sr25519::Pair,
	nonce: Option<u32>,
) -> runtime::UncheckedExtrinsic {
	let function = function.into();
	let current_block_hash = client.info().best_hash;
	let current_block = client.info().best_number.saturated_into();
	let genesis_block = client.hash(0).unwrap().unwrap();
	let nonce = nonce.unwrap_or_else(|| fetch_nonce(client, caller.public()));
	let period = runtime::BlockHashCount::get()
		.checked_next_power_of_two()
		.map(|c| c / 2)
		.unwrap_or(2) as u64;
	let tip = 0;
	let extra: runtime::SignedExtra = (
		frame_system::CheckNonZeroSender::<runtime::Runtime>::new(),
		frame_system::CheckSpecVersion::<runtime::Runtime>::new(),
		frame_system::CheckGenesis::<runtime::Runtime>::new(),
		frame_system::CheckEra::<runtime::Runtime>::from(generic::Era::mortal(
			period,
			current_block,
		)),
		frame_system::CheckNonce::<runtime::Runtime>::from(nonce),
		frame_system::CheckWeight::<runtime::Runtime>::new(),
		pallet_transaction_payment::ChargeTransactionPayment::<runtime::Runtime>::from(tip),
	);
	let raw_payload = runtime::SignedPayload::from_raw(
		function.clone(),
		extra.clone(),
		((), runtime::VERSION.spec_version, genesis_block, current_block_hash, (), (), ()),
	);
	let signature = raw_payload.using_encoded(|e| caller.sign(e));
	runtime::UncheckedExtrinsic::new_signed(
		function,
		caller.public().into(),
		runtime::Signature::Sr25519(signature),
		extra,
	)
}

/// Run a relay-chain validator node.
///
/// This is essentially a wrapper around
/// [`run_validator_node`](polkadot_test_service::run_validator_node).
pub fn run_relay_chain_validator_node(
	tokio_handle: tokio::runtime::Handle,
	key: Sr25519Keyring,
	storage_update_func: impl Fn(),
	boot_nodes: Vec<MultiaddrWithPeerId>,
	port: Option<u16>,
) -> polkadot_test_service::PolkadotTestNode {
	let mut config = polkadot_test_service::node_config(
		storage_update_func,
		tokio_handle,
		key,
		boot_nodes,
		true,
	);

	if let Some(port) = port {
		config.rpc_addr = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port));
	}

	polkadot_test_service::run_validator_node(
		config,
		Some(cumulus_test_relay_validation_worker_provider::VALIDATION_WORKER.into()),
	)
}
