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

use cumulus_client_collator::service::CollatorService;
use cumulus_client_consensus_aura::{
	collators::{
		lookahead::{self as aura, Params as AuraParams},
		slot_based::{
			self as slot_based, Params as SlotBasedParams, SlotBasedBlockImport,
			SlotBasedBlockImportHandle,
		},
	},
	ImportQueueParams,
};
use prometheus::Registry;
use runtime::AccountId;
use sc_executor::{HeapAllocStrategy, WasmExecutor, DEFAULT_HEAP_ALLOC_STRATEGY};
use sp_consensus_aura::sr25519::AuthorityPair;
use std::{
	collections::HashSet,
	future::Future,
	net::{Ipv4Addr, SocketAddr, SocketAddrV4},
	time::Duration,
};
use url::Url;

use crate::runtime::Weight;
use cumulus_client_cli::{CollatorOptions, RelayChainMode};
use cumulus_client_consensus_common::ParachainBlockImport as TParachainBlockImport;
use cumulus_client_pov_recovery::{RecoveryDelayRange, RecoveryHandle};
use cumulus_client_service::{
	build_network, prepare_node_config, start_relay_chain_tasks, BuildNetworkParams,
	CollatorSybilResistance, DARecoveryProfile, ParachainTracingExecuteBlock,
	StartRelayChainTasksParams,
};
use codec::Decode;
use cumulus_client_speculative_messaging::protocol::{
	ForwardMessageRequest, ForwardMessageResponse, PROTOCOL_NAME as SPEC_MSG_PROTOCOL_NAME,
};
use cumulus_pallet_speculative_messaging::inherent::SpecMsgInherentDataProvider;
use cumulus_primitives_core::{relay_chain::ValidationCode, GetParachainInfo, ParaId};
use cumulus_relay_chain_inprocess_interface::RelayChainInProcessInterface;
use cumulus_relay_chain_interface::{RelayChainError, RelayChainInterface, RelayChainResult};
use cumulus_relay_chain_minimal_node::build_minimal_relay_chain_node_with_rpc;
use parking_lot::Mutex;
use polkadot_primitives_speculative_messaging::{
	MessageBatch, OutgoingMessage, StoredMerkleTree,
};

use cumulus_test_runtime::{Hash, NodeBlock as Block, RuntimeApi};

use frame_system_rpc_runtime_api::AccountNonceApi;
use polkadot_node_subsystem::{errors::RecoveryError, messages::AvailabilityRecoveryMessage};
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{CandidateHash, CollatorPair};
use polkadot_service::ProvideRuntimeApi;
use sc_consensus::ImportQueue;
use sc_network::{
	config::{FullNetworkConfiguration, NetworkBackendType, TransportConfig},
	multiaddr,
	request_responses::{IncomingRequest, OutgoingResponse},
	service::traits::{NetworkBackend, NetworkService},
	NetworkBlock, NetworkStateInfo, ProtocolName,
};
use sc_service::{
	config::{
		BlocksPruning, DatabaseSource, ExecutorConfiguration, KeystoreConfig, MultiaddrWithPeerId,
		NetworkConfiguration, OffchainWorkerConfig, PruningMode, RpcBatchRequestConfig,
		RpcConfiguration, RpcEndpoint, WasmExecutionMethod,
	},
	BasePath, ChainSpec as ChainSpecService, Configuration, Error as ServiceError,
	PartialComponents, Role, RpcHandlers, TFullBackend, TFullClient, TaskManager,
};
use sp_arithmetic::traits::SaturatedConversion;
use sp_blockchain::HeaderBackend;
use sp_core::Pair;
use sp_keyring::Sr25519Keyring;
use sp_core::H256;
use sp_runtime::{codec::Encode, generic, MultiAddress};
use sp_state_machine::BasicExternalities;
use std::sync::Arc;
use substrate_test_client::{
	BlockchainEventsExt, RpcHandlersExt, RpcTransactionError, RpcTransactionOutput,
};

pub use chain_spec::*;
pub use cumulus_test_runtime as runtime;
pub use sp_keyring::Sr25519Keyring as Keyring;

const LOG_TARGET: &str = "cumulus-test-service";

/// The signature of the announce block fn.
pub type AnnounceBlockFn = Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>;

type HostFunctions =
	(sp_io::SubstrateHostFunctions, cumulus_client_service::storage_proof_size::HostFunctions);
/// The client type being used by the test service.
pub type Client = TFullClient<runtime::NodeBlock, runtime::RuntimeApi, WasmExecutor<HostFunctions>>;

/// The backend type being used by the test service.
pub type Backend = TFullBackend<Block>;

/// The block-import type being used by the test service.
pub type ParachainBlockImport =
	TParachainBlockImport<Block, SlotBasedBlockImport<Block, Arc<Client>, Client>, Backend>;

/// Transaction pool type used by the test service
pub type TransactionPool = Arc<sc_transaction_pool::TransactionPoolHandle<Block, Client>>;

/// Recovery handle that fails regularly to simulate unavailable povs.
pub struct FailingRecoveryHandle {
	overseer_handle: OverseerHandle,
	counter: u32,
	failed_hashes: HashSet<CandidateHash>,
}

