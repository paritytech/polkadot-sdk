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

use std::collections::HashMap;

use parity_scale_codec::Encode;

use futures::{
	channel::{mpsc, oneshot},
	executor, select,
	task::{noop_waker, Context, Poll},
	Future, FutureExt, StreamExt,
};
use rstest::rstest;

use sc_network::{self as network, ProtocolName};
use sp_keyring::Sr25519Keyring;

use polkadot_node_network_protocol::request_response::{
	v1::{self, ChunkResponse},
	Protocol, Recipient, ReqProtocolNames,
};
use polkadot_node_primitives::{BlockData, PoV, Proof};
use polkadot_node_subsystem::messages::AllMessages;
use polkadot_primitives::{CandidateHash, ChunkIndex, ValidatorIndex};

use super::*;
use crate::{metrics::Metrics, tests::mock::get_valid_chunk_data};

#[test]
fn task_can_be_canceled() {
	let req_protocol_names = ReqProtocolNames::new(&Hash::repeat_byte(0xff), None);
	let (task, _rx) = get_test_running_task(&req_protocol_names, 0.into(), 0.into());
	let (handle, kill) = oneshot::channel();
	std::mem::drop(handle);
	let running_task = task.run(kill);
	futures::pin_mut!(running_task);
	let waker = noop_waker();
	let mut ctx = Context::from_waker(&waker);
	assert!(running_task.poll(&mut ctx) == Poll::Ready(()), "Task is immediately finished");
}

/// Make sure task won't accept a chunk that has is invalid.
#[rstest]
#[case(Protocol::ChunkFetchingV1)]
#[case(Protocol::ChunkFetchingV2)]
fn task_does_not_accept_invalid_chunk(#[case] protocol: Protocol) {
	let req_protocol_names = ReqProtocolNames::new(&Hash::repeat_byte(0xff), None);
	let chunk_index = ChunkIndex(1);
	let validator_index = ValidatorIndex(0);
	let (mut task, rx) = get_test_running_task(&req_protocol_names, validator_index, chunk_index);
	let validators = vec![Sr25519Keyring::Alice.public().into()];
	task.group = validators;
	let protocol_name = req_protocol_names.get_name(protocol);
	let test = TestRun {
		chunk_responses: {
			[(
				Recipient::Authority(Sr25519Keyring::Alice.public().into()),
				get_response(
					protocol,
					protocol_name.clone(),
					Some((
						vec![1, 2, 3],
						Proof::try_from(vec![vec![9, 8, 2], vec![2, 3, 4]]).unwrap(),
						chunk_index,
					)),
				),
			)]
			.into_iter()
			.collect()
		},
		valid_chunks: HashSet::new(),
		req_protocol_names,
	};
	test.run(task, rx);
}

#[rstest]
#[case(Protocol::ChunkFetchingV1)]
#[case(Protocol::ChunkFetchingV2)]
fn task_stores_valid_chunk(#[case] protocol: Protocol) {
	let req_protocol_names = ReqProtocolNames::new(&Hash::repeat_byte(0xff), None);
	// In order for protocol version 1 to work, the chunk index needs to be equal to the validator
	// index.
	let chunk_index = ChunkIndex(0);
	let validator_index =
		if protocol == Protocol::ChunkFetchingV1 { ValidatorIndex(0) } else { ValidatorIndex(1) };
	let (mut task, rx) = get_test_running_task(&req_protocol_names, validator_index, chunk_index);
	let validators = vec![Sr25519Keyring::Alice.public().into()];
	let pov = PoV { block_data: BlockData(vec![45, 46, 47]) };
	let (root_hash, chunk) = get_valid_chunk_data(pov, 10, chunk_index);
	task.erasure_root = root_hash;
	task.group = validators;
	let protocol_name = req_protocol_names.get_name(protocol);

	let test = TestRun {
		chunk_responses: {
			[(
				Recipient::Authority(Sr25519Keyring::Alice.public().into()),
				get_response(
					protocol,
					protocol_name.clone(),
					Some((chunk.chunk.clone(), chunk.proof, chunk_index)),
				),
			)]
			.into_iter()
			.collect()
		},
		valid_chunks: [(chunk.chunk)].into_iter().collect(),
		req_protocol_names,
	};
	test.run(task, rx);
}

