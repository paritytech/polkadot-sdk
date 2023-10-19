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

use crate::task::REGULAR_CHUNKS_REQ_RETRY_THRESHOLD;

use super::*;
use std::{sync::Arc, time::Duration};

use assert_matches::assert_matches;
use futures::{executor, future};
use futures_timer::Delay;
use rstest::rstest;

use parity_scale_codec::Encode;
use polkadot_erasure_coding::{branches, obtain_chunks_v1 as obtain_chunks};
use polkadot_node_network_protocol::request_response::{
	self as req_res, v1::AvailableDataFetchingRequest, IncomingRequest, Recipient,
	ReqProtocolNames, Requests,
};
use polkadot_node_primitives::{BlockData, PoV, Proof};
use polkadot_node_subsystem::messages::{
	AllMessages, NetworkBridgeTxMessage, RuntimeApiMessage, RuntimeApiRequest,
};
use polkadot_node_subsystem_test_helpers::{
	make_subsystem_context, mock::new_leaf, TestSubsystemContextHandle,
};
use polkadot_node_subsystem_util::TimeoutExt;
use polkadot_primitives::{
	vstaging::ClientFeatures, AuthorityDiscoveryId, Hash, HeadData, IndexedVec,
	PersistedValidationData, ValidatorId,
};
use polkadot_primitives_test_helpers::{dummy_candidate_receipt, dummy_hash};
use sc_network::{IfDisconnected, OutboundFailure, RequestFailure};
use sp_keyring::Sr25519Keyring;

type VirtualOverseer = TestSubsystemContextHandle<AvailabilityRecoveryMessage>;

// Deterministic genesis hash for protocol names
const GENESIS_HASH: Hash = Hash::repeat_byte(0xff);

fn request_receiver() -> IncomingRequestReceiver<AvailableDataFetchingRequest> {
	let receiver =
		IncomingRequest::get_config_receiver(&ReqProtocolNames::new(&GENESIS_HASH, None));
	// Don't close the sending end of the request protocol. Otherwise, the subsystem will terminate.
	std::mem::forget(receiver.1.inbound_queue);
	receiver.0
}

fn test_harness<Fut: Future<Output = VirtualOverseer>>(
	subsystem: AvailabilityRecoverySubsystem,
	test: impl FnOnce(VirtualOverseer) -> Fut,
) {
	let _ = env_logger::builder()
		.is_test(true)
		.filter(Some("polkadot_availability_recovery"), log::LevelFilter::Trace)
		.try_init();

	let pool = sp_core::testing::TaskExecutor::new();

	let (context, virtual_overseer) = make_subsystem_context(pool.clone());

	let subsystem = async {
		subsystem.run(context).await.unwrap();
	};

	let test_fut = test(virtual_overseer);

	futures::pin_mut!(test_fut);
	futures::pin_mut!(subsystem);

	executor::block_on(future::join(
		async move {
			let mut overseer = test_fut.await;
			overseer_signal(&mut overseer, OverseerSignal::Conclude).await;
		},
		subsystem,
	))
	.1
}

const TIMEOUT: Duration = Duration::from_millis(300);

macro_rules! delay {
	($delay:expr) => {
		Delay::new(Duration::from_millis($delay)).await;
	};
}

async fn overseer_signal(
	overseer: &mut TestSubsystemContextHandle<AvailabilityRecoveryMessage>,
	signal: OverseerSignal,
) {
	delay!(50);
	overseer
		.send(FromOrchestra::Signal(signal))
		.timeout(TIMEOUT)
		.await
		.unwrap_or_else(|| {
			panic!("{}ms is more than enough for sending signals.", TIMEOUT.as_millis())
		});
}

async fn overseer_send(
	overseer: &mut TestSubsystemContextHandle<AvailabilityRecoveryMessage>,
	msg: AvailabilityRecoveryMessage,
) {
	gum::trace!(msg = ?msg, "sending message");
	overseer
		.send(FromOrchestra::Communication { msg })
		.timeout(TIMEOUT)
		.await
		.unwrap_or_else(|| {
			panic!("{}ms is more than enough for sending messages.", TIMEOUT.as_millis())
		});
}

async fn overseer_recv(
	overseer: &mut TestSubsystemContextHandle<AvailabilityRecoveryMessage>,
) -> AllMessages {
	gum::trace!("waiting for message ...");
	let msg = overseer.recv().timeout(TIMEOUT).await.expect("TIMEOUT is enough to recv.");
	gum::trace!(msg = ?msg, "received message");
	msg
}

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
	current: Hash,
	candidate: CandidateReceipt,
	session_index: SessionIndex,

	persisted_validation_data: PersistedValidationData,

	available_data: AvailableData,
	chunks: Vec<ErasureChunk>,
	invalid_chunks: Vec<ErasureChunk>,
}

impl TestState {
	fn threshold(&self) -> usize {
		recovery_threshold(self.validators.len()).unwrap()
	}

	fn systematic_threshold(&self) -> usize {
		systematic_recovery_threshold(self.validators.len()).unwrap()
	}

	fn impossibility_threshold(&self) -> usize {
		self.validators.len() - self.threshold() + 1
	}

