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

use colored::Colorize;
use polkadot_overseer::{
	Handle as OverseerHandle, MetricsTrait, Overseer, OverseerConnector, OverseerMetrics, SpawnGlue,
};
use sc_service::SpawnTaskHandle;
use std::time::Instant;
pub use test_state::TestState;

const LOG_TARGET: &str = "subsystem-bench::availability";

use crate::{
	dummy_builder,
	environment::{TestEnvironment, TestEnvironmentDependencies},
	mock::AlwaysSupportsParachains,
	network::new_network,
	usage::BenchmarkUsage,
};

mod test_state;

/// Helper function to build an overseer with the real implementation for `ApprovalDistribution` and
/// `ApprovalVoting` subsystems and mock subsystems for all others.
fn build_overseer(
	dependencies: &TestEnvironmentDependencies,
) -> (Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>, OverseerHandle) {
	let overseer_connector = OverseerConnector::with_event_capacity(64000);
	let overseer_metrics = OverseerMetrics::try_register(&dependencies.registry).unwrap();
	let spawn_task_handle = dependencies.task_manager.spawn_handle();
	let dummy = dummy_builder!(spawn_task_handle, overseer_metrics);
	let (overseer, raw_handle) =
		dummy.build_with_connector(overseer_connector).expect("Should not fail");
	let overseer_handle = OverseerHandle::new(raw_handle);

	(overseer, overseer_handle)
}

pub fn prepare_test(state: &TestState, with_prometheus_endpoint: bool) -> TestEnvironment {
	let dependencies = TestEnvironmentDependencies::default();
	let (network, _network_interface, _network_receiver) =
		new_network(&state.config, &dependencies, &state.test_authorities, vec![]);
	let (overseer, overseer_handle) = build_overseer(&dependencies);

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

pub async fn benchmark_statement_distribution(
	benchmark_name: &str,
	env: &mut TestEnvironment,
	state: &TestState,
) -> BenchmarkUsage {
	let config = env.config().clone();

	env.metrics().set_n_validators(config.n_validators);
	env.metrics().set_n_cores(config.n_cores);

	let test_start = Instant::now();
	for block_info in state.block_infos.iter() {
		let block_num = block_info.number as usize;
		gum::info!(target: LOG_TARGET, "Current block {}/{}", block_num, config.num_blocks);
		env.metrics().set_current_block(block_num);
		env.import_block(block_info.clone()).await;
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
