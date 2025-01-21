// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use crate::{
	configuration::TestAuthorities,
	dummy_builder,
	environment::{TestEnvironment, TestEnvironmentDependencies, GENESIS_HASH},
	mock::{
		candidate_backing::MockCandidateBacking,
		chain_api::{ChainApiState, MockChainApi},
		prospective_parachains::MockProspectiveParachains,
		runtime_api::{MockRuntimeApi, MockRuntimeApiCoreState},
		AlwaysSupportsParachains,
	},
	network::{new_network, NetworkEmulatorHandle, NetworkInterface, NetworkInterfaceReceiver},
	usage::BenchmarkUsage,
	NODE_UNDER_TEST,
};
use bitvec::vec::BitVec;
use codec::{Decode, Encode};
use colored::Colorize;
use itertools::Itertools;
use polkadot_network_bridge::WireMessage;
use polkadot_node_metrics::metrics::Metrics;
use polkadot_node_network_protocol::{
	authority_discovery::AuthorityDiscovery,
	grid_topology::{SessionGridTopology, TopologyPeerInfo},
	peer_set::{peer_sets_info, IsAuthority, PeerSet, PeerSetProtocolNames},
	request_response::{
		v2::{AttestedCandidateRequest, AttestedCandidateResponse},
		IncomingRequest, ReqProtocolNames,
	},
	v3::{self, BackedCandidateAcknowledgement, BackedCandidateManifest, StatementFilter},
	view, Versioned, View,
};
use polkadot_node_subsystem::messages::{
	network_bridge_event::NewGossipTopology, AllMessages, NetworkBridgeEvent,
	StatementDistributionMessage,
};
use polkadot_overseer::{
	Handle as OverseerHandle, Overseer, OverseerConnector, OverseerMetrics, SpawnGlue,
};
use polkadot_primitives::{
	AuthorityDiscoveryId, Block, CompactStatement, GroupIndex, Hash, Id, SignedStatement,
	SigningContext, ValidatorId, ValidatorIndex,
};
use polkadot_service::{
	overseer::{NetworkBridgeMetrics, NetworkBridgeRxSubsystem, NetworkBridgeTxSubsystem},
	runtime_traits::Zero,
};
use polkadot_statement_distribution::StatementDistributionSubsystem;
use rand::SeedableRng;
use sc_keystore::LocalKeystore;
use sc_network::{
	config::{
		FullNetworkConfiguration, MultiaddrWithPeerId, NetworkConfiguration, NonReservedPeerMode,
		Params, ProtocolId, Role, SetConfig, TransportConfig,
	},
	multiaddr,
	request_responses::ProtocolConfig,
	service::traits::{NotificationEvent, ValidationResult},
	IfDisconnected, NetworkWorker, NotificationMetrics,
};
use sc_network_common::sync::message::BlockAnnouncesHandshake;
use sc_network_types::PeerId;
use sc_service::SpawnTaskHandle;
use sp_application_crypto::Pair;
use sp_consensus::SyncOracle;
use sp_core::Bytes;
use sp_keystore::{Keystore, KeystorePtr};
use sp_runtime::RuntimeAppPublic;
use std::{
	collections::{HashMap, HashSet},
	sync::{atomic::Ordering, Arc},
	time::{Duration, Instant},
};
pub use test_state::TestState;

mod test_state;

// TODO: use same index for all nodes
const SESSION_INDEX: u32 = 0;
const LOG_TARGET: &str = "subsystem-bench::statement";

pub fn make_keystore() -> KeystorePtr {
	let keystore: KeystorePtr = Arc::new(LocalKeystore::in_memory());
	Keystore::sr25519_generate_new(&*keystore, ValidatorId::ID, Some("//Node0"))
		.expect("Insert key into keystore");
	Keystore::sr25519_generate_new(&*keystore, AuthorityDiscoveryId::ID, Some("//Node0"))
		.expect("Insert key into keystore");
	keystore
}

#[derive(Clone)]
pub struct DummySyncOracle;

impl SyncOracle for DummySyncOracle {
	fn is_major_syncing(&self) -> bool {
		false
	}

	fn is_offline(&self) -> bool {
		false
	}
}

#[derive(Clone, Debug)]
pub struct DummyAuthotiryDiscoveryService {
	by_peer_id: HashMap<PeerId, HashSet<AuthorityDiscoveryId>>,
}

impl DummyAuthotiryDiscoveryService {
	pub fn new(by_peer_id: HashMap<PeerId, AuthorityDiscoveryId>) -> Self {
		Self {
			by_peer_id: by_peer_id
				.into_iter()
				.map(|(peer_id, authority_id)| (peer_id, HashSet::from([authority_id])))
				.collect(),
		}
	}
}