	async fn test_runtime_api_session_info(&self, virtual_overseer: &mut VirtualOverseer) {
		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				relay_parent,
				RuntimeApiRequest::SessionInfo(
					session_index,
					tx,
				)
			)) => {
				assert_eq!(relay_parent, self.current);
				assert_eq!(session_index, self.session_index);

				tx.send(Ok(Some(SessionInfo {
					validators: self.validator_public.clone(),
					discovery_keys: self.validator_authority_id.clone(),
					// all validators in the same group.
					validator_groups: IndexedVec::<GroupIndex,Vec<ValidatorIndex>>::from(vec![(0..self.validators.len()).map(|i| ValidatorIndex(i as _)).collect()]),
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
				}))).unwrap();
			}
		);
	}

	async fn test_runtime_api_client_features(&self, virtual_overseer: &mut VirtualOverseer) {
		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				_relay_parent,
				RuntimeApiRequest::ClientFeatures(
					tx,
				)
			)) => {
				tx.send(Ok(
					ClientFeatures::AVAILABILITY_CHUNK_SHUFFLING
				)).unwrap();
			}
		);
	}

	async fn test_runtime_api_empty_client_features(&self, virtual_overseer: &mut VirtualOverseer) {
		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				_relay_parent,
				RuntimeApiRequest::ClientFeatures(
					tx,
				)
			)) => {
				tx.send(Ok(
					ClientFeatures::empty()
				)).unwrap();
			}
		);
	}

	async fn respond_to_block_number_query(
		&self,
		virtual_overseer: &mut VirtualOverseer,
		block_number: BlockNumber,
	) {
		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::ChainApi(
				ChainApiMessage::BlockNumber(_, tx)
			) => {
				let _ = tx.send(Ok(Some(block_number)));
			}
		)
	}

	async fn respond_to_available_data_query(
		&self,
		virtual_overseer: &mut VirtualOverseer,
		with_data: bool,
	) {
		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::AvailabilityStore(
				AvailabilityStoreMessage::QueryAvailableData(_, tx)
			) => {
				let _ = tx.send(if with_data {
					Some(self.available_data.clone())
				} else {
					gum::debug!("Sending None");
					None
				});
			}
		)
	}

	async fn respond_to_query_all_request(
		&self,
		virtual_overseer: &mut VirtualOverseer,
		send_chunk: impl Fn(usize) -> bool,
	) {
		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::AvailabilityStore(
				AvailabilityStoreMessage::QueryAllChunks(_, tx)
			) => {
				let v = self.chunks.iter()
					.filter(|c| send_chunk(c.index.0 as usize))
					.cloned()
					.collect();

				let _ = tx.send(v);
			}
		)
	}

	async fn respond_to_query_all_request_invalid(
		&self,
		virtual_overseer: &mut VirtualOverseer,
		send_chunk: impl Fn(usize) -> bool,
	) {
		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::AvailabilityStore(
				AvailabilityStoreMessage::QueryAllChunks(_, tx)
			) => {
				let v = self.invalid_chunks.iter()
					.filter(|c| send_chunk(c.index.0 as usize))
					.cloned()
					.collect();

				let _ = tx.send(v);
			}
		)
	}

	async fn test_chunk_requests(
		&self,
		candidate_hash: CandidateHash,
		virtual_overseer: &mut VirtualOverseer,
		n: usize,
		who_has: impl Fn(usize) -> Has,
		systematic_recovery: bool,
	) -> Vec<oneshot::Sender<std::result::Result<Vec<u8>, RequestFailure>>> {
		// arbitrary order.
		let mut i = 0;
		let mut senders = Vec::new();
		while i < n {
			// Receive a request for a chunk.
			assert_matches!(
				overseer_recv(virtual_overseer).await,
				AllMessages::NetworkBridgeTx(
					NetworkBridgeTxMessage::SendRequests(
						requests,
						_if_disconnected,
					)
				) => {
					for req in requests {
						i += 1;
						assert_matches!(
							req,
							Requests::ChunkFetchingV1(req) => {
								assert_eq!(req.payload.candidate_hash, candidate_hash);

								let chunk_index = req.payload.index.0 as usize;

								if systematic_recovery {
									assert!(chunk_index <= self.systematic_threshold(), "requsted non-systematic chunk");
								}

								let available_data = match who_has(chunk_index) {
									Has::No => Ok(None),
									Has::Yes => Ok(Some(self.chunks[chunk_index].clone().into())),
									Has::NetworkError(e) => Err(e),
									Has::DoesNotReturn => {
										senders.push(req.pending_response);
										continue
									}
								};

								let _ = req.pending_response.send(
									available_data.map(|r|
										req_res::v1::ChunkFetchingResponse::from(r).encode()
									)
								);
							}
						)
					}
				}
			);
		}
		senders
	}

	async fn test_full_data_requests(
		&self,
		candidate_hash: CandidateHash,
		virtual_overseer: &mut VirtualOverseer,
		who_has: impl Fn(usize) -> Has,
	) -> Vec<oneshot::Sender<std::result::Result<Vec<u8>, RequestFailure>>> {
		let mut senders = Vec::new();
		for _ in 0..self.validators.len() {
			// Receive a request for the full `AvailableData`.
			assert_matches!(
				overseer_recv(virtual_overseer).await,
				AllMessages::NetworkBridgeTx(
					NetworkBridgeTxMessage::SendRequests(
						mut requests,
						IfDisconnected::ImmediateError,
					)
				) => {
					assert_eq!(requests.len(), 1);

					assert_matches!(
						requests.pop().unwrap(),
						Requests::AvailableDataFetchingV1(req) => {
							assert_eq!(req.payload.candidate_hash, candidate_hash);
							let validator_index = self.validator_authority_id
								.iter()
								.position(|a| Recipient::Authority(a.clone()) == req.peer)
								.unwrap();

							let available_data = match who_has(validator_index) {
								Has::No => Ok(None),
								Has::Yes => Ok(Some(self.available_data.clone())),
								Has::NetworkError(e) => Err(e),
								Has::DoesNotReturn => {
									senders.push(req.pending_response);
									continue
								}
							};

							let done = available_data.as_ref().ok().map_or(false, |x| x.is_some());

							let _ = req.pending_response.send(
								available_data.map(|r|
									req_res::v1::AvailableDataFetchingResponse::from(r).encode()
								)
							);

							if done { break }
						}
					)
				}
			);
		}
		senders
	}
}

