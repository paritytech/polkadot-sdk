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

use colored::Colorize;

use futures::{
	channel::{mpsc, oneshot},
	stream::FuturesUnordered,
	FutureExt, SinkExt, StreamExt,
};
use polkadot_node_metrics::metrics::Metrics;

use polkadot_availability_recovery::AvailabilityRecoverySubsystem;

use parity_scale_codec::Encode;
use polkadot_node_network_protocol::request_response::{
	self as req_res, v1::ChunkResponse, IncomingRequest, ReqProtocolNames, Requests,
};
use rand::{distributions::Uniform, prelude::Distribution, thread_rng};

use prometheus::Registry;
use sc_network::{config::RequestResponseConfig, OutboundFailure, RequestFailure};

use polkadot_erasure_coding::{branches, obtain_chunks_v1 as obtain_chunks};
use polkadot_node_primitives::{BlockData, PoV, Proof};
use polkadot_node_subsystem::{
	messages::{
		AllMessages, AvailabilityRecoveryMessage, AvailabilityStoreMessage, NetworkBridgeTxMessage,
		RuntimeApiMessage, RuntimeApiRequest,
	},
	ActiveLeavesUpdate, FromOrchestra, OverseerSignal, Subsystem,
};
use std::net::{Ipv4Addr, SocketAddr};

use super::core::{keyring::Keyring, network::*, test_env::TestEnvironmentMetrics};

const LOG_TARGET: &str = "subsystem-bench::availability";

use polkadot_node_primitives::{AvailableData, ErasureChunk};

use polkadot_node_subsystem_test_helpers::{
	make_buffered_subsystem_context, mock::new_leaf, TestSubsystemContextHandle,
};
use polkadot_node_subsystem_util::TimeoutExt;
use polkadot_primitives::{
	AuthorityDiscoveryId, CandidateHash, CandidateReceipt, GroupIndex, Hash, HeadData, IndexedVec,
	PersistedValidationData, SessionIndex, SessionInfo, ValidatorId, ValidatorIndex,
};
use polkadot_primitives_test_helpers::{dummy_candidate_receipt, dummy_hash};
use sc_service::{SpawnTaskHandle, TaskManager};

pub mod configuration;

pub use configuration::{PeerLatency, TestConfiguration, TestSequence};

// Deterministic genesis hash for protocol names
const GENESIS_HASH: Hash = Hash::repeat_byte(0xff);

struct AvailabilityRecoverySubsystemInstance {
	_protocol_config: RequestResponseConfig,
}

/// The test environment is responsible for creating an instance of the availability recovery
/// subsystem and connecting it to an emulated overseer.
///
/// ## Mockups
/// We emulate the following subsystems:
/// - runtime api
/// - network bridge
/// - availability store
///
/// As the subsystem's performance depends on network connectivity, the test environment
/// emulates validator nodes on the network, see `NetworkEmulator`. The network emulation
/// is configurable in terms of peer bandwidth, latency and connection error rate using
/// uniform distribution sampling.
///
/// The mockup logic is implemented in `env_task` which owns and advances the `TestState`.
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
	// The Prometheus metrics registry
	registry: Registry,
	// A channel to the availability recovery subsystem
	to_subsystem: mpsc::Sender<FromOrchestra<AvailabilityRecoveryMessage>>,
	// Subsystem instance, currently keeps req/response protocol channel senders
	// for the whole duration of the test.
	instance: AvailabilityRecoverySubsystemInstance,
	// The test intial state. The current state is owned by `env_task`.
	state: TestState,
	// A handle to the network emulator.
	network: NetworkEmulator,
	// Configuration/env metrics
	metrics: TestEnvironmentMetrics,
}