impl FailingRecoveryHandle {
	/// Create a new FailingRecoveryHandle
	pub fn new(overseer_handle: OverseerHandle) -> Self {
		Self { overseer_handle, counter: 0, failed_hashes: Default::default() }
	}
}

#[async_trait::async_trait]
impl RecoveryHandle for FailingRecoveryHandle {
	async fn send_recovery_msg(
		&mut self,
		message: AvailabilityRecoveryMessage,
		origin: &'static str,
	) {
		let AvailabilityRecoveryMessage::RecoverAvailableData(ref receipt, _, _, _, _) = message;
		let candidate_hash = receipt.hash();

		// For every 3rd block we immediately signal unavailability to trigger
		// a retry. The same candidate is never failed multiple times to ensure progress.
		if self.counter.is_multiple_of(3) && self.failed_hashes.insert(candidate_hash) {
			tracing::info!(target: LOG_TARGET, ?candidate_hash, "Failing pov recovery.");

			let AvailabilityRecoveryMessage::RecoverAvailableData(_, _, _, _, back_sender) =
				message;
			back_sender
				.send(Err(RecoveryError::Unavailable))
				.expect("Return channel should work here.");
		} else {
			self.overseer_handle.send_msg(message, origin).await;
		}
		self.counter += 1;
	}
}

/// Assembly of PartialComponents (enough to run chain ops subcommands)
pub type Service = PartialComponents<
	Client,
	Backend,
	(),
	sc_consensus::import_queue::BasicQueue<Block>,
	sc_transaction_pool::TransactionPoolHandle<Block, Client>,
	(ParachainBlockImport, SlotBasedBlockImportHandle<Block>),
>;

/// Starts a `ServiceBuilder` for a full service.
///
/// Use this macro if you don't actually need the full service, but just the builder in order to
/// be able to perform chain operations.
pub fn new_partial(
	config: &mut Configuration,
	enable_import_proof_record: bool,
) -> Result<Service, sc_service::Error> {
	let heap_pages = config
		.executor
		.default_heap_pages
		.map_or(DEFAULT_HEAP_ALLOC_STRATEGY, |h| HeapAllocStrategy::Static { extra_pages: h as _ });

	let executor = WasmExecutor::builder()
		.with_execution_method(config.executor.wasm_method)
		.with_onchain_heap_alloc_strategy(heap_pages)
		.with_offchain_heap_alloc_strategy(heap_pages)
		.with_max_runtime_instances(config.executor.max_runtime_instances)
		.with_runtime_cache_size(config.executor.runtime_cache_size)
		.build();

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts_record_import::<Block, RuntimeApi, _>(
			config,
			None,
			executor,
			enable_import_proof_record,
			Default::default(),
		)?;
	let client = Arc::new(client);

	let (block_import, slot_based_handle) =
		SlotBasedBlockImport::new(client.clone(), client.clone());
	let block_import = ParachainBlockImport::new(block_import, backend.clone());

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

	let slot_duration = sc_consensus_aura::slot_duration(&*client)?;
	let import_queue = cumulus_client_consensus_aura::import_queue::<AuthorityPair, _, _, _, _, _>(
		ImportQueueParams {
			block_import: block_import.clone(),
			client: client.clone(),
			create_inherent_data_providers: move |_, ()| async move {
				let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

				let slot =
					sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
						*timestamp,
						slot_duration,
					);

				Ok((slot, timestamp))
			},
			spawner: &task_manager.spawn_essential_handle(),
			registry: None,
			telemetry: None,
		},
	)?;

	let params = PartialComponents {
		backend,
		client,
		import_queue,
		keystore_container,
		task_manager,
		transaction_pool,
		select_chain: (),
		other: (block_import, slot_based_handle),
	};

	Ok(params)
}

/// Create a spec-msg request-response protocol configuration.
///
/// Uses genesis hash in the protocol name for proper litep2p negotiation,
/// matching the pattern used by other relay chain request-response protocols
/// (e.g., the bootnode protocol).
fn spec_msg_request_response_config<
	B: sp_runtime::traits::Block,
	N: NetworkBackend<B, <B as sp_runtime::traits::Block>::Hash>,
>(
	genesis_hash: &[u8],
) -> (N::RequestResponseProtocolConfig, async_channel::Receiver<IncomingRequest>, String) {
	let (tx, rx) = async_channel::bounded(100);
	let hex: String = genesis_hash.iter().map(|b| format!("{b:02x}")).collect();
	let protocol_name = format!("/{hex}/spec-msg/1");
	tracing::info!(
		target: LOG_TARGET,
		%protocol_name,
		"Registering spec-msg request-response protocol",
	);
	let config = N::request_response_config(
		ProtocolName::from(protocol_name.clone()),
		Vec::new(),
		16 * 1024 * 1024, // MAX_REQUEST_SIZE
		1024,             // MAX_RESPONSE_SIZE
		Duration::from_secs(20),
		Some(tx),
	);
	(config, rx, protocol_name)
}

