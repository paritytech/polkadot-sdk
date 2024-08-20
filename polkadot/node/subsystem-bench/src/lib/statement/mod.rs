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
	network::{new_network, NetworkEmulatorHandle, NetworkInterface, NetworkInterfaceReceiver},
	usage::BenchmarkUsage,
	NODE_UNDER_TEST,
};
use bitvec::vec::BitVec;
use colored::Colorize;
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
	network: NetworkEmulatorHandle,
	network_interface: NetworkInterface,
	network_receiver: NetworkInterfaceReceiver,
	dependencies: &TestEnvironmentDependencies,
) -> (
	Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>,
	OverseerHandle,
	Vec<ProtocolConfig>,
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
	let network_bridge_tx = MockNetworkBridgeTx::new(
		network,
		network_interface.subsystem_sender(),
		state.test_authorities.clone(),
	);
	let network_bridge_rx = MockNetworkBridgeRx::new(network_receiver, Some(candidate_req_cfg));

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

	(overseer, overseer_handle, vec![statement_req_cfg])
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
	let (overseer, overseer_handle, cfg) =
		build_overseer(state, network.clone(), network_interface, network_receiver, &dependencies);

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
	let peer_connected_messages = env.network().generate_peer_connected(|e| {
		AllMessages::StatementDistribution(StatementDistributionMessage::NetworkBridgeUpdate(e))
	});
	let new_session_topology_messages =
		generate_new_session_topology(&topology, ValidatorIndex(NODE_UNDER_TEST));
	for message in peer_connected_messages.into_iter().chain(new_session_topology_messages) {
		env.send_message(message).await;
	}

	let test_start = Instant::now();
	let mut candidates_advertised = 0;
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
			gum::debug!(target: LOG_TARGET, "{}/{} manifest exchanges", manifests_count, candidates_advertised);

			if manifests_count == candidates_advertised {
				break;
			}
			tokio::time::sleep(Duration::from_millis(50)).await;
		}
	}

	let duration: u128 = test_start.elapsed().as_millis();
	gum::info!(target: LOG_TARGET, "All blocks processed in {}", format!("{:?}ms", duration).cyan());
	gum::info!(target: LOG_TARGET,
		"Avg block time: {}",
		format!("{} ms", test_start.elapsed().as_millis() / env.config().num_blocks as u128).red()
	);

	env.stop().await;
	env.collect_resource_usage(&["statement-distribution"])
}
