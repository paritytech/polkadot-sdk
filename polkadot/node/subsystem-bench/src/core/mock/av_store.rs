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
//!
//! A generic av store subsystem mockup suitable to be used in benchmarks.

use parity_scale_codec::Encode;
use polkadot_primitives::CandidateHash;

use std::collections::HashMap;

use futures::{channel::oneshot, FutureExt};

use polkadot_node_primitives::ErasureChunk;

use polkadot_node_subsystem::{
	messages::AvailabilityStoreMessage, overseer, SpawnedSubsystem, SubsystemError,
};

use polkadot_node_subsystem_types::OverseerSignal;

pub struct AvailabilityStoreState {
	candidate_hashes: HashMap<CandidateHash, usize>,
	chunks: Vec<Vec<ErasureChunk>>,
}

const LOG_TARGET: &str = "subsystem-bench::av-store-mock";

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
					_ => {
						unimplemented!("Unexpected av-store message")
					},
				},
			}
		}
	}
}
