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
use log::{debug, error, info, warn};
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
use std::{
	fmt,
	sync::Arc,
	time::{Duration, Instant},
};

/// Retries of a failed chunk write before the state sync gives up on its target. The chunk stays
/// in memory between the attempts, nothing is downloaded again.
const MAX_PARTIAL_STATE_WRITE_RETRIES: u32 = 4;

/// The wait between two write attempts of the same chunk.
const PARTIAL_STATE_WRITE_RETRY_DELAY: Duration = Duration::from_secs(1);

/// The state sync an earlier run left behind, logged as resumed. `Ok(None)` when nothing is
/// recorded. The attempt is resumed whatever its age: its target is a finalized block, every chunk
/// is verified against the target's state root, and block sync executes the blocks after it. Peers
/// serve the state of a finalized block for a bounded time only (256 blocks by default,
/// `sc_state_db::DEFAULT_MAX_BLOCK_CONSTRAINT`); a target every peer has pruned is downloaded from
/// nobody until a peer that kept it answers.
pub(crate) fn resumable_state_sync<B: BlockT, Client: PartialStateImport<B>>(
	client: &Client,
) -> sp_blockchain::Result<Option<ResumablePartialState<B>>> {
	let Some(resumable) = client.resumable_partial_state()? else {
		return Ok(None);
	};
	info!(
		target: LOG_TARGET,
		"Resuming the state sync of #{} ({}) interrupted earlier{}.",
		resumable.target.header.number(),
		resumable.target.header.hash(),
		if resumable.complete {
			", every chunk is in the database: importing the block"
		} else {
			""
		},
	);
	Ok(Some(resumable))
}

/// Generic state sync provider. Used for mocking in tests.
pub trait StateSyncProvider<B: BlockT>: Send + Sync {
	/// Validate and import a state response.
	fn import(&mut self, response: StateResponse) -> ImportResult<B>;
	/// Produce next state request.
	fn next_request(&self) -> StateRequest;
	/// The import of the target block, if the download already completed in an earlier run and
	/// only the import is left. Handed out once. A download in progress returns `None`: its
	/// import comes with the final chunk, as [`ImportResult::Import`].
	fn take_target_import(&mut self) -> Option<IncomingBlock<B>>;
	/// Retry a chunk whose write failed, once its delay has passed, without downloading it again.
	/// `None` if nothing is pending or it is too early, else what the response would have returned.
	fn retry_pending_write(&mut self) -> Option<ImportResult<B>>;
	/// A verified chunk waits to be written. No state request goes out meanwhile.
	fn has_pending_write(&self) -> bool;
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
	/// Every chunk is in the database. The target block is being imported, which checks that the
	/// state is complete.
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
	/// The whole state is in the database. Import the target block.
	Import(IncomingBlock<B>),
	/// Continue downloading.
	Continue,
	/// Bad state chunk.
	BadResponse,
	/// Chunks keep failing to write to the database. Give up on this target.
	Abort,
}

/// A verified chunk whose database write failed. Kept for a retry, nothing is downloaded again.
struct PendingWrite<B: BlockT> {
	/// The chunk. Its `end` is the cursor once written, `None` on the final chunk.
	chunk: PartialStateFor<B>,
	proof_size: u64,
	retries: u32,
	next_attempt: Instant,
}