/// Build a relay chain full node using `PolkadotServiceBuilder`, injecting
/// the spec-msg request-response protocol.
fn build_relay_with_spec_msg_protocol<Network>(
	config: Configuration,
	collator_key: Option<CollatorPair>,
) -> Result<
	(polkadot_service::NewFull, async_channel::Receiver<IncomingRequest>, String),
	polkadot_service::Error,
>
where
	Network: NetworkBackend<
		polkadot_primitives::Block,
		<polkadot_primitives::Block as sp_runtime::traits::Block>::Hash,
	>,
{
	use polkadot_service::builder::PolkadotServiceBuilder;

	let is_parachain_node = if let Some(ref key) = collator_key {
		polkadot_service::IsParachainNode::Collator(key.clone())
	} else {
		polkadot_service::IsParachainNode::Collator(CollatorPair::generate().0)
	};

	let mut workers_path = std::env::current_exe()
		.expect("current_exe should be available in test context; qed");
	workers_path.pop();
	workers_path.pop();

	let params = polkadot_service::NewFullParams {
		is_parachain_node,
		enable_beefy: true,
		force_authoring_backoff: false,
		telemetry_worker_handle: None,
		node_version: None,
		secure_validator_mode: false,
		workers_path: Some(workers_path),
		workers_names: None,
		overseer_gen: polkadot_service::CollatorOverseerGen,
		overseer_message_channel_capacity_override: None,
		malus_finality_delay: None,
		hwbench: None,
		execute_workers_max_num: None,
		prepare_workers_hard_max_num: None,
		prepare_workers_soft_max_num: None,
		keep_finalized_for: None,
		invulnerable_ah_collators: HashSet::new(),
		collator_protocol_hold_off: None,
		experimental_collator_protocol: false,
		collator_reputation_persist_interval: None,
	};

	let mut builder = PolkadotServiceBuilder::<_, Network>::new(config, params)?;

	// Inject spec-msg request-response protocol (genesis-hash-prefixed name)
	let genesis_hash = builder.genesis_hash();
	let (spec_msg_config, spec_msg_rx, protocol_name) =
		spec_msg_request_response_config::<polkadot_primitives::Block, Network>(
			genesis_hash.as_ref(),
		);
	builder.add_extra_request_response_protocol(spec_msg_config);

	Ok((builder.build()?, spec_msg_rx, protocol_name))
}

/// Result of building the relay chain interface with spec-msg support.
struct RelayChainBuildResult {
	relay_chain_interface: Arc<dyn RelayChainInterface + 'static>,
	/// Relay chain network service (for outbound requests + reading PeerId).
	/// `None` in external RPC mode.
	relay_network: Option<Arc<dyn NetworkService>>,
	/// Inbound spec-msg requests from remote relay peers.
	/// `None` in external RPC mode.
	spec_msg_rx: Option<async_channel::Receiver<IncomingRequest>>,
	/// The genesis-hash-prefixed protocol name for spec-msg requests.
	spec_msg_protocol: Option<String>,
}

async fn build_relay_chain_interface(
	relay_chain_config: Configuration,
	parachain_prometheus_registry: Option<&Registry>,
	collator_key: Option<CollatorPair>,
	collator_options: CollatorOptions,
	task_manager: &mut TaskManager,
) -> RelayChainResult<RelayChainBuildResult> {
	match collator_options.relay_chain_mode {
		cumulus_client_cli::RelayChainMode::Embedded => {
			let (relay_chain_full_node, spec_msg_rx, protocol_name) =
				match relay_chain_config.network.network_backend {
					NetworkBackendType::Libp2p =>
						build_relay_with_spec_msg_protocol::<sc_network::NetworkWorker<_, _>>(
							relay_chain_config,
							collator_key,
						),
					NetworkBackendType::Litep2p =>
						build_relay_with_spec_msg_protocol::<sc_network::Litep2pNetworkBackend>(
							relay_chain_config,
							collator_key,
						),
				}
				.map_err(|e| RelayChainError::Application(Box::new(e) as Box<_>))?;

			let relay_network = relay_chain_full_node.network.clone();

			let relay_chain_interface = Arc::new(RelayChainInProcessInterface::new(
				relay_chain_full_node.client.clone(),
				relay_chain_full_node.backend.clone(),
				relay_chain_full_node.sync_service.clone(),
				relay_chain_full_node.overseer_handle.ok_or(
					RelayChainError::GenericError(
						"Overseer should be running in full node.".to_string(),
					),
				)?,
			));

			task_manager.add_child(relay_chain_full_node.task_manager);
			tracing::info!("Using inprocess node with spec-msg protocol.");

			Ok(RelayChainBuildResult {
				relay_chain_interface,
				relay_network: Some(relay_network),
				spec_msg_rx: Some(spec_msg_rx),
				spec_msg_protocol: Some(protocol_name),
			})
		},
		cumulus_client_cli::RelayChainMode::ExternalRpc(rpc_target_urls) => {
			let (relay_chain_interface, _, _, _) = build_minimal_relay_chain_node_with_rpc(
				relay_chain_config,
				parachain_prometheus_registry,
				task_manager,
				rpc_target_urls,
			)
			.await?;

			Ok(RelayChainBuildResult {
				relay_chain_interface,
				relay_network: None,
				spec_msg_rx: None,
				spec_msg_protocol: None,
			})
		},
	}
}