#[async_trait::async_trait]
impl AuthorityDiscovery for DummyAuthotiryDiscoveryService {
	async fn get_addresses_by_authority_id(
		&mut self,
		authority: AuthorityDiscoveryId,
	) -> Option<HashSet<sc_network::Multiaddr>> {
		todo!("get_addresses_by_authority_id authority: {:?}", authority);
	}

	async fn get_authority_ids_by_peer_id(
		&mut self,
		peer_id: PeerId,
	) -> Option<HashSet<AuthorityDiscoveryId>> {
		self.by_peer_id.get(&peer_id).cloned()
	}
}

fn build_overseer(
	state: &TestState,
	_network: NetworkEmulatorHandle,
	_network_interface: NetworkInterface,
	_network_receiver: NetworkInterfaceReceiver,
	dependencies: &TestEnvironmentDependencies,
) -> (
	Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>,
	OverseerHandle,
	Vec<ProtocolConfig>,
) {
	let handle = Arc::new(dependencies.runtime.handle().clone());
	let _guard = handle.enter();
	let overseer_connector = OverseerConnector::with_event_capacity(64000);
	let overseer_metrics = OverseerMetrics::try_register(&dependencies.registry).unwrap();
	let spawn_task_handle = dependencies.task_manager.spawn_handle();
	let mock_runtime_api = MockRuntimeApi::new(
		state.config.clone(),
		state.test_authorities.clone(),
		state.candidate_receipts.clone(),
		Default::default(),
		Default::default(),
		0,
		MockRuntimeApiCoreState::Scheduled,
	);
	let chain_api_state = ChainApiState { block_headers: state.block_headers.clone() };
	let mock_chain_api = MockChainApi::new(chain_api_state);
	let mock_prospective_parachains = MockProspectiveParachains::new();
	let mock_candidate_backing = MockCandidateBacking::new(
		state.config.clone(),
		state
			.test_authorities
			.validator_pairs
			.get(NODE_UNDER_TEST as usize)
			.unwrap()
			.clone(),
		state.pvd.clone(),
		state.own_backing_group.clone(),
	);
	let req_protocol_names = ReqProtocolNames::new(GENESIS_HASH, None);
	let peer_set_protocol_names = PeerSetProtocolNames::new(GENESIS_HASH, None);
	let mut net_conf = NetworkConfiguration::new_local();
	net_conf.transport = TransportConfig::MemoryOnly;
	net_conf.listen_addresses =
		vec![multiaddr::Protocol::Memory(NODE_UNDER_TEST.saturating_add(1) as u64).into()];
	net_conf.node_key = state.test_authorities.node_key_configs[NODE_UNDER_TEST as usize].clone();

	let mut network_config =
		FullNetworkConfiguration::<Block, Hash, NetworkWorker<Block, Hash>>::new(&net_conf, None);

	// v1 requests
	// We don't use them in this benchmark but still need to add the protocol
	let (v1_statement_req_receiver, v1_statement_req_cfg) =
		IncomingRequest::get_config_receiver::<Block, sc_network::NetworkWorker<Block, Hash>>(
			&ReqProtocolNames::new(GENESIS_HASH, None),
		);
	network_config.add_request_response_protocol(v1_statement_req_cfg);
	// v2 requests
	let (v2_candidate_req_receiver, v2_candidate_req_cfg) =
		IncomingRequest::get_config_receiver::<Block, sc_network::NetworkWorker<Block, Hash>>(
			&ReqProtocolNames::new(GENESIS_HASH, None),
		);
	network_config.add_request_response_protocol(v2_candidate_req_cfg);
	let keystore = make_keystore();
	let subsystem = StatementDistributionSubsystem::new(
		keystore.clone(),
		v1_statement_req_receiver,
		v2_candidate_req_receiver,
		Metrics::try_register(&dependencies.registry).unwrap(),
		rand::rngs::StdRng::from_entropy(),
	);

	let role = Role::Authority;
	let peer_store_handle = network_config.peer_store_handle();
	let block_announces_protocol =
		format!("/{}/block-announces/1", array_bytes::bytes2hex("", GENESIS_HASH.as_ref()));
	let protocol_id = ProtocolId::from("sup");
	let notification_metrics = NotificationMetrics::new(Some(&dependencies.registry));
	let (block_announce_config, mut notification_service) =
		<NetworkWorker<Block, Hash> as sc_network::NetworkBackend<Block, Hash>>::notification_config(
			block_announces_protocol.into(),
			std::iter::once(format!("/{}/block-announces/1", protocol_id.as_ref()).into())
				.collect(),
			1024 * 1024,
			Some(sc_network::config::NotificationHandshake::new(
				BlockAnnouncesHandshake::<Block>::build(
					sc_network::Roles::from(&role),
					Zero::zero(),
					GENESIS_HASH,
					GENESIS_HASH,
				),
			)),
			// NOTE: `set_config` will be ignored by `protocol.rs` as the block announcement
			// protocol is still hardcoded into the peerset.
			SetConfig {
				in_peers: 0,
				out_peers: 0,
				reserved_nodes: vec![],
				non_reserved_mode: NonReservedPeerMode::Deny,
			},
			notification_metrics.clone(),
			Arc::clone(&peer_store_handle),
		);

	let notification_services = peer_sets_info::<Block, NetworkWorker<Block, Hash>>(
		IsAuthority::Yes,
		&peer_set_protocol_names,
		notification_metrics.clone(),
		Arc::clone(&peer_store_handle),
	)
	.into_iter()
	.map(|(mut config, (peerset, service))| {
		if matches!(peerset, PeerSet::Validation) {
			config.allow_non_reserved(10_000, 10_000);
		}
		network_config.add_notification_protocol(config);
		(peerset, service)
	})
	.collect::<std::collections::HashMap<PeerSet, Box<dyn sc_network::NotificationService>>>();
	let worker =
		<NetworkWorker<Block, Hash> as sc_network::NetworkBackend<Block, Hash>>::new(Params::<
			Block,
			Hash,
			NetworkWorker<Block, Hash>,
		> {
			block_announce_config,
			role,
			executor: {
				let spawn_handle = Clone::clone(&spawn_task_handle);
				Box::new(move |fut| {
					spawn_handle.spawn("libp2p-node", Some("networking"), fut);
				})
			},
			genesis_hash: GENESIS_HASH,
			network_config,
			protocol_id,
			fork_id: None,
			metrics_registry: Some(dependencies.registry.clone()),
			bitswap_config: None,
			notification_metrics,
		})
		.unwrap();

	let network_service =
		<NetworkWorker<Block, Hash> as sc_network::NetworkBackend<Block, Hash>>::network_service(
			&worker,
		);

	let network_bridge_metrics =
		NetworkBridgeMetrics::register(Some(&dependencies.registry)).unwrap();

	let authority_discovery_service =
		DummyAuthotiryDiscoveryService::new(state.test_authorities.peer_id_to_authority.clone());
	let notification_sinks = Arc::new(parking_lot::Mutex::new(HashMap::new()));
	let network_bridge_tx = NetworkBridgeTxSubsystem::new(
		Arc::clone(&network_service),
		authority_discovery_service.clone(),
		network_bridge_metrics.clone(),
		req_protocol_names,
		peer_set_protocol_names.clone(),
		Arc::clone(&notification_sinks),
	);
	let dummy_sync_oracle = Box::new(DummySyncOracle);
	let network_bridge_rx = NetworkBridgeRxSubsystem::new(
		Arc::clone(&network_service),
		authority_discovery_service,
		dummy_sync_oracle,
		network_bridge_metrics,
		peer_set_protocol_names,
		notification_services,
		Arc::clone(&notification_sinks),
		false,
	);

	let dummy = dummy_builder!(spawn_task_handle, overseer_metrics)
		.replace_runtime_api(|_| mock_runtime_api)
		.replace_chain_api(|_| mock_chain_api)
		.replace_prospective_parachains(|_| mock_prospective_parachains)
		.replace_candidate_backing(|_| mock_candidate_backing)
		.replace_statement_distribution(|_| subsystem)
		.replace_network_bridge_tx(|_| network_bridge_tx)
		.replace_network_bridge_rx(|_| network_bridge_rx);
	let (overseer, raw_handle) = dummy.build_with_connector(overseer_connector).unwrap();
	let overseer_handle = OverseerHandle::new(raw_handle);

	let spawn_handle = dependencies.task_manager.spawn_handle();
	spawn_handle.spawn("network-worker", "networking", worker.run());
	let syncing_name = Box::leak(format!("Peer {} syncing", NODE_UNDER_TEST).into_boxed_str());
	// Part of SyncingEngline which is not included to networking tasks
	spawn_handle.spawn(syncing_name, "test-environment", {
		let mut notification_service = notification_service.clone().unwrap();
		async move {
			while let Some(event) = notification_service.next_event().await {
				match event {
					NotificationEvent::ValidateInboundSubstream { result_tx, .. } =>
						result_tx.send(ValidationResult::Accept).unwrap(),
					_ => {},
				}
			}
		}
	});

	let ready = tokio::spawn({
		let network_service = Arc::clone(&network_service);
		async move {
			while network_service.listen_addresses().is_empty() {
				tokio::time::sleep(Duration::from_millis(10)).await;
			}
		}
	});
	let _ = tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(ready));

	let local_peer_id = network_service.local_peer_id();
	let listen_addresses = network_service.listen_addresses();
	gum::debug!(target: LOG_TARGET, ?local_peer_id, ?listen_addresses, "Peer {} ready", NODE_UNDER_TEST);

	(overseer, overseer_handle, vec![])
}

