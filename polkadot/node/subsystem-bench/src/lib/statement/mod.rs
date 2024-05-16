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
		network_bridge::{MockNetworkBridgeRx, MockNetworkBridgeTx},
		prospective_parachains::MockProspectiveParachains,
		runtime_api::{MockRuntimeApi, MockRuntimeApiCoreState},
		AlwaysSupportsParachains,
	},
	network::new_network,
	usage::BenchmarkUsage,
	NODE_UNDER_TEST, PEER_IN_NODE_GROUP,
};
use bitvec::vec::BitVec;
use colored::Colorize;
use futures::{channel::mpsc, FutureExt, StreamExt};
use itertools::Itertools;
use polkadot_node_metrics::metrics::Metrics;
use polkadot_node_network_protocol::{
	grid_topology::{SessionGridTopology, TopologyPeerInfo},
	request_response::{IncomingRequest, ReqProtocolNames},
	v3::{self, BackedCandidateManifest, StatementFilter},
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
	AuthorityDiscoveryId, Block, GroupIndex, Hash, Id, ValidatorId, ValidatorIndex,
};
use polkadot_statement_distribution::StatementDistributionSubsystem;
use rand::SeedableRng;
use sc_keystore::LocalKeystore;
use sc_network::request_responses::ProtocolConfig;
use sc_network_types::PeerId;
use sc_service::SpawnTaskHandle;
use sp_keystore::{Keystore, KeystorePtr};
use sp_runtime::RuntimeAppPublic;
use std::{
	sync::{atomic::Ordering, Arc},
	time::{Duration, Instant},
};
pub use test_state::TestState;

mod test_state;

const LOG_TARGET: &str = "subsystem-bench::statement";

pub fn make_keystore() -> KeystorePtr {
	let keystore: KeystorePtr = Arc::new(LocalKeystore::in_memory());
	Keystore::sr25519_generate_new(&*keystore, ValidatorId::ID, Some("//Node0"))
		.expect("Insert key into keystore");
	Keystore::sr25519_generate_new(&*keystore, AuthorityDiscoveryId::ID, Some("//Node0"))
		.expect("Insert key into keystore");
	keystore
}

fn build_overseer(
	state: &TestState,
	network_bridge: (MockNetworkBridgeTx, MockNetworkBridgeRx),
	dependencies: &TestEnvironmentDependencies,
) -> (
	Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>,
	OverseerHandle,
	Vec<ProtocolConfig>,
	mpsc::UnboundedReceiver<AllMessages>,
) {
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
	let (tx, rx) = mpsc::unbounded();
	let mock_candidate_backing = MockCandidateBacking::new(
		tx,
		state.validator_pairs.get(NODE_UNDER_TEST as usize).unwrap().clone(),
		state.pvd.clone(),
		state.node_group.clone(),
	);
	let (statement_req_receiver, statement_req_cfg) = IncomingRequest::get_config_receiver::<
		Block,
		sc_network::NetworkWorker<Block, Hash>,
	>(&ReqProtocolNames::new(GENESIS_HASH, None));
	let (candidate_req_receiver, candidate_req_cfg) = IncomingRequest::get_config_receiver::<
		Block,
		sc_network::NetworkWorker<Block, Hash>,
	>(&ReqProtocolNames::new(GENESIS_HASH, None));
	let keystore = make_keystore();
	let subsystem = StatementDistributionSubsystem::new(
		keystore.clone(),
		statement_req_receiver,
		candidate_req_receiver,
		Metrics::try_register(&dependencies.registry).unwrap(),
		rand::rngs::StdRng::from_entropy(),
	);
	let dummy = dummy_builder!(spawn_task_handle, overseer_metrics)
		.replace_runtime_api(|_| mock_runtime_api)
		.replace_chain_api(|_| mock_chain_api)
		.replace_prospective_parachains(|_| mock_prospective_parachains)
		.replace_candidate_backing(|_| mock_candidate_backing)
		.replace_statement_distribution(|_| subsystem)
		.replace_network_bridge_tx(|_| network_bridge.0)
		.replace_network_bridge_rx(|_| network_bridge.1);
	let (overseer, raw_handle) =
		dummy.build_with_connector(overseer_connector).expect("Should not fail");
	let overseer_handle = OverseerHandle::new(raw_handle);

	(overseer, overseer_handle, vec![statement_req_cfg, candidate_req_cfg], rx)
}

