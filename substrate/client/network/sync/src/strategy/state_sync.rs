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
	schema::v1::{StateRequest, StateResponse},
	LOG_TARGET,
};
use codec::{Decode, Encode};
use log::{debug, warn};
use sc_client_api::{
	partial_state::PartialStateTarget, CompactProof, PartialStateImport, ProofProvider,
};
use smallvec::SmallVec;
use sp_runtime::{
	traits::{Block as BlockT, Header, NumberFor, PartialStateFor},
	Justifications,
};
use std::{fmt, sync::Arc};

/// Consecutive database write failures tolerated before state sync gives up on the target.
const MAX_PARTIAL_STATE_WRITE_FAILURES: u32 = 3;

/// How far behind the finalized head the target of an interrupted state sync may be for the
/// download to resume after a restart: a quarter of the 256 blocks of state that peers keep by
/// default (`sc_state_db::DEFAULT_MAX_BLOCK_CONSTRAINT`), so that the rest of the download has
/// time before they prune the target. An older target is discarded and a new one is chosen.
pub const MAX_RESUME_DISTANCE: u32 = 64;

/// Whether the target of an interrupted state sync, at `stored`, is close enough behind the
/// finalized head `head` to resume the download, see [`MAX_RESUME_DISTANCE`].
pub fn is_resumable<B: BlockT>(stored: NumberFor<B>, head: NumberFor<B>) -> bool {
	stored <= head && head - stored <= MAX_RESUME_DISTANCE.into()
}

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
	/// The whole state is in the database: import the target block.
	Import(B::Hash, B::Header, Option<Vec<B::Extrinsic>>, Option<Justifications>),
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
			// Proofs are mandatory. The field stays for peers on older releases.
			no_proof: false,
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
/// Every verified chunk is written to the database as it arrives, and the cursor moves only once
/// that write succeeded. Nothing of the state is kept in memory between two responses.
pub struct StateSync<B: BlockT, Client> {
	metadata: StateSyncMetadata<B>,
	client: Arc<Client>,
}

impl<B, Client> StateSync<B, Client>
where
	B: BlockT,
	Client: ProofProvider<B> + PartialStateImport<B> + Send + Sync + 'static,
{
	/// Create a new instance. The client records the target, so that a restart can resume the
	/// download, and discards an earlier attempt that never completed. On `Err` nothing was
	/// recorded and the state sync must not start.
	pub fn new(
		client: Arc<Client>,
		target_header: B::Header,
		target_body: Option<Vec<B::Extrinsic>>,
		target_justifications: Option<Justifications>,
	) -> sp_blockchain::Result<Self> {
		client.begin_partial_state(PartialStateTarget {
			header: target_header.clone(),
			body: target_body.clone(),
			justifications: target_justifications.clone(),
		})?;
		Ok(Self::with_cursor(
			client,
			target_header,
			target_body,
			target_justifications,
			SmallVec::default(),
		))
	}

	/// Continue the download of `target`, recorded by an earlier run, from `cursor`: the start
	/// of the next range request. Nothing is recorded again.
	pub fn resume(
		client: Arc<Client>,
		target: PartialStateTarget<B>,
		cursor: Vec<Vec<u8>>,
	) -> Self {
		Self::with_cursor(
			client,
			target.header,
			target.body,
			target.justifications,
			SmallVec::from_vec(cursor),
		)
	}

	fn with_cursor(
		client: Arc<Client>,
		target_header: B::Header,
		target_body: Option<Vec<B::Extrinsic>>,
		target_justifications: Option<Justifications>,
		last_key: SmallVec<[Vec<u8>; 2]>,
	) -> Self {
		Self {
			client,
			metadata: StateSyncMetadata {
				last_key,
				target_header,
				target_body,
				target_justifications,
				complete: false,
				imported_bytes: 0,
				write_failures: 0,
			},
		}
	}
}

