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

//! The database side of a state sync: the trie nodes of a block's state go into `STATE` chunk by
//! chunk before the block is imported, and `STATE_META` records enough for a restart to resume
//! the download or to finish an interrupted discard. [`DbPartialState`] is the
//! [`PartialStateStore`] the backend hands out. It holds the backend's own database, blockchain
//! metadata and write statistics (clones of the same `Arc`s), and one mutex over the status of
//! the attempt. `begin`, `import`, `check_complete` and discards hold that mutex. So does
//! `Backend::try_commit_operation`, through [`DbPartialState::lock`], until the status is
//! cleared: the commit of the target block removes the records `begin` wrote, in
//! the block's own transaction, and nothing else touching the attempt can run in between.

use crate::{columns, stats::StateUsageStats, utils::Meta, DbHash, StorageDb};
use codec::{Decode, Encode};
use hash_db::EMPTY_PREFIX;
use log::{info, warn};
use parking_lot::{Mutex, MutexGuard, RwLock};
use sc_client_api::partial_state::{PartialStateStore, PartialStateTarget, ResumablePartialState};
use sp_blockchain::{Error as ClientError, Result as ClientResult};
use sp_database::Transaction;
use sp_runtime::traits::{
	Block as BlockT, HashingFor, Header as HeaderT, NumberFor, PartialStateFor,
};
use sp_trie::partial_state::{chunk_nodes, TrieNodeInfo};
use std::sync::Arc;

/// Keys in `STATE_META` that track a state sync in progress. `DbPartialState` writes them.
pub(crate) mod partial_state_keys {
	/// The block whose state we are syncing, a SCALE encoded `PartialStateTarget`.
	pub const TARGET: &[u8] = b"partial_state_target";
	/// Where the download continues: the cursor the last written chunk ended at, SCALE encoded.
	pub const CURSOR: &[u8] = b"partial_state_cursor";
	/// Set with the final chunk. Every node is in and only the block import is left, so a restart
	/// does not download the last range again.
	pub const DOWNLOADED: &[u8] = b"partial_state_downloaded";
	/// The state root of an attempt we are removing from `STATE`, SCALE encoded. A start that finds
	/// it finishes the removal. It replaces `TARGET`, the two are never found together.
	pub const DISCARDING: &[u8] = b"partial_state_discarding";
}

/// How many node removals go into one commit while discarding an attempt.
/// About 4 MiB of keys, well under parity-db's commit queue.
const DISCARD_BATCH: usize = 100_000;

/// One step of the walk that removes an attempt's nodes, see `DbPartialState::remove_nodes`.
enum DiscardStep<H: hash_db::Hasher> {
	/// Look the node up and, if present, queue its removal after its children.
	Visit(TrieNodeInfo<H>),
	/// Remove the node stored under this key.
	Remove(Vec<u8>),
}

/// Where the state sync that `STATE_META` records stands, mirrored in memory. One mutex guards it,
/// so a chunk write, a discard and the completing commit never interleave. It changes right after
/// the commit that changes the keys, and `load` reads it at start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PartialStateStatus<Hash> {
	/// `TARGET` is set: chunks of this block are accepted.
	Syncing { hash: Hash, state_root: Hash },
	/// `DISCARDING` is set: a discard failed after its first commit. The next one starts from the
	/// marker.
	Discarding { state_root: Hash },
}

/// How far the recorded attempt got, read from `STATE_META`.
struct PartialStateProgress {
	/// Where the next range starts. Empty before the first chunk.
	cursor: Vec<Vec<u8>>,
	/// The final chunk is written, only the block import is left.
	complete: bool,
}

/// Decodes the `TARGET` record.
fn decode_partial_state_target<Block: BlockT>(
	raw: &[u8],
) -> ClientResult<PartialStateTarget<Block>> {
	PartialStateTarget::<Block>::decode(&mut &raw[..]).map_err(|e| {
		ClientError::Backend(format!(
			"The record of a state sync in progress does not decode ({e}): the database was \
			 written by another version. Delete it and sync again."
		))
	})
}

/// Decodes the `DISCARDING` marker.
fn decode_partial_state_marker<Block: BlockT>(raw: &[u8]) -> ClientResult<Block::Hash> {
	Block::Hash::decode(&mut &raw[..]).map_err(|e| {
		ClientError::Backend(format!(
			"The marker of an interrupted state sync discard does not decode ({e}): the database \
			 was written by another version. Delete it and sync again."
		))
	})
}

/// Removes the target, the cursor and the downloaded marker of an attempt.
pub(crate) fn remove_partial_state_records(transaction: &mut Transaction<DbHash>) {
	transaction.remove(columns::STATE_META, partial_state_keys::TARGET);
	transaction.remove(columns::STATE_META, partial_state_keys::CURSOR);
	transaction.remove(columns::STATE_META, partial_state_keys::DOWNLOADED);
}

/// The state sync in progress on the database, if any: its records in `STATE_META`, its status
/// in memory, and the operations that write and remove its nodes in `STATE`.
///
/// The fields are the backend's own, shared: the storage the trie backend reads nodes from, the
/// blockchain metadata the guard against block state reads, and the write statistics. One mutex
/// guards the status, held across every chunk write, every discard and the whole of the commit
/// that completes the sync, so none of them interleave.
pub(crate) struct DbPartialState<Block: BlockT> {
	storage: Arc<StorageDb<Block>>,
	meta: Arc<RwLock<Meta<NumberFor<Block>, Block::Hash>>>,
	state_usage: Arc<StateUsageStats>,
	status: Mutex<Option<PartialStateStatus<Block::Hash>>>,
}

impl<Block: BlockT> DbPartialState<Block> {
	pub(crate) fn new(
		storage: Arc<StorageDb<Block>>,
		meta: Arc<RwLock<Meta<NumberFor<Block>, Block::Hash>>>,
		state_usage: Arc<StateUsageStats>,
	) -> Self {
		Self { storage, meta, state_usage, status: Mutex::new(None) }
	}