/// Start a node with the given parachain `Configuration` and relay chain `Configuration`.
///
/// This is the actual implementation that is abstract over the executor and the runtime api.
#[sc_tracing::logging::prefix_logs_with("Parachain")]
pub async fn start_node_impl<RB, Net: NetworkBackend<Block, Hash>>(
	parachain_config: Configuration,
	collator_key: Option<CollatorPair>,
	relay_chain_config: Configuration,
	wrap_announce_block: Option<Box<dyn FnOnce(AnnounceBlockFn) -> AnnounceBlockFn>>,
	fail_pov_recovery: bool,
	rpc_ext_builder: RB,
	collator_options: CollatorOptions,
	proof_recording_during_import: bool,
	use_slot_based_collator: bool,
) -> sc_service::error::Result<(
	TaskManager,
	Arc<Client>,
	Arc<dyn NetworkService>,
	RpcHandlers,
	TransactionPool,
	Arc<Backend>,
)>
where
	RB: Fn(Arc<Client>) -> Result<jsonrpsee::RpcModule<()>, sc_service::Error> + Send + 'static,
{
	let mut parachain_config = prepare_node_config(parachain_config);

	let params = new_partial(&mut parachain_config, proof_recording_during_import)?;

	let transaction_pool = params.transaction_pool.clone();
	let mut task_manager = params.task_manager;

	let client = params.client.clone();
	let backend = params.backend.clone();

	let block_import = params.other.0;
	let slot_based_handle = params.other.1;
	let relay_build = build_relay_chain_interface(
		relay_chain_config,
		parachain_config.prometheus_registry(),
		collator_key.clone(),
		collator_options.clone(),
		&mut task_manager,
	)
	.await
	.map_err(|e| sc_service::Error::Application(Box::new(e) as Box<_>))?;

	let relay_chain_interface = relay_build.relay_chain_interface;
	let relay_network = relay_build.relay_network;
	let spec_msg_rx = relay_build.spec_msg_rx;
	let spec_msg_protocol = relay_build.spec_msg_protocol;

	let import_queue_service = params.import_queue.service();
	let prometheus_registry = parachain_config.prometheus_registry().cloned();
	let net_config = FullNetworkConfiguration::<Block, Hash, Net>::new(
		&parachain_config.network,
		prometheus_registry.clone(),
	);

	let best_hash = client.chain_info().best_hash;
	let para_id = client
		.runtime_api()
		.parachain_id(best_hash)
		.map_err(|e| sc_service::Error::Application(Box::new(e) as Box<_>))?;
	tracing::info!("Parachain id: {:?}", para_id);

	let (network, system_rpc_tx, tx_handler_controller, sync_service) =
		build_network(BuildNetworkParams {
			parachain_config: &parachain_config,
			net_config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			para_id,
			spawn_handle: task_manager.spawn_handle(),
			spawn_essential_handle: task_manager.spawn_essential_handle(),
			relay_chain_interface: relay_chain_interface.clone(),
			import_queue: params.import_queue,
			metrics: Net::register_notification_metrics(
				parachain_config.prometheus_config.as_ref().map(|config| &config.registry),
			),
			sybil_resistance_level: CollatorSybilResistance::Resistant,
		})
		.await?;

	let keystore = params.keystore_container.keystore();
	let rpc_builder = {
		let client = client.clone();
		Box::new(move |_| rpc_ext_builder(client.clone()))
	};

	let rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		rpc_builder,
		client: client.clone(),
		transaction_pool: transaction_pool.clone(),
		task_manager: &mut task_manager,
		config: parachain_config,
		keystore: keystore.clone(),
		backend: backend.clone(),
		network: network.clone(),
		sync_service: sync_service.clone(),
		system_rpc_tx,
		tx_handler_controller,
		telemetry: None,
		tracing_execute_block: Some(Arc::new(ParachainTracingExecuteBlock::new(client.clone()))),
	})?;

	let announce_block = {
		let sync_service = sync_service.clone();
		Arc::new(move |hash, data| sync_service.announce_block(hash, data))
	};

	let announce_block = wrap_announce_block
		.map(|w| (w)(announce_block.clone()))
		.unwrap_or_else(|| announce_block);

	let overseer_handle = relay_chain_interface
		.overseer_handle()
		.map_err(|e| sc_service::Error::Application(Box::new(e)))?;

	let recovery_handle: Box<dyn RecoveryHandle> = if fail_pov_recovery {
		Box::new(FailingRecoveryHandle::new(overseer_handle.clone()))
	} else {
		Box::new(overseer_handle.clone())
	};
	let relay_chain_slot_duration = Duration::from_secs(6);

	start_relay_chain_tasks(StartRelayChainTasksParams {
		client: client.clone(),
		announce_block: announce_block.clone(),
		para_id,
		relay_chain_interface: relay_chain_interface.clone(),
		task_manager: &mut task_manager,
		// Increase speed of recovery for testing purposes.
		da_recovery_profile: DARecoveryProfile::Other(RecoveryDelayRange {
			min: Duration::from_secs(1),
			max: Duration::from_secs(5),
		}),
		import_queue: import_queue_service,
		relay_chain_slot_duration,
		recovery_handle,
		sync_service: sync_service.clone(),
		prometheus_registry: None,
	})?;

	// =========================================================================
	// Speculative messaging workers
	// =========================================================================

	// Shared incoming message metadata queue: Vec<(source, count, provides_root)>
	let incoming_queue: Arc<Mutex<Vec<(ParaId, u64, H256)>>> =
		Arc::new(Mutex::new(Vec::new()));

	// Spawn INBOUND handler: receives ForwardMessageRequest from relay peers,
	// validates, and queues metadata for the inherent data provider.
	if let Some(spec_msg_rx) = spec_msg_rx {
		let queue_for_handler = incoming_queue.clone();
		task_manager.spawn_handle().spawn("spec-msg-inbound", None, async move {
			while let Ok(req) = spec_msg_rx.recv().await {
				let IncomingRequest { payload, pending_response, peer } = req;

				match ForwardMessageRequest::decode(&mut &payload[..]) {
					Ok(fwd_req) => {
						let batch = &fwd_req.batch;
						let source = fwd_req.source_para;
						let count = batch.messages.len() as u64;
						let provides_root = batch.provides_root;

						tracing::debug!(
							target: LOG_TARGET,
							?source,
							count,
							?provides_root,
							?peer,
							"Received spec-msg batch",
						);

						// Queue metadata for the inherent data provider
						queue_for_handler.lock().push((source, count, provides_root));

						// Send acceptance response
						let response_bytes =
							ForwardMessageResponse::Accepted.encode();
						let _ = pending_response.send(OutgoingResponse {
							result: Ok(response_bytes),
							reputation_changes: Vec::new(),
							sent_feedback: None,
						});
					},
					Err(e) => {
						tracing::warn!(
							target: LOG_TARGET,
							?peer,
							error = ?e,
							"Failed to decode spec-msg request",
						);
						let response_bytes =
							ForwardMessageResponse::rejected("decode error").encode();
						let _ = pending_response.send(OutgoingResponse {
							result: Ok(response_bytes),
							reputation_changes: Vec::new(),
							sent_feedback: None,
						});
					},
				}
			}
			tracing::info!(target: LOG_TARGET, "spec-msg-inbound worker exiting");
		});
	}

	// Spawn OUTBOUND distributor: watches **best** (not finalized) parachain
	// blocks, reads PendingOutgoing from runtime storage, builds MessageBatch,
	// and sends to destination relay peers immediately. This is the
	// "speculative" part — we forward messages before they are finalized,
	// achieving ~1 relay block latency instead of waiting for finality
	// (which would add 2-3 relay blocks of delay).
	if let (Some(ref relay_net), Some(ref protocol)) = (&relay_network, &spec_msg_protocol) {
		let outbound_client = client.clone();
		let outbound_relay_net = relay_net.clone();
		let outbound_para_id = para_id;
		let outbound_protocol = protocol.clone();

		task_manager.spawn_handle().spawn("spec-msg-outbound", None, async move {
			use sc_client_api::{BlockchainEvents, StorageProvider};
			use sp_core::storage::StorageKey;

			// Subscribe to best (imported) blocks — NOT finalized.
			// This lets us forward messages as soon as the block is
			// built, before the relay chain has even included the
			// candidate. The relay chain's provides/requires matching
			// is the safety net if the block is reverted.
			let mut import_stream = outbound_client.import_notification_stream();

			while let Some(notification) = {
				use futures::StreamExt;
				import_stream.next().await
			} {
				// Only act on new best blocks (skip reorgs and non-best)
				if !notification.is_new_best {
					continue;
				}
				let block_hash = notification.hash;

				// Read PendingOutgoing storage for all destinations.
				// Storage prefix: twox_128("SpeculativeMessaging") ++ twox_128("PendingOutgoing")
				let pallet_prefix = sp_io::hashing::twox_128(b"SpeculativeMessaging");
				let storage_prefix = sp_io::hashing::twox_128(b"PendingOutgoing");
				let mut prefix_key = Vec::with_capacity(32);
				prefix_key.extend_from_slice(&pallet_prefix);
				prefix_key.extend_from_slice(&storage_prefix);

				let keys = match outbound_client.storage_keys(
					block_hash,
					Some(&StorageKey(prefix_key.clone())),
					None,
				) {
					Ok(keys) => keys.collect::<Vec<_>>(),
					Err(e) => {
						tracing::debug!(
							target: LOG_TARGET,
							?e,
							"Failed to read PendingOutgoing keys",
						);
						continue;
					},
				};

				if keys.is_empty() {
					continue;
				}

				// Read TopLevelTree for proof generation
				let tree_key = {
					let mut k = Vec::with_capacity(32);
					k.extend_from_slice(&pallet_prefix);
					k.extend_from_slice(&sp_io::hashing::twox_128(b"TopLevelTree"));
					k
				};
				let top_level_tree: Option<StoredMerkleTree> = outbound_client
					.storage(block_hash, &StorageKey(tree_key))
					.ok()
					.flatten()
					.and_then(|data| Decode::decode(&mut &data.0[..]).ok());

				let top_level_tree = match top_level_tree {
					Some(tree) => tree,
					None => continue,
				};

				let provides_root = top_level_tree.provides_commitment().root;

				// Read RelayPeers storage prefix
				let relay_peers_prefix = {
					let mut k = Vec::with_capacity(32);
					k.extend_from_slice(&pallet_prefix);
					k.extend_from_slice(&sp_io::hashing::twox_128(b"RelayPeers"));
					k
				};

				// For each destination with pending messages
				for storage_key in &keys {
					// Decode destination ParaId from the storage key
					// Key format: pallet_prefix(16) + storage_prefix(16) + Twox64Concat(dest)
					// Twox64Concat = twox_64(encoded) ++ encoded
					let key_bytes = &storage_key.0;
					if key_bytes.len() <= 32 + 8 {
						continue;
					}
					let dest_encoded = &key_bytes[32 + 8..]; // skip prefix + twox64 hash
					let destination: ParaId = match Decode::decode(&mut &dest_encoded[..]) {
						Ok(id) => id,
						Err(_) => continue,
					};

					// Read the messages
					let messages: Vec<OutgoingMessage> = match outbound_client
						.storage(block_hash, storage_key)
					{
						Ok(Some(data)) => match Decode::decode(&mut &data.0[..]) {
							Ok(msgs) => msgs,
							Err(_) => continue,
						},
						_ => continue,
					};

					if messages.is_empty() {
						continue;
					}

					// Generate subtree proof for destination
					let (subtree_root, subtree_proof) =
						match top_level_tree.generate_proof(destination) {
							Ok(proof) => proof,
							Err(e) => {
								tracing::warn!(
									target: LOG_TARGET,
									?destination,
									?e,
									"Failed to generate subtree proof",
								);
								continue;
							},
						};

					let batch = MessageBatch {
						source: outbound_para_id,
						source_block: block_hash,
						provides_root,
						subtree_root,
						subtree_inclusion_proof: subtree_proof,
						messages,
					};

					let fwd_request = ForwardMessageRequest {
						source_para: outbound_para_id,
						destination_para: destination,
						batch,
					};

					// Look up relay peer for destination
					let peer_key = {
						let dest_encoded = destination.encode();
						let twox64 = sp_io::hashing::twox_64(&dest_encoded);
						let mut k = relay_peers_prefix.clone();
						k.extend_from_slice(&twox64);
						k.extend_from_slice(&dest_encoded);
						k
					};

					let peer_id_bytes: Option<Vec<u8>> = outbound_client
						.storage(block_hash, &StorageKey(peer_key))
						.ok()
						.flatten()
						.and_then(|data| Decode::decode(&mut &data.0[..]).ok());

					let peer_id_bytes = match peer_id_bytes {
						Some(bytes) => bytes,
						None => {
							tracing::debug!(
								target: LOG_TARGET,
								?destination,
								"No relay peer registered for destination",
							);
							continue;
						},
					};

					// Convert to PeerId and send via relay network
					let peer_id =
						match sc_network::PeerId::from_bytes(&peer_id_bytes) {
							Ok(id) => id,
							Err(e) => {
								tracing::warn!(
									target: LOG_TARGET,
									?destination,
									?e,
									"Invalid relay peer ID",
								);
								continue;
							},
						};

					let request_payload = fwd_request.encode();
					let msg_count = fwd_request.batch.messages.len();
					tracing::info!(
						target: LOG_TARGET,
						?destination,
						?peer_id,
						msg_count,
						"Sending spec-msg batch to relay peer",
					);

					let relay_net = outbound_relay_net.clone();
					let protocol =
						ProtocolName::from(outbound_protocol.clone());
					tokio::spawn(async move {
						match relay_net
							.request(
								peer_id.into(),
								protocol,
								request_payload,
								None,
								sc_network::IfDisconnected::TryConnect,
							)
							.await
						{
							Ok((resp, _proto)) => tracing::info!(
								target: LOG_TARGET,
								resp_len = resp.len(),
								"spec-msg request succeeded",
							),
							Err(e) => tracing::warn!(
								target: LOG_TARGET,
								?e,
								"spec-msg request failed",
							),
						}
					});
				}
			}
			tracing::info!(target: LOG_TARGET, "spec-msg-outbound worker exiting");
		});
	}

	let collator_peer_id = network.local_peer_id();
	if let Some(collator_key) = collator_key {
		let proposer = sc_basic_authorship::ProposerFactory::new(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool.clone(),
			prometheus_registry.as_ref(),
			None,
		);

		let collator_service = CollatorService::new(
			client.clone(),
			Arc::new(task_manager.spawn_handle()),
			announce_block,
			client.clone(),
		);

		let client_for_aura = client.clone();

		if use_slot_based_collator {
			tracing::info!(target: LOG_TARGET, "Starting block authoring with slot based authoring.");
			let queue_for_slot = incoming_queue.clone();
			let params = SlotBasedParams {
				create_inherent_data_providers: move |_, ()| {
					let queue = queue_for_slot.clone();
					async move {
						let entries: Vec<(ParaId, u64, H256)> =
							std::mem::take(&mut *queue.lock());
						Ok(SpecMsgInherentDataProvider::new(entries))
					}
				},
				block_import,
				para_client: client.clone(),
				para_backend: backend.clone(),
				relay_client: relay_chain_interface,
				code_hash_provider: move |block_hash| {
					client_for_aura.code_at(block_hash).ok().map(|c| ValidationCode::from(c).hash())
				},
				keystore,
				collator_key,
				relay_chain_slot_duration,
				para_id,
				proposer,
				collator_service,
				authoring_duration: Duration::from_millis(2000),
				reinitialize: false,
				slot_offset: Duration::from_secs(1),
				block_import_handle: slot_based_handle,
				spawner: task_manager.spawn_essential_handle(),
				export_pov: None,
				max_pov_percentage: None,
				collator_peer_id,
			};

			slot_based::run::<Block, AuthorityPair, _, _, _, _, _, _, _, _, _>(params);
		} else {
			tracing::info!(target: LOG_TARGET, "Starting block authoring with lookahead collator.");
			let queue_for_lookahead = incoming_queue.clone();
			let params = AuraParams {
				create_inherent_data_providers: move |_, ()| {
					let queue = queue_for_lookahead.clone();
					async move {
						let entries: Vec<(ParaId, u64, H256)> =
							std::mem::take(&mut *queue.lock());
						Ok(SpecMsgInherentDataProvider::new(entries))
					}
				},
				block_import,
				para_client: client.clone(),
				para_backend: backend.clone(),
				relay_client: relay_chain_interface,
				code_hash_provider: move |block_hash| {
					client_for_aura.code_at(block_hash).ok().map(|c| ValidationCode::from(c).hash())
				},
				keystore,
				collator_key,
				collator_peer_id,
				para_id,
				overseer_handle,
				relay_chain_slot_duration,
				proposer,
				collator_service,
				authoring_duration: Duration::from_millis(2000),
				reinitialize: false,
				max_pov_percentage: None,
			};

			let fut = aura::run::<Block, AuthorityPair, _, _, _, _, _, _, _, _>(params);
			task_manager.spawn_essential_handle().spawn("aura", None, fut);
		}
	}

	Ok((task_manager, client, network, rpc_handlers, transaction_pool, backend))
}

