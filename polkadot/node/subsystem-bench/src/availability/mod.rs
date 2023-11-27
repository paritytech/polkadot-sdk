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
use itertools::Itertools;
use std::{
	collections::HashMap,
	iter::Cycle,
	ops::Sub,
	sync::Arc,
	time::{Duration, Instant},
};
use tokio::runtime::{Handle, Runtime};

use polkadot_node_subsystem::{
	BlockInfo, Event, Overseer, OverseerConnector, OverseerHandle, SpawnGlue,
};
use sc_network::request_responses::ProtocolConfig;

use colored::Colorize;

use futures::{channel::oneshot, stream::FuturesUnordered, FutureExt, SinkExt, StreamExt};
use polkadot_node_metrics::metrics::Metrics;

use polkadot_availability_recovery::AvailabilityRecoverySubsystem;

use parity_scale_codec::Encode;
use polkadot_node_network_protocol::request_response::{
	self as req_res, v1::ChunkResponse, IncomingRequest, ReqProtocolNames, Requests,
};
use rand::{distributions::Uniform, prelude::Distribution, thread_rng};

use prometheus::Registry;
use sc_network::{OutboundFailure, RequestFailure};

use polkadot_erasure_coding::{branches, obtain_chunks_v1 as obtain_chunks};
use polkadot_node_primitives::{BlockData, PoV, Proof};
use polkadot_node_subsystem::{
	messages::{AllMessages, AvailabilityRecoveryMessage},
	ActiveLeavesUpdate, OverseerSignal,
};
use std::net::{Ipv4Addr, SocketAddr};

use crate::core::{
	configuration::TestAuthorities,
	environment::TestEnvironmentDependencies,
	mock::{
		av_store,
		network_bridge::{self, MockNetworkBridgeTx, NetworkAvailabilityState},
		runtime_api, MockAvailabilityStore, MockRuntimeApi,
	},
};

use super::core::{
	configuration::{PeerLatency, TestConfiguration},
	environment::TestEnvironmentMetrics,
	mock::dummy_builder,
	network::*,
};

const LOG_TARGET: &str = "subsystem-bench::availability";

use polkadot_node_primitives::{AvailableData, ErasureChunk};

use super::{cli::TestObjective, core::mock::AlwaysSupportsParachains};
use polkadot_node_subsystem_test_helpers::mock::new_block_import_event;
use polkadot_node_subsystem_util::TimeoutExt;
use polkadot_primitives::{
	CandidateHash, CandidateReceipt, GroupIndex, Hash, HeadData, PersistedValidationData,
	SessionIndex, ValidatorIndex,
};
use polkadot_primitives_test_helpers::{dummy_candidate_receipt, dummy_hash};
use sc_service::{SpawnTaskHandle, TaskManager};

mod cli;
pub mod configuration;
pub use cli::{DataAvailabilityReadOptions, NetworkEmulation, NetworkOptions};
pub use configuration::AvailabilityRecoveryConfiguration;

// A dummy genesis hash
const GENESIS_HASH: Hash = Hash::repeat_byte(0xff);

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

fn build_overseer(
	spawn_task_handle: SpawnTaskHandle,
	runtime_api: MockRuntimeApi,
	av_store: MockAvailabilityStore,
	network_bridge: MockNetworkBridgeTx,
	availability_recovery: AvailabilityRecoverySubsystem,
) -> (Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>, OverseerHandle) {
	let overseer_connector = OverseerConnector::with_event_capacity(64000);
	let dummy = dummy_builder!(spawn_task_handle);
	let builder = dummy
		.replace_runtime_api(|_| runtime_api)
		.replace_availability_store(|_| av_store)
		.replace_network_bridge_tx(|_| network_bridge)
		.replace_availability_recovery(|_| availability_recovery);

	builder.build_with_connector(overseer_connector).expect("Should not fail")
}

/// Takes a test configuration and uses it to creates the `TestEnvironment`.
pub fn prepare_test(
	config: TestConfiguration,
	state: &mut TestState,
) -> (TestEnvironment, ProtocolConfig) {
	prepare_test_inner(config, state, TestEnvironmentDependencies::default())
}