	/// The status, locked. A block commit holds it for the whole commit, as long as no finalized
	/// block has state: afterwards nothing is recorded and the commit does not look.
	pub(crate) fn lock(&self) -> MutexGuard<'_, Option<PartialStateStatus<Block::Hash>>> {
		self.status.lock()
	}

	/// Picks up a state sync left by the previous run. If a discard was interrupted, the marker
	/// is there instead of the target, and the removal is finished now. A record that does not
	/// decode was written by another version, and the database is refused.
	pub(crate) fn load(&self) -> ClientResult<()> {
		let mut status = self.status.lock();
		match (
			self.storage.db.get(columns::STATE_META, partial_state_keys::DISCARDING),
			self.storage.db.get(columns::STATE_META, partial_state_keys::TARGET),
		) {
			(Some(_), Some(_)) => {
				return Err(ClientError::Backend(
					"The database records a state sync in progress and the discard of one at \
					 once, which never happens: it was written by another version. Delete it \
					 and sync again."
						.into(),
				))
			},
			(Some(raw), None) => {
				let state_root = decode_partial_state_marker::<Block>(&raw)?;
				*status = Some(PartialStateStatus::Discarding { state_root });
				self.discard(&mut status)?;
			},
			(None, Some(raw)) => {
				let target = decode_partial_state_target::<Block>(&raw)?;
				*status = Some(PartialStateStatus::Syncing {
					hash: target.header.hash(),
					state_root: *target.header.state_root(),
				});
			},
			(None, None) => {},
		}
		Ok(())
	}

	/// True once any block state was committed. From then on `STATE` is shared with real block
	/// state, so a state sync can neither start nor be discarded.
	fn holds_block_state(&self) -> bool {
		self.meta.read().finalized_state.is_some() || !self.storage.state_db.is_empty()
	}

	/// Reads the cursor and the downloaded marker of the recorded attempt.
	fn progress(&self) -> ClientResult<PartialStateProgress> {
		let cursor = match self.storage.db.get(columns::STATE_META, partial_state_keys::CURSOR) {
			Some(raw) => Vec::<Vec<u8>>::decode(&mut &raw[..]).map_err(|e| {
				ClientError::Backend(format!(
					"The cursor of a state sync in progress does not decode ({e}): the database \
					 was written by another version. Delete it and sync again."
				))
			})?,
			None => Vec::new(),
		};
		let complete = self
			.storage
			.db
			.get(columns::STATE_META, partial_state_keys::DOWNLOADED)
			.is_some();
		Ok(PartialStateProgress { cursor, complete })
	}

	/// Fails unless `state_root` is the recorded target's. The caller holds the mutex.
	fn ensure_syncing(
		status: &Option<PartialStateStatus<Block::Hash>>,
		state_root: Block::Hash,
	) -> ClientResult<()> {
		match *status {
			Some(PartialStateStatus::Syncing { state_root: recorded_root, .. })
				if recorded_root == state_root =>
			{
				Ok(())
			},
			Some(PartialStateStatus::Syncing { hash, state_root: recorded_root }) => {
				Err(ClientError::Backend(format!(
					"State root {state_root:?} is not the recorded state sync target's: \
					 {hash:?} has state root {recorded_root:?}"
				)))
			},
			Some(PartialStateStatus::Discarding { .. }) => Err(ClientError::Backend(format!(
				"State root {state_root:?} is not the recorded state sync target's: the recorded \
				 attempt is being discarded"
			))),
			None => Err(ClientError::Backend(format!(
				"State root {state_root:?} is not the recorded state sync target's: none is \
				 recorded"
			))),
		}
	}

	/// Removes the recorded attempt and its nodes from `STATE`. The caller holds the mutex and
	/// passes its guard. On return the status matches the database again: `None` when everything
	/// is gone, `Discarding` when a commit failed after the marker was written.
	pub(crate) fn discard(
		&self,
		status: &mut Option<PartialStateStatus<Block::Hash>>,
	) -> ClientResult<()> {
		let Some(current) = *status else {
			return Err(ClientError::Backend(
				"Cannot discard the partial state: no state sync is recorded".into(),
			));
		};
		// The walk below removes every node it reaches, so `STATE` must hold nothing but the
		// attempt. That is always the case here, this is a sanity check.
		if self.holds_block_state() {
			return Err(ClientError::Backend(
				"Cannot discard the partial state: the database holds block state next to the \
				 record of a state sync in progress, which never happens. Delete the database and \
				 sync again."
					.into(),
			));
		}
		let state_root = match current {
			// The removal takes several commits, so a marker brackets it. The first commit swaps
			// the records for the state root under `DISCARDING`. If we crash or fail after this
			// point, the next start or the next discard resumes from the marker.
			PartialStateStatus::Syncing { state_root, .. } => {
				let mut transaction = Transaction::new();
				remove_partial_state_records(&mut transaction);
				transaction.set_from_vec(
					columns::STATE_META,
					partial_state_keys::DISCARDING,
					state_root.encode(),
				);
				self.storage.db.commit(transaction)?;
				*status = Some(PartialStateStatus::Discarding { state_root });
				state_root
			},
			// An earlier discard failed after writing the marker. We start from there.
			PartialStateStatus::Discarding { state_root } => state_root,
		};

		let start = std::time::Instant::now();
		let removed = self.remove_nodes(state_root)?;
		*status = None;
		if let Err(e) = self.storage.db.optimize_db_col(columns::STATE) {
			warn!(target: "db", "Failed to optimize the database after discarding partial state: {e:?}");
		}
		info!(
			target: "db",
			"Discarded the partial state of state root {state_root:?}: {removed} nodes in {:?}",
			start.elapsed()
		);
		Ok(())
	}

	/// Walks the trie under `state_root` through what `STATE` holds and removes every node it
	/// finds, children first, in commits of `DISCARD_BATCH` removals. The last commit also drops
	/// the `DISCARDING` marker. Returns how many nodes went.
	fn remove_nodes(&self, state_root: Block::Hash) -> ClientResult<u64> {
		let walk_err = |e: sp_trie::partial_state::Error| {
			ClientError::Backend(format!(
				"Cannot discard the partial state of state root {state_root:?}: {e}"
			))
		};
		let root_key = self.storage.state_key(&state_root, EMPTY_PREFIX).into_owned();
		let mut stack: Vec<DiscardStep<HashingFor<Block>>> =
			vec![DiscardStep::Visit(TrieNodeInfo::root(state_root))];
		let mut transaction = Transaction::new();
		let (mut removed, mut pending) = (0u64, 0usize);
		while let Some(step) = stack.pop() {
			match step {
				DiscardStep::Visit(info) => {
					let prefix = info.db_prefix().map_err(walk_err)?;
					let key = self.storage.state_key(&info.hash, prefix.as_prefix()).into_owned();
					// A missing node was never downloaded, or an interrupted walk already removed
					// it. Children go before parents, so an earlier walk left nothing we need here.
					if let Some(encoded) = self.storage.db.get(columns::STATE, &key) {
						// Queue the removal before the children, so it pops after them. This
						// makes the root the last node removed, which the wait below relies on.
						stack.push(DiscardStep::Remove(key));
						let (_, children) = info.expand(&encoded).map_err(walk_err)?;
						stack.extend(children.into_iter().map(DiscardStep::Visit));
					}
				},
				DiscardStep::Remove(key) => {
					// On parity-db a node at two trie positions holds two references, one per
					// write. The walk reaches both positions and so dereferences it twice.
					transaction.remove(columns::STATE, &key);
					removed += 1;
					pending += 1;
					if pending == DISCARD_BATCH {
						let batch = std::mem::replace(&mut transaction, Transaction::new());
						self.storage.db.commit(batch)?;
						pending = 0;
					}
				},
			}
		}
		// The marker goes with the last batch. Once it is gone, the removal is complete.
		transaction.remove(columns::STATE_META, partial_state_keys::DISCARDING);
		self.storage.db.commit(transaction)?;

		// Wait until the removal is visible. On parity-db, `STATE` is reference counted, and a
		// removal only shows up to `get` once the background thread has planned its commit. The
		// next attempt reads `STATE` for the child tries it copies, so nothing of this one may
		// still be readable when we return.
		//
		// Polling the root is enough: it is the last node removed, and commits are planned in
		// order, so once the root reads as gone, everything removed before it is gone too.
		let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
		while self.storage.db.get(columns::STATE, &root_key).is_some() {
			if std::time::Instant::now() > deadline {
				warn!(
					target: "db",
					"The root of the discarded partial state still reads as present after 30 s"
				);
				break;
			}
			std::thread::sleep(std::time::Duration::from_millis(1));
		}
		Ok(removed)
	}
}