#[rstest]
#[case(Protocol::ChunkFetchingV1)]
#[case(Protocol::ChunkFetchingV2)]
fn task_does_not_accept_wrongly_indexed_chunk(#[case] protocol: Protocol) {
	let req_protocol_names = ReqProtocolNames::new(&Hash::repeat_byte(0xff), None);
	// In order for protocol version 1 to work, the chunk index needs to be equal to the validator
	// index.
	let chunk_index = ChunkIndex(0);
	let validator_index =
		if protocol == Protocol::ChunkFetchingV1 { ValidatorIndex(0) } else { ValidatorIndex(1) };
	let (mut task, rx) = get_test_running_task(&req_protocol_names, validator_index, chunk_index);

	let validators = vec![Sr25519Keyring::Alice.public().into()];
	let pov = PoV { block_data: BlockData(vec![45, 46, 47]) };
	let (_, other_chunk) = get_valid_chunk_data(pov.clone(), 10, ChunkIndex(3));
	let (root_hash, chunk) = get_valid_chunk_data(pov, 10, ChunkIndex(0));
	task.erasure_root = root_hash;
	task.request.index = chunk.index.into();
	task.group = validators;
	let protocol_name = req_protocol_names.get_name(protocol);

	let test = TestRun {
		chunk_responses: {
			[(
				Recipient::Authority(Sr25519Keyring::Alice.public().into()),
				get_response(
					protocol,
					protocol_name.clone(),
					Some((other_chunk.chunk.clone(), chunk.proof, other_chunk.index)),
				),
			)]
			.into_iter()
			.collect()
		},
		valid_chunks: HashSet::new(),
		req_protocol_names,
	};
	test.run(task, rx);
}

/// Task stores chunk, if there is at least one validator having a valid chunk.
#[rstest]
#[case(Protocol::ChunkFetchingV1)]
#[case(Protocol::ChunkFetchingV2)]
fn task_stores_valid_chunk_if_there_is_one(#[case] protocol: Protocol) {
	let req_protocol_names = ReqProtocolNames::new(&Hash::repeat_byte(0xff), None);
	// In order for protocol version 1 to work, the chunk index needs to be equal to the validator
	// index.
	let chunk_index = ChunkIndex(1);
	let validator_index =
		if protocol == Protocol::ChunkFetchingV1 { ValidatorIndex(1) } else { ValidatorIndex(2) };
	let (mut task, rx) = get_test_running_task(&req_protocol_names, validator_index, chunk_index);
	let pov = PoV { block_data: BlockData(vec![45, 46, 47]) };

	let validators = [
		// Only Alice has valid chunk - should succeed, even though she is tried last.
		Sr25519Keyring::Alice,
		Sr25519Keyring::Bob,
		Sr25519Keyring::Charlie,
		Sr25519Keyring::Dave,
		Sr25519Keyring::Eve,
	]
	.iter()
	.map(|v| v.public().into())
	.collect::<Vec<_>>();

	let (root_hash, chunk) = get_valid_chunk_data(pov, 10, chunk_index);
	task.erasure_root = root_hash;
	task.group = validators;
	let protocol_name = req_protocol_names.get_name(protocol);

	let test = TestRun {
		chunk_responses: {
			[
				(
					Recipient::Authority(Sr25519Keyring::Alice.public().into()),
					get_response(
						protocol,
						protocol_name.clone(),
						Some((chunk.chunk.clone(), chunk.proof, chunk_index)),
					),
				),
				(
					Recipient::Authority(Sr25519Keyring::Bob.public().into()),
					get_response(protocol, protocol_name.clone(), None),
				),
				(
					Recipient::Authority(Sr25519Keyring::Charlie.public().into()),
					get_response(
						protocol,
						protocol_name.clone(),
						Some((
							vec![1, 2, 3],
							Proof::try_from(vec![vec![9, 8, 2], vec![2, 3, 4]]).unwrap(),
							chunk_index,
						)),
					),
				),
			]
			.into_iter()
			.collect()
		},
		valid_chunks: [(chunk.chunk)].into_iter().collect(),
		req_protocol_names,
	};
	test.run(task, rx);
}

struct TestRun {
	/// Response to deliver for a given validator index.
	/// None means, answer with `NetworkError`.
	chunk_responses: HashMap<Recipient, (Vec<u8>, ProtocolName)>,
	/// Set of chunks that should be considered valid:
	valid_chunks: HashSet<Vec<u8>>,
	/// Request protocol names
	req_protocol_names: ReqProtocolNames,
}

