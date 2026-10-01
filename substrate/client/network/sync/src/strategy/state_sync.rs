// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! State sync support.

use crate::{
	schema::v1::{KeyValueStateEntry, StateEntry, StateRequest, StateResponse},
	LOG_TARGET,
};
use codec::{Decode, Encode};
use log::{debug, warn};
use sc_client_api::{CompactProof, PartialStateImport, ProofProvider};
use sc_consensus::ImportedState;
use smallvec::SmallVec;
use sp_core::storage::well_known_keys;
use sp_runtime::{
	traits::{Block as BlockT, Header, NumberFor, PartialStateFor},
	Justifications,
};
use std::{collections::HashMap, fmt, sync::Arc};

/// Consecutive database write failures tolerated before state sync gives up on the target.
const MAX_PARTIAL_STATE_WRITE_FAILURES: u32 = 3;

/// Generic state sync provider. Used for mocking in tests.
pub trait StateSyncProvider<B: BlockT>: Send + Sync {
	/// Validate and import a state response.
	fn import(&mut self, response: StateResponse) -> ImportResult<B>;
	/// Produce next state request.
	fn next_request(&self) -> StateRequest;
	/// Check if the state is complete.
	fn is_complete(&self) -> bool;
	/// Returns target block number.
	fn target_number(&self) -> NumberFor<B>;
	/// Returns target block hash.
	fn target_hash(&self) -> B::Hash;
	/// Returns state sync estimated progress.
	fn progress(&self) -> StateSyncProgress;
}

// Reported state sync phase.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum StateSyncPhase {
	// State download in progress.
	DownloadingState,
	// Download is complete, state is being imported.
	ImportingState,
}

impl fmt::Display for StateSyncPhase {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Self::DownloadingState => write!(f, "Downloading state"),
			Self::ImportingState => write!(f, "Importing state"),
		}
	}
}

/// Reported state download progress.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct StateSyncProgress {
	/// Estimated download percentage.
	pub percentage: u32,
	/// Total state size in bytes downloaded so far.
	pub size: u64,
	/// Current state sync phase.
	pub phase: StateSyncPhase,
}

/// Import state chunk result.
pub enum ImportResult<B: BlockT> {
	/// State is complete and ready for import.
	Import(B::Hash, B::Header, ImportedState<B>, Option<Vec<B::Extrinsic>>, Option<Justifications>),
	/// Continue downloading.
	Continue,
	/// Bad state chunk.
	BadResponse,
	/// The chunk could not be written to the database repeatedly. Give up on this target.
	Abort,
}

struct StateSyncMetadata<B: BlockT> {
	last_key: SmallVec<[Vec<u8>; 2]>,
	target_header: B::Header,
	target_body: Option<Vec<B::Extrinsic>>,
	target_justifications: Option<Justifications>,
	complete: bool,
	imported_bytes: u64,
	skip_proof: bool,
	/// Consecutive failures of `import_partial_state`, reset by the next successful write.
	write_failures: u32,
}

impl<B: BlockT> StateSyncMetadata<B> {
	fn target_hash(&self) -> B::Hash {
		self.target_header.hash()
	}

	/// Returns target block number.
	fn target_number(&self) -> NumberFor<B> {
		*self.target_header.number()
	}

	fn target_root(&self) -> B::Hash {
		*self.target_header.state_root()
	}

	fn next_request(&self) -> StateRequest {
		StateRequest {
			block: self.target_hash().encode(),
			start: self.last_key.clone().into_vec(),
			no_proof: self.skip_proof,
		}
	}

	fn progress(&self) -> StateSyncProgress {
		let cursor = *self.last_key.get(0).and_then(|last| last.get(0)).unwrap_or(&0u8);
		let percent_done = cursor as u32 * 100 / 256;
		StateSyncProgress {
			percentage: percent_done,
			size: self.imported_bytes,
			phase: if self.complete {
				StateSyncPhase::ImportingState
			} else {
				StateSyncPhase::DownloadingState
			},
		}
	}
}

/// State sync state machine.
///
/// With proofs, every verified chunk is written to the database as it arrives, and the cursor
/// moves only once that write succeeded. Without proofs, the key-values are accumulated in memory
/// until the whole state has been downloaded.
pub struct StateSync<B: BlockT, Client> {
	metadata: StateSyncMetadata<B>,
	state: HashMap<Vec<u8>, (Vec<(Vec<u8>, Vec<u8>)>, Vec<Vec<u8>>)>,
	client: Arc<Client>,
}