fn validator_pubkeys(val_ids: &[Sr25519Keyring]) -> IndexedVec<ValidatorIndex, ValidatorId> {
	val_ids.iter().map(|v| v.public().into()).collect()
}

pub fn validator_authority_id(val_ids: &[Sr25519Keyring]) -> Vec<AuthorityDiscoveryId> {
	val_ids.iter().map(|v| v.public().into()).collect()
}

pub fn derive_erasure_chunks_with_proofs_and_root(
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
			index: ChunkIndex(index as _),
			proof: Proof::try_from(proof).unwrap(),
		})
		.collect::<Vec<ErasureChunk>>();

	(erasure_chunks, root)
}

impl Default for TestState {
	fn default() -> Self {
		let validators = vec![
			Sr25519Keyring::Ferdie, // <- this node, role: validator
			Sr25519Keyring::Alice,
			Sr25519Keyring::Bob,
			Sr25519Keyring::Charlie,
			Sr25519Keyring::Dave,
			Sr25519Keyring::One,
			Sr25519Keyring::Two,
		];

		let validator_public = validator_pubkeys(&validators);
		let validator_authority_id = validator_authority_id(&validators);

		let current = Hash::repeat_byte(1);

		let mut candidate = dummy_candidate_receipt(dummy_hash());

		let session_index = 10;

		let persisted_validation_data = PersistedValidationData {
			parent_head: HeadData(vec![7, 8, 9]),
			relay_parent_number: Default::default(),
			max_pov_size: 1024,
			relay_parent_storage_root: Default::default(),
		};

		let pov = PoV { block_data: BlockData(vec![42; 64]) };

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
		candidate.descriptor.relay_parent = Hash::repeat_byte(10);

		Self {
			validators,
			validator_public,
			validator_authority_id,
			current,
			candidate,
			session_index,
			persisted_validation_data,
			available_data,
			chunks,
			invalid_chunks,
		}
	}
}

#[rstest]
#[case(true)]
#[case(false)]
fn availability_is_recovered_from_chunks_if_no_group_provided(#[case] systematic_recovery: bool) {
	let test_state = TestState::default();
	let (subsystem, threshold) = match systematic_recovery {
		true => (
			AvailabilityRecoverySubsystem::with_fast_path_then_systematic_chunks(
				request_receiver(),
				Metrics::new_dummy(),
			),
			test_state.systematic_threshold(),
		),
		false => (
			AvailabilityRecoverySubsystem::with_fast_path(request_receiver(), Metrics::new_dummy()),
			test_state.threshold(),
		),
	};

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				None,
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;

		let candidate_hash = test_state.candidate.hash();

		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
		test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;

		test_state
			.test_chunk_requests(
				candidate_hash,
				&mut virtual_overseer,
				threshold,
				|_| Has::Yes,
				systematic_recovery,
			)
			.await;

		// Recovered data should match the original one.
		assert_eq!(rx.await.unwrap().unwrap(), test_state.available_data);

		let (tx, rx) = oneshot::channel();

		// Test another candidate, send no chunks.
		let mut new_candidate = dummy_candidate_receipt(dummy_hash());

		new_candidate.descriptor.relay_parent = test_state.candidate.descriptor.relay_parent;

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				new_candidate.clone(),
				test_state.session_index,
				None,
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;

		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
		test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;

		if systematic_recovery {
			test_state
				.test_chunk_requests(
					new_candidate.hash(),
					&mut virtual_overseer,
					threshold,
					|_| Has::No,
					systematic_recovery,
				)
				.await;
			test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;
		}

		// Even if the recovery is systematic, we'll always fall back to regular recovery, so keep
		// this around.
		test_state
			.test_chunk_requests(
				new_candidate.hash(),
				&mut virtual_overseer,
				test_state.impossibility_threshold(),
				|_| Has::No,
				false,
			)
			.await;

		// A request times out with `Unavailable` error.
		assert_eq!(rx.await.unwrap().unwrap_err(), RecoveryError::Unavailable);
		virtual_overseer
	});
}

#[rstest]
#[case(true)]
#[case(false)]
fn availability_is_recovered_from_chunks_even_if_backing_group_supplied_if_chunks_only(
	#[case] systematic_recovery: bool,
) {
	let test_state = TestState::default();
	let (subsystem, threshold) = match systematic_recovery {
		true => (
			AvailabilityRecoverySubsystem::with_systematic_chunks(
				request_receiver(),
				Metrics::new_dummy(),
			),
			test_state.systematic_threshold(),
		),
		false => (
			AvailabilityRecoverySubsystem::with_chunks_only(
				request_receiver(),
				Metrics::new_dummy(),
			),
			test_state.threshold(),
		),
	};

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				Some(GroupIndex(0)),
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;

		let candidate_hash = test_state.candidate.hash();

		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
		test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;

		test_state
			.test_chunk_requests(
				candidate_hash,
				&mut virtual_overseer,
				threshold,
				|_| Has::Yes,
				systematic_recovery,
			)
			.await;

		// Recovered data should match the original one.
		assert_eq!(rx.await.unwrap().unwrap(), test_state.available_data);

		let (tx, rx) = oneshot::channel();

		// Test another candidate, send no chunks.
		let mut new_candidate = dummy_candidate_receipt(dummy_hash());

		new_candidate.descriptor.relay_parent = test_state.candidate.descriptor.relay_parent;

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				new_candidate.clone(),
				test_state.session_index,
				Some(GroupIndex(0)),
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;

		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
		test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;

		if systematic_recovery {
			test_state
				.test_chunk_requests(
					new_candidate.hash(),
					&mut virtual_overseer,
					threshold,
					|_| Has::No,
					systematic_recovery,
				)
				.await;
			test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;
		}

		// Even if the recovery is systematic, we'll always fall back to regular recovery, so keep
		// this around.
		test_state
			.test_chunk_requests(
				new_candidate.hash(),
				&mut virtual_overseer,
				test_state.impossibility_threshold(),
				|_| Has::No,
				false,
			)
			.await;

		// A request times out with `Unavailable` error.
		assert_eq!(rx.await.unwrap().unwrap_err(), RecoveryError::Unavailable);
		virtual_overseer
	});
}

