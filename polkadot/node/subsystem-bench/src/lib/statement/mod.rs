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
	network::new_network,
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
	grid_topology::{SessionGridTopology, TopologyPeerInfo},
	peer_set::PeerSet,
	request_response::{
		v2::{AttestedCandidateRequest, AttestedCandidateResponse},
		IncomingRequest, ReqProtocolNames,
	},
	v3::{self, BackedCandidateAcknowledgement, BackedCandidateManifest, StatementFilter},
	view, Versioned,
};
use polkadot_node_subsystem::messages::{
	network_bridge_event::NewGossipTopology, AllMessages, NetworkBridgeEvent,
	StatementDistributionMessage,
};
use polkadot_overseer::{
	Handle as OverseerHandle, Overseer, OverseerConnector, OverseerMetrics, SpawnGlue,
};
use polkadot_primitives::{
	AuthorityDiscoveryId, CompactStatement, GroupIndex, Id, SignedStatement, SigningContext,
	ValidatorId, ValidatorIndex,
};
use polkadot_service::overseer::{
	NetworkBridgeMetrics, NetworkBridgeRxSubsystem, NetworkBridgeTxSubsystem,
};
use polkadot_statement_distribution::StatementDistributionSubsystem;
use rand::SeedableRng;
use sc_keystore::LocalKeystore;
use sc_network::{
	config::{MultiaddrWithPeerId, NodeKeyConfig, ProtocolId, Role},
	request_responses::ProtocolConfig,
	service::traits::{NotificationEvent, RequestResponseConfig, ValidationResult},
	IfDisconnected, NetworkBackend, NotificationMetrics,
};
use sc_service::SpawnTaskHandle;
use sp_application_crypto::Pair;
use sp_core::{Bytes, H256};
use sp_keystore::{Keystore, KeystorePtr};
use sp_runtime::{traits::Block as BlockT, RuntimeAppPublic};
use std::{
	collections::HashMap,
	sync::{atomic::Ordering, Arc},
	time::{Duration, Instant},
};
pub use test_state::TestState;

use crate::network_utils;
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

