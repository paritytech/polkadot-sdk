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

use crate::core::network::{HandleNetworkMessage, NetworkMessage};
use futures::{channel::oneshot, FutureExt};
use parity_scale_codec::Encode;
use polkadot_node_network_protocol::request_response::{
	v1::{AvailableDataFetchingResponse, ChunkFetchingResponse, ChunkResponse},
	Requests,
};
use polkadot_node_primitives::{AvailableData, ErasureChunk};
use polkadot_node_subsystem::{
	messages::AvailabilityStoreMessage, overseer, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_types::OverseerSignal;
use polkadot_primitives::CandidateHash;
use sc_network::ProtocolName;
use std::collections::HashMap;

pub struct AvailabilityStoreState {
	candidate_hashes: HashMap<CandidateHash, usize>,
	chunks: Vec<Vec<ErasureChunk>>,
}

const LOG_TARGET: &str = "subsystem-bench::av-store-mock";

/// Mockup helper. Contains Ccunks and full availability data of all parachain blocks
/// used in a test.
pub struct NetworkAvailabilityState {
	pub candidate_hashes: HashMap<CandidateHash, usize>,
	pub available_data: Vec<AvailableData>,
	pub chunks: Vec<Vec<ErasureChunk>>,
}

// Implement access to the state.
impl HandleNetworkMessage for NetworkAvailabilityState {
	fn handle(
		&self,
		message: NetworkMessage,
		_node_sender: &mut futures::channel::mpsc::UnboundedSender<NetworkMessage>,
	) -> Option<NetworkMessage> {
		match message {
			NetworkMessage::RequestFromNode(peer, request) => match request {
				Requests::ChunkFetchingV1(outgoing_request) => {
					gum::debug!(target: LOG_TARGET, request = ?outgoing_request, "Received `RequestFromNode`");
					let validator_index: usize = outgoing_request.payload.index.0 as usize;
					let candidate_hash = outgoing_request.payload.candidate_hash;

					let candidate_index = self
						.candidate_hashes
						.get(&candidate_hash)
						.expect("candidate was generated previously; qed");
					gum::warn!(target: LOG_TARGET, ?candidate_hash, candidate_index, "Candidate mapped to index");

					let chunk: ChunkResponse =
						self.chunks.get(*candidate_index).unwrap()[validator_index].clone().into();
					let response = Ok((
						ChunkFetchingResponse::from(Some(chunk)).encode(),
						ProtocolName::Static("dummy"),
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
						ProtocolName::Static("dummy"),
					));
					outgoing_request
						.pending_response
						.send(response)
						.expect("Response is always sent succesfully");
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
		candidate_hashes: HashMap<CandidateHash, usize>,
	) -> MockAvailabilityStore {
		Self { state: AvailabilityStoreState { chunks, candidate_hashes } }
	}

	async fn respond_to_query_all_request(
		&self,
		candidate_hash: CandidateHash,
		send_chunk: impl Fn(usize) -> bool,
		tx: oneshot::Sender<Vec<ErasureChunk>>,
	) {
		let candidate_index = self
			.state
			.candidate_hashes
			.get(&candidate_hash)
			.expect("candidate was generated previously; qed");
		gum::debug!(target: LOG_TARGET, ?candidate_hash, candidate_index, "Candidate mapped to index");

		let v = self
			.state
			.chunks
			.get(*candidate_index)
			.unwrap()
			.iter()
			.filter(|c| send_chunk(c.index.0 as usize))
			.cloned()
			.collect();

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
						self.respond_to_query_all_request(candidate_hash, |index| index == 0, tx)
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

						let chunk_size =
							self.state.chunks.get(*candidate_index).unwrap()[0].encoded_size();
						let _ = tx.send(Some(chunk_size));
					},
					AvailabilityStoreMessage::StoreChunk { candidate_hash, chunk, tx } => {
						gum::debug!(target: LOG_TARGET, chunk_index = ?chunk.index ,candidate_hash = ?candidate_hash, "Responding to StoreChunk");
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