impl<Block: BlockT> PartialStateStore<Block> for DbPartialState<Block> {
	fn begin(&self, target: PartialStateTarget<Block>) -> ClientResult<()> {
		let hash = target.header.hash();
		let mut status = self.status.lock();
		if self.holds_block_state() {
			return Err(ClientError::Backend(format!(
				"Cannot begin the state sync of {hash:?}: the database holds block state, and a \
				 state sync only ever runs on one that holds none. Start with `--sync full`, or \
				 delete the database to sync its state again."
			)));
		}
		if status.is_some() {
			// A new target replaces an attempt that never completed. Its nodes go first, while
			// `STATE` holds nothing else.
			self.discard(&mut status)?;
		}
		let mut transaction = Transaction::new();
		transaction.set_from_vec(columns::STATE_META, partial_state_keys::TARGET, target.encode());
		transaction.remove(columns::STATE_META, partial_state_keys::CURSOR);
		transaction.remove(columns::STATE_META, partial_state_keys::DOWNLOADED);
		self.storage.db.commit(transaction)?;
		*status =
			Some(PartialStateStatus::Syncing { hash, state_root: *target.header.state_root() });
		Ok(())
	}

	fn import(&self, partial_state: &PartialStateFor<Block>) -> ClientResult<()> {
		let block_hash = partial_state.block_hash;
		let refused = |what: String| {
			sp_blockchain::Error::Storage(format!("state chunk of {block_hash:?} {what}"))
		};
		// Held across the commit, so no discard can run in between.
		let status = self.status.lock();
		let state_root = match *status {
			Some(PartialStateStatus::Syncing { hash, state_root }) if hash == block_hash => {
				state_root
			},
			Some(PartialStateStatus::Syncing { hash, .. }) => {
				return Err(refused(format!(
					"does not belong to the recorded state sync target {hash:?}"
				)))
			},
			Some(PartialStateStatus::Discarding { .. }) => {
				return Err(refused(
					"arrives while the recorded state sync is being discarded".into(),
				))
			},
			None => return Err(refused("arrives while no state sync is recorded".into())),
		};
		if partial_state.state_root != state_root {
			return Err(refused(format!(
				"is for state root {:?} while the recorded target's is {state_root:?}",
				partial_state.state_root
			)));
		}
		// We only accept the next range in order. Writing a position twice would raise its
		// reference count on parity-db for good, and a range written over a hole could never
		// complete.
		let progress = self.progress()?;
		if progress.complete {
			return Err(refused("arrives after the final chunk: the download is complete".into()));
		}
		if progress.cursor != partial_state.start {
			return Err(refused(format!(
				"starts at {:?} while the recorded cursor is {:?}",
				partial_state.start, progress.cursor
			)));
		}
		// The nodes go straight into `STATE`, bypassing state-db. They belong to no block until
		// the target block is imported with the state complete. They are keyed exactly as
		// `StorageDb::get` looks nodes up, so reads find them.
		let nodes = chunk_nodes(partial_state, |hash, prefix| {
			self.storage.db.get(columns::STATE, &self.storage.state_key(hash, prefix))
		})
		.map_err(|e| {
			sp_blockchain::Error::Storage(format!(
				"state chunk of {block_hash:?} does not decode: {e}"
			))
		})?;

		let mut transaction = Transaction::new();
		let (mut ops, mut bytes) = (0u64, 0u64);
		for node in nodes {
			let key = self.storage.state_key(&node.hash, node.prefix.as_prefix());
			ops += 1;
			bytes += (key.len() + node.encoded.len()) as u64;
			transaction.set_from_vec(columns::STATE, &key, node.encoded);
		}
		// The cursor is committed together with the nodes, so after a restart the download
		// continues exactly where the last committed chunk ended. The final chunk has no end and
		// sets the downloaded marker instead, so its range is not fetched again.
		match &partial_state.end {
			Some(end) => transaction.set_from_vec(
				columns::STATE_META,
				partial_state_keys::CURSOR,
				end.encode(),
			),
			None => transaction.set(columns::STATE_META, partial_state_keys::DOWNLOADED, &[]),
		}
		self.storage.db.commit(transaction)?;
		self.state_usage.tally_writes_nodes(ops, bytes);
		Ok(())
	}

	fn resumable(&self) -> ClientResult<Option<ResumablePartialState<Block>>> {
		// Under the mutex, so the three keys are read as one consistent record.
		let _status = self.status.lock();
		let Some(raw) = self.storage.db.get(columns::STATE_META, partial_state_keys::TARGET) else {
			return Ok(None);
		};
		let target = decode_partial_state_target::<Block>(&raw)?;
		let PartialStateProgress { cursor, complete } = self.progress()?;
		Ok(Some(ResumablePartialState { target, cursor, complete }))
	}

