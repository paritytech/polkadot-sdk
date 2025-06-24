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
		approval_voting_parallel::MockApprovalVotingParallel,
		availability_recovery::MockAvailabilityRecovery,
		candidate_backing::MockCandidateBacking,
		candidate_validation::MockCandidateValidation,
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
use polkadot_node_core_dispute_coordinator::{
	Config as DisputeCoordinatorConfig, DisputeCoordinatorSubsystem,
};
use polkadot_node_metrics::metrics::Metrics;
use polkadot_node_network_protocol::{
	grid_topology::{SessionGridTopology, TopologyPeerInfo},
	request_response::{IncomingRequest, ReqProtocolNames},
	v3::{self, BackedCandidateManifest, StatementFilter},
	view, ValidationProtocols, View,
};
use polkadot_node_primitives::{DisputeStatus, SignedDisputeStatement};
use polkadot_node_subsystem::messages::{
	network_bridge_event::NewGossipTopology, AllMessages, DisputeCoordinatorMessage,
	ImportStatementsResult, NetworkBridgeEvent, StatementDistributionMessage,
};
use polkadot_overseer::{
	Handle as OverseerHandle, Overseer, OverseerConnector, OverseerMetrics, SpawnGlue,
};
use polkadot_primitives::{
	AuthorityDiscoveryId, Block, GroupIndex, Hash, Id, ValidatorId, ValidatorIndex,
};
use sc_keystore::LocalKeystore;
use sc_network_types::PeerId;
use sc_service::SpawnTaskHandle;
use sp_core::Public;
use sp_keystore::Keystore;
use sp_runtime::RuntimeAppPublic;
use std::{
	sync::{atomic::Ordering, Arc},
	time::{Duration, Instant},
};
pub use test_state::TestState;

mod test_state;

const LOG_TARGET: &str = "subsystem-bench::disputes";

pub fn make_keystore() -> Arc<LocalKeystore> {
	let keystore = Arc::new(LocalKeystore::in_memory());
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
) -> (Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>, OverseerHandle) {
	let overseer_connector = OverseerConnector::with_event_capacity(64000);
	let overseer_metrics = OverseerMetrics::try_register(&dependencies.registry).unwrap();
	let spawn_task_handle = dependencies.task_manager.spawn_handle();

	let db = kvdb_memorydb::create(1);
	let db = polkadot_node_subsystem_util::database::kvdb_impl::DbAdapter::new(db, &[0]);
	let store = Arc::new(db);
	let config = DisputeCoordinatorConfig { col_dispute_data: 0 };
	let keystore = make_keystore();
	let approval_voting_parallel_enabled = true;
	let mock_runtime_api = MockRuntimeApi::new(
		state.config.clone(),
		state.test_authorities.clone(),
		state.candidate_receipts.clone(),
		state.candidate_events.clone(),
		Default::default(),
		0,
		MockRuntimeApiCoreState::Scheduled,
	);
	let chain_api_state = ChainApiState { block_headers: state.block_headers.clone() };
	let mock_chain_api = MockChainApi::new(chain_api_state);
	let mock_availability_recovery =
		MockAvailabilityRecovery::new(state.missing_availability.clone());
	let mock_approval_voting = MockApprovalVotingParallel::new();
	let mock_candidate_validation = MockCandidateValidation::new();
	let dispute_coordinator = DisputeCoordinatorSubsystem::new(
		store,
		config,
		keystore,
		Metrics::try_register(&dependencies.registry).unwrap(),
		approval_voting_parallel_enabled,
	);

	let dummy = dummy_builder!(spawn_task_handle, overseer_metrics)
		.replace_runtime_api(|_| mock_runtime_api)
		.replace_chain_api(|_| mock_chain_api)
		.replace_availability_recovery(|_| mock_availability_recovery)
		.replace_approval_voting_parallel(|_| mock_approval_voting)
		.replace_candidate_validation(|_| mock_candidate_validation)
		.replace_dispute_coordinator(|_| dispute_coordinator);

	let (overseer, raw_handle) = dummy.build_with_connector(overseer_connector).unwrap();
	let overseer_handle = OverseerHandle::new(raw_handle);

	(overseer, overseer_handle)
}

pub fn prepare_test(state: &TestState, with_prometheus_endpoint: bool) -> TestEnvironment {
	let dependencies = TestEnvironmentDependencies::default();
	let (network, network_interface, network_receiver) = new_network(
		&state.config,
		&dependencies,
		&state.test_authorities,
		vec![Arc::new(state.clone())],
	);
	let (overseer, overseer_handle) =
		build_overseer(state, network.clone(), network_interface, network_receiver, &dependencies);

	TestEnvironment::new(
		dependencies,
		state.config.clone(),
		network,
		overseer,
		overseer_handle,
		state.test_authorities.clone(),
		with_prometheus_endpoint,
	)
}