impl<B, Client> StateSyncProvider<B> for StateSync<B, Client>
where
	B: BlockT,
	Client: ProofProvider<B> + PartialStateImport<B> + Send + Sync + 'static,
{
	///  Validate and import a state response.
	fn import(&mut self, response: StateResponse) -> ImportResult<B> {
		if response.proof.is_empty() {
			debug!(target: LOG_TARGET, "Missing proof");
			return ImportResult::BadResponse;
		}
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

		// A response that is not the end of the range must move the cursor: an honest server
		// includes at least one key after the start of the range, and any key moves it. One that
		// does not (a range already received, an out-of-order range, a root-only proof) would
		// otherwise be asked from the same peer again and again. Leaving a child trie with no key
		// left moves the cursor too, up one level.
		let complete = completed == 0;
		let mut next_key = self.metadata.last_key.clone();
		if !complete &&
			(!values.update_last_key(completed, &mut next_key) ||
				next_key == self.metadata.last_key)
		{
			debug!(target: LOG_TARGET, "State response makes no progress at depth {completed}");
			return ImportResult::BadResponse;
		}

		// Write the chunk before moving the cursor: a chunk that never reached the database
		// is asked for again, because `last_key` still points at its start. The range the chunk
		// proves travels with it, and its end is stored as the cursor a restart continues from.
		let partial_state = PartialStateFor::<B> {
			block_hash: self.metadata.target_hash(),
			block_number: self.metadata.target_number(),
			state_root: self.metadata.target_root(),
			nodes,
			start: self.metadata.last_key.to_vec(),
			end: (!complete).then(|| next_key.to_vec()),
			child_trie_copies: values.child_trie_copies(),
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
		self.metadata.last_key = next_key;

		self.metadata.imported_bytes += proof_size;
		if complete {
			// Every chunk is already in the database. The block import checks that the state is
			// complete and marks it as available.
			self.metadata.complete = true;
			ImportResult::Import(
				self.metadata.target_hash(),
				self.metadata.target_header.clone(),
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
	use sc_client_api::{
		partial_state::ResumablePartialState, ChildInfo, DecodedCompactProof, StorageProof,
	};
	use sp_runtime::traits::{HashingFor, Header as HeaderT};
	use sp_state_machine::{KeyValueStates, KeyValueStorageLevel};
	use std::sync::{
		atomic::{AtomicBool, AtomicUsize, Ordering},
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
		/// The target `begin_partial_state` recorded, if any.
		begun: Mutex<Option<PartialStateTarget<Block>>>,
		/// Makes `begin_partial_state` fail.
		fail_begin: AtomicBool,
		/// The range of every chunk written: its start and, unless it was the final chunk, its
		/// end.
		ranges: Mutex<Vec<(Vec<Vec<u8>>, Option<Vec<Vec<u8>>>)>>,
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
				begun: Mutex::new(None),
				fail_begin: AtomicBool::new(false),
				ranges: Mutex::new(Vec::new()),
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
		fn begin_partial_state(
			&self,
			target: PartialStateTarget<Block>,
		) -> sp_blockchain::Result<()> {
			if self.fail_begin.load(Ordering::SeqCst) {
				return Err(sp_blockchain::Error::Storage("injected begin failure".into()));
			}
			*self.begun.lock().unwrap() = Some(target);
			Ok(())
		}

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
			self.ranges.lock().unwrap().push((partial_state.start, partial_state.end));
			Ok(())
		}

		fn resumable_partial_state(
			&self,
		) -> sp_blockchain::Result<Option<ResumablePartialState<Block>>> {
			Ok(None)
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
		StateSync::new(client, header(), None, None).expect("the stub records any target")
	}

	fn target() -> PartialStateTarget<Block> {
		PartialStateTarget { header: header(), body: None, justifications: None }
	}

	/// A level of the range read that yielded no key.
	fn level_without_keys() -> KeyValueStorageLevel {
		KeyValueStorageLevel { state_root: vec![], parent_storage_keys: vec![], key_values: vec![] }
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
			ImportResult::Import(hash, _, _, _) => assert_eq!(hash, header().hash()),
			_ => panic!("expected the target block import"),
		}
		assert!(sync.is_complete());
		assert_eq!(client.writes(), 1);
	}

	/// The final chunk has no range end: the database marks the download complete instead of
	/// recording a cursor.
	#[test]
	fn last_chunk_carries_no_range_end() {
		let client = StubClient::new(vec![], 0, 0);
		let mut sync = StateSync::resume(client.clone(), target(), vec![vec![0x80]]);

		assert!(matches!(sync.import(response()), ImportResult::Import(_, _, _, _)));
		assert_eq!(*client.ranges.lock().unwrap(), vec![(vec![vec![0x80]], None)]);
	}

	#[test]
	fn failed_write_of_the_last_chunk_does_not_complete() {
		let client = StubClient::new(vec![], 0, 1);
		let mut sync = state_sync(client.clone());

		assert!(matches!(sync.import(response()), ImportResult::Continue));
		assert!(!sync.is_complete());
		assert!(matches!(sync.import(response()), ImportResult::Import(_, _, _, _)));
		assert!(sync.is_complete());
		assert_eq!(client.writes(), 1);
	}

	#[test]
	fn new_records_the_target_and_resume_does_not() {
		let client = StubClient::new(vec![], 1, 0);
		let sync = state_sync(client.clone());
		let begun = client.begun.lock().unwrap().take().expect("the target was recorded");
		assert_eq!(begun, target());
		assert!(sync.next_request().start.is_empty());

		let cursor = vec![vec![0x80], vec![0x01]];
		let sync = StateSync::resume(client.clone(), begun, cursor.clone());
		assert!(client.begun.lock().unwrap().is_none(), "nothing is recorded again");
		assert_eq!(sync.next_request().start, cursor);
		assert_eq!(sync.target_hash(), header().hash());
		assert_eq!(sync.progress().percentage, 50);
	}

	#[test]
	fn new_fails_when_the_target_cannot_be_recorded() {
		let client = StubClient::new(vec![], 1, 0);
		client.fail_begin.store(true, Ordering::SeqCst);
		assert!(StateSync::new(client, header(), None, None).is_err());
	}

	#[test]
	fn successful_write_passes_the_range_to_the_database() {
		let client = StubClient::new(one_key_in_the_top_trie(), 1, 0);
		let mut sync = state_sync(client.clone());

		assert!(matches!(sync.import(response()), ImportResult::Continue));
		assert_eq!(*client.ranges.lock().unwrap(), vec![(vec![], Some(vec![vec![0x80]]))]);
		assert_eq!(sync.next_request().start, vec![vec![0x80]]);
	}

	/// Zero keys while the range is not exhausted: the cursor would stay where it is, and the same
	/// peer would be asked forever.
	#[test]
	fn response_without_progress_is_bad_and_writes_nothing() {
		let client = StubClient::new(vec![], 1, 0);
		let mut sync = state_sync(client.clone());

		assert!(matches!(sync.import(response()), ImportResult::BadResponse));
		assert_eq!(client.writes(), 0);
		assert!(sync.next_request().start.is_empty());
	}

	/// The stub answers the same key every time: the second response makes no progress.
	#[test]
	fn response_repeating_the_cursor_is_bad() {
		let client = StubClient::new(one_key_in_the_top_trie(), 1, 0);
		let mut sync = state_sync(client.clone());

		assert!(matches!(sync.import(response()), ImportResult::Continue));
		assert!(matches!(sync.import(response()), ImportResult::BadResponse));
		assert_eq!(client.writes(), 1);
		assert_eq!(sync.next_request().start, vec![vec![0x80]]);
	}

	/// Stopped inside a child trie that yielded no key: the cursor cannot be updated.
	#[test]
	fn failed_cursor_update_is_bad() {
		let client = StubClient::new(vec![level_without_keys(), level_without_keys()], 2, 0);
		let mut sync = state_sync(client.clone());

		assert!(matches!(sync.import(response()), ImportResult::BadResponse));
		assert_eq!(client.writes(), 0);
	}

	/// Resumed inside a child trie, the response ends it without a key and stops at the top level:
	/// the cursor shrinks to the child trie's key, which is progress.
	#[test]
	fn leaving_a_child_trie_without_keys_is_progress() {
		let client = StubClient::new(vec![level_without_keys()], 1, 0);
		let mut sync = StateSync::resume(client.clone(), target(), vec![vec![0x80], vec![0x01]]);

		assert!(matches!(sync.import(response()), ImportResult::Continue));
		assert_eq!(sync.next_request().start, vec![vec![0x80]]);
		assert_eq!(client.writes(), 1);
	}

	#[test]
	fn is_resumable_within_the_distance_and_not_ahead() {
		let distance = MAX_RESUME_DISTANCE as u64;
		assert!(is_resumable::<Block>(100, 100));
		assert!(is_resumable::<Block>(100, 100 + distance));
		assert!(!is_resumable::<Block>(100, 101 + distance));
		assert!(!is_resumable::<Block>(101, 100));
	}
}
