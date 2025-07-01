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

//! Subsystem benchmark for the dispute coordinator and dispute distribution subsystems.
//!
//! Scenarios:
//! 1. Dispute participation.
//!   - Dispute distribution receives a DisputeRequest message from Validator 1 with votes:
//!     - valid (Validator 1).
//!     - invalid (Validator 3) <- malicious.
//!   - Dispute distribution sends DisputeCoordinatorMessage::ImportStatements.
//!   - Dispute coordinator imports the votes and participate in the dispute.
//!   - Dispute coordinator sends DisputeDistributionMessage::SendDispute.
//!   - Dispute distribution sends DisputeRequest to all validators.
//! 2. TODO: Dispute confirmed: we need 1/3+1 votes per candidate.
//! 3. TODO: Dispute concluded: we need 2/3+1 votes per candidate. Here we can test db population
//! 4. TODO: Spamming: a combination of scenario 3 + multiple of scenario 1

use crate::{
	dummy_builder,
	environment::{TestEnvironment, TestEnvironmentDependencies, GENESIS_HASH},
	mock::{
		approval_voting_parallel::MockApprovalVotingParallel,
		availability_recovery::MockAvailabilityRecovery,
		candidate_validation::MockCandidateValidation,
		chain_api::{ChainApiState, MockChainApi},
		network_bridge::{MockNetworkBridgeRx, MockNetworkBridgeTx},
		runtime_api::{MockRuntimeApi, MockRuntimeApiCoreState},
		AlwaysSupportsParachains,
	},
	network::{new_network, NetworkEmulatorHandle, NetworkInterface, NetworkInterfaceReceiver},
	usage::BenchmarkUsage,
};
use codec::Encode;
use colored::Colorize;
use polkadot_dispute_distribution::DisputeDistributionSubsystem;
use polkadot_node_core_dispute_coordinator::{
	Config as DisputeCoordinatorConfig, DisputeCoordinatorSubsystem,
};
use polkadot_node_metrics::metrics::Metrics;
use polkadot_node_network_protocol::request_response::{IncomingRequest, ReqProtocolNames};
use polkadot_overseer::{
	Handle as OverseerHandle, Overseer, OverseerConnector, OverseerMetrics, SpawnGlue,
};
use polkadot_primitives::{AuthorityDiscoveryId, Block, Hash, ValidatorId};
use sc_keystore::LocalKeystore;
use sc_network::request_responses::IncomingRequest as RawIncomingRequest;
use sc_service::SpawnTaskHandle;
use serde::{Deserialize, Serialize};
use sp_keystore::Keystore;
use sp_runtime::RuntimeAppPublic;
use std::{sync::Arc, time::Instant};
pub use test_state::TestState;

mod test_state;

const LOG_TARGET: &str = "subsystem-bench::disputes";

/// Parameters specific to the approvals benchmark
#[derive(Debug, Clone, Serialize, Deserialize, clap::Parser)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct DisputesOptions {
	#[clap(short, long, default_value_t = 10)]
	/// The number of disputes to participate in.
	pub n_disputes: u32,
}

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
	let (dispute_req_receiver, dispute_req_cfg) = IncomingRequest::get_config_receiver::<
		Block,
		sc_network::NetworkWorker<Block, Hash>,
	>(&ReqProtocolNames::new(GENESIS_HASH, None));
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
	let mock_availability_recovery = MockAvailabilityRecovery::new();
	let mock_approval_voting = MockApprovalVotingParallel::new();
	let mock_candidate_validation = MockCandidateValidation::new();
	let network_bridge_tx = MockNetworkBridgeTx::new(
		network,
		network_interface.subsystem_sender(),
		state.test_authorities.clone(),
	);
	let network_bridge_rx = MockNetworkBridgeRx::new(network_receiver, Some(dispute_req_cfg));
	let dispute_distribution = DisputeDistributionSubsystem::new(
		keystore.clone(),
		dispute_req_receiver,
		state.test_authorities.clone(),
		Metrics::try_register(&dependencies.registry).unwrap(),
	);
	let dispute_coordinator = DisputeCoordinatorSubsystem::new(
		store,
		config,
		keystore,
		Metrics::try_register(&dependencies.registry).unwrap(),
	);

	let dummy = dummy_builder!(spawn_task_handle, overseer_metrics)
		.replace_runtime_api(|_| mock_runtime_api)
		.replace_chain_api(|_| mock_chain_api)
		.replace_availability_recovery(|_| mock_availability_recovery)
		.replace_approval_voting_parallel(|_| mock_approval_voting)
		.replace_candidate_validation(|_| mock_candidate_validation)
		.replace_network_bridge_tx(|_| network_bridge_tx)
		.replace_network_bridge_rx(|_| network_bridge_rx)
		.replace_dispute_distribution(|_| dispute_distribution)
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

		let candidate_receipts =
			state.candidate_receipts.get(&block_info.hash).expect("pregenerated");
		for candidate_receipt in candidate_receipts.iter() {
			let peer_id = *env.authorities().peer_ids.get(1).expect("all validators have ids");
			let payload =
				state.dispute_requests.get(&candidate_receipt.hash()).expect("pregenerated");
			let (pending_response, pending_response_receiver) =
				futures::channel::oneshot::channel();
			let request =
				RawIncomingRequest { peer: peer_id, payload: payload.encode(), pending_response };
			let peer = env
				.authorities()
				.validator_authority_id
				.get(1)
				.expect("all validators have keys");

			assert!(env.network().is_peer_connected(peer), "Peer {:?} is not connected", peer);
			env.network().send_request_from_peer(peer, request).unwrap();
			let res = pending_response_receiver.await.expect("dispute request sent");
			gum::debug!(target: LOG_TARGET, "Dispute request sent to node from peer {:?}", res);
		}

		let candidate_hashes =
			candidate_receipts.iter().map(|receipt| receipt.hash()).collect::<Vec<_>>();
		let requests_expected = candidate_hashes.len() *
			(state.config.n_validators * state.config.connectivity / 100 - 1);

		loop {
			let requests_sent = candidate_hashes
				.iter()
				.map(|candidate_hash| {
					state
						.requests_tracker
						.lock()
						.unwrap()
						.get(candidate_hash)
						.unwrap_or(&Default::default())
						.len()
				})
				.sum::<usize>();

			gum::info!(target: LOG_TARGET, "Waiting for dispute requests to be sent: {}/{}", requests_sent, requests_expected);
			if requests_sent == requests_expected {
				break;
			}

			tokio::time::sleep(std::time::Duration::from_millis(100)).await;
		}
	}

	let duration: u128 = test_start.elapsed().as_millis();
	gum::info!(target: LOG_TARGET, "All blocks processed in {}", format!("{:?}ms", duration).cyan());
	gum::info!(target: LOG_TARGET,
		"Avg block time: {}",
		format!("{} ms", test_start.elapsed().as_millis() / env.config().num_blocks as u128).red()
	);

	env.stop().await;
	env.collect_resource_usage(&["dispute-coordinator", "dispute-distribution"], false)
}
