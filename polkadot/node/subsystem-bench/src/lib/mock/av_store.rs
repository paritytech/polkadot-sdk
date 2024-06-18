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

//! A generic av store subsystem mockup suitable to be used in benchmarks.

use crate::network::{HandleNetworkMessage, NetworkMessage};
use codec::Encode;
use futures::{channel::oneshot, FutureExt};
use polkadot_node_network_protocol::request_response::{
	v1::AvailableDataFetchingResponse, v2::ChunkFetchingResponse, Protocol, ReqProtocolNames,
	Requests,
};
use polkadot_node_primitives::{AvailableData, ErasureChunk};
use polkadot_node_subsystem::{
	messages::AvailabilityStoreMessage, overseer, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_types::OverseerSignal;
use polkadot_primitives::{CandidateHash, ChunkIndex, CoreIndex, ValidatorIndex};
use std::collections::HashMap;

pub struct AvailabilityStoreState {
	candidate_hashes: HashMap<CandidateHash, usize>,
	chunks: Vec<Vec<ErasureChunk>>,
	chunk_indices: Vec<Vec<ChunkIndex>>,
	candidate_hash_to_core_index: HashMap<CandidateHash, CoreIndex>,
}

const LOG_TARGET: &str = "subsystem-bench::av-store-mock";

/// Mockup helper. Contains Chunks and full availability data of all parachain blocks
/// used in a test.
#[derive(Clone)]
pub struct NetworkAvailabilityState {
	pub req_protocol_names: ReqProtocolNames,
	pub candidate_hashes: HashMap<CandidateHash, usize>,
	pub available_data: Vec<AvailableData>,
	pub chunks: Vec<Vec<ErasureChunk>>,
	pub chunk_indices: Vec<Vec<ChunkIndex>>,
	pub candidate_hash_to_core_index: HashMap<CandidateHash, CoreIndex>,
}

// Implement access to the state.
#[async_trait::async_trait]
impl HandleNetworkMessage for NetworkAvailabilityState {
	async fn handle(
		&self,
		message: NetworkMessage,
		_node_sender: &mut futures::channel::mpsc::UnboundedSender<NetworkMessage>,
	) -> Option<NetworkMessage> {
		match message {
			NetworkMessage::RequestFromNode(peer, request) => match request {
				Requests::ChunkFetching(outgoing_request) => {
					gum::debug!(target: LOG_TARGET, request = ?outgoing_request, "Received `RequestFromNode`");
					let validator_index: usize = outgoing_request.payload.index.0 as usize;
					let candidate_hash = outgoing_request.payload.candidate_hash;

					let candidate_index = self
						.candidate_hashes
						.get(&candidate_hash)
						.expect("candidate was generated previously; qed");
					gum::warn!(target: LOG_TARGET, ?candidate_hash, candidate_index, "Candidate mapped to index");

					let candidate_chunks = self.chunks.get(*candidate_index).unwrap();
					let chunk_indices = self
						.chunk_indices
						.get(
							self.candidate_hash_to_core_index.get(&candidate_hash).unwrap().0
								as usize,
						)
						.unwrap();

					let chunk = candidate_chunks
						.get(chunk_indices.get(validator_index).unwrap().0 as usize)
						.unwrap();

					let response = Ok((
						ChunkFetchingResponse::from(Some(chunk.clone())).encode(),
						self.req_protocol_names.get_name(Protocol::ChunkFetchingV2),
					));

					if let Err(err) = outgoing_request.pending_response.send(response) {
						gum::error!(target: LOG_TARGET, ?err, "Failed to send `ChunkFetchingResponse`");
					}

					None
				},
				Requests::AvailableDataFetchingV1(outgoing_request) => {
					let candidate_hash = outgoing_request.payload.candidate_hash;
					let candidate_index = self
						.candidate_hashes
						.get(&candidate_hash)
						.expect("candidate was generated previously; qed");
					gum::debug!(target: LOG_TARGET, ?candidate_hash, candidate_index, "Candidate mapped to index");

					let available_data = self.available_data.get(*candidate_index).unwrap().clone();

					let response = Ok((
						AvailableDataFetchingResponse::from(Some(available_data)).encode(),
						self.req_protocol_names.get_name(Protocol::AvailableDataFetchingV1),
					));
					outgoing_request
						.pending_response
						.send(response)
						.expect("Response is always sent successfully");
					None
				},
				_ => Some(NetworkMessage::RequestFromNode(peer, request)),
			},

			message => Some(message),
		}
	}
}

/// A mock of the availability store subsystem. This one also generates all the
/// candidates that a
pub struct MockAvailabilityStore {
	state: AvailabilityStoreState,
}

impl MockAvailabilityStore {
	pub fn new(
		chunks: Vec<Vec<ErasureChunk>>,
		chunk_indices: Vec<Vec<ChunkIndex>>,
		candidate_hashes: HashMap<CandidateHash, usize>,
		candidate_hash_to_core_index: HashMap<CandidateHash, CoreIndex>,
	) -> MockAvailabilityStore {
		Self {
			state: AvailabilityStoreState {
				chunks,
				candidate_hashes,
				chunk_indices,
				candidate_hash_to_core_index,
			},
		}
	}

	async fn respond_to_query_all_request(
		&self,
		candidate_hash: CandidateHash,
		send_chunk: impl Fn(ValidatorIndex) -> bool,
		tx: oneshot::Sender<Vec<(ValidatorIndex, ErasureChunk)>>,
	) {
		let candidate_index = self
			.state
			.candidate_hashes
			.get(&candidate_hash)
			.expect("candidate was generated previously; qed");
		gum::debug!(target: LOG_TARGET, ?candidate_hash, candidate_index, "Candidate mapped to index");

		let n_validators = self.state.chunks[0].len();
		let candidate_chunks = self.state.chunks.get(*candidate_index).unwrap();
		let core_index = self.state.candidate_hash_to_core_index.get(&candidate_hash).unwrap();
		// We'll likely only send our chunk, so use capacity 1.
		let mut v = Vec::with_capacity(1);

		for validator_index in 0..n_validators {
			if !send_chunk(ValidatorIndex(validator_index as u32)) {
				continue;
			}
			let chunk_index = self
				.state
				.chunk_indices
				.get(core_index.0 as usize)
				.unwrap()
				.get(validator_index)
				.unwrap();

			let chunk = candidate_chunks.get(chunk_index.0 as usize).unwrap().clone();
			v.push((ValidatorIndex(validator_index as u32), chunk.clone()));
		}

		let _ = tx.send(v);
	}
}

#[overseer::subsystem(AvailabilityStore, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockAvailabilityStore {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "test-environment", future }
	}
}

