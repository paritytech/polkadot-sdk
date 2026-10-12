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
use log::{debug, info, warn};
use sc_client_api::{
	partial_state::{PartialStateTarget, ResumablePartialState},
	CompactProof, PartialStateImport, ProofProvider,
};
use sc_consensus::{ImportedState, IncomingBlock};
use smallvec::SmallVec;
use sp_runtime::{
	traits::{Block as BlockT, Header, NumberFor, PartialStateFor},
	Justifications,
};
use std::{fmt, sync::Arc};

/// Consecutive database write failures tolerated before state sync gives up on the target. Every
/// failure has the same range asked again from the same peer, and a server answers an identical
/// request twice within a minute before it refuses the peer and we drop it
/// (`state_request_handler::MAX_NUMBER_OF_SAME_REQUESTS_PER_PEER`): the limit stays within that.
const MAX_PARTIAL_STATE_WRITE_FAILURES: u32 = 2;

/// How far behind the finalized head the target of an interrupted state sync may be for the
/// download to resume after a restart: a quarter of the 256 blocks of state that peers keep by
/// default (`sc_state_db::DEFAULT_MAX_BLOCK_CONSTRAINT`), so that the rest of the download has
/// time before they prune the target. An older target is discarded and a new one is chosen.
pub(crate) const MAX_RESUME_DISTANCE: u32 = 64;

/// Whether the target of an interrupted state sync, at `stored`, is close enough behind the
/// finalized head `head` to resume the download, see [`MAX_RESUME_DISTANCE`].
pub(crate) fn is_resumable<B: BlockT>(stored: NumberFor<B>, head: NumberFor<B>) -> bool {
	stored <= head && head - stored <= MAX_RESUME_DISTANCE.into()
}

/// The state sync attempt an earlier run of this node recorded, if its download can continue
/// towards the finalized head at `head`: the target must be at most [`MAX_RESUME_DISTANCE`] blocks
/// behind `head` and not newer, and no header of a warp proof may sit at or above it
/// (`highest_proof_header`, warp sync only). Such headers are imported without state and are never
/// executed: a target below one would stall finality behind it, and a target that is one is already
/// in the chain, where its state import would be skipped. Otherwise `None`: the attempt is
/// discarded when the new one begins.
pub(crate) fn resumable_state_sync<B: BlockT, Client: PartialStateImport<B>>(
	client: &Client,
	head: NumberFor<B>,
	highest_proof_header: Option<NumberFor<B>>,
) -> Option<ResumablePartialState<B>> {
	let resumable = match client.resumable_partial_state() {
		Ok(resumable) => resumable?,
		Err(e) => {
			warn!(target: LOG_TARGET, "Failed to read the record of an earlier state sync: {e}");
			return None;
		},
	};
	let stored = *resumable.target.header.number();
	let hash = resumable.target.header.hash();
	if !is_resumable::<B>(stored, head) {
		info!(
			target: LOG_TARGET,
			"The state sync of #{stored} ({hash}) interrupted earlier cannot resume towards \
			 #{head}: the target must be at most {MAX_RESUME_DISTANCE} blocks behind, starting over.",
		);
		return None;
	}
	if highest_proof_header.is_some_and(|proof_header| proof_header >= stored) {
		info!(
			target: LOG_TARGET,
			"The state sync of #{stored} ({hash}) interrupted earlier cannot resume: the warp proof \
			 imported a header at or above it, starting over.",
		);
		return None;
	}
	info!(
		target: LOG_TARGET,
		"Resuming the state sync of #{stored} ({hash}) interrupted earlier{}.",
		if resumable.complete {
			", every chunk is in the database: importing the block"
		} else {
			""
		},
	);
	Some(resumable)
}

/// Generic state sync provider. Used for mocking in tests.
pub trait StateSyncProvider<B: BlockT>: Send + Sync {
	/// Validate and import a state response.
	fn import(&mut self, response: StateResponse) -> ImportResult<B>;
	/// Produce next state request.
	fn next_request(&self) -> StateRequest;
	/// The import of the target block, yielded once, when every chunk was in the database before
	/// this provider was created (a download completed by an earlier run, see
	/// [`StateSync::resume`]). `None` otherwise: a download in progress yields the import through
	/// [`ImportResult::Import`] with its final chunk.
	fn take_target_import(&mut self) -> Option<IncomingBlock<B>>;
	/// Check if the state is complete.
	fn is_complete(&self) -> bool;
	/// Returns target block number.
	fn target_number(&self) -> NumberFor<B>;
	/// Returns target block hash.
	fn target_hash(&self) -> B::Hash;
	/// Returns state sync estimated progress.
	fn progress(&self) -> StateSyncProgress;
}