#[rstest]
#[case(true)]
#[case(false)]
fn bad_merkle_path_leads_to_recovery_error(#[case] systematic_recovery: bool) {
	let mut test_state = TestState::default();
	let subsystem = match systematic_recovery {
		true => AvailabilityRecoverySubsystem::with_systematic_chunks(
			request_receiver(),
			Metrics::new_dummy(),
		),
		false => AvailabilityRecoverySubsystem::with_chunks_only(
			request_receiver(),
			Metrics::new_dummy(),
		),
	};

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				None,
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;

		let candidate_hash = test_state.candidate.hash();

		// Create some faulty chunks.
		for chunk in test_state.chunks.iter_mut() {
			chunk.chunk = vec![0; 32];
		}

		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
		test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;

		if systematic_recovery {
			test_state
				.test_chunk_requests(
					candidate_hash,
					&mut virtual_overseer,
					test_state.systematic_threshold(),
					|_| Has::No,
					systematic_recovery,
				)
				.await;
			test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;
		}

		test_state
			.test_chunk_requests(
				candidate_hash,
				&mut virtual_overseer,
				test_state.impossibility_threshold(),
				|_| Has::Yes,
				false,
			)
			.await;

		// A request times out with `Unavailable` error.
		assert_eq!(rx.await.unwrap().unwrap_err(), RecoveryError::Unavailable);
		virtual_overseer
	});
}

#[rstest]
#[case(true)]
#[case(false)]
fn wrong_chunk_index_leads_to_recovery_error(#[case] systematic_recovery: bool) {
	let mut test_state = TestState::default();

	let subsystem = match systematic_recovery {
		true => AvailabilityRecoverySubsystem::with_systematic_chunks(
			request_receiver(),
			Metrics::new_dummy(),
		),
		false => AvailabilityRecoverySubsystem::with_chunks_only(
			request_receiver(),
			Metrics::new_dummy(),
		),
	};

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				None,
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;

		let candidate_hash = test_state.candidate.hash();

		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
		test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;

		// Chunks should fail the index check as they don't have the correct index.
		let first_chunk = test_state.chunks[0].clone();
		test_state.chunks[0] = test_state.chunks[1].clone();
		for c_index in 1..test_state.chunks.len() {
			test_state.chunks[c_index] = first_chunk.clone();
		}

		if systematic_recovery {
			test_state
				.test_chunk_requests(
					candidate_hash,
					&mut virtual_overseer,
					test_state.systematic_threshold(),
					|_| Has::Yes,
					systematic_recovery,
				)
				.await;

			test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;
		}

		test_state
			.test_chunk_requests(
				candidate_hash,
				&mut virtual_overseer,
				test_state.chunks.len() - 1,
				|_| Has::Yes,
				false,
			)
			.await;

		// A request times out with `Unavailable` error as there are no good peers.
		assert_eq!(rx.await.unwrap().unwrap_err(), RecoveryError::Unavailable);
		virtual_overseer
	});
}

#[rstest]
#[case(true)]
#[case(false)]
fn invalid_erasure_coding_leads_to_invalid_error(#[case] systematic_recovery: bool) {
	let mut test_state = TestState::default();

	let (subsystem, threshold) = match systematic_recovery {
		true => (
			AvailabilityRecoverySubsystem::with_fast_path_then_systematic_chunks(
				request_receiver(),
				Metrics::new_dummy(),
			),
			test_state.systematic_threshold(),
		),
		false => (
			AvailabilityRecoverySubsystem::with_fast_path(request_receiver(), Metrics::new_dummy()),
			test_state.threshold(),
		),
	};

	test_harness(subsystem, |mut virtual_overseer| async move {
		let pov = PoV { block_data: BlockData(vec![69; 64]) };

		let (bad_chunks, bad_erasure_root) = derive_erasure_chunks_with_proofs_and_root(
			test_state.chunks.len(),
			&AvailableData {
				validation_data: test_state.persisted_validation_data.clone(),
				pov: Arc::new(pov),
			},
			|i, chunk| *chunk = vec![i as u8; 32],
		);

		test_state.chunks = bad_chunks;
		test_state.candidate.descriptor.erasure_root = bad_erasure_root;

		let candidate_hash = test_state.candidate.hash();

		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				None,
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;

		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
		test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;

		test_state
			.test_chunk_requests(
				candidate_hash,
				&mut virtual_overseer,
				threshold,
				|_| Has::Yes,
				systematic_recovery,
			)
			.await;

		// f+1 'valid' chunks can't produce correct data.
		assert_eq!(rx.await.unwrap().unwrap_err(), RecoveryError::Invalid);
		virtual_overseer
	});
}