#[overseer::contextbounds(AvailabilityStore, prefix = self::overseer)]
impl MockAvailabilityStore {
	async fn run<Context>(self, mut ctx: Context) {
		gum::debug!(target: LOG_TARGET, "Subsystem running");
		loop {
			let msg = ctx.recv().await.expect("Overseer never fails us");

			match msg {
				orchestra::FromOrchestra::Signal(signal) =>
					if signal == OverseerSignal::Conclude {
						return
					},
				orchestra::FromOrchestra::Communication { msg } => match msg {
					AvailabilityStoreMessage::QueryAvailableData(candidate_hash, tx) => {
						gum::debug!(target: LOG_TARGET, candidate_hash = ?candidate_hash, "Responding to QueryAvailableData");

						// We never have the full available data.
						let _ = tx.send(None);
					},
					AvailabilityStoreMessage::QueryAllChunks(candidate_hash, tx) => {
						// We always have our own chunk.
						gum::debug!(target: LOG_TARGET, candidate_hash = ?candidate_hash, "Responding to QueryAllChunks");
						self.respond_to_query_all_request(
							candidate_hash,
							|index| index == 0.into(),
							tx,
						)
						.await;
					},
					AvailabilityStoreMessage::QueryChunkSize(candidate_hash, tx) => {
						gum::debug!(target: LOG_TARGET, candidate_hash = ?candidate_hash, "Responding to QueryChunkSize");

						let candidate_index = self
							.state
							.candidate_hashes
							.get(&candidate_hash)
							.expect("candidate was generated previously; qed");
						gum::debug!(target: LOG_TARGET, ?candidate_hash, candidate_index, "Candidate mapped to index");

						let chunk_size = self
							.state
							.chunks
							.get(*candidate_index)
							.unwrap()
							.first()
							.unwrap()
							.encoded_size();
						let _ = tx.send(Some(chunk_size));
					},
					AvailabilityStoreMessage::StoreChunk {
						candidate_hash,
						chunk,
						tx,
						validator_index,
					} => {
						gum::debug!(
							target: LOG_TARGET,
							chunk_index = ?chunk.index,
							validator_index = ?validator_index,
							candidate_hash = ?candidate_hash,
							"Responding to StoreChunk"
						);
						let _ = tx.send(Ok(()));
					},
					_ => {
						unimplemented!("Unexpected av-store message")
					},
				},
			}
		}
	}
}