	fn check_complete(&self, state_root: Block::Hash) -> ClientResult<()> {
		// Held across the walk, so no discard can run in between.
		let mut status = self.status.lock();
		Self::ensure_syncing(&status, state_root)?;
		let state = crate::DbStateBuilder::new(self.storage.clone(), state_root).build();
		if let Err(e) = sc_client_api::partial_state::check_trie_complete(state) {
			// Every chunk was written and still a node is missing. The attempt cannot complete,
			// so its nodes go now, before anything else writes `STATE`.
			self.discard(&mut status).map_err(|discard| {
				ClientError::Backend(format!("{e}; discarding its state failed too: {discard}"))
			})?;
			return Err(e);
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		tests::{reopen_backend, Block},
		Backend, BlocksPruning, DatabaseSource, DatabaseType,
	};
	use hash_db::HashDB;
	use sc_client_api::{
		backend::{Backend as BTrait, BlockImportOperation as Op, NewBlockState},
		TrieCacheContext,
	};
	use sp_core::storage::ChildInfo;
	use sp_database::Database;
	use sp_runtime::{testing::Header, StateVersion, Storage};
	use sp_state_machine::Backend as StateBackend;
	use sp_trie::{partial_state::PartialState, LayoutV0, MemoryDB, TrieDBMutBuilder, TrieMut};
	use std::sync::atomic::{AtomicUsize, Ordering};

	/// Databases the partial-state tests run against. The in-memory kvdb and RocksDB store
	/// state nodes under `prefix ++ hash`. parity-db counts references itself and stores them
	/// under the bare hash, see `StorageDb::state_key`.
	#[derive(Debug, Clone, Copy)]
	enum PartialStateDb {
		KvdbMemdb,
		ParityDb,
		RocksDb,
	}

	/// A fresh database of the given kind, and the directory it lives in when it is on disk.
	fn partial_state_test_db(
		kind: PartialStateDb,
	) -> (Arc<dyn Database<DbHash>>, Option<tempfile::TempDir>) {
		match kind {
			PartialStateDb::KvdbMemdb => {
				(sp_database::as_database(kvdb_memorydb::create(crate::utils::NUM_COLUMNS)), None)
			},
			PartialStateDb::ParityDb => {
				let tmp = tempfile::TempDir::new().unwrap();
				let db =
					crate::parity_db::open::<DbHash>(tmp.path(), DatabaseType::Full, true, false)
						.expect("parity-db open succeeds in test");
				(db, Some(tmp))
			},
			PartialStateDb::RocksDb => {
				let tmp = tempfile::TempDir::new().unwrap();
				let mut cfg = kvdb_rocksdb::DatabaseConfig::with_columns(crate::utils::NUM_COLUMNS);
				cfg.create_if_missing = true;
				let db = kvdb_rocksdb::Database::open(&cfg, tmp.path())
					.expect("kvdb-rocksdb open succeeds in test");
				(sp_database::as_database(db), Some(tmp))
			},
		}
	}

	/// A backend on a fresh database of the given kind.
	fn partial_state_test_backend(
		kind: PartialStateDb,
	) -> (Backend<Block>, Arc<dyn Database<DbHash>>, Option<tempfile::TempDir>) {
		let (db, tmp) = partial_state_test_db(kind);
		(partial_state_backend_on(db.clone()), db, tmp)
	}

	/// A backend on a fresh database of the given kind that records every write to `STATE`.
	fn observed_partial_state_test_backend(
		kind: PartialStateDb,
	) -> (Backend<Block>, Arc<ObservingDb>, Option<tempfile::TempDir>) {
		let (inner, tmp) = partial_state_test_db(kind);
		let db = Arc::new(ObservingDb {
			inner,
			commits_until_failure: AtomicUsize::new(0),
			state_writes: Default::default(),
		});
		(partial_state_backend_on(db.clone()), db, tmp)
	}

	fn partial_state_backend_on(db: Arc<dyn Database<DbHash>>) -> Backend<Block> {
		Backend::<Block>::new_test_with_tx_storage_source(
			BlocksPruning::Some(10),
			10,
			// A fresh database: the create flag makes `Backend::new` initialise the state-db meta.
			DatabaseSource::Custom { db, require_create_flag: true },
			Default::default(),
		)
	}

	/// Closes `backend` and `db` and opens the same database again, as a restart does: the tests
	/// read what the next start would find.
	fn reopen_partial_state_db(
		kind: PartialStateDb,
		backend: Backend<Block>,
		db: Arc<dyn Database<DbHash>>,
		tmp: &Option<tempfile::TempDir>,
	) -> Arc<dyn Database<DbHash>> {
		drop(backend);
		let path = || tmp.as_ref().expect("on-disk databases run in a temporary directory").path();
		match kind {
			PartialStateDb::KvdbMemdb => db,
			PartialStateDb::ParityDb => {
				drop(db);
				crate::parity_db::open::<DbHash>(path(), DatabaseType::Full, false, false)
					.expect("parity-db reopens in test")
			},
			PartialStateDb::RocksDb => {
				drop(db);
				let cfg = kvdb_rocksdb::DatabaseConfig::with_columns(crate::utils::NUM_COLUMNS);
				sp_database::as_database(
					kvdb_rocksdb::Database::open(&cfg, path())
						.expect("kvdb-rocksdb reopens in test"),
				)
			},
		}
	}

	/// The record `begin` takes for `header`, as fast sync begins it.
	fn partial_state_target(header: &Header) -> PartialStateTarget<Block> {
		PartialStateTarget { header: header.clone(), body: None, justifications: None }
	}

	/// The key `STATE` holds the root node of `header`'s state under.
	fn root_key(backend: &Backend<Block>, header: &Header) -> Vec<u8> {
		backend.storage.state_key(&header.state_root, EMPTY_PREFIX).to_vec()
	}

	/// Commits a genesis block with its (empty) state, as a full node does at its first start.
	fn commit_genesis_state(backend: &Backend<Block>) -> ClientResult<Header> {
		let mut op = backend.begin_operation()?;
		let state_root = op.set_genesis_state(Storage::default(), true, StateVersion::V0)?;
		let header = Header {
			number: 0,
			parent_hash: Default::default(),
			state_root,
			digest: Default::default(),
			extrinsics_root: Default::default(),
		};
		op.set_block_data(header.clone(), Some(vec![]), None, NewBlockState::Final, true)?;
		backend.commit_operation(op)?;
		Ok(header)
	}

	/// A chunk whose commit fails must be accepted again, and the retry writes the nodes the failed
	/// commit lost: nothing about the chunk is remembered, so the retry is the same write.
	#[test]
	fn import_retries_after_failed_commit() {
		let expected_key_values =
			vec![(vec![0u8; 40], vec![0u8; 1]), (vec![1u8; 40], vec![1u8; 1])];
		let (nodes, header) = partial_state_chunk(&expected_key_values);
		let (backend, db, _tmp) = observed_partial_state_test_backend(PartialStateDb::KvdbMemdb);

		backend.partial_state.begin(partial_state_target(&header)).unwrap();

		// `Backend::new` has committed its own initialisation by now: arm the failure only here.
		db.fail_commit(1);
		assert!(backend
			.partial_state
			.import(&with_end(chunk(&header, nodes.clone()), &[vec![0x80]]))
			.is_err());
		assert!(!db.armed(), "the injected failure was consumed by the chunk's commit");
		let cursor = || backend.partial_state.resumable().unwrap().unwrap().cursor;
		assert!(cursor().is_empty(), "the cursor is stored with the nodes, or not at all");

		backend
			.partial_state
			.import(&with_end(chunk(&header, nodes), &[vec![0x80]]))
			.unwrap();
		assert_eq!(cursor(), vec![vec![0x80]]);

		import_synced_block(&backend, &header).unwrap();
		assert_eq!(state_pairs(&backend, &header), expected_key_values);
	}

	/// The child tries of the download tests: two with the same content, one different.
	const DOWNLOAD_CHILD_TRIES: [(&[u8], u8); 3] = [(b"alpha", 1), (b"bravo", 1), (b"charlie", 2)];

	/// A state with a dozen top keys and the child tries above, as a peer serves it, and the
	/// header of the block that has it.
	fn download_state() -> (sp_state_machine::InMemoryBackend<HashingFor<Block>>, Header) {
		let mut remote = sp_state_machine::new_in_mem::<HashingFor<Block>>();
		let top: sp_state_machine::StorageCollection =
			(0u8..12).map(|i| (vec![i; 20], Some(vec![i; 40]))).collect();
		let child = |fill: u8| -> sp_state_machine::StorageCollection {
			(0u8..4).map(|i| (vec![fill, i], Some(vec![fill; 33 + i as usize]))).collect()
		};
		let mut changes = vec![(None, top)];
		for (name, fill) in DOWNLOAD_CHILD_TRIES {
			changes.push((Some(ChildInfo::new_default(name)), child(fill)));
		}
		remote.insert(changes, StateVersion::V1);
		let header = Header {
			number: 1,
			parent_hash: Default::default(),
			state_root: *remote.root(),
			digest: Default::default(),
			extrinsics_root: Default::default(),
		};
		(remote, header)
	}

	/// Downloads `header`'s state from `remote` into `backend`, in the ranges the range prover
	/// cuts at `size_limit` bytes and the verifier reads back, and returns the backend with the
	/// number of chunks and of child trie copies reported. Unless the first chunk is the last, the
	/// backend is closed and reopened on `db` after it and the download goes on from the recorded
	/// cursor.
	fn download(
		remote: &sp_state_machine::InMemoryBackend<HashingFor<Block>>,
		header: &Header,
		mut backend: Backend<Block>,
		db: &Arc<ObservingDb>,
		size_limit: usize,
	) -> (Backend<Block>, usize, usize) {
		let root = header.state_root;
		let mut cursor = smallvec::SmallVec::<[Vec<u8>; 2]>::new();
		let (mut chunks, mut copies) = (0, 0);
		loop {
			let (proof, _) =
				sp_state_machine::prove_range_read_with_child_with_size_on_trie_backend(
					remote, size_limit, &cursor,
				)
				.unwrap();
			let (values, completed) = sp_state_machine::read_range_proof_check_with_child::<
				HashingFor<Block>,
			>(root, proof.clone(), &cursor)
			.unwrap();
			let complete = completed == 0;
			let mut next = cursor.clone();
			if !complete {
				assert!(values.update_last_key(completed, &mut next));
			}
			copies += values.child_trie_copies().len();
			backend
				.partial_state
				.import(&PartialState {
					block_hash: header.hash(),
					state_root: root,
					nodes: proof.into_memory_db(),
					start: cursor.to_vec(),
					end: (!complete).then(|| next.to_vec()),
					child_trie_copies: values.child_trie_copies(),
				})
				.unwrap();
			chunks += 1;
			if complete {
				break;
			}
			if chunks == 1 {
				drop(backend);
				backend = reopen_backend(db.clone(), BlocksPruning::Some(10));
				let resumable = backend.partial_state.resumable().unwrap().expect("recorded");
				assert_eq!(resumable.cursor, next.to_vec());
				assert!(!resumable.complete);
			}
			cursor = next;
		}
		(backend, chunks, copies)
	}

	/// Every position of `remote`'s state as `backend` keys it: prefix and hash, or the hash alone
	/// on a database that counts references, once per position. Sorted, for a comparison with the
	/// keys a download wrote.
	fn expected_state_keys(
		backend: &Backend<Block>,
		remote: &sp_state_machine::InMemoryBackend<HashingFor<Block>>,
	) -> Vec<Vec<u8>> {
		let prefix_keys = backend.storage.prefix_keys;
		let mut keys: Vec<Vec<u8>> = remote
			.backend_storage()
			.clone()
			.drain()
			.into_iter()
			.filter(|(_, (_, rc))| *rc > 0)
			.map(|(key, _)| if prefix_keys { key } else { key[key.len() - 32..].to_vec() })
			.collect();
		keys.sort();
		keys
	}

	/// A real download, chunk by chunk as the range prover cuts it, for several size limits, with a
	/// restart after the first chunk and one after the last. `begin` records the target, the
	/// chunks write every position of the state exactly once, the twin child trie's positions
	/// included, which come from the first twin's nodes: on parity-db a second write of a node
	/// raises its reference count for good. After the restart the download is complete and nothing
	/// is written again. The block completes, the records are gone and every key reads back.
	///
	/// The limits give a range per key, then ranges where one resumes inside the first twin,
	/// reaches the second and takes the nodes it skipped from the first twin's, then a single
	/// range.
	#[rstest::rstest]
	#[case::kvdb_memdb(PartialStateDb::KvdbMemdb)]
	#[case::paritydb(PartialStateDb::ParityDb)]
	#[case::rocksdb(PartialStateDb::RocksDb)]
	fn download_writes_each_position_once(#[case] kind: PartialStateDb) {
		let (remote, header) = download_state();
		for size_limit in [1usize, 550, usize::MAX] {
			let (backend, db, _tmp) = observed_partial_state_test_backend(kind);
			assert_eq!(backend.partial_state.resumable().unwrap(), None);
			backend.partial_state.begin(partial_state_target(&header)).unwrap();
			assert_eq!(
				backend.partial_state.resumable().unwrap(),
				Some(ResumablePartialState {
					target: partial_state_target(&header),
					cursor: vec![],
					complete: false
				}),
			);
			let (backend, chunks, copies) = download(&remote, &header, backend, &db, size_limit);
			assert!(size_limit != 1 || chunks > 10, "a limit of one byte cuts a range per key");
			// A response that reads both twins skips the second one and reports it as a copy.
			assert!(size_limit == 1 || copies == 1, "the twin is reported as a copy");
			assert!(size_limit != 550 || chunks > 2, "the twins are read by a resumed range");

			let mut writes = db.state_writes.lock().clone();
			writes.sort();
			assert_eq!(
				writes,
				expected_state_keys(&backend, &remote),
				"size limit {size_limit}: each position is written exactly once"
			);

			drop(backend);
			let backend = reopen_backend(db.clone(), BlocksPruning::Some(10));
			let resumable = backend.partial_state.resumable().unwrap().expect("recorded");
			assert!(resumable.complete);
			assert_eq!(db.state_writes.lock().len(), writes.len(), "nothing is written again");

			import_synced_block(&backend, &header).unwrap();
			assert_eq!(backend.partial_state.resumable().unwrap(), None);
			assert_eq!(db.get(columns::STATE_META, partial_state_keys::DOWNLOADED), None);
			let state = backend.state_at(header.hash(), TrieCacheContext::Untrusted).unwrap();
			for (name, fill) in DOWNLOAD_CHILD_TRIES {
				let child_info = ChildInfo::new_default(name);
				for i in 0u8..4 {
					assert_eq!(
						state.child_storage(&child_info, &[fill, i]).unwrap(),
						Some(vec![fill; 33 + i as usize])
					);
				}
			}
		}
	}

	/// A chunk is written only for the recorded target, from its cursor, and never after the
	/// final chunk: anything else would write nodes the discard cannot reach, leave a hole, or
	/// raise a reference count for good. A refused chunk changes nothing.
	#[test]
	fn chunks_out_of_order_are_refused() {
		let key_values = vec![(vec![0u8; 40], vec![0u8; 1])];
		let (nodes, header) = partial_state_chunk(&key_values);
		let (other_nodes, other) = partial_state_chunk(&[(vec![2u8; 40], vec![2u8; 1])]);
		let (backend, db, _tmp) = observed_partial_state_test_backend(PartialStateDb::KvdbMemdb);
		let from = |start: &[Vec<u8>]| {
			let mut next = chunk(&header, nodes.clone());
			next.start = start.to_vec();
			next
		};

		assert!(backend.partial_state.import(&from(&[])).is_err(), "nothing recorded");
		backend.partial_state.begin(partial_state_target(&header)).unwrap();
		assert!(backend.partial_state.import(&chunk(&other, other_nodes)).is_err(), "other block");
		let mut wrong_root = from(&[]);
		wrong_root.state_root = other.state_root;
		assert!(backend.partial_state.import(&wrong_root).is_err(), "other state root");
		assert!(backend.partial_state.import(&from(&[vec![0x80]])).is_err(), "past the cursor");
		assert!(db.state_writes.lock().is_empty(), "nothing was written");

		backend.partial_state.import(&with_end(from(&[]), &[vec![0x80]])).unwrap();
		assert!(backend.partial_state.import(&from(&[])).is_err(), "the first range again");
		assert_eq!(
			backend.partial_state.resumable().unwrap().map(|record| record.cursor),
			Some(vec![vec![0x80]]),
			"the cursor is untouched"
		);

		backend.partial_state.import(&from(&[vec![0x80]])).unwrap();
		assert!(backend.partial_state.resumable().unwrap().is_some_and(|record| record.complete));
		let writes = db.state_writes.lock().len();
		assert!(
			backend.partial_state.import(&from(&[vec![0x80]])).is_err(),
			"after the final chunk"
		);
		assert_eq!(db.state_writes.lock().len(), writes, "nothing was written");

		import_synced_block(&backend, &header).unwrap();
		assert_eq!(state_pairs(&backend, &header), key_values);
	}

	/// The first real state commit, here a genesis state as a `--sync full` restart writes it,
	/// discards the attempt it finds recorded: afterwards `STATE` is shared with block state and
	/// could never be emptied.
	#[rstest::rstest]
	#[case::kvdb_memdb(PartialStateDb::KvdbMemdb)]
	#[case::paritydb(PartialStateDb::ParityDb)]
	#[case::rocksdb(PartialStateDb::RocksDb)]
	fn first_state_commit_discards_a_recorded_attempt(#[case] kind: PartialStateDb) {
		let (nodes, header) = partial_state_chunk(&[(vec![0u8; 40], vec![0u8; 1])]);
		let (backend, db, tmp) = partial_state_test_backend(kind);
		backend.partial_state.begin(partial_state_target(&header)).unwrap();
		let chunk = chunk(&header, nodes);
		backend.partial_state.import(&with_end(chunk, &[vec![0x80]])).unwrap();
		let root_key = root_key(&backend, &header);

		let genesis = commit_genesis_state(&backend).unwrap();
		assert_eq!(backend.partial_state.resumable().unwrap(), None);
		assert!(backend.have_state_at(genesis.hash(), 0));
		// From now on a state sync only ever runs on a database that holds no block state.
		assert!(backend.partial_state.begin(partial_state_target(&header)).is_err());
		assert!(commit_synced_block(&backend, &header).is_err(), "nothing recorded, lock skipped");
		assert!(db.get(columns::STATE_META, partial_state_keys::TARGET).is_none());

		let db = reopen_partial_state_db(kind, backend, db, &tmp);
		assert!(db.get(columns::STATE, &root_key).is_none(), "the attempt's nodes are gone");
		let backend = reopen_backend(db, BlocksPruning::Some(10));
		assert!(backend.have_state_at(genesis.hash(), 0), "the genesis state is intact");
	}

	/// Every chunk was written and still a node is missing: the attempt cannot complete and is
	/// discarded, nodes and records.
	#[test]
	fn failed_completion_discards_the_attempt() {
		let kind = PartialStateDb::KvdbMemdb;
		let (nodes, header) =
			partial_state_chunk(&[(vec![0u8; 40], vec![0u8; 1]), (vec![1u8; 40], vec![1u8; 1])]);
		let mut root_only = MemoryDB::default();
		root_only.insert(EMPTY_PREFIX, &nodes.get(&header.state_root, EMPTY_PREFIX).unwrap());
		let (backend, db, tmp) = partial_state_test_backend(kind);
		backend.partial_state.begin(partial_state_target(&header)).unwrap();
		let chunk = chunk(&header, root_only);
		backend.partial_state.import(&with_end(chunk, &[vec![0x80]])).unwrap();
		let root_key = root_key(&backend, &header);

		assert!(import_synced_block(&backend, &header).is_err());
		assert_eq!(backend.partial_state.resumable().unwrap(), None);

		let db = reopen_partial_state_db(kind, backend, db, &tmp);
		assert!(db.get(columns::STATE, &root_key).is_none());
	}

	/// A discard that fails after its first commit leaves the marker and the nodes. The memory
	/// knows it, so chunks are refused, and the next start finishes the removal from the marker.
	#[test]
	fn failed_discard_after_the_marker_is_finished_at_the_next_start() {
		let kind = PartialStateDb::KvdbMemdb;
		let (nodes, header) = partial_state_chunk(&[(vec![0u8; 40], vec![0u8; 1])]);
		let (_, other) = partial_state_chunk(&[(vec![2u8; 40], vec![2u8; 1])]);
		let (backend, db, tmp) = observed_partial_state_test_backend(kind);
		backend.partial_state.begin(partial_state_target(&header)).unwrap();
		backend
			.partial_state
			.import(&with_end(chunk(&header, nodes.clone()), &[vec![0x80]]))
			.unwrap();
		let root_key = root_key(&backend, &header);
		// The first commit of a discard replaces the records with the marker. The second removes
		// the nodes and the marker.
		db.fail_commit(2);
		assert!(backend.partial_state.begin(partial_state_target(&other)).is_err());
		assert!(!db.armed(), "the injected failure was consumed by the discard");
		assert!(db.get(columns::STATE_META, partial_state_keys::DISCARDING).is_some());
		assert!(db.get(columns::STATE_META, partial_state_keys::TARGET).is_none());
		assert!(db.get(columns::STATE, &root_key).is_some(), "the removal did not happen");
		assert_eq!(backend.partial_state.resumable().unwrap(), None);
		assert!(
			backend.partial_state.import(&chunk(&header, nodes)).is_err(),
			"no chunk is written while the attempt is being discarded"
		);

		let db = reopen_partial_state_db(kind, backend, db, &tmp);
		let backend = reopen_backend(db.clone(), BlocksPruning::Some(10));
		assert_eq!(backend.partial_state.resumable().unwrap(), None);
		let db = reopen_partial_state_db(kind, backend, db, &tmp);
		assert!(db.get(columns::STATE, &root_key).is_none(), "the removal was finished");
		assert!(db.get(columns::STATE_META, partial_state_keys::DISCARDING).is_none());
	}

	/// A block other than the recorded target cannot complete the attempt. With another state root
	/// neither the check nor a commit marked as synced accepts it. With the same state root the
	/// check passes and the commit refuses. Neither failure discards anything: the recorded target
	/// still completes. With nothing recorded, the synced commit is refused too.
	#[test]
	fn completion_by_another_block_is_an_error() {
		let key_values = vec![(vec![0u8; 40], vec![0u8; 1])];
		let (nodes, header) = partial_state_chunk(&key_values);
		let (_, other) = partial_state_chunk(&[(vec![2u8; 40], vec![2u8; 1])]);
		let (backend, db, _tmp) = partial_state_test_backend(PartialStateDb::KvdbMemdb);
		assert!(commit_synced_block(&backend, &header).is_err(), "none recorded");
		backend.partial_state.begin(partial_state_target(&header)).unwrap();
		backend.partial_state.import(&chunk(&header, nodes)).unwrap();
		let root_key = root_key(&backend, &header);

		assert!(backend.partial_state.check_complete(other.state_root).is_err());
		assert!(commit_synced_block(&backend, &other).is_err());
		assert!(import_synced_block(&backend, &other).is_err());
		let same_root = Header { number: 2, ..header.clone() };
		assert!(import_synced_block(&backend, &same_root).is_err());
		assert!(
			backend.partial_state.resumable().unwrap().is_some_and(|record| record.complete),
			"the attempt is untouched"
		);
		assert!(db.get(columns::STATE, &root_key).is_some(), "its nodes are untouched");

		import_synced_block(&backend, &header).unwrap();
		assert_eq!(state_pairs(&backend, &header), key_values);
	}

	/// A trie whose two leaves are one node: the keys differ in their first nibble only and hold
	/// the same value, so the leaves have the same partial key and hash, under two prefixes.
	fn shared_leaf_chunk() -> (MemoryDB<HashingFor<Block>>, Header) {
		let key = |first: u8| [vec![first], vec![0u8; 40]].concat();
		partial_state_chunk(&[(key(0x10), vec![1u8]), (key(0x20), vec![1u8])])
	}

	/// The keys `STATE` holds `chunk`'s nodes under, one per trie position, as a download writes
	/// them.
	fn chunk_keys(
		backend: &Backend<Block>,
		chunk: PartialState<HashingFor<Block>>,
	) -> Result<Vec<Vec<u8>>, sp_trie::partial_state::Error> {
		Ok(chunk_nodes(&chunk, |_, _| None)?
			.into_iter()
			.map(|node| backend.storage.state_key(&node.hash, node.prefix.as_prefix()).into_owned())
			.collect())
	}

	/// A new target replaces an attempt that never completed. Its nodes are gone, and only the new
	/// target is recorded. A node at two trie positions is written once per position, so twice on
	/// a reference-counting database. The discard reaches both positions and every reference goes.
	#[rstest::rstest]
	#[case::kvdb_memdb(PartialStateDb::KvdbMemdb)]
	#[case::paritydb(PartialStateDb::ParityDb)]
	#[case::rocksdb(PartialStateDb::RocksDb)]
	fn discard_removes_a_node_shared_by_two_positions(#[case] kind: PartialStateDb) {
		let (nodes, header) = shared_leaf_chunk();
		let (_, other) = partial_state_chunk(&[(vec![2u8; 40], vec![2u8; 1])]);
		let (backend, db, tmp) = partial_state_test_backend(kind);
		let keys = chunk_keys(&backend, chunk(&header, nodes.clone())).unwrap();
		assert_eq!(keys.len(), 3, "the root and the leaf at two positions");
		backend.partial_state.begin(partial_state_target(&header)).unwrap();
		backend.partial_state.import(&chunk(&header, nodes)).unwrap();
		for key in &keys {
			assert!(db.get(columns::STATE, key).is_some());
		}

		backend.partial_state.begin(partial_state_target(&other)).unwrap();
		assert_eq!(
			backend.partial_state.resumable().unwrap(),
			Some(ResumablePartialState {
				target: partial_state_target(&other),
				cursor: vec![],
				complete: false
			}),
		);

		let db = reopen_partial_state_db(kind, backend, db, &tmp);
		for key in &keys {
			assert!(db.get(columns::STATE, key).is_none(), "every reference is gone");
		}
		assert!(db.get(columns::STATE_META, partial_state_keys::DISCARDING).is_none());
	}

	/// Builds a one-level trie holding `key_values` and returns its nodes as a state-sync chunk
	/// would carry them, together with a header whose state root is the trie root.
	fn partial_state_chunk(
		key_values: &[(Vec<u8>, Vec<u8>)],
	) -> (MemoryDB<HashingFor<Block>>, Header) {
		let mut nodes = MemoryDB::default();
		let mut state_root: <Block as BlockT>::Hash = Default::default();
		let mut trie =
			TrieDBMutBuilder::<LayoutV0<HashingFor<Block>>>::new(&mut nodes, &mut state_root)
				.build();
		for (k, v) in key_values {
			trie.insert(k, v).unwrap();
		}
		trie.commit();
		drop(trie);

		let header = Header {
			number: 1,
			parent_hash: Default::default(),
			state_root,
			digest: Default::default(),
			extrinsics_root: Default::default(),
		};
		(nodes, header)
	}

	/// A chunk of the whole state of `header`: from the start, with no range end and no copies.
	fn chunk(header: &Header, nodes: MemoryDB<HashingFor<Block>>) -> PartialStateFor<Block> {
		PartialState {
			block_hash: header.hash(),
			state_root: header.state_root,
			nodes,
			start: vec![],
			end: None,
			child_trie_copies: vec![],
		}
	}

	/// `chunk` with `end` as the cursor its range ends with.
	fn with_end(
		mut chunk: PartialState<HashingFor<Block>>,
		end: &[Vec<u8>],
	) -> PartialState<HashingFor<Block>> {
		chunk.end = Some(end.to_vec());
		chunk
	}

	/// Imports `header` as the block whose state was delivered through `import`,
	/// returning the outcome of the block's commit.
	fn import_synced_block(backend: &Backend<Block>, header: &Header) -> ClientResult<()> {
		backend.partial_state.check_complete(header.state_root)?;
		commit_synced_block(backend, header)
	}

	/// Commits `header` marked as having its state from a state sync, without the completeness
	/// check `import_synced_block` makes first: what the commit itself refuses.
	fn commit_synced_block(backend: &Backend<Block>, header: &Header) -> ClientResult<()> {
		let mut op = backend.begin_operation()?;
		op.set_block_data(header.clone(), None, None, NewBlockState::Normal, true)?;
		op.mark_state_synced()?;
		backend.commit_operation(op)
	}

	fn state_pairs(backend: &Backend<Block>, header: &Header) -> Vec<(Vec<u8>, Vec<u8>)> {
		backend
			.state_at(header.hash(), TrieCacheContext::Untrusted)
			.unwrap()
			.pairs(Default::default())
			.unwrap()
			.map(Result::unwrap)
			.collect()
	}

	/// Wraps a database of any kind. Fails the `n`th `commit` after `fail_commit(n)`, and records
	/// the key of every write to the `STATE` column.
	struct ObservingDb {
		inner: Arc<dyn Database<DbHash>>,
		/// Commits left before the one that fails. Zero means none will.
		commits_until_failure: AtomicUsize,
		state_writes: Mutex<Vec<Vec<u8>>>,
	}

	impl ObservingDb {
		/// Makes the `nth` commit from now fail. The ones before and after it go through.
		fn fail_commit(&self, nth: usize) {
			self.commits_until_failure.store(nth, Ordering::SeqCst);
		}

		fn armed(&self) -> bool {
			self.commits_until_failure.load(Ordering::SeqCst) > 0
		}
	}

	impl Database<DbHash> for ObservingDb {
		fn commit(&self, transaction: Transaction<DbHash>) -> sp_database::error::Result<()> {
			let left = self.commits_until_failure.fetch_update(
				Ordering::SeqCst,
				Ordering::SeqCst,
				|left| (left > 0).then(|| left - 1),
			);
			if left == Ok(1) {
				return Err(sp_database::error::DatabaseError(Box::new(std::io::Error::other(
					"injected commit failure",
				))));
			}
			for change in &transaction.0 {
				if let sp_database::Change::Set(col, key, _) = change {
					if *col == columns::STATE {
						self.state_writes.lock().push(key.clone());
					}
				}
			}
			self.inner.commit(transaction)
		}

		fn get(&self, col: sp_database::ColumnId, key: &[u8]) -> Option<Vec<u8>> {
			self.inner.get(col, key)
		}

		fn contains(&self, col: sp_database::ColumnId, key: &[u8]) -> bool {
			self.inner.contains(col, key)
		}

		fn value_size(&self, col: sp_database::ColumnId, key: &[u8]) -> Option<usize> {
			self.inner.value_size(col, key)
		}

		fn with_get(&self, col: sp_database::ColumnId, key: &[u8], f: &mut dyn FnMut(&[u8])) {
			self.inner.with_get(col, key, f)
		}

		// The key layout follows these two, so they must be the inner database's answers.
		fn supports_ref_counting(&self) -> bool {
			self.inner.supports_ref_counting()
		}

		fn sanitize_key(&self, key: &mut Vec<u8>) {
			self.inner.sanitize_key(key)
		}

		fn optimize_db_col(&self, col: sp_database::ColumnId) -> sp_database::error::Result<()> {
			self.inner.optimize_db_col(col)
		}
	}
}