fn build_peer(state: Arc<TestState>, dependencies: &TestEnvironmentDependencies, index: u16) {
	let handle = Arc::new(dependencies.runtime.handle().clone());
	let _guard = handle.enter();

	let node_peer_id =
		state.test_authorities.peer_ids.get(NODE_UNDER_TEST as usize).unwrap().clone();
	let node_multiaddr =
		multiaddr::Protocol::Memory(NODE_UNDER_TEST.saturating_add(1) as u64).into();
	let req_protocol_names = ReqProtocolNames::new(GENESIS_HASH, None);
	let peer_set_protocol_names = PeerSetProtocolNames::new(GENESIS_HASH, None);
	let mut net_conf = NetworkConfiguration::new_local();
	net_conf.transport = TransportConfig::MemoryOnly;
	net_conf.listen_addresses =
		vec![multiaddr::Protocol::Memory(index.saturating_add(1) as u64).into()];
	net_conf.node_key = state.test_authorities.node_key_configs[index as usize].clone();

	let mut network_config =
		FullNetworkConfiguration::<Block, Hash, NetworkWorker<Block, Hash>>::new(&net_conf, None);
	let (mut candidate_req_receiver, candidate_req_cfg) =
		IncomingRequest::<AttestedCandidateRequest>::get_config_receiver::<
			Block,
			NetworkWorker<Block, Hash>,
		>(&req_protocol_names);
	let candidate_req_name = candidate_req_cfg.name.clone();
	network_config.add_request_response_protocol(candidate_req_cfg);
	let role = Role::Authority;
	let peer_store_handle = network_config.peer_store_handle();
	let block_announces_protocol =
		format!("/{}/block-announces/1", array_bytes::bytes2hex("", GENESIS_HASH.as_ref()));
	let protocol_id = ProtocolId::from("sup");
	let notification_metrics = NotificationMetrics::new(None);
	let (block_announce_config, mut notification_service) =
		<NetworkWorker<Block, Hash> as sc_network::NetworkBackend<Block, Hash>>::notification_config(
			block_announces_protocol.into(),
			std::iter::once(format!("/{}/block-announces/1", protocol_id.as_ref()).into())
				.collect(),
			1024 * 1024,
			Some(sc_network::config::NotificationHandshake::new(
				BlockAnnouncesHandshake::<Block>::build(
					sc_network::Roles::from(&role),
					Zero::zero(),
					GENESIS_HASH,
					GENESIS_HASH,
				),
			)),
			// NOTE: `set_config` will be ignored by `protocol.rs` as the block announcement
			// protocol is still hardcoded into the peerset.
			SetConfig {
				in_peers: 0,
				out_peers: 0,
				reserved_nodes: vec![],
				non_reserved_mode: NonReservedPeerMode::Deny,
			},
			notification_metrics.clone(),
			Arc::clone(&peer_store_handle),
		);

	let mut notification_services = peer_sets_info::<Block, NetworkWorker<Block, Hash>>(
		IsAuthority::Yes,
		&peer_set_protocol_names,
		notification_metrics.clone(),
		Arc::clone(&peer_store_handle),
	)
	.into_iter()
	.map(|(config, (_peerset, service))| {
		network_config.add_notification_protocol(config);
		service
	})
	.collect_vec();
	let validation_position = notification_services
		.iter()
		.position(|service| service.protocol().contains("/validation/"))
		.expect("Should be only one validation service");
	let mut validation_notification_service = notification_services.remove(validation_position);
	let collation_position = notification_services
		.iter()
		.position(|service| service.protocol().contains("/collation/"))
		.expect("Should be only one collation service");
	let mut collation_notification_service = notification_services.remove(collation_position);
	assert!(notification_services.is_empty());

	let spawn_task_handle = dependencies.task_manager.spawn_handle();
	let worker =
		<NetworkWorker<Block, Hash> as sc_network::NetworkBackend<Block, Hash>>::new(Params::<
			Block,
			Hash,
			NetworkWorker<Block, Hash>,
		> {
			block_announce_config,
			role,
			executor: {
				let spawn_handle = Clone::clone(&spawn_task_handle);
				Box::new(move |fut| {
					spawn_handle.spawn("libp2p-node", Some("test-environment"), fut);
				})
			},
			genesis_hash: GENESIS_HASH,
			network_config,
			protocol_id,
			fork_id: None,
			metrics_registry: None,
			bitswap_config: None,
			notification_metrics: notification_metrics.clone(),
		})
		.unwrap();
	let network_service =
		<NetworkWorker<Block, Hash> as sc_network::NetworkBackend<Block, Hash>>::network_service(
			&worker,
		);
	let peer_id = network_service.local_peer_id();
	let spawn_handle = dependencies.task_manager.spawn_handle();
	let peer_worker_name = Box::leak(format!("Peer {} worker", index).into_boxed_str());
	spawn_handle.spawn(peer_worker_name, "test-environment", worker.run());
	let peer_notifications_name =
		Box::leak(format!("Peer {} notifications", index).into_boxed_str());
	spawn_handle.spawn(peer_notifications_name, "test-environment", {
		let state = Arc::clone(&state);
		let network_service = Arc::clone(&network_service);
		let node_peer_id = state.test_authorities.peer_ids.get(NODE_UNDER_TEST as usize).unwrap().clone();
		async move {
			loop {
				tokio::select! {
					event = notification_service.next_event() => {
						if let Some(NotificationEvent::ValidateInboundSubstream { result_tx, .. }) = event {
							result_tx.send(ValidationResult::Accept).unwrap();
						}
					},
					event = validation_notification_service.next_event() => {
						if let Some(NotificationEvent::NotificationReceived { peer, notification }) = event {
							assert_eq!(peer, node_peer_id, "Notifications only from the node under test are allowed");
							let message: Bytes = notification.into();
							let message = WireMessage::<v3::ValidationProtocol>::decode(&mut message.as_ref()).unwrap();
							match message {
								WireMessage::ProtocolMessage(v3::ValidationProtocol::StatementDistribution(v3::StatementDistributionMessage::Statement(relay_parent, statement))) => {
									let statement_validator_index = statement.unchecked_validator_index().0;
									gum::debug!(target: LOG_TARGET, "Peer {} received a statement from validator {}", index, statement_validator_index);
									let candidate_hash = *statement.unchecked_payload().candidate_hash();
									let statements_sent_count = state
										.statements_tracker
										.get(&candidate_hash)
										.unwrap()
										.get(index as usize)
										.unwrap()
										.as_ref();
									if statements_sent_count.load(Ordering::SeqCst) {
										gum::debug!(target: LOG_TARGET, "Peer {} already sent own statement back to validator {}", index, statement_validator_index);
										continue
									} else {
										statements_sent_count.store(true, Ordering::SeqCst);
									}

									let group_statements = state.statements.get(&candidate_hash).unwrap();
									if !group_statements.iter().any(|s| s.unchecked_validator_index().0 == index as u32)
									{
										gum::debug!(target: LOG_TARGET, "No cluster statements for Validator {}", index);
										continue
									}

									let statement = CompactStatement::Valid(candidate_hash);
									let context = SigningContext { parent_hash: relay_parent, session_index: SESSION_INDEX };
									let payload = statement.signing_payload(&context);
									let pair = state.test_authorities.validator_pairs.get(index as usize).unwrap();
									let signature = pair.sign(&payload[..]);
									let statement = SignedStatement::new(statement, ValidatorIndex(index as u32), signature, &context, &pair.public())
										.unwrap()
										.as_unchecked()
										.to_owned();
									let message = WireMessage::ProtocolMessage(v3::ValidationProtocol::StatementDistribution(v3::StatementDistributionMessage::Statement(relay_parent, statement)));
									let _ = validation_notification_service.send_async_notification(&peer, message.encode()).await;
									gum::debug!(target: LOG_TARGET, "Peer {} sent own statement back to validator {}", index, statement_validator_index);
								}
								WireMessage::ProtocolMessage(v3::ValidationProtocol::StatementDistribution(v3::StatementDistributionMessage::BackedCandidateManifest(manifest))) => {
									let backing_group = state.session_info.validator_groups.get(manifest.group_index).unwrap();
									let group_size = backing_group.len();
									let is_own_backing_group = backing_group.contains(&ValidatorIndex(NODE_UNDER_TEST));

									if is_own_backing_group {
										let req = AttestedCandidateRequest { candidate_hash: manifest.candidate_hash, mask: StatementFilter::blank(state.own_backing_group.len()) };
										gum::debug!(target: LOG_TARGET, ?req, "Peer {} is requesting candidate {}", index, manifest.candidate_hash);
										let mut tries = 0;
										// Could be decoded as AttestedCandidateResponse, but we don't need it
										let _response = loop {
											match network_service.request(peer, candidate_req_name.clone(), req.encode(), None, IfDisconnected::ImmediateError).await {
												Ok((response, _)) => break response,
												Err(e) => {
													tries += 1;
													if tries > 100 {
														panic!("Peer {} failed to request candidate {}: {:?}", index, manifest.candidate_hash, e);
													}
													tokio::time::sleep(Duration::from_millis(10)).await;
													continue;
												}
											}
										};
										gum::debug!(target: LOG_TARGET, "Peer {} received requested candidate {}", index, manifest.candidate_hash);
									}

									let seconded_in_group = BitVec::from_iter((0..group_size).map(|_| true));
									// TODO: Use a seconding peer id
									let validated_in_group = BitVec::from_iter((0..group_size).map(|i| !(i == 1)));

									let ack = BackedCandidateAcknowledgement { candidate_hash: manifest.candidate_hash, statement_knowledge: StatementFilter { seconded_in_group, validated_in_group } };
									let message = WireMessage::ProtocolMessage(v3::ValidationProtocol::StatementDistribution(v3::StatementDistributionMessage::BackedCandidateKnown(ack)));
									validation_notification_service.send_async_notification(&peer, message.encode()).await.unwrap();
									gum::debug!(target: LOG_TARGET, "Peer {} sent backed candidate known {}", index, manifest.candidate_hash);
									state.manifests_tracker
										.get(&manifest.candidate_hash)
										.unwrap()
										.as_ref()
										.store(true, Ordering::SeqCst);
									let manifests_count = state
										.manifests_tracker
										.values()
										.filter(|v| v.load(Ordering::SeqCst))
										.collect::<Vec<_>>()
										.len();
									gum::debug!(target: LOG_TARGET, "Peer {} tracked {} manifest", index, manifests_count);
								},
								WireMessage::ProtocolMessage(v3::ValidationProtocol::StatementDistribution(v3::StatementDistributionMessage::BackedCandidateKnown(ack))) => {
									gum::debug!(target: LOG_TARGET, "Peer {} received backed candidate known {}", index, ack.candidate_hash);
									state.manifests_tracker
										.get(&ack.candidate_hash)
										.unwrap()
										.as_ref()
										.store(true, Ordering::SeqCst);
									let manifests_count = state
										.manifests_tracker
										.values()
										.filter(|v| v.load(Ordering::SeqCst))
										.collect::<Vec<_>>()
										.len();
									gum::debug!(target: LOG_TARGET, "Peer {} tracked {} manifest", index, manifests_count);
								},
								WireMessage::ViewUpdate(_) => {},
								message => {
									gum::error!(target: LOG_TARGET, "Peer {} received unknown message {:?}", index, message);
								}
							}
						}
					},
					event = collation_notification_service.next_event() => {
						gum::error!(target: LOG_TARGET, "Peer {} received collation {:?}", index, event);
					},
				}
			}
		}
	});
	let peer_requests_name = Box::leak(format!("Peer {} requests", index).into_boxed_str());
	spawn_handle.spawn(peer_requests_name, "test-environment", async move {
		loop {
			let req = candidate_req_receiver.recv(|| vec![]).await.unwrap();
			let payload = req.payload;
			gum::debug!(target: LOG_TARGET, ?peer_id, "Peer {} received AttestedCandidateRequest", index);
			let candidate_receipt = state
				.commited_candidate_receipts
				.values()
				.flatten()
				.find(|v| v.hash() == payload.candidate_hash)
				.unwrap()
				.clone();
			let persisted_validation_data = state.pvd.clone();
			let statements = state
				.statements
				.get(&payload.candidate_hash)
				.unwrap()
				.clone();
			let res = AttestedCandidateResponse {
				candidate_receipt,
				persisted_validation_data,
				statements,
			};
			req.pending_response.send_response(res).unwrap();
			gum::debug!(target: LOG_TARGET, ?peer_id, "Peer {} answered request with AttestedCandidateResponse", index);
		}
	});

	network_service
		.add_reserved_peer(MultiaddrWithPeerId { peer_id: node_peer_id, multiaddr: node_multiaddr })
		.unwrap();

	let ready = tokio::spawn({
		let network_service = Arc::clone(&network_service);
		async move {
			while network_service.listen_addresses().is_empty() {
				tokio::time::sleep(Duration::from_millis(10)).await;
			}
		}
	});
	let _ = tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(ready));

	let local_peer_id = network_service.local_peer_id();
	let listen_addresses = network_service.listen_addresses();
	gum::debug!(target: LOG_TARGET, ?local_peer_id, ?listen_addresses, "Peer {} ready", index);
}