#[test]
fn fast_path_backing_group_recovers() {
	let test_state = TestState::default();
	let subsystem =
		AvailabilityRecoverySubsystem::with_fast_path(request_receiver(), Metrics::new_dummy());

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				Some(GroupIndex(0)),
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;

		let candidate_hash = test_state.candidate.hash();

		let who_has = |i| match i {
			3 => Has::Yes,
			_ => Has::No,
		};

		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;

		test_state
			.test_full_data_requests(candidate_hash, &mut virtual_overseer, who_has)
			.await;

		// Recovered data should match the original one.
		assert_eq!(rx.await.unwrap().unwrap(), test_state.available_data);
		virtual_overseer
	});
}

#[rstest]
#[case(true, false)]
#[case(false, true)]
#[case(false, false)]
fn recovers_from_only_chunks_if_pov_large(
	#[case] systematic_recovery: bool,
	#[case] skip_availability_store: bool,
) {
	let test_state = TestState::default();

	let (subsystem, threshold) = match (systematic_recovery, skip_availability_store) {
		(true, false) => (
			AvailabilityRecoverySubsystem::with_systematic_chunks_if_pov_large(
				request_receiver(),
				Metrics::new_dummy(),
			),
			test_state.systematic_threshold(),
		),
		(false, false) => (
			AvailabilityRecoverySubsystem::with_chunks_if_pov_large(
				request_receiver(),
				Metrics::new_dummy(),
			),
			test_state.threshold(),
		),
		(false, true) => (
			AvailabilityRecoverySubsystem::with_availability_store_skip(
				request_receiver(),
				Metrics::new_dummy(),
			),
			test_state.threshold(),
		),
		(_, _) => unreachable!(),
	};

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				Some(GroupIndex(0)),
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;

		let candidate_hash = test_state.candidate.hash();

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::AvailabilityStore(
				AvailabilityStoreMessage::QueryChunkSize(_, tx)
			) => {
				let _ = tx.send(Some(1000000));
			}
		);

		if !skip_availability_store {
			test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
			test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;
		}

		test_state
			.test_chunk_requests(
				candidate_hash,
				&mut virtual_overseer,
				threshold,
				|_| Has::Yes,
				systematic_recovery,
			)
			.await;

		// Recovered data should match the original one.
		assert_eq!(rx.await.unwrap().unwrap(), test_state.available_data);

		let (tx, rx) = oneshot::channel();

		// Test another candidate, send no chunks.
		let mut new_candidate = dummy_candidate_receipt(dummy_hash());

		new_candidate.descriptor.relay_parent = test_state.candidate.descriptor.relay_parent;

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				new_candidate.clone(),
				test_state.session_index,
				Some(GroupIndex(0)),
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::AvailabilityStore(
				AvailabilityStoreMessage::QueryChunkSize(_, tx)
			) => {
				let _ = tx.send(Some(1000000));
			}
		);

		if !skip_availability_store {
			test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
			test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;
		}

		if systematic_recovery {
			test_state
				.test_chunk_requests(
					new_candidate.hash(),
					&mut virtual_overseer,
					test_state.systematic_threshold(),
					|_| Has::No,
					systematic_recovery,
				)
				.await;
			if !skip_availability_store {
				test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;
			}
		}
		test_state
			.test_chunk_requests(
				new_candidate.hash(),
				&mut virtual_overseer,
				test_state.impossibility_threshold(),
				|_| Has::No,
				false,
			)
			.await;

		// A request times out with `Unavailable` error.
		assert_eq!(rx.await.unwrap().unwrap_err(), RecoveryError::Unavailable);
		virtual_overseer
	});
}

#[rstest]
#[case(true, false)]
#[case(false, true)]
#[case(false, false)]
fn fast_path_backing_group_recovers_if_pov_small(
	#[case] systematic_recovery: bool,
	#[case] skip_availability_store: bool,
) {
	let test_state = TestState::default();

	let subsystem = match (systematic_recovery, skip_availability_store) {
		(true, false) => AvailabilityRecoverySubsystem::with_systematic_chunks_if_pov_large(
			request_receiver(),
			Metrics::new_dummy(),
		),

		(false, false) => AvailabilityRecoverySubsystem::with_chunks_if_pov_large(
			request_receiver(),
			Metrics::new_dummy(),
		),
		(false, true) => AvailabilityRecoverySubsystem::with_availability_store_skip(
			request_receiver(),
			Metrics::new_dummy(),
		),
		(_, _) => unreachable!(),
	};

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				Some(GroupIndex(0)),
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;

		let candidate_hash = test_state.candidate.hash();

		let who_has = |i| match i {
			3 => Has::Yes,
			_ => Has::No,
		};

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::AvailabilityStore(
				AvailabilityStoreMessage::QueryChunkSize(_, tx)
			) => {
				let _ = tx.send(Some(100));
			}
		);

		if !skip_availability_store {
			test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
		}

		test_state
			.test_full_data_requests(candidate_hash, &mut virtual_overseer, who_has)
			.await;

		// Recovered data should match the original one.
		assert_eq!(rx.await.unwrap().unwrap(), test_state.available_data);
		virtual_overseer
	});
}