impl TestEnvironment {
	// Create a new test environment with specified initial state and prometheus registry.
	// We use prometheus metrics to collect per job task poll time and subsystem metrics.
	pub fn new(runtime: tokio::runtime::Handle, state: TestState, registry: Registry) -> Self {
		let task_manager: TaskManager = TaskManager::new(runtime.clone(), Some(&registry)).unwrap();
		let (instance, virtual_overseer) = AvailabilityRecoverySubsystemInstance::new(
			&registry,
			task_manager.spawn_handle(),
			state.config().use_fast_path,
		);

		let metrics =
			TestEnvironmentMetrics::new(&registry).expect("Metrics need to be registered");
		let mut network = NetworkEmulator::new(
			state.config().n_validators,
			state.validator_authority_id.clone(),
			state.config().peer_bandwidth,
			task_manager.spawn_handle(),
			&registry,
		);

		// Copy sender for later when we need to inject messages in to the subsystem.
		let to_subsystem = virtual_overseer.tx.clone();

		let task_state = state.clone();
		let task_network = network.clone();
		let spawn_handle = task_manager.spawn_handle();

		// Our node rate limiting
		let mut rx_limiter = RateLimit::new(10, state.config.bandwidth);
		let (ingress_tx, mut ingress_rx) = tokio::sync::mpsc::unbounded_channel::<NetworkAction>();
		let our_network_stats = network.peer_stats(0);

		spawn_handle.spawn_blocking("our-node-rx", "test-environment", async move {
			while let Some(action) = ingress_rx.recv().await {
				let size = action.size();

				// account for our node receiving the data.
				our_network_stats.inc_received(size);

				rx_limiter.reap(size).await;
				action.run().await;
			}
		});

		// We need to start a receiver to process messages from the subsystem.
		// This mocks an overseer and all dependent subsystems
		task_manager.spawn_handle().spawn_blocking(
			"test-environment",
			"test-environment",
			async move { Self::env_task(virtual_overseer, task_state, task_network, ingress_tx).await },
		);

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

		TestEnvironment { task_manager, registry, to_subsystem, instance, state, network, metrics }
	}