pub fn prepare_test(
	state: &TestState,
	with_prometheus_endpoint: bool,
) -> (TestEnvironment, Vec<ProtocolConfig>) {
	let dependencies = TestEnvironmentDependencies::default();
	let (network, network_interface, network_receiver) = new_network(
		&state.config,
		&dependencies,
		&state.test_authorities,
		vec![Arc::new(state.clone())],
	);

	let arc_state = Arc::new(state.clone());
	let (overseer, overseer_handle, cfg) =
		build_overseer(state, network.clone(), network_interface, network_receiver, &dependencies);
	(0..state.config.n_validators)
		.filter(|i| *i != NODE_UNDER_TEST as usize)
		.for_each(|i| build_peer(Arc::clone(&arc_state), &dependencies, i as u16));

	(
		TestEnvironment::new(
			dependencies,
			state.config.clone(),
			network,
			overseer,
			overseer_handle,
			state.test_authorities.clone(),
			with_prometheus_endpoint,
		),
		cfg,
	)
}

pub fn generate_peer_view_change(block_hash: Hash, peer_id: PeerId) -> AllMessages {
	let network = NetworkBridgeEvent::PeerViewChange(peer_id, View::new([block_hash], 0));

	AllMessages::StatementDistribution(StatementDistributionMessage::NetworkBridgeUpdate(network))
}

