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
	core::{configuration::PeerLatency, mock::AlwaysSupportsParachains, network::NetworkEmulator},
	TestConfiguration,
};
use core::time::Duration;
use polkadot_node_subsystem::{Event, Overseer, OverseerHandle, SpawnGlue, TimeoutExt};
use polkadot_node_subsystem_types::Hash;
use polkadot_node_subsystem_util::metrics::prometheus::{
	self, Gauge, Histogram, PrometheusError, Registry, U64,
};
use rand::{
	distributions::{Distribution, Uniform},
	thread_rng,
};
use sc_service::{SpawnTaskHandle, TaskManager};
use std::net::{Ipv4Addr, SocketAddr};
use tokio::runtime::{Handle, Runtime};

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
	// A task manager that tracks task poll durations allows us to measure
	// per task CPU usage as we do in the Polkadot node.
	task_manager: TaskManager,
	// Our runtime
	runtime: tokio::runtime::Runtime,
	// A runtime handle
	runtime_handle: tokio::runtime::Handle,
	// The Prometheus metrics registry
	registry: Registry,
	// A handle to the lovely overseer
	overseer_handle: OverseerHandle,
	// The test intial state. The current state is owned by `env_task`.
	config: TestConfiguration,
	// A handle to the network emulator.
	network: NetworkEmulator,
	// Configuration/env metrics
	metrics: TestEnvironmentMetrics,
}

impl TestEnvironment {
	// Create a new test environment with specified initial state and prometheus registry.
	// We use prometheus metrics to collect per job task poll time and subsystem metrics.
	pub fn new(
		task_manager: TaskManager,
		config: TestConfiguration,
		registry: Registry,
		runtime: Runtime,
		network: NetworkEmulator,
		overseer: Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>,
		overseer_handle: OverseerHandle,
	) -> Self {
		let metrics =
			TestEnvironmentMetrics::new(&registry).expect("Metrics need to be registered");

		let spawn_handle = task_manager.spawn_handle();
		spawn_handle.spawn_blocking("overseer", "overseer", overseer.run());

		let registry_clone = registry.clone();
		task_manager
			.spawn_handle()
			.spawn_blocking("prometheus", "test-environment", async move {
				prometheus_endpoint::init_prometheus(
					SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::LOCALHOST), 9999),
					registry_clone,
				)
				.await
				.unwrap();
			});

		TestEnvironment {
			task_manager,
			runtime_handle: runtime.handle().clone(),
			runtime,
			registry,
			overseer_handle,
			config,
			network,
			metrics,
		}
	}

	pub fn config(&self) -> &TestConfiguration {
		&self.config
	}

	pub fn network(&mut self) -> &mut NetworkEmulator {
		&mut self.network
	}

	pub fn registry(&self) -> &Registry {
		&self.registry
	}

	/// Produce a randomized duration between `min` and `max`.
	fn random_latency(maybe_peer_latency: Option<&PeerLatency>) -> Option<Duration> {
		if let Some(peer_latency) = maybe_peer_latency {
			Some(
				Uniform::from(peer_latency.min_latency..=peer_latency.max_latency)
					.sample(&mut thread_rng()),
			)
		} else {
			None
		}
	}

	pub fn metrics(&self) -> &TestEnvironmentMetrics {
		&self.metrics
	}

	pub fn runtime(&self) -> Handle {
		self.runtime_handle.clone()
	}

	// Send a message to the subsystem under test environment.
	pub async fn send_message(&mut self, msg: Event) {
		self.overseer_handle
			.send(msg)
			.timeout(MAX_TIME_OF_FLIGHT)
			.await
			.unwrap_or_else(|| {
				panic!("{}ms maximum time of flight breached", MAX_TIME_OF_FLIGHT.as_millis())
			})
			.expect("send never fails");
	}
}