	pub fn config(&self) -> &TestConfiguration {
		self.state.config()
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

	/// Generate a random error based on `probability`.
	/// `probability` should be a number between 0 and 100.
	fn random_error(probability: usize) -> bool {
		Uniform::from(0..=99).sample(&mut thread_rng()) < probability
	}

	pub fn request_size(request: &Requests) -> u64 {
		match request {
			Requests::ChunkFetchingV1(outgoing_request) =>
				outgoing_request.payload.encoded_size() as u64,
			Requests::AvailableDataFetchingV1(outgoing_request) =>
				outgoing_request.payload.encoded_size() as u64,
			_ => panic!("received an unexpected request"),
		}
	}

	pub fn respond_to_send_request(
		state: &mut TestState,
		request: Requests,
		ingress_tx: tokio::sync::mpsc::UnboundedSender<NetworkAction>,
	) -> NetworkAction {
		match request {
			Requests::ChunkFetchingV1(outgoing_request) => {
				let validator_index: usize = outgoing_request.payload.index.0 as usize;
				let candidate_hash = outgoing_request.payload.candidate_hash;

				let candidate_index = state
					.candidate_hashes
					.get(&candidate_hash)
					.expect("candidate was generated previously; qed");
				gum::warn!(target: LOG_TARGET, ?candidate_hash, candidate_index, "Candidate mapped to index");

				let chunk: ChunkResponse = state.chunks.get(*candidate_index as usize).unwrap()
					[validator_index]
					.clone()
					.into();
				let mut size = chunk.encoded_size();

				let response = if Self::random_error(state.config().error) {
					// Error will not account to any bandwidth used.
					size = 0;
					Err(RequestFailure::Network(OutboundFailure::ConnectionClosed))
				} else {
					Ok(req_res::v1::ChunkFetchingResponse::from(Some(chunk)).encode())
				};

				let authority_discovery_id = match outgoing_request.peer {
					req_res::Recipient::Authority(authority_discovery_id) => authority_discovery_id,
					_ => panic!("Peer recipient not supported yet"),
				};
				let authority_discovery_id_clone = authority_discovery_id.clone();

				let future = async move {
					let _ = outgoing_request.pending_response.send(response);
				}
				.boxed();

				let future_wrapper = async move {
					// Forward the response to the ingress channel of our node.
					// On receive side we apply our node receiving rate limit.
					let action =
						NetworkAction::new(authority_discovery_id_clone, future, size, None);
					ingress_tx.send(action).unwrap();
				}
				.boxed();

				NetworkAction::new(
					authority_discovery_id,
					future_wrapper,
					size,
					// Generate a random latency based on configuration.
					Self::random_latency(state.config().latency.as_ref()),
				)
			},
			Requests::AvailableDataFetchingV1(outgoing_request) => {
				let candidate_hash = outgoing_request.payload.candidate_hash;
				let candidate_index = state
					.candidate_hashes
					.get(&candidate_hash)
					.expect("candidate was generated previously; qed");
				gum::warn!(target: LOG_TARGET, ?candidate_hash, candidate_index, "Candidate mapped to index");

				let available_data =
					state.available_data.get(*candidate_index as usize).unwrap().clone();

				let size = available_data.encoded_size();

				let response = if Self::random_error(state.config().error) {
					Err(RequestFailure::Network(OutboundFailure::ConnectionClosed))
				} else {
					Ok(req_res::v1::AvailableDataFetchingResponse::from(Some(available_data))
						.encode())
				};

				let future = async move {
					let _ = outgoing_request.pending_response.send(response);
				}
				.boxed();

				let authority_discovery_id = match outgoing_request.peer {
					req_res::Recipient::Authority(authority_discovery_id) => authority_discovery_id,
					_ => panic!("Peer recipient not supported yet"),
				};
				let authority_discovery_id_clone = authority_discovery_id.clone();

				let future_wrapper = async move {
					// Forward the response to the ingress channel of our node.
					// On receive side we apply our node receiving rate limit.
					let action =
						NetworkAction::new(authority_discovery_id_clone, future, size, None);
					ingress_tx.send(action).unwrap();
				}
				.boxed();

				NetworkAction::new(
					authority_discovery_id,
					future_wrapper,
					size,
					// Generate a random latency based on configuration.
					Self::random_latency(state.config().latency.as_ref()),
				)
			},
			_ => panic!("received an unexpected request"),
		}
	}

	// A task that mocks dependent subsystems based on environment configuration.
	// TODO: Spawn real subsystems, user overseer builder.
	async fn env_task(
		mut ctx: TestSubsystemContextHandle<AvailabilityRecoveryMessage>,
		mut state: TestState,
		mut network: NetworkEmulator,
		ingress_tx: tokio::sync::mpsc::UnboundedSender<NetworkAction>,
	) {
		loop {
			futures::select! {
				maybe_message = ctx.maybe_recv().fuse() => {
					let message = if let Some(message) = maybe_message{
						message
					} else {
						gum::info!("{}", "Test completed".bright_blue());
						return
					};

					gum::trace!(target: LOG_TARGET, ?message, "Env task received message");

					match message {
						AllMessages::NetworkBridgeTx(
							NetworkBridgeTxMessage::SendRequests(
								requests,
								_if_disconnected,
							)
						) => {
							for request in requests {
								network.inc_sent(Self::request_size(&request));
								let action = Self::respond_to_send_request(&mut state, request, ingress_tx.clone());
								// Account for our node sending the request over the emulated network.
								network.submit_peer_action(action.peer(), action);
							}
						},
						AllMessages::AvailabilityStore(AvailabilityStoreMessage::QueryAvailableData(_candidate_hash, tx)) => {
							// TODO: Simulate av store load by delaying the response.
							state.respond_none_to_available_data_query(tx).await;
						},
						AllMessages::AvailabilityStore(AvailabilityStoreMessage::QueryAllChunks(candidate_hash, tx)) => {
							// Test env: We always have our own chunk.
							state.respond_to_query_all_request(candidate_hash, |index| index == state.validator_index.0 as usize, tx).await;
						},
						AllMessages::AvailabilityStore(
							AvailabilityStoreMessage::QueryChunkSize(candidate_hash, tx)
						) => {
							let candidate_index = state.candidate_hashes.get(&candidate_hash).expect("candidate was generated previously; qed");
							gum::debug!(target: LOG_TARGET, ?candidate_hash, candidate_index, "Candidate mapped to index");

							let chunk_size = state.chunks.get(*candidate_index as usize).unwrap()[0].encoded_size();
							let _ = tx.send(Some(chunk_size));
						}
						AllMessages::RuntimeApi(RuntimeApiMessage::Request(
							_relay_parent,
							RuntimeApiRequest::SessionInfo(
								_session_index,
								tx,
							)
						)) => {
							tx.send(Ok(Some(state.session_info()))).unwrap();
						}
						_ => panic!("Unexpected input")
					}
				}
			}
		}
	}

	// Send a message to the subsystem under test environment.
	pub async fn send_message(&mut self, msg: AvailabilityRecoveryMessage) {
		gum::trace!(msg = ?msg, "sending message");
		self.to_subsystem
			.send(FromOrchestra::Communication { msg })
			.timeout(MAX_TIME_OF_FLIGHT)
			.await
			.unwrap_or_else(|| {
				panic!("{}ms maximum time of flight breached", MAX_TIME_OF_FLIGHT.as_millis())
			})
			.unwrap();
	}

	// Send a signal to the subsystem under test environment.
	pub async fn send_signal(&mut self, signal: OverseerSignal) {
		self.to_subsystem
			.send(FromOrchestra::Signal(signal))
			.timeout(MAX_TIME_OF_FLIGHT)
			.await
			.unwrap_or_else(|| {
				panic!(
					"{}ms is more than enough for sending signals.",
					MAX_TIME_OF_FLIGHT.as_millis()
				)
			})
			.unwrap();
	}
}

/// Implementation for chunks only
/// TODO: all recovery methods.
impl AvailabilityRecoverySubsystemInstance {
	pub fn new(
		registry: &Registry,
		spawn_task_handle: SpawnTaskHandle,
		use_fast_path: bool,
	) -> (Self, TestSubsystemContextHandle<AvailabilityRecoveryMessage>) {
		let (context, virtual_overseer) = make_buffered_subsystem_context(
			spawn_task_handle.clone(),
			128,
			"availability-recovery-subsystem",
		);
		let (collation_req_receiver, req_cfg) =
			IncomingRequest::get_config_receiver(&ReqProtocolNames::new(&GENESIS_HASH, None));

		let subsystem = if use_fast_path {
			AvailabilityRecoverySubsystem::with_fast_path(
				collation_req_receiver,
				Metrics::try_register(&registry).unwrap(),
			)
		} else {
			AvailabilityRecoverySubsystem::with_chunks_only(
				collation_req_receiver,
				Metrics::try_register(&registry).unwrap(),
			)
		};

		let spawned_subsystem = subsystem.start(context);
		let subsystem_future = async move {
			spawned_subsystem.future.await.unwrap();
		};

		spawn_task_handle.spawn_blocking(
			spawned_subsystem.name,
			spawned_subsystem.name,
			subsystem_future,
		);

		(Self { _protocol_config: req_cfg }, virtual_overseer)
	}
}

pub fn random_pov_size(min_pov_size: usize, max_pov_size: usize) -> usize {
	random_uniform_sample(min_pov_size, max_pov_size)
}

fn random_uniform_sample<T: Into<usize> + From<usize>>(min_value: T, max_value: T) -> T {
	Uniform::from(min_value.into()..=max_value.into())
		.sample(&mut thread_rng())
		.into()
}

// We use this to bail out sending messages to the subsystem if it is overloaded such that
// the time of flight is breaches 5s.
// This should eventually be a test parameter.
const MAX_TIME_OF_FLIGHT: Duration = Duration::from_millis(5000);

#[derive(Clone)]
pub struct TestState {
	validator_public: Vec<ValidatorId>,
	validator_authority_id: Vec<AuthorityDiscoveryId>,
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
	/// Next candidate index in
	config: TestConfiguration,
}

impl TestState {
	fn config(&self) -> &TestConfiguration {
		&self.config
	}