/// Takes a test configuration and uses it to creates the `TestEnvironment`.
pub fn prepare_test_with_dependencies(
	config: TestConfiguration,
	state: &mut TestState,
	dependencies: TestEnvironmentDependencies,
) -> (TestEnvironment, ProtocolConfig) {
	prepare_test_inner(config, state, dependencies)
}

fn prepare_test_inner(
	config: TestConfiguration,
	state: &mut TestState,
	dependencies: TestEnvironmentDependencies,
) -> (TestEnvironment, ProtocolConfig) {
	// We need to first create the high level test state object.
	// This will then be decomposed into per subsystem states.
	let candidate_count = config.n_cores * config.num_blocks;
	state.generate_candidates(candidate_count);

	// Generate test authorities.
	let test_authorities = config.generate_authorities();

	let runtime_api = runtime_api::MockRuntimeApi::new(
		config.clone(),
		test_authorities.validator_public.clone(),
		test_authorities.validator_authority_id.clone(),
	);

	let av_store =
		av_store::MockAvailabilityStore::new(state.chunks.clone(), state.candidate_hashes.clone());

	let availability_state = NetworkAvailabilityState {
		candidate_hashes: state.candidate_hashes.clone(),
		available_data: state.available_data.clone(),
		chunks: state.chunks.clone(),
	};

	let network = NetworkEmulator::new(
		config.n_validators.clone(),
		test_authorities.validator_authority_id.clone(),
		config.peer_bandwidth,
		dependencies.task_manager.spawn_handle(),
		&dependencies.registry,
	);

	let network_bridge_tx = network_bridge::MockNetworkBridgeTx::new(
		config.clone(),
		availability_state,
		network.clone(),
	);

	let use_fast_path = match &state.config().objective {
		TestObjective::DataAvailabilityRead(options) => options.fetch_from_backers,
		_ => panic!("Unexpected objective"),
	};

	let (collation_req_receiver, req_cfg) =
		IncomingRequest::get_config_receiver(&ReqProtocolNames::new(&GENESIS_HASH, None));

	let subsystem = if use_fast_path {
		AvailabilityRecoverySubsystem::with_fast_path(
			collation_req_receiver,
			Metrics::try_register(&dependencies.registry).unwrap(),
		)
	} else {
		AvailabilityRecoverySubsystem::with_chunks_only(
			collation_req_receiver,
			Metrics::try_register(&dependencies.registry).unwrap(),
		)
	};

	let (overseer, overseer_handle) = build_overseer(
		dependencies.task_manager.spawn_handle(),
		runtime_api,
		av_store,
		network_bridge_tx,
		subsystem,
	);

	(
		TestEnvironment::new(
			dependencies.task_manager,
			config,
			dependencies.registry,
			dependencies.runtime,
			network,
			overseer,
			overseer_handle,
		),
		req_cfg,
	)
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

// We use this to bail out sending messages to the subsystem if it is overloaded such that
// the time of flight is breaches 5s.
// This should eventually be a test parameter.
const MAX_TIME_OF_FLIGHT: Duration = Duration::from_millis(5000);

#[derive(Clone)]
pub struct TestState {
	// Full test configuration
	config: TestConfiguration,
	// State starts here.
	test_authorities: TestAuthorities,
	// The test node validator index.
	validator_index: ValidatorIndex,
	session_index: SessionIndex,
	pov_sizes: Cycle<std::vec::IntoIter<usize>>,
	// Generated candidate receipts to be used in the test
	candidates: Cycle<std::vec::IntoIter<CandidateReceipt>>,
	candidates_generated: usize,
	// Map from pov size to candidate index
	pov_size_to_candidate: HashMap<usize, usize>,
	// Map from generated candidate hashes to candidate index in `available_data`
	// and `chunks`.
	candidate_hashes: HashMap<CandidateHash, usize>,
	persisted_validation_data: PersistedValidationData,

	candidate_receipts: Vec<CandidateReceipt>,
	available_data: Vec<AvailableData>,
	chunks: Vec<Vec<ErasureChunk>>,
}

impl TestState {
	fn config(&self) -> &TestConfiguration {
		&self.config
	}

	pub fn next_candidate(&mut self) -> Option<CandidateReceipt> {
		let candidate = self.candidates.next();
		let candidate_hash = candidate.as_ref().unwrap().hash();
		gum::trace!(target: LOG_TARGET, "Next candidate selected {:?}", candidate_hash);
		candidate
	}

	pub fn authorities(&self) -> &TestAuthorities {
		&self.test_authorities
	}

	/// Generate candidates to be used in the test.
	pub fn generate_candidates(&mut self, count: usize) {
		gum::info!(target: LOG_TARGET,"{}", format!("Pre-generating {} candidates.", count).bright_blue());

		// Generate all candidates
		self.candidates = (0..count)
			.map(|index| {
				let pov_size = self.pov_sizes.next().expect("This is a cycle; qed");
				let candidate_index = *self
					.pov_size_to_candidate
					.get(&pov_size)
					.expect("pov_size always exists; qed");
				let mut candidate_receipt = self.candidate_receipts[candidate_index].clone();

				// Make it unique.
				candidate_receipt.descriptor.relay_parent = Hash::from_low_u64_be(index as u64);
				// Store the new candidate in the state
				self.candidate_hashes.insert(candidate_receipt.hash(), candidate_index);

				gum::debug!(target: LOG_TARGET, candidate_hash = ?candidate_receipt.hash(), "new candidate");

				candidate_receipt
			})
			.collect::<Vec<_>>()
			.into_iter()
			.cycle();
	}

	pub fn new(config: &TestConfiguration) -> Self {
		let config = config.clone();
		let test_authorities = config.generate_authorities();

		let validator_index = ValidatorIndex(0);
		let mut chunks = Vec::new();
		let mut available_data = Vec::new();
		let mut candidate_receipts = Vec::new();
		let mut pov_size_to_candidate = HashMap::new();
		let session_index = 10;

		// we use it for all candidates.
		let persisted_validation_data = PersistedValidationData {
			parent_head: HeadData(vec![7, 8, 9]),
			relay_parent_number: Default::default(),
			max_pov_size: 1024,
			relay_parent_storage_root: Default::default(),
		};

		// For each unique pov we create a candidate receipt.
		for (index, pov_size) in config.pov_sizes().iter().cloned().unique().enumerate() {
			gum::info!(target: LOG_TARGET, index, pov_size, "{}", "Generating template candidate".bright_blue());

			let mut candidate_receipt = dummy_candidate_receipt(dummy_hash());
			let pov = PoV { block_data: BlockData(vec![index as u8; pov_size]) };

			let new_available_data = AvailableData {
				validation_data: persisted_validation_data.clone(),
				pov: Arc::new(pov),
			};

			let (new_chunks, erasure_root) = derive_erasure_chunks_with_proofs_and_root(
				config.n_validators,
				&new_available_data,
				|_, _| {},
			);

			candidate_receipt.descriptor.erasure_root = erasure_root;

			chunks.push(new_chunks);
			available_data.push(new_available_data);
			pov_size_to_candidate.insert(pov_size, index);
			candidate_receipts.push(candidate_receipt);
		}

		let pov_sizes = config.pov_sizes().to_vec().into_iter().cycle();
		gum::info!(target: LOG_TARGET, "{}","Created test environment.".bright_blue());

		Self {
			config,
			test_authorities,
			validator_index,
			session_index,
			persisted_validation_data,
			available_data,
			candidate_receipts,
			chunks,
			pov_size_to_candidate,
			pov_sizes,
			candidates_generated: 0,
			candidate_hashes: HashMap::new(),
			candidates: Vec::new().into_iter().cycle(),
		}
	}
}

fn derive_erasure_chunks_with_proofs_and_root(
	n_validators: usize,
	available_data: &AvailableData,
	alter_chunk: impl Fn(usize, &mut Vec<u8>),
) -> (Vec<ErasureChunk>, Hash) {
	let mut chunks: Vec<Vec<u8>> = obtain_chunks(n_validators, available_data).unwrap();

	for (i, chunk) in chunks.iter_mut().enumerate() {
		alter_chunk(i, chunk)
	}

	// create proofs for each erasure chunk
	let branches = branches(chunks.as_ref());

	let root = branches.root();
	let erasure_chunks = branches
		.enumerate()
		.map(|(index, (proof, chunk))| ErasureChunk {
			chunk: chunk.to_vec(),
			index: ValidatorIndex(index as _),
			proof: Proof::try_from(proof).unwrap(),
		})
		.collect::<Vec<ErasureChunk>>();

	(erasure_chunks, root)
}

pub async fn bench_chunk_recovery(env: &mut TestEnvironment, mut state: TestState) {
	let config = env.config().clone();

	env.send_message(new_block_import_event(Hash::repeat_byte(1), 1)).await;

	let start_marker = Instant::now();
	let mut batch = FuturesUnordered::new();
	let mut availability_bytes = 0u128;

	env.metrics().set_n_validators(config.n_validators);
	env.metrics().set_n_cores(config.n_cores);

	for block_num in 0..env.config().num_blocks {
		gum::info!(target: LOG_TARGET, "Current block {}/{}", block_num + 1, env.config().num_blocks);
		env.metrics().set_current_block(block_num);

		let block_start_ts = Instant::now();
		for candidate_num in 0..config.n_cores as u64 {
			let candidate =
				state.next_candidate().expect("We always send up to n_cores*num_blocks; qed");
			let (tx, rx) = oneshot::channel();
			batch.push(rx);

			let message = Event::MsgToSubsystem {
				msg: AllMessages::AvailabilityRecovery(
					AvailabilityRecoveryMessage::RecoverAvailableData(
						candidate.clone(),
						1,
						Some(GroupIndex(
							candidate_num as u32 % (std::cmp::max(5, config.n_cores) / 5) as u32,
						)),
						tx,
					),
				),
				origin: LOG_TARGET,
			};
			env.send_message(message).await;
		}

		gum::info!("{}", format!("{} recoveries pending", batch.len()).bright_black());
		while let Some(completed) = batch.next().await {
			let available_data = completed.unwrap().unwrap();
			env.metrics().on_pov_size(available_data.encoded_size());
			availability_bytes += available_data.encoded_size() as u128;
		}

		let block_time_delta =
			Duration::from_secs(6).saturating_sub(Instant::now().sub(block_start_ts));

		let block_time = Instant::now().sub(block_start_ts).as_millis() as u64;
		env.metrics().set_block_time(block_time);
		gum::info!("Block time {}", format!("{:?}ms", block_time).cyan());
		gum::info!(target: LOG_TARGET,"{}", format!("Sleeping till end of block ({}ms)", block_time_delta.as_millis()).bright_black());
		tokio::time::sleep(block_time_delta).await;
	}

	env.send_message(Event::Stop).await;
	let duration: u128 = start_marker.elapsed().as_millis();
	let availability_bytes = availability_bytes / 1024;
	gum::info!("All blocks processed in {}", format!("{:?}ms", duration).cyan());
	gum::info!(
		"Throughput: {}",
		format!("{} KiB/block", availability_bytes / env.config().num_blocks as u128).bright_red()
	);
	gum::info!(
		"Block time: {}",
		format!("{} ms", start_marker.elapsed().as_millis() / env.config().num_blocks as u128)
			.red()
	);

	let stats = env.network().stats();
	gum::info!(
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
	);

	let test_metrics = super::core::display::parse_metrics(&env.registry());
	let subsystem_cpu_metrics =
		test_metrics.subset_with_label_value("task_group", "availability-recovery-subsystem");
	let total_cpu = subsystem_cpu_metrics.sum_by("substrate_tasks_polling_duration_sum");
	gum::info!(target: LOG_TARGET, "Total subsystem CPU usage {}", format!("{:.2}s", total_cpu).bright_purple());
	gum::info!(target: LOG_TARGET, "CPU usage per block {}", format!("{:.2}s", total_cpu/env.config().num_blocks as f64).bright_purple());

	let test_env_cpu_metrics =
		test_metrics.subset_with_label_value("task_group", "test-environment");
	let total_cpu = test_env_cpu_metrics.sum_by("substrate_tasks_polling_duration_sum");
	gum::info!(target: LOG_TARGET, "Total test environment CPU usage {}", format!("{:.2}s", total_cpu).bright_purple());
	gum::info!(target: LOG_TARGET, "CPU usage per block {}", format!("{:.2}s", total_cpu/env.config().num_blocks as f64).bright_purple());
}