fn build_overseer<B, N>(
	state: &TestState,
	dependencies: &TestEnvironmentDependencies,
) -> (
	Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>,
	OverseerHandle,
	Vec<ProtocolConfig>,
	sc_network::PeerId,
	sc_network::Multiaddr,
)
where
	B: BlockT<Hash = H256> + 'static,
	N: NetworkBackend<B, B::Hash>,
{
	let _guard = dependencies.runtime.handle().enter();

	let node_key: NodeKeyConfig =
		state.test_authorities.node_key_configs[NODE_UNDER_TEST as usize].clone();
	let listen_addr = state.test_authorities.authority_id_to_addr
		[&state.test_authorities.validator_authority_id[NODE_UNDER_TEST as usize]]
		.clone();
	let role = Role::Authority;
	let protocol_id = ProtocolId::from("sup");
	let notification_metrics = NotificationMetrics::new(Some(&dependencies.registry));

	let mut network_config = network_utils::build_network_config::<B, N>(
		node_key,
		listen_addr,
		Some(dependencies.registry.clone()),
	);

	let req_protocol_names = ReqProtocolNames::new(GENESIS_HASH, None);
	// v1 requests
	// We don't use them in this benchmark but still need to add the protocol
	let (v1_statement_req_receiver, v1_statement_req_cfg) =
		IncomingRequest::get_config_receiver::<B, N>(&req_protocol_names);
	network_config.add_request_response_protocol(v1_statement_req_cfg);
	// v2 requests
	let (v2_candidate_req_receiver, v2_candidate_req_cfg) =
		IncomingRequest::get_config_receiver::<B, N>(&req_protocol_names);
	network_config.add_request_response_protocol(v2_candidate_req_cfg);

	let (peer_set_protocol_names, peer_set_services) =
		network_utils::build_peer_set_services::<B, N>(&mut network_config, &notification_metrics);

	let (worker, network_service, mut block_announce_service) =
		network_utils::build_network_worker::<B, N>(
			dependencies,
			network_config,
			role,
			protocol_id,
			notification_metrics,
			Some(dependencies.registry.clone()),
			"networking",
		);

	let overseer_connector = OverseerConnector::with_event_capacity(64000);
	let overseer_metrics = OverseerMetrics::try_register(&dependencies.registry).unwrap();
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

	let keystore = make_keystore();
	let subsystem = StatementDistributionSubsystem::new(
		keystore.clone(),
		v1_statement_req_receiver,
		v2_candidate_req_receiver,
		Metrics::try_register(&dependencies.registry).unwrap(),
		rand::rngs::StdRng::from_entropy(),
	);

	let network_bridge_metrics =
		NetworkBridgeMetrics::register(Some(&dependencies.registry)).unwrap();

	let authority_discovery_service = network_utils::DummyAuthotiryDiscoveryService::new(
		state.test_authorities.peer_id_to_authority.clone(),
		state.test_authorities.authority_id_to_addr.clone(),
	);
	let notification_sinks = Arc::new(parking_lot::Mutex::new(HashMap::new()));
	let network_bridge_tx = NetworkBridgeTxSubsystem::new(
		Arc::clone(&network_service),
		authority_discovery_service.clone(),
		network_bridge_metrics.clone(),
		req_protocol_names,
		peer_set_protocol_names.clone(),
		Arc::clone(&notification_sinks),
	);
	let dummy_sync_oracle = Box::new(network_utils::DummySyncOracle);
	let network_bridge_rx = NetworkBridgeRxSubsystem::new(
		Arc::clone(&network_service),
		authority_discovery_service,
		dummy_sync_oracle,
		network_bridge_metrics,
		peer_set_protocol_names,
		peer_set_services,
		Arc::clone(&notification_sinks),
		false,
	);

	let spawn_handle = dependencies.task_manager.spawn_handle();
	let dummy = dummy_builder!(spawn_handle, overseer_metrics)
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
		let mut notification_service = block_announce_service.clone().unwrap();
		async move {
			while let Some(event) = notification_service.next_event().await {
				if let NotificationEvent::ValidateInboundSubstream { result_tx, .. } = event {
					result_tx.send(ValidationResult::Accept).unwrap()
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

	(
		overseer,
		overseer_handle,
		vec![],
		local_peer_id,
		listen_addresses.first().expect("listen addresses must not be empty").clone(),
	)
}

fn build_peer<B, N>(
	state: Arc<TestState>,
	dependencies: &TestEnvironmentDependencies,
	index: u16,
	node_peer_id: sc_network::PeerId,
	node_multiaddr: sc_network::Multiaddr,
) where
	B: BlockT<Hash = H256> + 'static,
	N: NetworkBackend<B, B::Hash>,
{
	let _guard = dependencies.runtime.handle().enter();

	let node_key: NodeKeyConfig = state.test_authorities.node_key_configs[index as usize].clone();
	let listen_addr = state.test_authorities.authority_id_to_addr
		[&state.test_authorities.validator_authority_id[index as usize]]
		.clone();
	let role = Role::Authority;
	let protocol_id = ProtocolId::from("sup");
	let notification_metrics = NotificationMetrics::new(None);

	let mut network_config =
		network_utils::build_network_config::<B, N>(node_key, listen_addr, None);

	let req_protocol_names = ReqProtocolNames::new(GENESIS_HASH, None);
	let (mut candidate_req_receiver, candidate_req_cfg) =
		IncomingRequest::<AttestedCandidateRequest>::get_config_receiver::<B, N>(
			&req_protocol_names,
		);
	let candidate_req_name = candidate_req_cfg.protocol_name().clone();
	network_config.add_request_response_protocol(candidate_req_cfg);

	let (_peer_set_protocol_names, mut peer_set_services) =
		network_utils::build_peer_set_services::<B, N>(&mut network_config, &notification_metrics);
	let mut validation_service = peer_set_services
		.remove(&PeerSet::Validation)
		.expect("validation protocol was enabled so `NotificationService` must exist; qed");
	let mut collation_service = peer_set_services
		.remove(&PeerSet::Collation)
		.expect("collation protocol was enabled so `NotificationService` must exist; qed");
	assert!(peer_set_services.is_empty());

	let (worker, network_service, mut block_announce_service) =
		network_utils::build_network_worker::<B, N>(
			dependencies,
			network_config,
			role,
			protocol_id,
			notification_metrics,
			None,
			"test-environment",
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
		let node_peer_id = *state.test_authorities.peer_ids.get(NODE_UNDER_TEST as usize).unwrap();
		async move {
			loop {
				tokio::select! {
					event = block_announce_service.next_event() => {
						if let Some(NotificationEvent::ValidateInboundSubstream { result_tx, .. }) = event {
							result_tx.send(ValidationResult::Accept).unwrap();
						}
					},
					event = validation_service.next_event() => {
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
									let _ = validation_service.send_async_notification(&peer, message.encode()).await;
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
									let validated_in_group = BitVec::from_iter((0..group_size).map(|i| i != 1));

									let ack = BackedCandidateAcknowledgement { candidate_hash: manifest.candidate_hash, statement_knowledge: StatementFilter { seconded_in_group, validated_in_group } };
									let message = WireMessage::ProtocolMessage(v3::ValidationProtocol::StatementDistribution(v3::StatementDistributionMessage::BackedCandidateKnown(ack)));
									validation_service.send_async_notification(&peer, message.encode()).await.unwrap();
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
					event = collation_service.next_event() => {
						gum::error!(target: LOG_TARGET, "Peer {} received collation {:?}", index, event);
					},
				}
			}
		}
	});
	let peer_requests_name = Box::leak(format!("Peer {} requests", index).into_boxed_str());
	spawn_handle.spawn(peer_requests_name, "test-environment", async move {
		loop {
			let req = candidate_req_receiver.recv(Vec::new).await.unwrap();
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

pub fn prepare_test<B, N>(
	state: &TestState,
	with_prometheus_endpoint: bool,
) -> (TestEnvironment, Vec<ProtocolConfig>)
where
	B: BlockT<Hash = H256> + 'static,
	N: NetworkBackend<B, B::Hash>,
{
	let dependencies = TestEnvironmentDependencies::default();
	let (network, _network_interface, _network_receiver) = new_network(
		&state.config,
		&dependencies,
		&state.test_authorities,
		vec![Arc::new(state.clone())],
	);

	let arc_state = Arc::new(state.clone());
	let (overseer, overseer_handle, cfg, peer_id, listen_address) =
		build_overseer::<B, N>(state, &dependencies);
	(0..state.config.n_validators)
		.filter(|i| *i != NODE_UNDER_TEST as usize)
		.for_each(|i| {
			build_peer::<B, N>(
				Arc::clone(&arc_state),
				&dependencies,
				i as u16,
				peer_id,
				listen_address.clone(),
			)
		});

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
	env.send_message(AllMessages::StatementDistribution(
		StatementDistributionMessage::NetworkBridgeUpdate(NetworkBridgeEvent::NewGossipTopology(
			NewGossipTopology {
				session: 0,
				topology: topology.clone(),
				local_index: Some(ValidatorIndex(NODE_UNDER_TEST)),
			},
		)),
	))
	.await;

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