/// A Cumulus test node instance used for testing.
pub struct TestNode {
	/// TaskManager's instance.
	pub task_manager: TaskManager,
	/// Client's instance.
	pub client: Arc<Client>,
	/// Node's network.
	pub network: Arc<dyn NetworkService>,
	/// The `MultiaddrWithPeerId` to this node. This is useful if you want to pass it as "boot
	/// node" to other nodes.
	pub addr: MultiaddrWithPeerId,
	/// RPCHandlers to make RPC queries.
	pub rpc_handlers: RpcHandlers,
	/// Node's transaction pool
	pub transaction_pool: TransactionPool,
	/// Node's backend
	pub backend: Arc<Backend>,
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
	relay_chain_mode: RelayChainMode,
	endowed_accounts: Vec<AccountId>,
	record_proof_during_import: bool,
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
			endowed_accounts: Default::default(),
			relay_chain_mode: RelayChainMode::Embedded,
			record_proof_during_import: true,
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

	/// Record proofs during import.
	pub fn import_proof_recording(mut self, should_record_proof: bool) -> TestNodeBuilder {
		self.record_proof_during_import = should_record_proof;
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

		let collator_options = CollatorOptions {
			relay_chain_mode: self.relay_chain_mode,
			embedded_dht_bootnode: true,
			dht_bootnode_discovery: true,
		};

		relay_chain_config.network.node_name =
			format!("{} (relay chain)", relay_chain_config.network.node_name);

		let (task_manager, client, network, rpc_handlers, transaction_pool, backend) =
			match relay_chain_config.network.network_backend {
				sc_network::config::NetworkBackendType::Libp2p => {
					start_node_impl::<_, sc_network::NetworkWorker<_, _>>(
						parachain_config,
						self.collator_key,
						relay_chain_config,
						self.wrap_announce_block,
						false,
						|_| Ok(jsonrpsee::RpcModule::new(())),
						collator_options,
						self.record_proof_during_import,
						false,
					)
					.await
					.expect("could not create Cumulus test service")
				},
				sc_network::config::NetworkBackendType::Litep2p => {
					start_node_impl::<_, sc_network::Litep2pNetworkBackend>(
						parachain_config,
						self.collator_key,
						relay_chain_config,
						self.wrap_announce_block,
						false,
						|_| Ok(jsonrpsee::RpcModule::new(())),
						collator_options,
						self.record_proof_during_import,
						false,
					)
					.await
					.expect("could not create Cumulus test service")
				},
			};
		let peer_id = network.local_peer_id();
		let multiaddr = polkadot_test_service::get_listen_address(network.clone()).await;
		let addr = MultiaddrWithPeerId { multiaddr, peer_id };

		TestNode { task_manager, client, network, addr, rpc_handlers, transaction_pool, backend }
	}
}