	async fn respond_none_to_available_data_query(
		&self,
		tx: oneshot::Sender<Option<AvailableData>>,
	) {
		let _ = tx.send(None);
	}

	fn session_info(&self) -> SessionInfo {
		let my_vec = (0..self.config().n_validators)
			.map(|i| ValidatorIndex(i as _))
			.collect::<Vec<_>>();

		let validator_groups = my_vec.chunks(5).map(|x| Vec::from(x)).collect::<Vec<_>>();

		SessionInfo {
			validators: self.validator_public.clone().into(),
			discovery_keys: self.validator_authority_id.clone(),
			validator_groups: IndexedVec::<GroupIndex, Vec<ValidatorIndex>>::from(validator_groups),
			assignment_keys: vec![],
			n_cores: self.config().n_cores as u32,
			zeroth_delay_tranche_width: 0,
			relay_vrf_modulo_samples: 0,
			n_delay_tranches: 0,
			no_show_slots: 0,
			needed_approvals: 0,
			active_validator_indices: vec![],
			dispute_period: 6,
			random_seed: [0u8; 32],
		}
	}
	async fn respond_to_query_all_request(
		&self,
		candidate_hash: CandidateHash,
		send_chunk: impl Fn(usize) -> bool,
		tx: oneshot::Sender<Vec<ErasureChunk>>,
	) {
		let candidate_index = self
			.candidate_hashes
			.get(&candidate_hash)
			.expect("candidate was generated previously; qed");
		gum::debug!(target: LOG_TARGET, ?candidate_hash, candidate_index, "Candidate mapped to index");

		let v = self
			.chunks
			.get(*candidate_index as usize)
			.unwrap()
			.iter()
			.filter(|c| send_chunk(c.index.0 as usize))
			.cloned()
			.collect();

		let _ = tx.send(v);
	}

