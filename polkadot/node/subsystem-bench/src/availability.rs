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

use std::{sync::Arc, time::Duration};

use assert_matches::assert_matches;
use env_logger::Env;
use futures::{
	channel::{mpsc, oneshot},
	executor, future, Future, FutureExt, SinkExt,
};
use futures_timer::Delay;
use polkadot_node_metrics::metrics::Metrics;

use polkadot_availability_recovery::{AvailabilityRecoverySubsystem, Metrics as SubsystemMetrics};

use parity_scale_codec::Encode;
use polkadot_node_network_protocol::request_response::{
	self as req_res, v1::ChunkResponse, IncomingRequest, Recipient, ReqProtocolNames, Requests,
};

use prometheus::Registry;
use sc_network::{config::RequestResponseConfig, IfDisconnected, OutboundFailure, RequestFailure};

use polkadot_erasure_coding::{branches, obtain_chunks_v1 as obtain_chunks};
use polkadot_node_primitives::{BlockData, PoV, Proof};
use polkadot_node_subsystem::{
	errors::RecoveryError,
	jaeger,
	messages::{
		AllMessages, AvailabilityRecoveryMessage, AvailabilityStoreMessage, NetworkBridgeTxMessage,
		RuntimeApiMessage, RuntimeApiRequest,
	},
	overseer, ActiveLeavesUpdate, FromOrchestra, OverseerSignal, SpawnedSubsystem, Subsystem,
	SubsystemContext, SubsystemError, SubsystemResult,
};

const LOG_TARGET: &str = "subsystem-bench::availability";

use polkadot_erasure_coding::recovery_threshold;
use polkadot_node_primitives::{AvailableData, ErasureChunk};
// use polkadot_node_subsystem::{
// 	errors::RecoveryError,
// 	jaeger,
// 	messages::{AvailabilityRecoveryMessage, AvailabilityStoreMessage},
// 	overseer, ActiveLeavesUpdate, FromOrchestra, OverseerSignal, SpawnedSubsystem,
// 	SubsystemContext, SubsystemError, SubsystemResult,
// };
use polkadot_node_subsystem_test_helpers::{
	make_subsystem_context, mock::new_leaf, TestSubsystemContextHandle,
};
use polkadot_node_subsystem_util::TimeoutExt;
use polkadot_primitives::{
	AuthorityDiscoveryId, CandidateHash, CandidateReceipt, GroupIndex, Hash, HeadData, IndexedVec,
	PersistedValidationData, SessionIndex, SessionInfo, ValidatorId, ValidatorIndex,
};
use polkadot_primitives_test_helpers::{dummy_candidate_receipt, dummy_hash};
use sc_service::{SpawnTaskHandle, TaskManager};

type VirtualOverseer = TestSubsystemContextHandle<AvailabilityRecoveryMessage>;

// Deterministic genesis hash for protocol names
const GENESIS_HASH: Hash = Hash::repeat_byte(0xff);

struct AvailabilityRecoverySubsystemInstance {
	protocol_config: RequestResponseConfig,
}

pub struct EnvParams {
	// The candidate we will recover in the benchmark.
	candidate: CandidateReceipt,
}

// Implements a mockup of NetworkBridge and AvilabilityStore to support provide state for
// `AvailabilityRecoverySubsystemInstance`
pub struct TestEnvironment {
	// A tokio runtime to use in the test
	runtime: tokio::runtime::Handle,
	// A task manager that tracks task poll durations.
	task_manager: TaskManager,
	// The Prometheus metrics registry
	registry: Registry,
	// A test overseer.
	to_subsystem: mpsc::Sender<FromOrchestra<AvailabilityRecoveryMessage>>,
	// Parameters
	params: EnvParams,
	// Subsystem instance, currently keeps req/response protocol channel senders.
	instance: AvailabilityRecoverySubsystemInstance,
}

impl TestEnvironment {
	pub fn new(runtime: tokio::runtime::Handle, mut params: EnvParams, registry: Registry) -> Self {
		let task_manager: TaskManager = TaskManager::new(runtime.clone(), Some(&registry)).unwrap();
		let (instance, virtual_overseer) = AvailabilityRecoverySubsystemInstance::new(
			&registry,
			task_manager.spawn_handle(),
			runtime.clone(),
		);

		// TODO: support parametrization of initial test state
		// n_validator, n_cores.
		let state = TestState::new(params.candidate.clone());
		// Override candidate after computing erasure in `TestState::new`
		params.candidate = state.candidate();

		// Create channel to inject messages int the subsystem.
		let to_subsystem = virtual_overseer.tx.clone();

		// We need to start a receiver to process messages from the subsystem.
		task_manager.spawn_handle().spawn_blocking(
			"test-environment",
			"test-environment",
			async move { Self::env_task(virtual_overseer, state).await },
		);

		TestEnvironment { runtime, task_manager, registry, to_subsystem, params, instance }
	}