#[rstest]
#[case(true)]
#[case(false)]
fn no_answers_in_fast_path_causes_chunk_requests(#[case] systematic_recovery: bool) {
	let test_state = TestState::default();

	let (subsystem, threshold) = match systematic_recovery {
		true => (
			AvailabilityRecoverySubsystem::with_fast_path_then_systematic_chunks(
				request_receiver(),
				Metrics::new_dummy(),
			),
			test_state.systematic_threshold(),
		),
		false => (
			AvailabilityRecoverySubsystem::with_fast_path(request_receiver(), Metrics::new_dummy()),
			test_state.threshold(),
		),
	};

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				Some(GroupIndex(0)),
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;

		let candidate_hash = test_state.candidate.hash();

		// mix of timeout and no.
		let who_has = |i| match i {
			0 | 3 => Has::No,
			_ => Has::timeout(),
		};

		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;

		test_state
			.test_full_data_requests(candidate_hash, &mut virtual_overseer, who_has)
			.await;

		test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;

		test_state
			.test_chunk_requests(
				candidate_hash,
				&mut virtual_overseer,
				threshold,
				|_| Has::Yes,
				systematic_recovery,
			)
			.await;

		// Recovered data should match the original one.
		assert_eq!(rx.await.unwrap().unwrap(), test_state.available_data);
		virtual_overseer
	});
}

#[rstest]
#[case(true)]
#[case(false)]
fn task_canceled_when_receivers_dropped(#[case] systematic_recovery: bool) {
	let test_state = TestState::default();

	let subsystem = match systematic_recovery {
		true => AvailabilityRecoverySubsystem::with_systematic_chunks(
			request_receiver(),
			Metrics::new_dummy(),
		),
		false => AvailabilityRecoverySubsystem::with_chunks_only(
			request_receiver(),
			Metrics::new_dummy(),
		),
	};

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, _) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				None,
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;

		for _ in 0..test_state.validators.len() {
			match virtual_overseer.recv().timeout(TIMEOUT).await {
				None => return virtual_overseer,
				Some(_) => continue,
			}
		}

		panic!("task requested all validators without concluding")
	});
}

#[rstest]
#[case(true)]
#[case(false)]
fn chunks_retry_until_all_nodes_respond(#[case] systematic_recovery: bool) {
	let test_state = TestState::default();
	let subsystem = match systematic_recovery {
		true => AvailabilityRecoverySubsystem::with_systematic_chunks(
			request_receiver(),
			Metrics::new_dummy(),
		),
		false => AvailabilityRecoverySubsystem::with_chunks_only(
			request_receiver(),
			Metrics::new_dummy(),
		),
	};

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				Some(GroupIndex(0)),
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;

		let candidate_hash = test_state.candidate.hash();

		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
		test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;

		if systematic_recovery {
			test_state
				.test_chunk_requests(
					candidate_hash,
					&mut virtual_overseer,
					test_state.systematic_threshold(),
					|_| Has::timeout(),
					true,
				)
				.await;
			test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;
		}

		test_state
			.test_chunk_requests(
				candidate_hash,
				&mut virtual_overseer,
				test_state.impossibility_threshold(),
				|_| Has::timeout(),
				false,
			)
			.await;

		// We get to go another round! Actually, we get to go an infinite number of times.
		test_state
			.test_chunk_requests(
				candidate_hash,
				&mut virtual_overseer,
				test_state.impossibility_threshold(),
				|_| Has::No,
				false,
			)
			.await;

		// Recovery is impossible.
		assert_eq!(rx.await.unwrap().unwrap_err(), RecoveryError::Unavailable);
		virtual_overseer
	});
}

#[test]
fn network_bridge_not_returning_responses_wont_stall_retrieval() {
	let test_state = TestState::default();

	let subsystem =
		AvailabilityRecoverySubsystem::with_chunks_only(request_receiver(), Metrics::new_dummy());

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				Some(GroupIndex(0)),
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;

		let candidate_hash = test_state.candidate.hash();

		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
		test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;

		// How many validators should not respond at all:
		let not_returning_count = 1;

		// Not returning senders won't cause the retrieval to stall:
		let _senders = test_state
			.test_chunk_requests(
				candidate_hash,
				&mut virtual_overseer,
				not_returning_count,
				|_| Has::DoesNotReturn,
				false,
			)
			.await;

		test_state
			.test_chunk_requests(
				candidate_hash,
				&mut virtual_overseer,
				// Should start over:
				test_state.validators.len() + 3,
				|_| Has::timeout(),
				false,
			)
			.await;

		// we get to go another round!
		test_state
			.test_chunk_requests(
				candidate_hash,
				&mut virtual_overseer,
				test_state.threshold(),
				|_| Has::Yes,
				false,
			)
			.await;

		// Recovered data should match the original one:
		assert_eq!(rx.await.unwrap().unwrap(), test_state.available_data);
		virtual_overseer
	});
}

#[rstest]
#[case(true)]
#[case(false)]
fn all_not_returning_requests_still_recovers_on_return(#[case] systematic_recovery: bool) {
	let test_state = TestState::default();
	let subsystem = match systematic_recovery {
		true => AvailabilityRecoverySubsystem::with_systematic_chunks(
			request_receiver(),
			Metrics::new_dummy(),
		),
		false => AvailabilityRecoverySubsystem::with_chunks_only(
			request_receiver(),
			Metrics::new_dummy(),
		),
	};

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				Some(GroupIndex(0)),
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;

		let candidate_hash = test_state.candidate.hash();

		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
		test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;
		let n = if systematic_recovery {
			test_state.systematic_threshold()
		} else {
			test_state.validators.len()
		};

		let senders = test_state
			.test_chunk_requests(
				candidate_hash,
				&mut virtual_overseer,
				n,
				|_| Has::DoesNotReturn,
				false,
			)
			.await;

		future::join(
			async {
				Delay::new(Duration::from_millis(10)).await;
				// Now retrieval should be able to recover.
				std::mem::drop(senders);
			},
			async {
				if systematic_recovery {
					test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;
				}
				test_state
					.test_chunk_requests(
						candidate_hash,
						&mut virtual_overseer,
						// Should start over:
						test_state.validators.len(),
						|_| Has::timeout(),
						false,
					)
					.await
			},
		)
		.await;

		// we get to go another round!
		test_state
			.test_chunk_requests(
				candidate_hash,
				&mut virtual_overseer,
				test_state.threshold(),
				|_| Has::Yes,
				false,
			)
			.await;

		// Recovered data should match the original one:
		assert_eq!(rx.await.unwrap().unwrap(), test_state.available_data);
		virtual_overseer
	});
}