pub async fn benchmark_dispute_coordinator(
	env: &mut TestEnvironment,
	state: &TestState,
) -> BenchmarkUsage {
	let config = env.config().clone();

	let test_start = Instant::now();

	for block_info in state.block_infos.iter() {
		let block_num = block_info.number as usize;
		gum::info!(target: LOG_TARGET, "Current block {}/{} {:?}", block_num, config.num_blocks, block_info.hash);
		env.metrics().set_current_block(block_num);
		env.import_block(block_info.clone()).await;

		let candidate_receipt1 = &state.candidate_receipts.get(&block_info.hash).unwrap()[0];
		let candidate_receipt2 = &state.candidate_receipts.get(&block_info.hash).unwrap()[1];
		let (valid_vote1, invalid_vote1) =
			&state.signed_dispute_statements.get(&block_info.hash).unwrap()[0];
		let (valid_vote2, invalid_vote2) =
			&state.signed_dispute_statements.get(&block_info.hash).unwrap()[1];

		let (confirmation_tx, confirmation_rx) = futures::channel::oneshot::channel();
		env.send_message(AllMessages::DisputeCoordinator(
			DisputeCoordinatorMessage::ImportStatements {
				candidate_receipt: candidate_receipt1.clone(),
				session: 1,
				statements: vec![
					(valid_vote1.clone(), ValidatorIndex(3)),
					(invalid_vote1.clone(), ValidatorIndex(1)),
				],
				pending_confirmation: Some(confirmation_tx),
			},
		))
		.await;
		assert_eq!(confirmation_rx.await.unwrap(), ImportStatementsResult::ValidImport);

		let (active_disputes_tx, active_disputes_rx) = futures::channel::oneshot::channel();
		env.send_message(AllMessages::DisputeCoordinator(
			DisputeCoordinatorMessage::ActiveDisputes(active_disputes_tx),
		))
		.await;

		assert_eq!(
			active_disputes_rx.await.unwrap(),
			vec![(1, candidate_receipt1.hash(), DisputeStatus::Active)]
		);

		let (query_candidate_votes_tx, query_candidate_votes_rx) =
			futures::channel::oneshot::channel();
		env.send_message(AllMessages::DisputeCoordinator(
			DisputeCoordinatorMessage::QueryCandidateVotes(
				vec![(1, candidate_receipt1.hash())],
				query_candidate_votes_tx,
			),
		))
		.await;
		let (_, _, votes) = query_candidate_votes_rx.await.unwrap().get(0).unwrap().clone();
		assert_eq!(votes.valid.raw().len(), 2);
		assert_eq!(votes.invalid.len(), 1);

		let (confirmation_tx, confirmation_rx) = futures::channel::oneshot::channel();
		env.send_message(AllMessages::DisputeCoordinator(
			DisputeCoordinatorMessage::ImportStatements {
				candidate_receipt: candidate_receipt2.clone(),
				session: 1,
				statements: vec![
					(valid_vote2.clone(), ValidatorIndex(3)),
					(invalid_vote2.clone(), ValidatorIndex(1)),
				],
				pending_confirmation: Some(confirmation_tx),
			},
		))
		.await;
		assert_eq!(confirmation_rx.await.unwrap(), ImportStatementsResult::ValidImport);

		let (query_candidate_votes_tx, query_candidate_votes_rx) =
			futures::channel::oneshot::channel();
		env.send_message(AllMessages::DisputeCoordinator(
			DisputeCoordinatorMessage::QueryCandidateVotes(
				vec![(1, candidate_receipt2.hash())],
				query_candidate_votes_tx,
			),
		))
		.await;
		let (_, _, votes) = query_candidate_votes_rx.await.unwrap().get(0).unwrap().clone();
		assert_eq!(votes.valid.raw().len(), 1);
		assert_eq!(votes.invalid.len(), 1);
	}

	let duration: u128 = test_start.elapsed().as_millis();
	gum::info!(target: LOG_TARGET, "All blocks processed in {}", format!("{:?}ms", duration).cyan());
	gum::info!(target: LOG_TARGET,
		"Avg block time: {}",
		format!("{} ms", test_start.elapsed().as_millis() / env.config().num_blocks as u128).red()
	);

	env.stop().await;
	env.collect_resource_usage(&["dispute-coordinator"], false)
}
