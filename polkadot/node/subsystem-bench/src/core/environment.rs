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
//! Test environment implementation
use crate::{
	core::{mock::AlwaysSupportsParachains, network::NetworkEmulator},
	TestConfiguration,
};
use colored::Colorize;
use core::time::Duration;
use futures::FutureExt;
use polkadot_overseer::{BlockInfo, Handle as OverseerHandle};

use polkadot_node_subsystem::{messages::AllMessages, Overseer, SpawnGlue, TimeoutExt};
use polkadot_node_subsystem_types::Hash;
use polkadot_node_subsystem_util::metrics::prometheus::{
	self, Gauge, Histogram, PrometheusError, Registry, U64,
};

use sc_network::peer_store::LOG_TARGET;
use sc_service::{SpawnTaskHandle, TaskManager};
use std::{
	fmt::Display,
	net::{Ipv4Addr, SocketAddr},
};
use tokio::runtime::Handle;

const MIB: f64 = 1024.0 * 1024.0;

/// Test environment/configuration metrics
#[derive(Clone)]
pub struct TestEnvironmentMetrics {
	/// Number of bytes sent per peer.
	n_validators: Gauge<U64>,
	/// Number of received sent per peer.
	n_cores: Gauge<U64>,
	/// PoV size
	pov_size: Histogram,
	/// Current block
	current_block: Gauge<U64>,
	/// Current block
	block_time: Gauge<U64>,
}

impl TestEnvironmentMetrics {
	pub fn new(registry: &Registry) -> Result<Self, PrometheusError> {
		let mut buckets = prometheus::exponential_buckets(16384.0, 2.0, 9)
			.expect("arguments are always valid; qed");
		buckets.extend(vec![5.0 * MIB, 6.0 * MIB, 7.0 * MIB, 8.0 * MIB, 9.0 * MIB, 10.0 * MIB]);

		Ok(Self {
			n_validators: prometheus::register(
				Gauge::new(
					"subsystem_benchmark_n_validators",
					"Total number of validators in the test",
				)?,
				registry,
			)?,
			n_cores: prometheus::register(
				Gauge::new(
					"subsystem_benchmark_n_cores",
					"Number of cores we fetch availability for each block",
				)?,
				registry,
			)?,
			current_block: prometheus::register(
				Gauge::new("subsystem_benchmark_current_block", "The current test block")?,
				registry,
			)?,
			block_time: prometheus::register(
				Gauge::new("subsystem_benchmark_block_time", "The time it takes for the target subsystems(s) to complete all the requests in a block")?,
				registry,
			)?,
			pov_size: prometheus::register(
				Histogram::with_opts(
					prometheus::HistogramOpts::new(
						"subsystem_benchmark_pov_size",
						"The compressed size of the proof of validity of a candidate",
					)
					.buckets(buckets),
				)?,
				registry,
			)?,
		})
	}

	pub fn set_n_validators(&self, n_validators: usize) {
		self.n_validators.set(n_validators as u64);
	}

	pub fn set_n_cores(&self, n_cores: usize) {
		self.n_cores.set(n_cores as u64);
	}

	pub fn set_current_block(&self, current_block: usize) {
		self.current_block.set(current_block as u64);
	}

	pub fn set_block_time(&self, block_time_ms: u64) {
		self.block_time.set(block_time_ms);
	}

	pub fn on_pov_size(&self, pov_size: usize) {
		self.pov_size.observe(pov_size as f64);
	}
}

fn new_runtime() -> tokio::runtime::Runtime {
	tokio::runtime::Builder::new_multi_thread()
		.thread_name("subsystem-bench")
		.enable_all()
		.thread_stack_size(3 * 1024 * 1024)
		.build()
		.unwrap()
}

/// Wrapper for dependencies
pub struct TestEnvironmentDependencies {
	pub registry: Registry,
	pub task_manager: TaskManager,
	pub runtime: tokio::runtime::Runtime,
}

impl Default for TestEnvironmentDependencies {
	fn default() -> Self {
		let runtime = new_runtime();
		let registry = Registry::new();
		let task_manager: TaskManager =
			TaskManager::new(runtime.handle().clone(), Some(&registry)).unwrap();

		Self { runtime, registry, task_manager }
	}
}

// A dummy genesis hash
pub const GENESIS_HASH: Hash = Hash::repeat_byte(0xff);

// We use this to bail out sending messages to the subsystem if it is overloaded such that
// the time of flight is breaches 5s.
// This should eventually be a test parameter.
const MAX_TIME_OF_FLIGHT: Duration = Duration::from_millis(5000);