/// Create a Cumulus `Configuration`.
///
/// By default a TCP socket will be used, therefore you need to provide nodes if you want the
/// node to be connected to other nodes.
///
/// If `nodes_exclusive` is `true`, the node will only connect to the given `nodes` and not to any
/// other node.
///
/// The `storage_update_func` can be used to make adjustments to the runtime genesis.
pub fn node_config(
	storage_update_func: impl Fn(),
	tokio_handle: tokio::runtime::Handle,
	key: Sr25519Keyring,
	nodes: Vec<MultiaddrWithPeerId>,
	nodes_exclusive: bool,
	para_id: ParaId,
	is_collator: bool,
	endowed_accounts: Vec<AccountId>,
) -> Result<Configuration, ServiceError> {
	let base_path = BasePath::new_temp_dir()?;
	let root = base_path.path().join(format!("cumulus_test_service_{}", key));
	let role = if is_collator { Role::Authority } else { Role::Full };
	let key_seed = key.to_seed();
	let mut spec = Box::new(chain_spec::get_chain_spec_with_extra_endowed(
		Some(para_id),
		endowed_accounts,
		cumulus_test_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
	));

	let mut storage = spec.as_storage_builder().build_storage().expect("could not build storage");

	BasicExternalities::execute_with_storage(&mut storage, storage_update_func);
	spec.set_storage(storage);

	let mut network_config = NetworkConfiguration::new(
		format!("{} (parachain)", key_seed),
		"network/test/0.1",
		Default::default(),
		None,
	);

	if nodes_exclusive {
		network_config.default_peers_set.reserved_nodes = nodes;
		network_config.default_peers_set.non_reserved_mode =
			sc_network::config::NonReservedPeerMode::Deny;
	} else {
		network_config.boot_nodes = nodes;
	}

	network_config.allow_non_globals_in_dht = true;

	let addr: multiaddr::Multiaddr = "/ip4/127.0.0.1/tcp/0".parse().expect("valid address; qed");
	network_config.listen_addresses.push(addr.clone());
	network_config.transport =
		TransportConfig::Normal { enable_mdns: false, allow_private_ip: true };

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
		warm_up_trie_cache: None,
		state_pruning: Some(PruningMode::ArchiveAll),
		blocks_pruning: BlocksPruning::KeepAll,
		chain_spec: spec,
		executor: ExecutorConfiguration {
			wasm_method: WasmExecutionMethod::Compiled {
				instantiation_strategy:
					sc_executor_wasmtime::InstantiationStrategy::PoolingCopyOnWrite,
			},
			..ExecutorConfiguration::default()
		},
		rpc: RpcConfiguration {
			addr: None,
			max_connections: Default::default(),
			cors: None,
			methods: Default::default(),
			max_request_size: Default::default(),
			max_response_size: Default::default(),
			id_provider: None,
			max_subs_per_conn: Default::default(),
			port: 9945,
			message_buffer_capacity: Default::default(),
			batch_config: RpcBatchRequestConfig::Unlimited,
			rate_limit: None,
			rate_limit_whitelisted_ips: Default::default(),
			rate_limit_trust_proxy_headers: Default::default(),
			request_logger_limit: 1024,
		},
		prometheus_config: None,
		telemetry_endpoints: None,
		offchain_worker: OffchainWorkerConfig { enabled: true, indexing_enabled: false },
		force_authoring: false,
		disable_grandpa: false,
		dev_key_seed: Some(key_seed),
		tracing_targets: None,
		tracing_receiver: Default::default(),
		announce_block: true,
		data_path: root,
		base_path,
		wasm_runtime_overrides: None,
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
	let tx_ext: runtime::TxExtension = (
		frame_system::AuthorizeCall::<runtime::Runtime>::new(),
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
	)
		.into();
	let raw_payload = runtime::SignedPayload::from_raw(
		function.clone(),
		tx_ext.clone(),
		((), (), runtime::VERSION.spec_version, genesis_block, current_block_hash, (), (), ()),
	);
	let signature = raw_payload.using_encoded(|e| caller.sign(e));
	runtime::UncheckedExtrinsic::new_signed(
		function,
		MultiAddress::Id(caller.public().into()),
		runtime::Signature::Sr25519(signature),
		tx_ext,
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
		tokio_handle.clone(),
		key,
		boot_nodes,
		true,
	);

	if let Some(port) = port {
		config.rpc.addr = Some(vec![RpcEndpoint {
			batch_config: config.rpc.batch_config,
			cors: config.rpc.cors.clone(),
			listen_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)),
			max_connections: config.rpc.max_connections,
			max_payload_in_mb: config.rpc.max_request_size,
			max_payload_out_mb: config.rpc.max_response_size,
			max_subscriptions_per_connection: config.rpc.max_subs_per_conn,
			max_buffer_capacity_per_connection: config.rpc.message_buffer_capacity,
			rpc_methods: config.rpc.methods,
			rate_limit: config.rpc.rate_limit,
			rate_limit_trust_proxy_headers: config.rpc.rate_limit_trust_proxy_headers,
			rate_limit_whitelisted_ips: config.rpc.rate_limit_whitelisted_ips.clone(),
			retry_random_port: true,
			is_optional: false,
		}]);
	}

	let mut workers_path = std::env::current_exe().unwrap();
	workers_path.pop();
	workers_path.pop();

	tokio_handle.block_on(async move {
		polkadot_test_service::run_validator_node(config, Some(workers_path)).await
	})
}