#[rstest]
#[case(true)]
#[case(false)]
fn returns_early_if_we_have_the_data(#[case] systematic_recovery: bool) {
	let test_state = TestState::default();
	let subsystem = match systematic_recovery {
		true => AvailabilityRecoverySubsystem::with_systematic_chunks(
			request_receiver(),
			Metrics::new_dummy(),
		),
		false => AvailabilityRecoverySubsystem::with_chunks_only(
			request_receiver(),
			Metrics::new_dummy(),
		),
	};

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				None,
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;
		test_state.respond_to_available_data_query(&mut virtual_overseer, true).await;

		assert_eq!(rx.await.unwrap().unwrap(), test_state.available_data);
		virtual_overseer
	});
}

#[test]
fn returns_early_if_present_in_the_subsystem_cache() {
	let test_state = TestState::default();
	let subsystem =
		AvailabilityRecoverySubsystem::with_fast_path(request_receiver(), Metrics::new_dummy());

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				Some(GroupIndex(0)),
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;

		let candidate_hash = test_state.candidate.hash();

		let who_has = |i| match i {
			3 => Has::Yes,
			_ => Has::No,
		};

		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;

		test_state
			.test_full_data_requests(candidate_hash, &mut virtual_overseer, who_has)
			.await;

		// Recovered data should match the original one.
		assert_eq!(rx.await.unwrap().unwrap(), test_state.available_data);

		// A second recovery for the same candidate will return early as it'll be present in the
		// cache.
		let (tx, rx) = oneshot::channel();
		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				Some(GroupIndex(0)),
				tx,
			),
		)
		.await;
		assert_eq!(rx.await.unwrap().unwrap(), test_state.available_data);

		virtual_overseer
	});
}

#[rstest]
#[case(true)]
#[case(false)]
fn does_not_query_local_validator(#[case] systematic_recovery: bool) {
	let test_state = TestState::default();
	let (subsystem, threshold) = match systematic_recovery {
		true => (
			AvailabilityRecoverySubsystem::with_systematic_chunks(
				request_receiver(),
				Metrics::new_dummy(),
			),
			test_state.systematic_threshold(),
		),
		false => (
			AvailabilityRecoverySubsystem::with_chunks_only(
				request_receiver(),
				Metrics::new_dummy(),
			),
			test_state.threshold(),
		),
	};

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				None,
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;
		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
		test_state.respond_to_query_all_request(&mut virtual_overseer, |i| i == 0).await;

		let candidate_hash = test_state.candidate.hash();

		// second round, make sure it uses the local chunk.
		test_state
			.test_chunk_requests(
				candidate_hash,
				&mut virtual_overseer,
				threshold - 1,
				|i| if i == 0 { panic!("requested from local validator") } else { Has::Yes },
				systematic_recovery,
			)
			.await;

		assert_eq!(rx.await.unwrap().unwrap(), test_state.available_data);
		virtual_overseer
	});
}

#[rstest]
#[case(true)]
#[case(false)]
fn invalid_local_chunk(#[case] systematic_recovery: bool) {
	let test_state = TestState::default();
	let subsystem = match systematic_recovery {
		true => AvailabilityRecoverySubsystem::with_systematic_chunks(
			request_receiver(),
			Metrics::new_dummy(),
		),
		false => AvailabilityRecoverySubsystem::with_chunks_only(
			request_receiver(),
			Metrics::new_dummy(),
		),
	};

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				None,
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;
		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
		test_state
			.respond_to_query_all_request_invalid(&mut virtual_overseer, |i| i == 0)
			.await;

		let candidate_hash = test_state.candidate.hash();

		// If systematic recovery detects invalid local chunk, it'll directly go to regular
		// recovery.
		if systematic_recovery {
			test_state
				.respond_to_query_all_request_invalid(&mut virtual_overseer, |i| i == 0)
				.await;
		}

		test_state
			.test_chunk_requests(
				candidate_hash,
				&mut virtual_overseer,
				test_state.threshold(),
				|i| if i == 0 { panic!("requested from local validator") } else { Has::Yes },
				false,
			)
			.await;

		assert_eq!(rx.await.unwrap().unwrap(), test_state.available_data);
		virtual_overseer
	});
}

#[test]
fn systematic_chunks_are_not_requested_again_in_regular_recovery() {
	// Run this test multiple times, as the order in which requests are made is random and we want
	// to make sure that we catch regressions.
	for _ in 0..TestState::default().chunks.len() {
		let test_state = TestState::default();
		let subsystem = AvailabilityRecoverySubsystem::with_systematic_chunks(
			request_receiver(),
			Metrics::new_dummy(),
		);

		test_harness(subsystem, |mut virtual_overseer| async move {
			overseer_signal(
				&mut virtual_overseer,
				OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
					test_state.current,
					1,
				))),
			)
			.await;

			let (tx, rx) = oneshot::channel();

			overseer_send(
				&mut virtual_overseer,
				AvailabilityRecoveryMessage::RecoverAvailableData(
					test_state.candidate.clone(),
					test_state.session_index,
					None,
					tx,
				),
			)
			.await;

			test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
			test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
			test_state.test_runtime_api_client_features(&mut virtual_overseer).await;
			test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
			test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;

			test_state
				.test_chunk_requests(
					test_state.candidate.hash(),
					&mut virtual_overseer,
					test_state.systematic_threshold(),
					|i| if i == 0 { Has::No } else { Has::Yes },
					true,
				)
				.await;

			// Falls back to regular recovery.
			test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;

			test_state
				.test_chunk_requests(
					test_state.candidate.hash(),
					&mut virtual_overseer,
					1,
					|i: usize| {
						if i < test_state.systematic_threshold() {
							panic!("Already requested")
						} else {
							Has::Yes
						}
					},
					false,
				)
				.await;

			assert_eq!(rx.await.unwrap().unwrap(), test_state.available_data);
			virtual_overseer
		});
	}
}