pub fn generate_new_session_topology(
	topology: &SessionGridTopology,
	test_node: ValidatorIndex,
) -> Vec<AllMessages> {
	let event = NetworkBridgeEvent::NewGossipTopology(NewGossipTopology {
		session: 0,
		topology: topology.clone(),
		local_index: Some(test_node),
	});
	vec![AllMessages::StatementDistribution(StatementDistributionMessage::NetworkBridgeUpdate(
		event,
	))]
}

/// Generates a topology to be used for this benchmark.
pub fn generate_topology(test_authorities: &TestAuthorities) -> SessionGridTopology {
	let keyrings = test_authorities
		.validator_authority_id
		.clone()
		.into_iter()
		.zip(test_authorities.peer_ids.clone())
		.collect_vec();

	let topology = keyrings
		.clone()
		.into_iter()
		.enumerate()
		.map(|(index, (discovery_id, peer_id))| TopologyPeerInfo {
			peer_ids: vec![peer_id],
			validator_index: ValidatorIndex(index as u32),
			discovery_id,
		})
		.collect_vec();
	let shuffled = (0..keyrings.len()).collect_vec();

	SessionGridTopology::new(shuffled, topology)
}

pub async fn benchmark_statement_distribution(
	env: &mut TestEnvironment,
	state: &TestState,
) -> BenchmarkUsage {
	state.reset_trackers();

	let connected_validators = state
		.test_authorities
		.validator_authority_id
		.iter()
		.enumerate()
		.filter_map(|(i, id)| if env.network().is_peer_connected(id) { Some(i) } else { None })
		.collect_vec();
	let seconding_validator_in_own_backing_group = state
		.own_backing_group
		.iter()
		.find(|v| connected_validators.contains(&(v.0 as usize)))
		.unwrap()
		.to_owned();

	let config = env.config().clone();
	let groups = state.session_info.validator_groups.clone();
	let own_backing_group_index = groups
		.iter()
		.position(|group| group.iter().any(|v| v.0 == NODE_UNDER_TEST))
		.unwrap();

	env.metrics().set_n_validators(config.n_validators);
	env.metrics().set_n_cores(config.n_cores);

	let topology = generate_topology(&state.test_authorities);
	let peer_connected_messages = vec![];
	let new_session_topology_messages =
		generate_new_session_topology(&topology, ValidatorIndex(NODE_UNDER_TEST));
	for message in peer_connected_messages.into_iter().chain(new_session_topology_messages) {
		env.send_message(message).await;
	}

	let test_start = Instant::now();
	let mut candidates_advertised = 0;

	tokio::time::sleep(Duration::from_secs(5)).await;
	for block_info in state.block_infos.iter() {
		let block_num = block_info.number as usize;
		gum::info!(target: LOG_TARGET, "Current block {}/{} {:?}", block_num, config.num_blocks, block_info.hash);
		env.metrics().set_current_block(block_num);
		env.import_block(block_info.clone()).await;

		for peer_view_change in env
			.network()
			.generate_statement_distribution_peer_view_change(view![block_info.hash])
		{
			env.send_message(peer_view_change).await;
		}

		let seconding_peer_id = *state
			.test_authorities
			.peer_ids
			.get(seconding_validator_in_own_backing_group.0 as usize)
			.unwrap();
		let candidate = state.candidate_receipts.get(&block_info.hash).unwrap().first().unwrap();
		let candidate_hash = candidate.hash();
		let statement = state
			.statements
			.get(&candidate_hash)
			.unwrap()
			.get(seconding_validator_in_own_backing_group.0 as usize)
			.unwrap()
			.clone();
		let message = AllMessages::StatementDistribution(
			StatementDistributionMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
				seconding_peer_id,
				Versioned::V3(v3::StatementDistributionMessage::Statement(
					block_info.hash,
					statement,
				)),
			)),
		);
		env.send_message(message).await;
		gum::info!(target: LOG_TARGET, ?seconding_peer_id, "Peer {} sent a statement initiating statement exchange in a cluster", seconding_validator_in_own_backing_group.0);

		let max_messages_per_candidate = state.config.max_candidate_depth + 1;
		// One was just sent for the own backing group
		let mut messages_tracker = (0..groups.len())
			.map(|i| if i == own_backing_group_index { max_messages_per_candidate } else { 0 })
			.collect_vec();

		let neighbors =
			topology.compute_grid_neighbors_for(ValidatorIndex(NODE_UNDER_TEST)).unwrap();
		let connected_neighbors_x = neighbors
			.validator_indices_x
			.iter()
			.filter(|&v| connected_validators.contains(&(v.0 as usize)))
			.cloned()
			.collect_vec();
		let connected_neighbors_y = neighbors
			.validator_indices_y
			.iter()
			.filter(|&v| connected_validators.contains(&(v.0 as usize)))
			.cloned()
			.collect_vec();
		let one_hop_peers_and_groups = connected_neighbors_x
			.iter()
			.chain(connected_neighbors_y.iter())
			.map(|validator_index| {
				let peer_id =
					*state.test_authorities.peer_ids.get(validator_index.0 as usize).unwrap();
				let group_index =
					groups.iter().position(|group| group.contains(validator_index)).unwrap();
				(peer_id, group_index)
			})
			.collect_vec();
		let two_hop_x_peers_and_groups = connected_neighbors_x
			.iter()
			.flat_map(|validator_index| {
				let peer_id =
					*state.test_authorities.peer_ids.get(validator_index.0 as usize).unwrap();
				topology
					.compute_grid_neighbors_for(*validator_index)
					.unwrap()
					.validator_indices_y
					.iter()
					.map(|validator_neighbor| {
						let group_index = groups
							.iter()
							.position(|group| group.contains(validator_neighbor))
							.unwrap();
						(peer_id, group_index)
					})
					.collect_vec()
			})
			.collect_vec();
		let two_hop_y_peers_and_groups = connected_neighbors_y
			.iter()
			.flat_map(|validator_index| {
				let peer_id =
					*state.test_authorities.peer_ids.get(validator_index.0 as usize).unwrap();
				topology
					.compute_grid_neighbors_for(*validator_index)
					.unwrap()
					.validator_indices_x
					.iter()
					.map(|validator_neighbor| {
						let group_index = groups
							.iter()
							.position(|group| group.contains(validator_neighbor))
							.unwrap();
						(peer_id, group_index)
					})
					.collect_vec()
			})
			.collect_vec();

		for (seconding_peer_id, group_index) in one_hop_peers_and_groups
			.into_iter()
			.chain(two_hop_x_peers_and_groups)
			.chain(two_hop_y_peers_and_groups)
		{
			let messages_sent_count = messages_tracker.get_mut(group_index).unwrap();
			if *messages_sent_count == max_messages_per_candidate {
				continue
			}
			*messages_sent_count += 1;

			let candidate_hash = state
				.candidate_receipts
				.get(&block_info.hash)
				.unwrap()
				.get(group_index)
				.unwrap()
				.hash();
			let manifest = BackedCandidateManifest {
				relay_parent: block_info.hash,
				candidate_hash,
				group_index: GroupIndex(group_index as u32),
				para_id: Id::new(group_index as u32 + 1),
				parent_head_data_hash: state.pvd.parent_head.hash(),
				statement_knowledge: StatementFilter {
					seconded_in_group: BitVec::from_iter(
						groups.get(GroupIndex(group_index as u32)).unwrap().iter().map(|_| true),
					),
					validated_in_group: BitVec::from_iter(
						groups.get(GroupIndex(group_index as u32)).unwrap().iter().map(|_| false),
					),
				},
			};
			let message = AllMessages::StatementDistribution(
				StatementDistributionMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
					seconding_peer_id,
					Versioned::V3(v3::StatementDistributionMessage::BackedCandidateManifest(
						manifest,
					)),
				)),
			);
			env.send_message(message).await;
		}

		candidates_advertised += messages_tracker.iter().filter(|&&v| v > 0).collect_vec().len();

		loop {
			let manifests_count = state
				.manifests_tracker
				.values()
				.filter(|v| v.load(Ordering::SeqCst))
				.collect::<Vec<_>>()
				.len();
			gum::trace!(target: LOG_TARGET, "{}/{} manifest exchanges", manifests_count, candidates_advertised);

			if manifests_count == candidates_advertised {
				break;
			}
			tokio::time::sleep(Duration::from_millis(1000)).await;
		}
	}

	let duration: u128 = test_start.elapsed().as_millis();
	gum::info!(target: LOG_TARGET, "All blocks processed in {}", format!("{:?}ms", duration).cyan());
	gum::info!(target: LOG_TARGET,
		"Avg block time: {}",
		format!("{} ms", test_start.elapsed().as_millis() / env.config().num_blocks as u128).red()
	);

	env.stop().await;
	env.collect_resource_usage(&["statement-distribution", "networking"], false)
}