/// The test environment is the high level wrapper of all things required to test
/// a certain subsystem.
///
/// ## Mockups
/// The overseer is passed in during construction and it can host an arbitrary number of
/// real subsystems instances and the corresponding mocked instances such that the real
/// subsystems can get their messages answered.
///
/// As the subsystem's performance depends on network connectivity, the test environment
/// emulates validator nodes on the network, see `NetworkEmulator`. The network emulation
/// is configurable in terms of peer bandwidth, latency and connection error rate using
/// uniform distribution sampling.
///
///
/// ## Usage
/// `TestEnvironment` is used in tests to send `Overseer` messages or signals to the subsystem
/// under test.
///
/// ## Collecting test metrics
///
/// ### Prometheus
/// A prometheus endpoint is exposed while the test is running. A local Prometheus instance
/// can scrape it every 1s and a Grafana dashboard is the preferred way of visualizing
/// the performance characteristics of the subsystem.
///
/// ### CLI
/// A subset of the Prometheus metrics are printed at the end of the test.
pub struct TestEnvironment {
	/// Test dependencies
	dependencies: TestEnvironmentDependencies,
	/// A runtime handle
	runtime_handle: tokio::runtime::Handle,
	/// A handle to the lovely overseer
	overseer_handle: OverseerHandle,
	/// The test configuration.
	config: TestConfiguration,
	/// A handle to the network emulator.
	network: NetworkEmulator,
	/// Configuration/env metrics
	metrics: TestEnvironmentMetrics,
}

impl TestEnvironment {
	/// Create a new test environment
	pub fn new(
		dependencies: TestEnvironmentDependencies,
		config: TestConfiguration,
		network: NetworkEmulator,
		overseer: Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>,
		overseer_handle: OverseerHandle,
	) -> Self {
		let metrics = TestEnvironmentMetrics::new(&dependencies.registry)
			.expect("Metrics need to be registered");

		let spawn_handle = dependencies.task_manager.spawn_handle();
		spawn_handle.spawn_blocking("overseer", "overseer", overseer.run().boxed());

		let registry_clone = dependencies.registry.clone();
		dependencies.task_manager.spawn_handle().spawn_blocking(
			"prometheus",
			"test-environment",
			async move {
				prometheus_endpoint::init_prometheus(
					SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::LOCALHOST), 9999),
					registry_clone,
				)
				.await
				.unwrap();
			},
		);

		TestEnvironment {
			runtime_handle: dependencies.runtime.handle().clone(),
			dependencies,
			overseer_handle,
			config,
			network,
			metrics,
		}
	}

	pub fn config(&self) -> &TestConfiguration {
		&self.config
	}

	pub fn network(&self) -> &NetworkEmulator {
		&self.network
	}

	pub fn registry(&self) -> &Registry {
		&self.dependencies.registry
	}

	pub fn metrics(&self) -> &TestEnvironmentMetrics {
		&self.metrics
	}

	pub fn runtime(&self) -> Handle {
		self.runtime_handle.clone()
	}

	// Send a message to the subsystem under test environment.
	pub async fn send_message(&mut self, msg: AllMessages) {
		self.overseer_handle
			.send_msg(msg, LOG_TARGET)
			.timeout(MAX_TIME_OF_FLIGHT)
			.await
			.unwrap_or_else(|| {
				panic!("{}ms maximum time of flight breached", MAX_TIME_OF_FLIGHT.as_millis())
			});
	}

	// Send an `ActiveLeavesUpdate` signal to all subsystems under test.
	pub async fn import_block(&mut self, block: BlockInfo) {
		self.overseer_handle
			.block_imported(block)
			.timeout(MAX_TIME_OF_FLIGHT)
			.await
			.unwrap_or_else(|| {
				panic!("{}ms maximum time of flight breached", MAX_TIME_OF_FLIGHT.as_millis())
			});
	}

	// Stop overseer and subsystems.
	pub async fn stop(&mut self) {
		self.overseer_handle.stop().await;
	}
}

impl Display for TestEnvironment {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let stats = self.network().stats();

		writeln!(f, "\n")?;
		writeln!(
			f,
			"Total received from network: {}",
			format!(
				"{} MiB",
				stats
					.iter()
					.enumerate()
					.map(|(_index, stats)| stats.tx_bytes_total as u128)
					.sum::<u128>() / (1024 * 1024)
			)
			.cyan()
		)?;
		writeln!(
			f,
			"Total sent to network: {}",
			format!("{} KiB", stats[0].tx_bytes_total / (1024)).cyan()
		)?;

		let test_metrics = super::display::parse_metrics(self.registry());
		let subsystem_cpu_metrics =
			test_metrics.subset_with_label_value("task_group", "availability-recovery");
		let total_cpu = subsystem_cpu_metrics.sum_by("substrate_tasks_polling_duration_sum");
		writeln!(f, "Total subsystem CPU usage {}", format!("{:.2}s", total_cpu).bright_purple())?;
		writeln!(
			f,
			"CPU usage per block {}",
			format!("{:.2}s", total_cpu / self.config().num_blocks as f64).bright_purple()
		)?;

		let test_env_cpu_metrics =
			test_metrics.subset_with_label_value("task_group", "test-environment");
		let total_cpu = test_env_cpu_metrics.sum_by("substrate_tasks_polling_duration_sum");
		writeln!(
			f,
			"Total test environment CPU usage {}",
			format!("{:.2}s", total_cpu).bright_purple()
		)?;
		writeln!(
			f,
			"CPU usage per block {}",
			format!("{:.2}s", total_cpu / self.config().num_blocks as f64).bright_purple()
		)
	}
}