struct StateSyncMetadata<B: BlockT> {
	last_key: SmallVec<[Vec<u8>; 2]>,
	target_header: B::Header,
	target_body: Option<Vec<B::Extrinsic>>,
	target_justifications: Option<Justifications>,
	complete: bool,
	/// The download completed in an earlier run and `take_target_import` was not called yet.
	import_pending: bool,
	imported_bytes: u64,
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
			// Proofs are mandatory now. The field only stays for peers on older releases.
			no_proof: false,
		}
	}

	/// The target block as the import queue takes it: its state comes from the download, nothing
	/// is executed, and importing it is fine even if the block is already known.
	fn target_block(&self) -> IncomingBlock<B> {
		IncomingBlock {
			hash: self.target_hash(),
			header: Some(self.target_header.clone()),
			body: self.target_body.clone(),
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
		// Once complete, the cursor is stuck at the start of the final range: report 100 instead.
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
/// Every verified chunk goes straight to the database, and the cursor only moves once that write
/// succeeded. Between two responses only a chunk whose write failed lives in memory, for a retry.
pub struct StateSync<B: BlockT, Client> {
	metadata: StateSyncMetadata<B>,
	client: Arc<Client>,
	pending_write: Option<PendingWrite<B>>,
}

impl<B, Client> StateSync<B, Client>
where
	B: BlockT,
	Client: ProofProvider<B> + PartialStateImport<B> + Send + Sync + 'static,
{
	/// Create a new instance. The client records the target so a restart can resume the download,
	/// and drops any earlier attempt that never completed. On `Err` nothing was recorded and the
	/// state sync must not start.
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

	/// Continue the attempt an earlier run recorded, from its cursor. If every chunk was already
	/// written there is nothing to download: [`StateSyncProvider::take_target_import`] hands out
	/// the target import instead. Nothing is recorded again.
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
			},
			pending_write: None,
		}
	}

	/// Write `pending`; on failure keep it for a retry after the delay, up to the limit.
	fn try_write(&mut self, mut pending: PendingWrite<B>) -> ImportResult<B> {
		match self.client.import_partial_state(&pending.chunk) {
			Ok(()) => {
				self.metadata.imported_bytes += pending.proof_size;
				match pending.chunk.end {
					Some(end) => {
						self.metadata.last_key = SmallVec::from_vec(end);
						ImportResult::Continue
					},
					None => {
						// Everything is in the database. The block import now checks that the
						// state is complete and marks it as available.
						self.metadata.complete = true;
						ImportResult::Import(self.metadata.target_block())
					},
				}
			},
			Err(e) if pending.retries >= MAX_PARTIAL_STATE_WRITE_RETRIES => {
				error!(
					target: LOG_TARGET,
					"Failed to write a state chunk to the database after {} retries, giving up on \
					 the state sync: {e}",
					pending.retries,
				);
				ImportResult::Abort
			},
			Err(e) => {
				pending.retries += 1;
				warn!(
					target: LOG_TARGET,
					"Failed to write a state chunk to the database ({}/{} retries), retrying in \
					 {} s: {e}",
					pending.retries,
					MAX_PARTIAL_STATE_WRITE_RETRIES,
					PARTIAL_STATE_WRITE_RETRY_DELAY.as_secs(),
				);
				pending.next_attempt = Instant::now() + PARTIAL_STATE_WRITE_RETRY_DELAY;
				self.pending_write = Some(pending);
				ImportResult::Continue
			},
		}
	}

	/// Makes the pending write due now, so tests need not wait for the delay.
	#[cfg(test)]
	pub(crate) fn expire_write_delay(&mut self) {
		if let Some(pending) = &mut self.pending_write {
			pending.next_attempt = Instant::now();
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
			// A late answer to the final request (another peer answered it first). Everything is
			// in the database already, nothing left to verify or write.
			debug!(target: LOG_TARGET, "Ignoring a state response after the download completed");
			return ImportResult::Continue;
		}
		if self.pending_write.is_some() {
			// No request is out while a chunk waits to be written: this answers a cancelled one.
			debug!(target: LOG_TARGET, "Ignoring a state response while a chunk waits to be written");
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

		// A chunk that is not the last one must move the cursor. An honest server always puts at
		// least one key after the start of the range, and any key moves it. Without this check a
		// response that makes no progress (a range we already have, one out of order, a root-only
		// proof) would be asked from the same peer forever. Leaving a child trie that has no key
		// left moves the cursor up one level, so that counts as progress too.
		let complete = completed == 0;
		let mut next_key = self.metadata.last_key.clone();
		if !complete &&
			(!values.update_last_key(completed, &mut next_key) ||
				next_key == self.metadata.last_key)
		{
			debug!(target: LOG_TARGET, "State response makes no progress at depth {completed}");
			return ImportResult::BadResponse;
		}

		// Write the chunk before moving the cursor. The chunk carries the range it proves, and its
		// end becomes the cursor a restart continues from. If the write fails, the chunk is kept
		// and `retry_pending_write` tries again, nothing is downloaded twice.
		let chunk = PartialStateFor::<B> {
			block_hash: self.metadata.target_hash(),
			state_root: self.metadata.target_root(),
			nodes,
			start: self.metadata.last_key.to_vec(),
			end: (!complete).then(|| next_key.to_vec()),
			child_trie_copies: values.child_trie_copies(),
		};
		self.try_write(PendingWrite { chunk, proof_size, retries: 0, next_attempt: Instant::now() })
	}

	fn retry_pending_write(&mut self) -> Option<ImportResult<B>> {
		if Instant::now() < self.pending_write.as_ref()?.next_attempt {
			return None;
		}
		let pending = self.pending_write.take()?;
		Some(self.try_write(pending))
	}

	fn has_pending_write(&self) -> bool {
		self.pending_write.is_some()
	}

	/// Produce next state request.
	fn next_request(&self) -> StateRequest {
		self.metadata.next_request()
	}

	fn take_target_import(&mut self) -> Option<IncomingBlock<B>> {
		// Only `resume` sets this, for a download that completed before the restart.
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
	use sp_blockchain::HeaderBackend;
	use sp_runtime::traits::Header as HeaderT;
	use substrate_test_runtime_client::{
		runtime::{Block, Header},
		DefaultTestClientBuilderExt, TestClient, TestClientBuilder, TestClientBuilderExt,
	};

	/// A node holding the genesis state, which serves the proofs, and the header of that state:
	/// the target of the sync.
	fn remote() -> (TestClient, Header) {
		let remote = TestClientBuilder::new().build();
		let header = remote.expect_header(remote.info().genesis_hash).expect("genesis is there");
		(remote, header)
	}

	/// A node without state, as one that state-syncs starts, with a sync of `header` begun.
	fn syncing(header: &Header) -> (Arc<TestClient>, StateSync<Block, TestClient>) {
		let client = Arc::new(TestClientBuilder::new().set_no_genesis().build());
		let sync = StateSync::new(client.clone(), header.clone(), None, None)
			.expect("an empty database records any target");
		(client, sync)
	}

	/// `remote`'s answer to the request starting at `start`, holding about `limit` bytes of
	/// proof. A limit of one byte stops the range at its first key.
	fn response(
		remote: &TestClient,
		header: &Header,
		start: &[Vec<u8>],
		limit: usize,
	) -> StateResponse {
		let (proof, _) = remote
			.read_proof_collection(header.hash(), start, limit)
			.expect("the remote holds the state");
		StateResponse { entries: vec![], proof: proof.encode() }
	}

	fn target(header: &Header) -> PartialStateTarget<Block> {
		PartialStateTarget { header: header.clone(), body: None, justifications: None }
	}

	/// A target with `header`'s state root under another hash. Recorded on the syncing node, it
	/// makes the database refuse the chunks of `header`'s sync.
	fn other_target(header: &Header) -> PartialStateTarget<Block> {
		let other = Header::new(
			1,
			Default::default(),
			*header.state_root(),
			header.hash(),
			Default::default(),
		);
		target(&other)
	}

	/// The sync the syncing node's database recorded.
	fn record(client: &TestClient) -> ResumablePartialState<Block> {
		client
			.resumable_partial_state()
			.expect("the database can be read")
			.expect("a sync is recorded")
	}

	/// A download in chunks: each chunk's end becomes the cursor the database records and the
	/// next request starts from, the final chunk yields the target import. A chunk already
	/// written, or a late answer after the final one, changes nothing.
	#[test]
	fn download_in_chunks_ends_with_the_target_import() {
		let (remote, header) = remote();
		let (client, mut sync) = syncing(&header);
		let no_proof = StateResponse { entries: vec![], proof: vec![] };
		assert!(matches!(sync.import(no_proof), ImportResult::BadResponse));

		let first = response(&remote, &header, &[], 1);
		assert!(matches!(sync.import(first.clone()), ImportResult::Continue));
		let cursor = sync.next_request().start;
		assert!(!cursor.is_empty(), "the state did not fit in one chunk");
		assert_eq!(record(&client).cursor, cursor);
		assert_eq!(sync.progress().phase, StateSyncPhase::DownloadingState);

		// Already written: its range ends at the cursor, so it makes no progress.
		assert!(matches!(sync.import(first), ImportResult::BadResponse));
		assert_eq!(record(&client).cursor, cursor, "nothing was written");

		match sync.import(response(&remote, &header, &cursor, usize::MAX)) {
			ImportResult::Import(block) => {
				assert_eq!(block.hash, header.hash());
				assert_eq!(block.header, Some(header.clone()));
				assert!(block.state.is_some() && block.import_existing && block.skip_execution);
				assert!(block.allow_missing_state);
			},
			_ => panic!("expected the target block import"),
		}
		assert!(sync.is_complete() && record(&client).complete);
		assert_eq!(sync.progress().percentage, 100);
		assert_eq!(sync.progress().phase, StateSyncPhase::ImportingState);
		assert!(sync.take_target_import().is_none(), "the import went with the final chunk");

		// A late answer to the final request, another peer answered it first.
		let late = response(&remote, &header, &cursor, usize::MAX);
		assert!(matches!(sync.import(late), ImportResult::Continue));
		assert!(!sync.has_pending_write());
	}

	/// A chunk the database refused is kept: nothing is written, the cursor stays, and a response
	/// that arrives meanwhile is ignored. Once the delay has passed the retry writes it, and the
	/// cursor moves without a second download.
	#[test]
	fn failed_write_is_retried_from_memory() {
		let (remote, header) = remote();
		let (client, mut sync) = syncing(&header);
		// Another target recorded under the sync's feet: the database refuses its chunks.
		client.begin_partial_state(other_target(&header)).unwrap();

		assert!(matches!(sync.import(response(&remote, &header, &[], 1)), ImportResult::Continue));
		assert!(sync.has_pending_write());
		assert!(sync.next_request().start.is_empty(), "the cursor did not move");
		assert!(record(&client).cursor.is_empty());
		// Not even decoded: with nothing pending a missing proof is a bad response.
		let no_proof = StateResponse { entries: vec![], proof: vec![] };
		assert!(matches!(sync.import(no_proof), ImportResult::Continue), "ignored while pending");
		assert!(sync.retry_pending_write().is_none(), "no retry before the delay");

		client.begin_partial_state(target(&header)).unwrap();
		sync.expire_write_delay();
		assert!(matches!(sync.retry_pending_write(), Some(ImportResult::Continue)));
		assert!(!sync.has_pending_write());
		let cursor = sync.next_request().start;
		assert!(!cursor.is_empty());
		assert_eq!(record(&client).cursor, cursor, "the retry wrote the chunk kept in memory");
	}

	/// The last retry can still succeed. If it fails too, the state sync gives up: nothing was
	/// written and the cursor stayed.
	#[test]
	fn the_limit_is_four_retries() {
		let (remote, header) = remote();
		let after_the_last_retry = |recover: bool| {
			let (client, mut sync) = syncing(&header);
			client.begin_partial_state(other_target(&header)).unwrap();
			let whole_state = response(&remote, &header, &[], usize::MAX);
			assert!(matches!(sync.import(whole_state), ImportResult::Continue));
			for _ in 1..MAX_PARTIAL_STATE_WRITE_RETRIES {
				sync.expire_write_delay();
				assert!(matches!(sync.retry_pending_write(), Some(ImportResult::Continue)));
				assert!(sync.has_pending_write());
			}
			if recover {
				client.begin_partial_state(target(&header)).unwrap();
			}
			sync.expire_write_delay();
			let last = sync.retry_pending_write();
			assert!(!sync.has_pending_write());
			(client, sync, last)
		};

		let (client, sync, last) = after_the_last_retry(true);
		assert!(matches!(last, Some(ImportResult::Import(_))));
		assert!(sync.is_complete() && record(&client).complete);

		let (client, sync, last) = after_the_last_retry(false);
		assert!(matches!(last, Some(ImportResult::Abort)));
		assert!(!sync.is_complete());
		assert!(sync.next_request().start.is_empty());
		let untouched = ResumablePartialState {
			target: other_target(&header),
			cursor: vec![],
			complete: false,
		};
		assert_eq!(record(&client), untouched, "nothing was written");
	}

	/// `new` records the target, `resume` never records again: it continues from the cursor the
	/// database holds. A record left after the final chunk hands out the target import, once.
	#[test]
	fn resume_continues_from_the_recorded_cursor() {
		let (remote, header) = remote();
		let (client, mut sync) = syncing(&header);
		let begun =
			ResumablePartialState { target: target(&header), cursor: vec![], complete: false };
		assert_eq!(record(&client), begun);
		assert!(matches!(sync.import(response(&remote, &header, &[], 1)), ImportResult::Continue));
		let interrupted = record(&client);
		assert!(!interrupted.cursor.is_empty());

		// A restart mid-download.
		let mut sync = StateSync::resume(client.clone(), interrupted.clone());
		assert_eq!(record(&client), interrupted, "nothing is recorded again");
		assert!(sync.take_target_import().is_none(), "the download is not complete");
		assert_eq!(sync.next_request().start, interrupted.cursor);
		let rest = response(&remote, &header, &interrupted.cursor, usize::MAX);
		assert!(matches!(sync.import(rest), ImportResult::Import(_)));

		// A restart after the final chunk, before the target import landed.
		let complete = record(&client);
		assert!(complete.complete);
		let mut sync = StateSync::resume(client, complete);
		assert!(sync.is_complete());
		assert_eq!(sync.progress().percentage, 100);
		assert!(sync.take_target_import().is_some_and(|block| block.hash == header.hash()));
		assert!(sync.take_target_import().is_none(), "handed out once");
	}
}