impl TestRun {
	fn run(self, task: RunningTask, rx: mpsc::Receiver<FromFetchTask>) {
		sp_tracing::init_for_tests();
		let mut rx = rx.fuse();
		let task = task.run_inner().fuse();
		futures::pin_mut!(task);
		executor::block_on(async {
			let mut end_ok = false;
			loop {
				let msg = select!(
					from_task = rx.next() => {
						match from_task {
							Some(msg) => msg,
							None => break,
						}
					},
					() = task =>
						break,
				);
				match msg {
					FromFetchTask::Concluded(_) => break,
					FromFetchTask::Failed(_) => break,
					FromFetchTask::Message(msg) => end_ok = self.handle_message(msg).await,
				}
			}
			if !end_ok {
				panic!("Task ended prematurely (failed to store valid chunk)!");
			}
		});
	}

	/// Returns true, if after processing of the given message it would be OK for the stream to
	/// end.
	async fn handle_message(
		&self,
		msg: overseer::AvailabilityDistributionOutgoingMessages,
	) -> bool {
		let msg = AllMessages::from(msg);
		match msg {
			AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendRequests(
				reqs,
				IfDisconnected::ImmediateError,
			)) => {
				let mut valid_responses = 0;
				for req in reqs {
					let req = match req {
						Requests::ChunkFetching(req) => req,
						_ => panic!("Unexpected request"),
					};
					let response =
						self.chunk_responses.get(&req.peer).ok_or(network::RequestFailure::Refused);

					if let Ok((resp, protocol)) = response {
						let chunk = if protocol ==
							&self.req_protocol_names.get_name(Protocol::ChunkFetchingV1)
						{
							Into::<Option<v1::ChunkResponse>>::into(
								v1::ChunkFetchingResponse::decode(&mut &resp[..]).unwrap(),
							)
							.map(|c| c.chunk)
						} else if protocol ==
							&self.req_protocol_names.get_name(Protocol::ChunkFetchingV2)
						{
							Into::<Option<ErasureChunk>>::into(
								v2::ChunkFetchingResponse::decode(&mut &resp[..]).unwrap(),
							)
							.map(|c| c.chunk)
						} else {
							unreachable!()
						};

						if let Some(chunk) = chunk {
							if self.valid_chunks.contains(&chunk) {
								valid_responses += 1;
							}
						}

						req.pending_response
							.send(response.cloned())
							.expect("Sending response should succeed");
					}
				}
				return (valid_responses == 0) && self.valid_chunks.is_empty()
			},
			AllMessages::AvailabilityStore(AvailabilityStoreMessage::StoreChunk {
				chunk,
				tx,
				..
			}) => {
				assert!(self.valid_chunks.contains(&chunk.chunk));
				tx.send(Ok(())).expect("Answering fetching task should work");
				return true
			},
			_ => {
				gum::debug!(target: LOG_TARGET, "Unexpected message");
				return false
			},
		}
	}
}

/// Get a `RunningTask` filled with (mostly) dummy values.
fn get_test_running_task(
	req_protocol_names: &ReqProtocolNames,
	validator_index: ValidatorIndex,
	chunk_index: ChunkIndex,
) -> (RunningTask, mpsc::Receiver<FromFetchTask>) {
	let (tx, rx) = mpsc::channel(0);

	(
		RunningTask {
			session_index: 0,
			group_index: GroupIndex(0),
			group: Vec::new(),
			request: v2::ChunkFetchingRequest {
				candidate_hash: CandidateHash([43u8; 32].into()),
				index: validator_index,
			},
			erasure_root: Hash::repeat_byte(99),
			relay_parent: Hash::repeat_byte(71),
			sender: tx,
			metrics: Metrics::new_dummy(),
			span: jaeger::Span::Disabled,
			req_v1_protocol_name: req_protocol_names.get_name(Protocol::ChunkFetchingV1),
			req_v2_protocol_name: req_protocol_names.get_name(Protocol::ChunkFetchingV2),
			chunk_index,
		},
		rx,
	)
}

/// Make a versioned ChunkFetchingResponse.
fn get_response(
	protocol: Protocol,
	protocol_name: ProtocolName,
	chunk: Option<(Vec<u8>, Proof, ChunkIndex)>,
) -> (Vec<u8>, ProtocolName) {
	(
		match protocol {
			Protocol::ChunkFetchingV1 => if let Some((chunk, proof, _)) = chunk {
				v1::ChunkFetchingResponse::Chunk(ChunkResponse { chunk, proof })
			} else {
				v1::ChunkFetchingResponse::NoSuchChunk
			}
			.encode(),
			Protocol::ChunkFetchingV2 => if let Some((chunk, proof, index)) = chunk {
				v2::ChunkFetchingResponse::Chunk(ErasureChunk { chunk, index, proof })
			} else {
				v2::ChunkFetchingResponse::NoSuchChunk
			}
			.encode(),
			_ => unreachable!(),
		},
		protocol_name,
	)
}