impl<B, Client> StateSync<B, Client>
where
	B: BlockT,
	Client: ProofProvider<B> + PartialStateImport<B> + Send + Sync + 'static,
{
	///  Create a new instance.
	pub fn new(
		client: Arc<Client>,
		target_header: B::Header,
		target_body: Option<Vec<B::Extrinsic>>,
		target_justifications: Option<Justifications>,
		skip_proof: bool,
	) -> Self {
		Self {
			client,
			metadata: StateSyncMetadata {
				last_key: SmallVec::default(),
				target_header,
				target_body,
				target_justifications,
				complete: false,
				imported_bytes: 0,
				skip_proof,
				write_failures: 0,
			},
			state: HashMap::default(),
		}
	}

	fn process_state_key_values(
		&mut self,
		state_root: Vec<u8>,
		key_values: impl IntoIterator<Item = (Vec<u8>, Vec<u8>)>,
	) {
		let is_top = state_root.is_empty();

		let entry = self.state.entry(state_root).or_default();

		if entry.0.len() > 0 && entry.1.len() > 1 {
			// Already imported child_trie with same root.
			// Warning this will not work with parallel download.
			return;
		}

		let mut child_storage_roots = Vec::new();

		for (key, value) in key_values {
			// Skip all child key root (will be recalculated on import)
			if is_top && well_known_keys::is_child_storage_key(key.as_slice()) {
				child_storage_roots.push((value, key));
			} else {
				self.metadata.imported_bytes += key.len() as u64;
				entry.0.push((key, value));
			}
		}

		for (root, storage_key) in child_storage_roots {
			self.state.entry(root).or_default().1.push(storage_key);
		}
	}

	fn process_state_unverified(&mut self, response: StateResponse) -> bool {
		let mut complete = true;
		// if the trie is a child trie and one of its parent trie is empty,
		// the parent cursor stays valid.
		// Empty parent trie content only happens when all the response content
		// is part of a single child trie.
		let first_entry_is_empty =
			response.entries.first().is_some_and(|entry| entry.entries.is_empty());
		if self.metadata.last_key.len() == 2 && first_entry_is_empty {
			// Do not remove the parent trie position.
			self.metadata.last_key.pop();
		} else {
			self.metadata.last_key.clear();
		}
		for state in response.entries {
			debug!(
				target: LOG_TARGET,
				"Importing state from {:?} to {:?}",
				state.entries.last().map(|e| sp_core::hexdisplay::HexDisplay::from(&e.key)),
				state.entries.first().map(|e| sp_core::hexdisplay::HexDisplay::from(&e.key)),
			);

			if !state.complete {
				if let Some(e) = state.entries.last() {
					self.metadata.last_key.push(e.key.clone());
				}
				complete = false;
			}

			let KeyValueStateEntry { state_root, entries, complete: _ } = state;
			self.process_state_key_values(
				state_root,
				entries.into_iter().map(|StateEntry { key, value }| (key, value)),
			);
		}
		complete
	}
}