	pub fn next_candidate(&mut self) -> Option<CandidateReceipt> {
		let candidate = self.candidates.next();
		let candidate_hash = candidate.as_ref().unwrap().hash();
		gum::trace!(target: LOG_TARGET, "Next candidate selected {:?}", candidate_hash);
		candidate
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

	pub fn new(config: TestConfiguration) -> Self {
		let keyrings = (0..config.n_validators)
			.map(|peer_index| Keyring::new(format!("Node{}", peer_index).into()))
			.collect::<Vec<_>>();

		// Generate `AuthorityDiscoveryId`` for each peer
		let validator_public: Vec<ValidatorId> = keyrings
			.iter()
			.map(|keyring: &Keyring| keyring.clone().public().into())
			.collect::<Vec<_>>();

		let validator_authority_id: Vec<AuthorityDiscoveryId> = keyrings
			.iter()
			.map(|keyring| keyring.clone().public().into())
			.collect::<Vec<_>>()
			.into();

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
			validator_public,
			validator_authority_id,
			validator_index,
			session_index,
			persisted_validation_data,
			available_data,
			candidate_receipts,
			chunks,
			config,
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

pub async fn bench_chunk_recovery(env: &mut TestEnvironment) {
	let config = env.config().clone();

	env.send_signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
		Hash::repeat_byte(1),
		1,
	))))
	.await;

	let start_marker = Instant::now();
	let mut batch = FuturesUnordered::new();
	let mut availability_bytes = 0u128;

	env.metrics().set_n_validators(config.n_validators);
	env.metrics().set_n_cores(config.n_cores);

	for block_num in 0..env.config().num_blocks {
		gum::info!(target: LOG_TARGET, "Current block {}/{}", block_num, env.config().num_blocks);
		env.metrics().set_current_block(block_num);

		let block_start_ts = Instant::now();
		for candidate_num in 0..config.n_cores as u64 {
			let candidate =
				env.state.next_candidate().expect("We always send up to n_cores*num_blocks; qed");
			let (tx, rx) = oneshot::channel();
			batch.push(rx);

			env.send_message(AvailabilityRecoveryMessage::RecoverAvailableData(
				candidate.clone(),
				1,
				Some(GroupIndex(
					candidate_num as u32 % (std::cmp::max(5, config.n_cores) / 5) as u32,
				)),
				tx,
			))
			.await;
		}

		gum::info!("{}", format!("{} requests pending", batch.len()).bright_black());
		while let Some(completed) = batch.next().await {
			let available_data = completed.unwrap().unwrap();
			env.metrics().on_pov_size(available_data.encoded_size());
			availability_bytes += available_data.encoded_size() as u128;
		}

		let block_time_delta =
			Duration::from_secs(6).saturating_sub(Instant::now().sub(block_start_ts));
		gum::info!(target: LOG_TARGET,"{}", format!("Sleeping till end of block ({}ms)", block_time_delta.as_millis()).bright_black());
		tokio::time::sleep(block_time_delta).await;
	}

	env.send_signal(OverseerSignal::Conclude).await;
	let duration = start_marker.elapsed().as_millis();
	let availability_bytes = availability_bytes / 1024;
	gum::info!("Benchmark completed in {}", format!("{:?}ms", duration).cyan());
	gum::info!(
		"Throughput: {}",
		format!("{} KiB/block", availability_bytes / env.config().num_blocks as u128).bright_red()
	);
	gum::info!(
		"Block time: {}",
		format!("{} ms", start_marker.elapsed().as_millis() / env.config().num_blocks as u128).red()
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
	gum::info!(target: LOG_TARGET, "Total subsystem CPU usage {}", format!("{:.2}s", subsystem_cpu_metrics.sum_by("substrate_tasks_polling_duration_sum")).bright_purple());

	let test_env_cpu_metrics =
		test_metrics.subset_with_label_value("task_group", "test-environment");
	gum::info!(target: LOG_TARGET, "Total test environment CPU usage {}", format!("{:.2}s", test_env_cpu_metrics.sum_by("substrate_tasks_polling_duration_sum")).bright_purple());
}