#[rstest]
#[case(true, true)]
#[case(true, false)]
#[case(false, true)]
#[case(false, false)]
fn chunk_indices_are_shuffled(#[case] systematic_recovery: bool, #[case] shuffling_enabled: bool) {
	let test_state = TestState::default();
	let subsystem = match systematic_recovery {
		true => AvailabilityRecoverySubsystem::with_systematic_chunks(
			request_receiver(),
			Metrics::new_dummy(),
		),
		false => AvailabilityRecoverySubsystem::with_chunks_only(
			request_receiver(),
			Metrics::new_dummy(),
		),
	};

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, _rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				None,
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;

		if shuffling_enabled {
			test_state.test_runtime_api_client_features(&mut virtual_overseer).await;
		} else {
			test_state.test_runtime_api_empty_client_features(&mut virtual_overseer).await;
		}

		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
		test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;

		let mut chunk_indices: Vec<(usize, usize)> = vec![];

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::NetworkBridgeTx(
				NetworkBridgeTxMessage::SendRequests(
					requests,
					_if_disconnected,
				)
			) => {
				for req in requests {
					assert_matches!(
						req,
						Requests::ChunkFetchingV1(req) => {
							assert_eq!(req.payload.candidate_hash, test_state.candidate.hash());

							let chunk_index = req.payload.index.0 as usize;
							let validator_index = test_state.validator_authority_id.iter().enumerate().find(|(_, id)| {
								if let Recipient::Authority(auth_id) = &req.peer {
									if *id == auth_id {
										return true
									}
								}
								false
							}).expect("validator not found").0;

							if systematic_recovery {
								assert!(chunk_index <= test_state.systematic_threshold(), "requsted non-systematic chunk");
							}

							chunk_indices.push((chunk_index, validator_index));
						}
					)
				}
			}
		);

		if shuffling_enabled {
			assert!(!chunk_indices.iter().any(|(c_index, v_index)| c_index == v_index));
		} else {
			assert!(chunk_indices.iter().all(|(c_index, v_index)| c_index == v_index));
		}

		virtual_overseer
	});
}

#[rstest]
#[case(true)]
#[case(false)]
fn number_of_request_retries_is_bounded(#[case] should_fail: bool) {
	let mut test_state = TestState::default();
	// We need the number of validators to be evenly divisible by the threshold for this test to be
	// easier to write.
	let n_validators = 6;
	test_state.validators.truncate(n_validators);
	test_state.validator_authority_id.truncate(n_validators);
	let mut temp = test_state.validator_public.to_vec();
	temp.truncate(n_validators);
	test_state.validator_public = temp.into();

	let (chunks, erasure_root) = derive_erasure_chunks_with_proofs_and_root(
		n_validators,
		&test_state.available_data,
		|_, _| {},
	);
	test_state.chunks = chunks;
	test_state.candidate.descriptor.erasure_root = erasure_root;

	let subsystem =
		AvailabilityRecoverySubsystem::with_chunks_only(request_receiver(), Metrics::new_dummy());

	test_harness(subsystem, |mut virtual_overseer| async move {
		overseer_signal(
			&mut virtual_overseer,
			OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(new_leaf(
				test_state.current,
				1,
			))),
		)
		.await;

		let (tx, rx) = oneshot::channel();

		overseer_send(
			&mut virtual_overseer,
			AvailabilityRecoveryMessage::RecoverAvailableData(
				test_state.candidate.clone(),
				test_state.session_index,
				None,
				tx,
			),
		)
		.await;

		test_state.test_runtime_api_session_info(&mut virtual_overseer).await;
		test_state.respond_to_block_number_query(&mut virtual_overseer, 1).await;
		test_state.test_runtime_api_client_features(&mut virtual_overseer).await;
		test_state.respond_to_available_data_query(&mut virtual_overseer, false).await;
		test_state.respond_to_query_all_request(&mut virtual_overseer, |_| false).await;

		// Network errors are considered non-fatal for regular chunk recovery but should be retried
		// `REGULAR_CHUNKS_REQ_RETRY_THRESHOLD` times.
		for _ in 1..REGULAR_CHUNKS_REQ_RETRY_THRESHOLD {
			test_state
				.test_chunk_requests(
					test_state.candidate.hash(),
					&mut virtual_overseer,
					test_state.chunks.len(),
					|_| Has::timeout(),
					false,
				)
				.await;
		}

		if should_fail {
			test_state
				.test_chunk_requests(
					test_state.candidate.hash(),
					&mut virtual_overseer,
					test_state.chunks.len(),
					|_| Has::timeout(),
					false,
				)
				.await;

			assert_eq!(rx.await.unwrap().unwrap_err(), RecoveryError::Unavailable);
		} else {
			test_state
				.test_chunk_requests(
					test_state.candidate.hash(),
					&mut virtual_overseer,
					test_state.threshold(),
					|_| Has::Yes,
					false,
				)
				.await;

			assert_eq!(rx.await.unwrap().unwrap(), test_state.available_data);
		}

		virtual_overseer
	});
}
