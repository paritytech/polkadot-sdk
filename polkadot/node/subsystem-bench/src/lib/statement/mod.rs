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
	dummy_builder,
	environment::{TestEnvironment, TestEnvironmentDependencies, GENESIS_HASH},
	mock::{
		chain_api::{ChainApiState, MockChainApi},
		network_bridge::{MockNetworkBridgeRx, MockNetworkBridgeTx},
		prospective_parachains::MockProspectiveParachains,
		runtime_api::MockRuntimeApi,
		AlwaysSupportsParachains,
	},
	network::new_network,
	usage::BenchmarkUsage,
};
use colored::Colorize;
use polkadot_node_metrics::metrics::Metrics;
use polkadot_node_network_protocol::request_response::{IncomingRequest, ReqProtocolNames};
use polkadot_node_primitives::{SignedFullStatementWithPVD, Statement};
use polkadot_node_subsystem::messages::{AllMessages, StatementDistributionMessage};
use polkadot_overseer::{
	Handle as OverseerHandle, Overseer, OverseerConnector, OverseerMetrics, SpawnGlue,
};
use polkadot_primitives::{Block, Hash, SigningContext, ValidatorIndex, ValidatorPair};
use polkadot_statement_distribution::StatementDistributionSubsystem;
use rand::SeedableRng;
use sc_keystore::LocalKeystore;
use sc_network::request_responses::ProtocolConfig;
use sc_service::SpawnTaskHandle;
use sp_core::Pair;
use std::{sync::Arc, time::Instant};
pub use test_state::TestState;

mod test_state;

const LOG_TARGET: &str = "subsystem-bench::availability";

fn build_overseer(
	state: &TestState,
	network_bridge: (MockNetworkBridgeTx, MockNetworkBridgeRx),
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
	);
	let chain_api_state = ChainApiState { block_headers: state.block_headers.clone() };
	let mock_chain_api = MockChainApi::new(chain_api_state);
	let mock_prospective_parachains = MockProspectiveParachains::new();
	let (statement_req_receiver, statement_req_cfg) = IncomingRequest::get_config_receiver::<
		Block,
		sc_network::NetworkWorker<Block, Hash>,
	>(&ReqProtocolNames::new(GENESIS_HASH, None));
	let (candidate_req_receiver, candidate_req_cfg) = IncomingRequest::get_config_receiver::<
		Block,
		sc_network::NetworkWorker<Block, Hash>,
	>(&ReqProtocolNames::new(GENESIS_HASH, None));
	let subsystem = StatementDistributionSubsystem::new(
		Arc::new(LocalKeystore::in_memory()),
		statement_req_receiver,
		candidate_req_receiver,
		Metrics::try_register(&dependencies.registry).unwrap(),
		rand::rngs::StdRng::from_entropy(),
	);
	let dummy = dummy_builder!(spawn_task_handle, overseer_metrics)
		.replace_runtime_api(|_| mock_runtime_api)
		.replace_chain_api(|_| mock_chain_api)
		.replace_prospective_parachains(|_| mock_prospective_parachains)
		.replace_statement_distribution(|_| subsystem)
		.replace_network_bridge_tx(|_| network_bridge.0)
		.replace_network_bridge_rx(|_| network_bridge.1);
	let (overseer, raw_handle) =
		dummy.build_with_connector(overseer_connector).expect("Should not fail");
	let overseer_handle = OverseerHandle::new(raw_handle);

	(overseer, overseer_handle, vec![statement_req_cfg, candidate_req_cfg])
}

pub fn prepare_test(
	state: &TestState,
	with_prometheus_endpoint: bool,
) -> (TestEnvironment, Vec<ProtocolConfig>) {
	let dependencies = TestEnvironmentDependencies::default();
	let (network, network_interface, network_receiver) =
		new_network(&state.config, &dependencies, &state.test_authorities, vec![]);
	let network_bridge_tx = MockNetworkBridgeTx::new(
		network.clone(),
		network_interface.subsystem_sender(),
		state.test_authorities.clone(),
	);
	let network_bridge_rx = MockNetworkBridgeRx::new(network_receiver, None);
	let (overseer, overseer_handle, cfg) =
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
	)
}

pub async fn benchmark_statement_distribution(
	benchmark_name: &str,
	env: &mut TestEnvironment,
	state: &TestState,
) -> BenchmarkUsage {
	let config = env.config().clone();

	env.metrics().set_n_validators(config.n_validators);
	env.metrics().set_n_cores(config.n_cores);

	let pair = ValidatorPair::generate().0;

	let test_start = Instant::now();
	for block_info in state.block_infos.iter() {
		let block_num = block_info.number as usize;
		gum::info!(target: LOG_TARGET, "Current block {}/{}", block_num, config.num_blocks);
		env.metrics().set_current_block(block_num);
		env.import_block(block_info.clone()).await;

		let receipts = state
			.commited_candidate_receipts
			.get(&block_info.hash)
			.expect("Pregenerated")
			.clone();

		for receipt in receipts {
			let statement = Statement::Seconded(receipt);
			let context = SigningContext { parent_hash: block_info.parent_hash, session_index: 0 };
			let payload = statement.to_compact().signing_payload(&context);
			let signature = pair.sign(&payload[..]);
			let message = AllMessages::StatementDistribution(StatementDistributionMessage::Share(
				block_info.hash,
				SignedFullStatementWithPVD::new(
					statement.supply_pvd(state.persisted_validation_data.clone()),
					ValidatorIndex(0),
					signature,
					&context,
					&pair.public(),
				)
				.unwrap(),
			));
			env.send_message(message).await;
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