impl<B, Client> StateSyncProvider<B> for StateSync<B, Client>
where
	B: BlockT,
	Client: ProofProvider<B> + PartialStateImport<B> + Send + Sync + 'static,
{
	///  Validate and import a state response.
	fn import(&mut self, response: StateResponse) -> ImportResult<B> {
		if self.metadata.skip_proof && response.entries.is_empty() {
			debug!(target: LOG_TARGET, "Missing state entries");
			return ImportResult::BadResponse;
		} else if !self.metadata.skip_proof && response.proof.is_empty() {
			debug!(target: LOG_TARGET, "Missing proof");
			return ImportResult::BadResponse;
		}
		let complete = if !self.metadata.skip_proof {
			debug!(target: LOG_TARGET, "Importing state from {} trie nodes", response.proof.len());
			let proof_size = response.proof.len() as u64;
			let proof = match CompactProof::decode(&mut response.proof.as_ref()) {
				Ok(proof) => proof,
				Err(e) => {
					debug!(target: LOG_TARGET, "Error decoding proof: {:?}", e);
					return ImportResult::BadResponse;
				},
			};
			let (values, completed, nodes) = match self.client.verify_range_proof(
				self.metadata.target_root(),
				proof,
				self.metadata.last_key.as_slice(),
			) {
				Err(e) => {
					debug!(
						target: LOG_TARGET,
						"StateResponse failed proof verification: {}",
						e,
					);
					return ImportResult::BadResponse;
				},
				Ok(values) => values,
			};
			debug!(target: LOG_TARGET, "Verified state response with {} keys", values.len());

			// Write the chunk before moving the cursor: a chunk that never reached the database
			// is asked for again, because `last_key` still points at its start.
			let partial_state = PartialStateFor::<B> {
				block_hash: self.metadata.target_hash(),
				block_number: self.metadata.target_number(),
				state_root: self.metadata.target_root(),
				nodes,
			};
			if let Err(e) = self.client.import_partial_state(partial_state) {
				self.metadata.write_failures += 1;
				warn!(
					target: LOG_TARGET,
					"Failed to write state chunk to the database ({}/{}): {e}",
					self.metadata.write_failures,
					MAX_PARTIAL_STATE_WRITE_FAILURES,
				);
				return if self.metadata.write_failures >= MAX_PARTIAL_STATE_WRITE_FAILURES {
					ImportResult::Abort
				} else {
					ImportResult::Continue
				};
			}
			self.metadata.write_failures = 0;

			let complete = completed == 0;
			if !complete && !values.update_last_key(completed, &mut self.metadata.last_key) {
				debug!(target: LOG_TARGET, "Error updating key cursor, depth: {}", completed);
			};

			self.metadata.imported_bytes += proof_size;
			complete
		} else {
			self.process_state_unverified(response)
		};
		if complete {
			self.metadata.complete = true;
			let target_hash = self.metadata.target_hash();
			let state = if self.metadata.skip_proof {
				ImportedState::KeyValues {
					block: target_hash,
					state: std::mem::take(&mut self.state).into(),
				}
			} else {
				// Every chunk is already in the database. The block import checks that the
				// state is complete and marks it as available.
				ImportedState::Proof
			};
			ImportResult::Import(
				target_hash,
				self.metadata.target_header.clone(),
				state,
				self.metadata.target_body.clone(),
				self.metadata.target_justifications.clone(),
			)
		} else {
			ImportResult::Continue
		}
	}

	/// Produce next state request.
	fn next_request(&self) -> StateRequest {
		self.metadata.next_request()
	}

	/// Check if the state is complete.
	fn is_complete(&self) -> bool {
		self.metadata.complete
	}

	/// Returns target block number.
	fn target_number(&self) -> NumberFor<B> {
		self.metadata.target_number()
	}

	/// Returns target block hash.
	fn target_hash(&self) -> B::Hash {
		self.metadata.target_hash()
	}

	/// Returns state sync estimated progress.
	fn progress(&self) -> StateSyncProgress {
		self.metadata.progress()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sc_client_api::{ChildInfo, DecodedCompactProof, StorageProof};
	use sp_runtime::traits::{HashingFor, Header as HeaderT};
	use sp_state_machine::{KeyValueStates, KeyValueStorageLevel};
	use std::sync::{
		atomic::{AtomicUsize, Ordering},
		Mutex,
	};
	use substrate_test_runtime_client::runtime::{Block, Header};

	/// A client whose range-proof verification returns a fixed answer and whose partial-state
	/// writes fail a configurable number of times before they start succeeding.
	struct StubClient {
		/// Key-values and the depth the range read stopped at, as `verify_range_proof` reports.
		verified: Mutex<(Vec<KeyValueStorageLevel>, usize)>,
		failing_writes: AtomicUsize,
		writes: AtomicUsize,
	}

	impl StubClient {
		fn new(
			key_values: Vec<KeyValueStorageLevel>,
			completed: usize,
			failing_writes: usize,
		) -> Arc<Self> {
			Arc::new(Self {
				verified: Mutex::new((key_values, completed)),
				failing_writes: AtomicUsize::new(failing_writes),
				writes: AtomicUsize::new(0),
			})
		}

		fn writes(&self) -> usize {
			self.writes.load(Ordering::SeqCst)
		}
	}

	impl ProofProvider<Block> for StubClient {
		fn read_proof(
			&self,
			_: <Block as BlockT>::Hash,
			_: &mut dyn Iterator<Item = &[u8]>,
		) -> sp_blockchain::Result<StorageProof> {
			unimplemented!()
		}

		fn read_child_proof(
			&self,
			_: <Block as BlockT>::Hash,
			_: &ChildInfo,
			_: &mut dyn Iterator<Item = &[u8]>,
		) -> sp_blockchain::Result<StorageProof> {
			unimplemented!()
		}

		fn execution_proof(
			&self,
			_: <Block as BlockT>::Hash,
			_: &str,
			_: &[u8],
		) -> sp_blockchain::Result<(Vec<u8>, StorageProof)> {
			unimplemented!()
		}

		fn read_proof_collection(
			&self,
			_: <Block as BlockT>::Hash,
			_: &[Vec<u8>],
			_: usize,
		) -> sp_blockchain::Result<(CompactProof, u32)> {
			unimplemented!()
		}

		fn storage_collection(
			&self,
			_: <Block as BlockT>::Hash,
			_: &[Vec<u8>],
			_: usize,
		) -> sp_blockchain::Result<Vec<(KeyValueStorageLevel, bool)>> {
			unimplemented!()
		}

		fn verify_range_proof(
			&self,
			_root: <Block as BlockT>::Hash,
			_proof: CompactProof,
			_start_keys: &[Vec<u8>],
		) -> sp_blockchain::Result<(KeyValueStates, usize, DecodedCompactProof<HashingFor<Block>>)>
		{
			let (key_values, completed) = &*self.verified.lock().unwrap();
			Ok((KeyValueStates(key_values.clone()), *completed, Default::default()))
		}
	}

	impl PartialStateImport<Block> for StubClient {
		fn import_partial_state(
			&self,
			partial_state: PartialStateFor<Block>,
		) -> sp_blockchain::Result<()> {
			assert_eq!(partial_state.block_hash, header().hash());
			let failing = self.failing_writes.load(Ordering::SeqCst);
			if failing > 0 {
				self.failing_writes.store(failing - 1, Ordering::SeqCst);
				return Err(sp_blockchain::Error::Storage("injected write failure".into()));
			}
			self.writes.fetch_add(1, Ordering::SeqCst);
			Ok(())
		}
	}

	fn header() -> Header {
		Header::new(
			1,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		)
	}

	fn state_sync(client: Arc<StubClient>) -> StateSync<Block, StubClient> {
		StateSync::new(client, header(), None, None, false)
	}

	/// A response whose proof decodes (to an empty compact proof); the stub never reads it.
	fn response() -> StateResponse {
		StateResponse { entries: vec![], proof: vec![0u8] }
	}

	/// One key in the top trie, with the range read stopped at depth 1: a chunk in the middle of
	/// the download.
	fn one_key_in_the_top_trie() -> Vec<KeyValueStorageLevel> {
		vec![KeyValueStorageLevel {
			state_root: vec![],
			parent_storage_keys: vec![],
			key_values: vec![(vec![0x80], vec![])],
		}]
	}

	#[test]
	fn failed_write_keeps_the_cursor_and_continues() {
		let client = StubClient::new(one_key_in_the_top_trie(), 1, 1);
		let mut sync = state_sync(client.clone());

		assert!(matches!(sync.import(response()), ImportResult::Continue));
		assert_eq!(sync.metadata.write_failures, 1);
		assert!(sync.next_request().start.is_empty(), "the same range is asked again");
		assert_eq!(client.writes(), 0);
		assert!(!sync.is_complete());
	}

	#[test]
	fn successful_write_moves_the_cursor_and_resets_the_failures() {
		let client = StubClient::new(one_key_in_the_top_trie(), 1, 1);
		let mut sync = state_sync(client.clone());

		assert!(matches!(sync.import(response()), ImportResult::Continue));
		assert!(matches!(sync.import(response()), ImportResult::Continue));
		assert_eq!(sync.metadata.write_failures, 0);
		assert_eq!(sync.next_request().start, vec![vec![0x80]]);
		assert_eq!(sync.progress().percentage, 50);
		assert_eq!(client.writes(), 1);
	}

	#[test]
	fn third_consecutive_failed_write_aborts() {
		let client = StubClient::new(one_key_in_the_top_trie(), 1, 3);
		let mut sync = state_sync(client.clone());

		assert!(matches!(sync.import(response()), ImportResult::Continue));
		assert!(matches!(sync.import(response()), ImportResult::Continue));
		assert!(matches!(sync.import(response()), ImportResult::Abort));
		assert!(sync.next_request().start.is_empty());
		assert_eq!(client.writes(), 0);
		assert!(!sync.is_complete());
	}

	#[test]
	fn last_chunk_is_written_before_the_target_import_is_requested() {
		let client = StubClient::new(vec![], 0, 0);
		let mut sync = state_sync(client.clone());

		match sync.import(response()) {
			ImportResult::Import(hash, _, ImportedState::Proof, _, _) => {
				assert_eq!(hash, header().hash())
			},
			_ => panic!("expected the target block import with the proof state"),
		}
		assert!(sync.is_complete());
		assert_eq!(client.writes(), 1);
	}

	#[test]
	fn failed_write_of_the_last_chunk_does_not_complete() {
		let client = StubClient::new(vec![], 0, 1);
		let mut sync = state_sync(client.clone());

		assert!(matches!(sync.import(response()), ImportResult::Continue));
		assert!(!sync.is_complete());
		assert!(matches!(
			sync.import(response()),
			ImportResult::Import(_, _, ImportedState::Proof, _, _)
		));
		assert!(sync.is_complete());
		assert_eq!(client.writes(), 1);
	}
}