/// Reported state sync phase.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum StateSyncPhase {
	/// Chunks are being downloaded and written to the database.
	DownloadingState,
	/// Every chunk is in the database; the target block is being imported, which checks that
	/// the state is complete.
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
	/// The whole state is in the database: import the target block, with `ImportedState`.
	Import(IncomingBlock<B>),
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
	/// The download completed in an earlier run: the target import is yielded once by
	/// `take_target_import`.
	import_pending: bool,
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

	/// The target block as the import queue takes it: with the state the download wrote, nothing
	/// to execute, and the import allowed even if the block is already known.
	fn target_block(&self) -> IncomingBlock<B> {
		IncomingBlock {
			hash: self.target_hash(),
			header: Some(self.target_header.clone()),
			body: self.target_body.clone(),
			indexed_body: None,
			justifications: self.target_justifications.clone(),
			origin: None,
			allow_missing_state: true,
			import_existing: true,
			skip_execution: true,
			state: Some(ImportedState),
		}
	}

	fn progress(&self) -> StateSyncProgress {
		let cursor = *self.last_key.get(0).and_then(|last| last.get(0)).unwrap_or(&0u8);
		// The cursor stays at the start of the final range once the download is complete.
		let percent_done = if self.complete { 100 } else { cursor as u32 * 100 / 256 };
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

	/// Continue the attempt an earlier run recorded: the download goes on from its cursor, or,
	/// when every chunk is already written, only the target import is left and
	/// [`StateSyncProvider::take_target_import`] yields it. Nothing is recorded again.
	pub fn resume(client: Arc<Client>, resumable: ResumablePartialState<B>) -> Self {
		let ResumablePartialState { target, cursor, complete } = resumable;
		let mut sync = Self::with_cursor(
			client,
			target.header,
			target.body,
			target.justifications,
			SmallVec::from_vec(cursor),
		);
		sync.metadata.complete = complete;
		sync.metadata.import_pending = complete;
		sync
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
				import_pending: false,
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
		if self.metadata.complete {
			// A late answer to the final request, from a peer that answered after another one
			// did: every chunk is in the database, nothing is left to verify or write.
			debug!(target: LOG_TARGET, "Ignoring a state response after the download completed");
			return ImportResult::Continue;
		}
		if response.proof.is_empty() {
			debug!(target: LOG_TARGET, "Missing proof");
			return ImportResult::BadResponse;
		}
		debug!(target: LOG_TARGET, "Verifying a state proof of {} bytes", response.proof.len());
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
			ImportResult::Import(self.metadata.target_block())
		} else {
			ImportResult::Continue
		}
	}

	/// Produce next state request.
	fn next_request(&self) -> StateRequest {
		self.metadata.next_request()
	}

	fn take_target_import(&mut self) -> Option<IncomingBlock<B>> {
		std::mem::take(&mut self.metadata.import_pending).then(|| self.metadata.target_block())
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
		/// What `resumable_partial_state` reports.
		resumable: Mutex<Option<ResumablePartialState<Block>>>,
		/// Makes `resumable_partial_state` fail.
		fail_resumable: AtomicBool,
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
				resumable: Mutex::new(None),
				fail_resumable: AtomicBool::new(false),
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
			if self.fail_resumable.load(Ordering::SeqCst) {
				return Err(sp_blockchain::Error::Storage("injected read failure".into()));
			}
			Ok(self.resumable.lock().unwrap().clone())
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

	/// The record of an earlier run's attempt on the target, left at `cursor`.
	fn resumable(cursor: Vec<Vec<u8>>, complete: bool) -> ResumablePartialState<Block> {
		ResumablePartialState { target: target(), cursor, complete }
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
	fn second_consecutive_failed_write_aborts() {
		let client = StubClient::new(one_key_in_the_top_trie(), 1, 2);
		let mut sync = state_sync(client.clone());

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
			ImportResult::Import(block) => {
				assert_eq!(block.hash, header().hash());
				assert_eq!(block.header, Some(header()));
				assert!(block.state.is_some() && block.import_existing && block.skip_execution);
				assert!(block.allow_missing_state);
			},
			_ => panic!("expected the target block import"),
		}
		assert!(sync.is_complete());
		assert!(sync.take_target_import().is_none(), "the import went with the final chunk");
		assert_eq!(client.writes(), 1);
	}

	/// The final chunk has no range end: the database marks the download complete instead of
	/// recording a cursor.
	#[test]
	fn last_chunk_carries_no_range_end() {
		let client = StubClient::new(vec![], 0, 0);
		let mut sync = StateSync::resume(client.clone(), resumable(vec![vec![0x80]], false));

		assert!(matches!(sync.import(response()), ImportResult::Import(_)));
		assert_eq!(*client.ranges.lock().unwrap(), vec![(vec![vec![0x80]], None)]);
	}

	#[test]
	fn failed_write_of_the_last_chunk_does_not_complete() {
		let client = StubClient::new(vec![], 0, 1);
		let mut sync = state_sync(client.clone());

		assert!(matches!(sync.import(response()), ImportResult::Continue));
		assert!(!sync.is_complete());
		assert!(matches!(sync.import(response()), ImportResult::Import(_)));
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
		let record =
			ResumablePartialState { target: begun, cursor: cursor.clone(), complete: false };
		let sync = StateSync::resume(client.clone(), record);
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
		let mut sync =
			StateSync::resume(client.clone(), resumable(vec![vec![0x80], vec![0x01]], false));

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

	/// A download that completed in an earlier run has only the target import left: it is yielded
	/// once, and no range is requested or written.
	#[test]
	fn resumed_complete_attempt_yields_the_target_import_once() {
		let client = StubClient::new(vec![], 0, 0);
		let mut sync = StateSync::resume(client.clone(), resumable(vec![vec![0x80]], true));

		assert!(sync.is_complete());
		assert_eq!(sync.progress().phase, StateSyncPhase::ImportingState);
		let block = sync.take_target_import().expect("the import is pending");
		assert_eq!(block.hash, header().hash());
		assert!(block.state.is_some() && block.allow_missing_state && block.import_existing);
		assert!(block.skip_execution);
		assert!(sync.take_target_import().is_none(), "yielded once");
		assert_eq!(client.writes(), 0);
	}

	/// A late answer to the final request changes nothing: it is neither written again nor
	/// counted as a failure.
	#[test]
	fn response_after_completion_is_ignored() {
		let client = StubClient::new(vec![], 0, 0);
		let mut sync = state_sync(client.clone());

		assert!(matches!(sync.import(response()), ImportResult::Import(_)));
		assert!(matches!(sync.import(response()), ImportResult::Continue));
		assert_eq!(client.writes(), 1);
		assert_eq!(sync.metadata.write_failures, 0);
		assert!(sync.is_complete());
	}

	#[test]
	fn resumed_download_yields_no_import() {
		let client = StubClient::new(vec![], 1, 0);
		let mut sync = StateSync::resume(client.clone(), resumable(vec![vec![0x80]], false));

		assert!(sync.take_target_import().is_none());
		assert!(!sync.is_complete());
		assert_eq!(sync.next_request().start, vec![vec![0x80]]);
	}

	#[test]
	fn resumable_state_sync_follows_the_distance_and_the_proof_rules() {
		let distance = MAX_RESUME_DISTANCE as u64;
		let client = StubClient::new(vec![], 1, 0);
		let target_at = |number| {
			let header = Header::new(
				number,
				Default::default(),
				Default::default(),
				Default::default(),
				Default::default(),
			);
			PartialStateTarget { header, body: None, justifications: None }
		};
		let record =
			|complete| ResumablePartialState { target: target_at(100), cursor: vec![], complete };

		assert!(resumable_state_sync(&*client, 100, None).is_none(), "nothing recorded");

		*client.resumable.lock().unwrap() = Some(record(false));
		assert!(resumable_state_sync(&*client, 100, None).is_some());
		assert!(resumable_state_sync(&*client, 100 + distance, None).is_some());
		assert!(resumable_state_sync(&*client, 101 + distance, None).is_none(), "too old");
		assert!(resumable_state_sync(&*client, 99, None).is_none(), "ahead of the head");
		assert!(resumable_state_sync(&*client, 100, Some(99)).is_some(), "proof below");
		assert!(resumable_state_sync(&*client, 100, Some(100)).is_none(), "proof at the target");
		assert!(resumable_state_sync(&*client, 100, Some(101)).is_none(), "proof above");

		*client.resumable.lock().unwrap() = Some(record(true));
		assert!(
			resumable_state_sync(&*client, 100, None).is_some_and(|record| record.complete),
			"a complete record passes through"
		);

		client.fail_resumable.store(true, Ordering::SeqCst);
		assert!(resumable_state_sync(&*client, 100, None).is_none(), "read error");
	}
}