	pub fn params(&self) -> &EnvParams {
		&self.params
	}

	async fn respond_to_send_request(state: &mut TestState, request: Requests) {
		match request {
			Requests::ChunkFetchingV1(outgoing_request) => {
				let validator_index = outgoing_request.payload.index.0 as usize;
				let chunk: ChunkResponse = state.chunks[validator_index].clone().into();

				let _ = outgoing_request
					.pending_response
					.send(Ok(req_res::v1::ChunkFetchingResponse::from(Some(chunk)).encode()));
			},
			_ => panic!("received an unexpected request"),
		}
	}

	// A task that mocks dependent subsystems based on environment configuration.
	// TODO: Spawn real subsystems, user overseer builder.
	async fn env_task(
		mut ctx: TestSubsystemContextHandle<AvailabilityRecoveryMessage>,
		mut state: TestState,
	) {
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
								// TODO: add latency variance when answering requests. This should be an env parameter.
								Self::respond_to_send_request(&mut state, request).await;
							}
						},
						AllMessages::AvailabilityStore(AvailabilityStoreMessage::QueryAvailableData(_candidate_hash, tx)) => {
							// TODO: Simulate av store load by delaying the response.
							state.respond_none_to_available_data_query(tx).await;
						},
						AllMessages::AvailabilityStore(AvailabilityStoreMessage::QueryAllChunks(_candidate_hash, tx)) => {
							// Test env: We always have our own chunk.
							state.respond_to_query_all_request(|index| index == state.validator_index.0 as usize, tx).await;
						},
						AllMessages::AvailabilityStore(
							AvailabilityStoreMessage::QueryChunkSize(_, tx)
						) => {
							let chunk_size = state.chunks[0].encoded_size();
							let _ = tx.send(Some(chunk_size));
						}
						AllMessages::RuntimeApi(RuntimeApiMessage::Request(
							relay_parent,
							RuntimeApiRequest::SessionInfo(
								session_index,
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
				panic!("{}ms is more than enough for sending signals.", TIMEOUT.as_millis())
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
		runtime: tokio::runtime::Handle,
	) -> (Self, TestSubsystemContextHandle<AvailabilityRecoveryMessage>) {
		let (context, virtual_overseer) = make_subsystem_context(spawn_task_handle.clone());
		let (collation_req_receiver, req_cfg) =
			IncomingRequest::get_config_receiver(&ReqProtocolNames::new(&GENESIS_HASH, None));
		let subsystem = AvailabilityRecoverySubsystem::with_chunks_only(
			collation_req_receiver,
			Metrics::try_register(&registry).unwrap(),
		);

		let spawned_subsystem = subsystem.start(context);
		let subsystem_future = async move {
			spawned_subsystem.future.await.unwrap();
		};

		spawn_task_handle.spawn_blocking(
			spawned_subsystem.name,
			spawned_subsystem.name,
			subsystem_future,
		);

		(Self { protocol_config: req_cfg }, virtual_overseer)
	}
}

const TIMEOUT: Duration = Duration::from_millis(300);

// We use this to bail out sending messages to the subsystem if it is overloaded such that
// the time of flight is breaches 5s.
// This should eventually be a test parameter.
const MAX_TIME_OF_FLIGHT: Duration = Duration::from_millis(5000);

macro_rules! delay {
	($delay:expr) => {
		Delay::new(Duration::from_millis($delay)).await;
	};
}

use sp_keyring::Sr25519Keyring;

#[derive(Debug)]
enum Has {
	No,
	Yes,
	NetworkError(RequestFailure),
	/// Make request not return at all, instead the sender is returned from the function.
	///
	/// Note, if you use `DoesNotReturn` you have to keep the returned senders alive, otherwise the
	/// subsystem will receive a cancel event and the request actually does return.
	DoesNotReturn,
}

impl Has {
	fn timeout() -> Self {
		Has::NetworkError(RequestFailure::Network(OutboundFailure::Timeout))
	}
}

#[derive(Clone)]
struct TestState {
	validators: Vec<Sr25519Keyring>,
	validator_public: IndexedVec<ValidatorIndex, ValidatorId>,
	validator_authority_id: Vec<AuthorityDiscoveryId>,
	// The test node validator index.
	validator_index: ValidatorIndex,
	candidate: CandidateReceipt,
	session_index: SessionIndex,

	persisted_validation_data: PersistedValidationData,

	available_data: AvailableData,
	chunks: Vec<ErasureChunk>,
	invalid_chunks: Vec<ErasureChunk>,
}

impl TestState {
	fn candidate(&self) -> CandidateReceipt {
		self.candidate.clone()
	}

	fn threshold(&self) -> usize {
		recovery_threshold(self.validators.len()).unwrap()
	}

	fn impossibility_threshold(&self) -> usize {
		self.validators.len() - self.threshold() + 1
	}

	async fn respond_to_available_data_query(&self, tx: oneshot::Sender<Option<AvailableData>>) {
		let _ = tx.send(Some(self.available_data.clone()));
	}

	async fn respond_none_to_available_data_query(
		&self,
		tx: oneshot::Sender<Option<AvailableData>>,
	) {
		let _ = tx.send(None);
	}

	fn session_info(&self) -> SessionInfo {
		SessionInfo {
			validators: self.validator_public.clone(),
			discovery_keys: self.validator_authority_id.clone(),
			// all validators in the same group.
			validator_groups: IndexedVec::<GroupIndex, Vec<ValidatorIndex>>::from(vec![(0..self
				.validators
				.len())
				.map(|i| ValidatorIndex(i as _))
				.collect()]),
			assignment_keys: vec![],
			n_cores: 0,
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
		send_chunk: impl Fn(usize) -> bool,
		tx: oneshot::Sender<Vec<ErasureChunk>>,
	) {
		let v = self.chunks.iter().filter(|c| send_chunk(c.index.0 as usize)).cloned().collect();

		let _ = tx.send(v);
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

impl TestState {
	fn new(mut candidate: CandidateReceipt) -> Self {
		let validators = vec![
			Sr25519Keyring::Ferdie, // <- this node, role: validator
			Sr25519Keyring::Alice,
			Sr25519Keyring::Bob,
			Sr25519Keyring::Charlie,
			Sr25519Keyring::Dave,
		];

		let validator_public = validator_pubkeys(&validators);
		let validator_authority_id = validator_authority_id(&validators);
		let validator_index = ValidatorIndex(0);

		let session_index = 10;

		let persisted_validation_data = PersistedValidationData {
			parent_head: HeadData(vec![7, 8, 9]),
			relay_parent_number: Default::default(),
			max_pov_size: 1024,
			relay_parent_storage_root: Default::default(),
		};

		/// A 5MB PoV.
		let pov = PoV { block_data: BlockData(vec![42; 1024 * 1024 * 5]) };

		let available_data = AvailableData {
			validation_data: persisted_validation_data.clone(),
			pov: Arc::new(pov),
		};

		let (chunks, erasure_root) = derive_erasure_chunks_with_proofs_and_root(
			validators.len(),
			&available_data,
			|_, _| {},
		);
		// Mess around:
		let invalid_chunks = chunks
			.iter()
			.cloned()
			.map(|mut chunk| {
				if chunk.chunk.len() >= 2 && chunk.chunk[0] != chunk.chunk[1] {
					chunk.chunk[0] = chunk.chunk[1];
				} else if chunk.chunk.len() >= 1 {
					chunk.chunk[0] = !chunk.chunk[0];
				} else {
					chunk.proof = Proof::dummy_proof();
				}
				chunk
			})
			.collect();
		debug_assert_ne!(chunks, invalid_chunks);

		candidate.descriptor.erasure_root = erasure_root;

		Self {
			validators,
			validator_public,
			validator_authority_id,
			validator_index,
			candidate,
			session_index,
			persisted_validation_data,
			available_data,
			chunks,
			invalid_chunks,
		}
	}
}

pub fn bench_chunk_recovery_params() -> EnvParams {
	let mut candidate = dummy_candidate_receipt(dummy_hash());
	EnvParams { candidate }
}
pub async fn bench_chunk_recovery(env: &mut TestEnvironment) {
	env.send_signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
		Hash::repeat_byte(1),
		1,
	))))
	.await;

	let mut candidate = env.params().candidate.clone();

	for candidate_num in 0..10u64 {
		let (tx, rx) = oneshot::channel();

		candidate.descriptor.relay_parent = Hash::from_low_u64_be(candidate_num);

		env.send_message(AvailabilityRecoveryMessage::RecoverAvailableData(
			candidate.clone(),
			1,
			Some(GroupIndex(0)),
			tx,
		))
		.await;

		let available_data = rx.await.unwrap().unwrap();
	}
	env.send_signal(OverseerSignal::Conclude).await;
}
