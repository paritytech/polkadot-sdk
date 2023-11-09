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

use std::{
	collections::HashMap,
	sync::Arc,
	time::{Duration, Instant},
};

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
use rand::{distributions::Uniform, prelude::Distribution, seq::IteratorRandom, thread_rng};

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

mod configuration;
mod network;

pub use configuration::TestConfiguration;

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

		// Copy sender for later when we need to inject messages in to the subsystem.
		let to_subsystem = virtual_overseer.tx.clone();

		let task_state = state.clone();
		let spawn_task_handle = task_manager.spawn_handle();
		// We need to start a receiver to process messages from the subsystem.
		// This mocks an overseer and all dependent subsystems
		task_manager.spawn_handle().spawn_blocking(
			"test-environment",
			"test-environment",
			async move { Self::env_task(virtual_overseer, task_state, spawn_task_handle).await },
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

		TestEnvironment { task_manager, registry, to_subsystem, instance, state }
	}

	pub fn config(&self) -> &TestConfiguration {
		self.state.config()
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

	/// Generate a random error based on `probability`.
	/// `probability` should be a number between 0 and 100.
	fn random_error(probability: usize) -> bool {
		Uniform::from(0..=99).sample(&mut thread_rng()) < probability
	}

	pub fn respond_to_send_request(state: &mut TestState, request: Requests) -> NetworkAction {
		match request {
			Requests::ChunkFetchingV1(outgoing_request) => {
				let validator_index = outgoing_request.payload.index.0 as usize;
				let chunk: ChunkResponse =
					state.chunks.get(&outgoing_request.payload.candidate_hash).unwrap()
						[validator_index]
						.clone()
						.into();
				let size = chunk.encoded_size();

				let response = if Self::random_error(state.config().error) {
					Err(RequestFailure::Network(OutboundFailure::ConnectionClosed))
				} else {
					Ok(req_res::v1::ChunkFetchingResponse::from(Some(chunk)).encode())
				};

				let future = async move {
					let _ = outgoing_request.pending_response.send(response);
				}
				.boxed();

				NetworkAction::new(
					validator_index,
					future,
					size,
					// Generate a random latency based on configuration.
					Self::random_latency(state.config().latency.as_ref()),
				)
			},
			Requests::AvailableDataFetchingV1(outgoing_request) => {
				// TODO: do better, by implementing diff authority ids and mapping network actions
				// to authority id,
				let validator_index =
					Uniform::from(0..state.config().n_validators).sample(&mut thread_rng());
				let available_data =
					state.candidates.get(&outgoing_request.payload.candidate_hash).unwrap().clone();
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

				NetworkAction::new(
					validator_index,
					future,
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
		spawn_task_handle: SpawnTaskHandle,
	) {
		// Emulate `n_validators` each with 1MiB of bandwidth available.
		let mut network = NetworkEmulator::new(
			state.config().n_validators,
			state.config().bandwidth,
			spawn_task_handle,
		);

		loop {
			futures::select! {
				message = ctx.recv().fuse() => {
					gum::debug!(target: LOG_TARGET, ?message, "Env task received message");

					match message {
						AllMessages::NetworkBridgeTx(
							NetworkBridgeTxMessage::SendRequests(
								requests,
								_if_disconnected,
							)
						) => {
							for request in requests {
								let action = Self::respond_to_send_request(&mut state, request);
								network.submit_peer_action(action.index(), action);
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
							let chunk_size = state.chunks.get(&candidate_hash).unwrap()[0].encoded_size();
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
			4096 * 4,
			"availability-recovery",
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

const TIMEOUT: Duration = Duration::from_millis(300);

// We use this to bail out sending messages to the subsystem if it is overloaded such that
// the time of flight is breaches 5s.
// This should eventually be a test parameter.
const MAX_TIME_OF_FLIGHT: Duration = Duration::from_millis(5000);

use sp_keyring::Sr25519Keyring;

use crate::availability::network::NetworkAction;

use self::{configuration::PeerLatency, network::NetworkEmulator};

#[derive(Clone)]
pub struct TestState {
	validators: Vec<Sr25519Keyring>,
	validator_public: IndexedVec<ValidatorIndex, ValidatorId>,
	validator_authority_id: Vec<AuthorityDiscoveryId>,
	// The test node validator index.
	validator_index: ValidatorIndex,
	// Per core candidates receipts.
	candidate_receipts: Vec<CandidateReceipt>,
	session_index: SessionIndex,

	persisted_validation_data: PersistedValidationData,
	/// A per size pov mapping to available data.
	candidates: HashMap<CandidateHash, AvailableData>,

	chunks: HashMap<CandidateHash, Vec<ErasureChunk>>,
	config: TestConfiguration,
}

impl TestState {
	fn config(&self) -> &TestConfiguration {
		&self.config
	}

	fn candidate(&self, candidate_index: usize) -> CandidateReceipt {
		self.candidate_receipts.get(candidate_index).unwrap().clone()
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
			validators: self.validator_public.clone(),
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
		let v = self
			.chunks
			.get(&candidate_hash)
			.unwrap()
			.iter()
			.filter(|c| send_chunk(c.index.0 as usize))
			.cloned()
			.collect();

		let _ = tx.send(v);
	}

	pub fn new(config: TestConfiguration) -> Self {
		let validators = (0..config.n_validators as u64)
			.into_iter()
			.map(|_v| Sr25519Keyring::Alice)
			.collect::<Vec<_>>();

		let validator_public = validator_pubkeys(&validators);
		let validator_authority_id = validator_authority_id(&validators);
		let validator_index = ValidatorIndex(0);
		let mut pov_size_to_candidate = HashMap::new();
		let mut chunks = HashMap::new();
		let mut candidates = HashMap::new();
		let session_index = 10;

		// we use it for all candidates.
		let persisted_validation_data = PersistedValidationData {
			parent_head: HeadData(vec![7, 8, 9]),
			relay_parent_number: Default::default(),
			max_pov_size: 1024,
			relay_parent_storage_root: Default::default(),
		};

		// Create initial candidate receipts
		let mut candidate_receipts = config
			.pov_sizes
			.iter()
			.map(|_index| dummy_candidate_receipt(dummy_hash()))
			.collect::<Vec<_>>();

		for (index, pov_size) in config.pov_sizes.iter().enumerate() {
			let candidate = &mut candidate_receipts[index];
			// a hack to make candidate unique.
			candidate.descriptor.relay_parent = Hash::from_low_u64_be(index as u64);

			// We reuse candidates of same size, to speed up the test startup.
			let (erasure_root, available_data, new_chunks) =
				pov_size_to_candidate.entry(pov_size).or_insert_with(|| {
					let pov = PoV { block_data: BlockData(vec![index as u8; *pov_size]) };

					let available_data = AvailableData {
						validation_data: persisted_validation_data.clone(),
						pov: Arc::new(pov),
					};

					let (new_chunks, erasure_root) = derive_erasure_chunks_with_proofs_and_root(
						validators.len(),
						&available_data,
						|_, _| {},
					);

					candidate.descriptor.erasure_root = erasure_root;

					chunks.insert(candidate.hash(), new_chunks.clone());
					candidates.insert(candidate.hash(), available_data.clone());

					(erasure_root, available_data, new_chunks)
				});

			candidate.descriptor.erasure_root = *erasure_root;
			candidates.insert(candidate.hash(), available_data.clone());
			chunks.insert(candidate.hash(), new_chunks.clone());
		}

		Self {
			validators,
			validator_public,
			validator_authority_id,
			validator_index,
			candidate_receipts,
			session_index,
			persisted_validation_data,
			candidates,
			chunks,
			config,
		}
	}
}

fn validator_pubkeys(val_ids: &[Sr25519Keyring]) -> IndexedVec<ValidatorIndex, ValidatorId> {
	val_ids.iter().map(|v| v.public().into()).collect()
}

fn validator_authority_id(val_ids: &[Sr25519Keyring]) -> Vec<AuthorityDiscoveryId> {
	val_ids.iter().map(|v| v.public().into()).collect()
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

	for loop_num in 0..env.config().num_loops {
		gum::info!(target: LOG_TARGET, loop_num, "Starting loop");

		for candidate_num in 0..config.n_cores as u64 {
			let candidate = env.state.candidate(candidate_num as usize);

			let (tx, rx) = oneshot::channel();
			batch.push(rx);

			env.send_message(AvailabilityRecoveryMessage::RecoverAvailableData(
				candidate.clone(),
				1,
				Some(GroupIndex(candidate_num as u32 % (config.n_cores / 5) as u32)),
				tx,
			))
			.await;

			// // TODO: select between futures unordered of rx await and timer to send next request.
			// if batch.len() >= config.max_parallel_recoveries {
			// 	for rx in std::mem::take(&mut batch) {
			// 		let available_data = rx.await.unwrap().unwrap();
			// 		availability_bytes += available_data.encoded_size() as u128;
			// 	}
			// }
		}

		while let Some(completed) = batch.next().await {
			let available_data = completed.unwrap().unwrap();
			availability_bytes += available_data.encoded_size() as u128;
		}
	}
	println!("Waiting for subsystem to complete work... {} requests ", batch.len());

	env.send_signal(OverseerSignal::Conclude).await;
	let duration = start_marker.elapsed().as_millis();
	let tput = ((availability_bytes) / duration) * 1000;
	println!("Benchmark completed in {:?}ms", duration);
	println!("Throughput: {}KiB/s", tput / 1024);

	tokio::time::sleep(Duration::from_secs(1)).await;
}