pub fn prepare_test(
	state: &TestState,
	with_prometheus_endpoint: bool,
) -> (TestEnvironment, Vec<ProtocolConfig>, mpsc::UnboundedReceiver<AllMessages>) {
	let dependencies = TestEnvironmentDependencies::default();
	let (network, network_interface, network_receiver) = new_network(
		&state.config,
		&dependencies,
		&state.test_authorities,
		vec![Arc::new(state.clone())],
	);
	let network_bridge_tx = MockNetworkBridgeTx::new(
		network.clone(),
		network_interface.subsystem_sender(),
		state.test_authorities.clone(),
	);
	let network_bridge_rx = MockNetworkBridgeRx::new(network_receiver, None);
	let (overseer, overseer_handle, cfg, to_subsystems) =
		build_overseer(state, (network_bridge_tx, network_bridge_rx), &dependencies);

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
		to_subsystems,
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
	benchmark_name: &str,
	env: &mut TestEnvironment,
	state: &TestState,
	mut to_subsystems: mpsc::UnboundedReceiver<AllMessages>,
) -> BenchmarkUsage {
	state.reset_trackers();

	let connected_validators = state
		.test_authorities
		.validator_authority_id
		.iter()
		.enumerate()
		.filter_map(|(i, id)| if env.network().is_peer_connected(id) { Some(i) } else { None })
		.collect_vec();

	let config = env.config().clone();
	let groups = state.session_info.validator_groups.clone();
	let node_group_index = groups
		.iter()
		.position(|group| group.iter().any(|v| v.0 == NODE_UNDER_TEST))
		.expect("Pregenerated");

	env.metrics().set_n_validators(config.n_validators);
	env.metrics().set_n_cores(config.n_cores);

	// First create the initialization messages that make sure that then node under
	// tests receives notifications about the topology used and the connected peers.
	let topology = generate_topology(&state.test_authorities);
	let mut initialization_messages =
		env.network().generate_statement_distribution_peer_connected();
	initialization_messages
		.extend(generate_new_session_topology(&topology, ValidatorIndex(NODE_UNDER_TEST)));
	for message in initialization_messages {
		env.send_message(message).await;
	}

	let test_start = Instant::now();
	let mut total_message_count = 0;
	for block_info in state.block_infos.iter() {
		let block_num = block_info.number as usize;
		gum::info!(target: LOG_TARGET, "Current block {}/{} {}", block_num, config.num_blocks, block_info.hash);
		env.metrics().set_current_block(block_num);
		env.import_block(block_info.clone()).await;

		for update in env
			.network()
			.generate_statement_distribution_peer_view_change(view![block_info.hash])
		{
			env.send_message(update).await;
		}

		let seconding_peer_id =
			*state.test_authorities.peer_ids.get(PEER_IN_NODE_GROUP as usize).unwrap();
		let candidate = state.candidate_receipts.get(&block_info.hash).unwrap().first().unwrap();
		let candidate_hash = candidate.hash();
		let statement = state
			.statements
			.get(&candidate_hash)
			.unwrap()
			.get(PEER_IN_NODE_GROUP as usize)
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
		// Just sent for the node group
		let mut message_tracker = (0..groups.len()).map(|i| i == node_group_index).collect_vec();

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

		let one_hop_initiators = connected_neighbors_x
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
		let two_hop_x_initiators = connected_neighbors_x
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
		let two_hop_y_initiators = connected_neighbors_y
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

		for (seconding_peer_id, group_index) in one_hop_initiators
			.into_iter()
			.chain(two_hop_x_initiators)
			.chain(two_hop_y_initiators)
		{
			let sent = message_tracker.get_mut(group_index).unwrap();
			if *sent {
				continue
			}
			*sent = true;

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

		total_message_count += message_tracker.iter().filter(|&&v| v).collect_vec().len();

		let mut timeout = message_check_delay();
		loop {
			futures::select! {
				msg = to_subsystems.next() => {
					if let Some(msg) = msg {
						env.send_message(msg).await;
					}
				},
				_ = timeout => {
					let manifest_count = state
						.manifests_tracker
						.values()
						.filter(|v| v.load(Ordering::SeqCst))
						.collect::<Vec<_>>()
						.len();
					gum::debug!(target: LOG_TARGET, "{}/{} manifest exchanges", manifest_count, total_message_count);
					if manifest_count == total_message_count  {
						break;
					} else {
						timeout = message_check_delay();
					}
				}
			}
		}
	}

	let duration: u128 = test_start.elapsed().as_millis();
	gum::info!(target: LOG_TARGET, "All blocks processed in {}", format!("{:?}ms", duration).cyan());
	gum::info!(target: LOG_TARGET,
		"Avg block time: {}",
		format!("{} ms", test_start.elapsed().as_millis() / env.config().num_blocks as u128).red()
	);

	env.stop().await;
	env.collect_resource_usage(benchmark_name, &["statement-distribution"])
}

fn message_check_delay() -> futures::prelude::future::Fuse<futures_timer::Delay> {
	futures_timer::Delay::new(Duration::from_millis(50)).fuse()
}
