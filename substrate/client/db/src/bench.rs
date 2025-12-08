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

//! State backend that's useful for benchmarking

use crate::{DbState, DbStateBuilder};
use hash_db::{Hasher as DbHasher, Prefix};
use kvdb::{DBTransaction, KeyValueDB};
use linked_hash_map::LinkedHashMap;
use parking_lot::Mutex;
use sp_core::{
	hexdisplay::HexDisplay,
	storage::{ChildInfo, TrackedStorageKey},
};
use sp_runtime::{traits::Hash, StateVersion, Storage};
use sp_state_machine::{
	backend::Backend as StateBackend, BackendTransaction, ChildStorageCollection, DBValue,
	IterArgs, StorageCollection, StorageIterator, StorageKey, StorageValue,
};
use sp_trie::{
	cache::{CacheSize, SharedTrieCache},
	prefixed_key, MemoryDB, MerkleValue,
};
use std::{
	cell::{Cell, RefCell},
	collections::HashMap,
	sync::Arc,
};

type State<H> = DbState<H>;

struct StorageDb<Hasher> {
	db: Arc<dyn KeyValueDB>,
	_phantom: std::marker::PhantomData<Hasher>,
}

impl<Hasher: Hash> sp_state_machine::Storage<Hasher> for StorageDb<Hasher> {
	fn get(&self, key: &Hasher::Output, prefix: Prefix) -> Result<Option<DBValue>, String> {
		let prefixed_key = prefixed_key::<Hasher>(key, prefix);
		self.db
			.get(0, &prefixed_key)
			.map_err(|e| format!("Database backend error: {:?}", e))
	}
}

/// Tracks storage access during benchmarking for accurate weight calculation.
///
/// # Overview
///
/// This struct tracks all storage reads and writes that occur during benchmarking.
/// It supports prefix-based whitelisting, which allows excluding certain storage
/// keys from weight calculations (e.g., system-level storage with fixed costs).
///
/// # Key Features
///
/// 1. **Prefix-based whitelisting**: Storage keys can be whitelisted by prefix, making it easy to
///    exclude entire storage maps or specific pallet storage.
/// 2. **Child trie support**: Tracks storage in both main and child tries.
/// 3. **Efficient tracking**: Minimizes overhead when tracking is disabled.
/// 4. **Read/write aggregation**: Combines multiple accesses to the same key.
///
/// # Example
///
/// ```
/// let mut tracker = KeyTracker::new(true);
/// tracker.add_whitelist(&[TrackedStorageKey::new(b"System::")]);
/// tracker.add_read_key(None, b"System::Account"); // Whitelisted
/// tracker.add_read_key(None, b"Balances::Total"); // Not whitelisted
/// ```
struct KeyTracker {
	enable_tracking: bool,
	/// Key tracker for keys in the main trie.
	/// We track the total number of reads and writes to these keys,
	/// not de-duplicated for repeats.
	main_keys: LinkedHashMap<Vec<u8>, TrackedStorageKey>,
	/// Key tracker for keys in a child trie.
	/// Child trie are identified by their storage key (i.e. `ChildInfo::storage_key()`)
	/// We track the total number of reads and writes to these keys,
	/// not de-duplicated for repeats.
	child_keys: LinkedHashMap<Vec<u8>, LinkedHashMap<Vec<u8>, TrackedStorageKey>>,
	/// Storage key prefixes that should be excluded from weight calculations.
	///
	/// # Usage
	/// Typically used for system-level storage (e.g., "System", "Timestamp") that
	/// have fixed costs and shouldn't affect transaction weight calculations.
	/// Keys matching any prefix in this list are tracked but marked as whitelisted.
	whitelisted_prefixes: Vec<Vec<u8>>,
}

/// State that manages the backend database reference. Allows runtime to control the database.
pub struct BenchmarkingState<Hasher: Hash> {
	root: Cell<Hasher::Output>,
	genesis_root: Hasher::Output,
	state: RefCell<Option<State<Hasher>>>,
	db: Cell<Option<Arc<dyn KeyValueDB>>>,
	genesis: HashMap<Vec<u8>, (Vec<u8>, i32)>,
	record: Cell<Vec<Vec<u8>>>,
	key_tracker: Arc<Mutex<KeyTracker>>,
	whitelist: RefCell<Vec<TrackedStorageKey>>,
	proof_recorder: Option<sp_trie::recorder::Recorder<Hasher>>,
	proof_recorder_root: Cell<Hasher::Output>,
	shared_trie_cache: SharedTrieCache<Hasher>,
}

/// A raw iterator over the `BenchmarkingState`.
pub struct RawIter<Hasher: Hash> {
	inner: <DbState<Hasher> as StateBackend<Hasher>>::RawIter,
	child_trie: Option<Vec<u8>>,
	key_tracker: Arc<Mutex<KeyTracker>>,
}

impl<Hasher: Hash> StorageIterator<Hasher> for RawIter<Hasher> {
	type Backend = BenchmarkingState<Hasher>;
	type Error = String;

	fn next_key(&mut self, backend: &Self::Backend) -> Option<Result<StorageKey, Self::Error>> {
		match self.inner.next_key(backend.state.borrow().as_ref()?) {
			Some(Ok(key)) => {
				self.key_tracker.lock().add_read_key(self.child_trie.as_deref(), &key);
				Some(Ok(key))
			},
			result => result,
		}
	}

	fn next_pair(
		&mut self,
		backend: &Self::Backend,
	) -> Option<Result<(StorageKey, StorageValue), Self::Error>> {
		match self.inner.next_pair(backend.state.borrow().as_ref()?) {
			Some(Ok((key, value))) => {
				self.key_tracker.lock().add_read_key(self.child_trie.as_deref(), &key);
				Some(Ok((key, value)))
			},
			result => result,
		}
	}

	fn was_complete(&self) -> bool {
		self.inner.was_complete()
	}
}

impl<Hasher: Hash> BenchmarkingState<Hasher> {
	/// Create a new instance that creates a database in a temporary dir.
	pub fn new(
		genesis: Storage,
		_cache_size_mb: Option<usize>,
		record_proof: bool,
		enable_tracking: bool,
	) -> Result<Self, String> {
		let state_version = sp_runtime::StateVersion::default();
		let mut root = Default::default();
		let mut mdb = MemoryDB::<Hasher>::default();
		sp_trie::trie_types::TrieDBMutBuilderV1::<Hasher>::new(&mut mdb, &mut root).build();

		let mut state = BenchmarkingState {
			state: RefCell::new(None),
			db: Cell::new(None),
			root: Cell::new(root),
			genesis: Default::default(),
			genesis_root: Default::default(),
			record: Default::default(),
			key_tracker: Arc::new(Mutex::new(KeyTracker::new(enable_tracking))),
			whitelist: Default::default(),
			proof_recorder: record_proof.then(Default::default),
			proof_recorder_root: Cell::new(root),
			// Enable the cache, but do not sync anything to the shared state.
			shared_trie_cache: SharedTrieCache::new(CacheSize::new(0), None),
		};

		state.add_whitelist_to_tracker();

		state.reopen()?;
		let child_delta = genesis.children_default.values().map(|child_content| {
			(
				&child_content.child_info,
				child_content.data.iter().map(|(k, v)| (k.as_ref(), Some(v.as_ref()))),
			)
		});
		let (root, transaction): (Hasher::Output, _) =
			state.state.borrow().as_ref().unwrap().full_storage_root(
				genesis.top.iter().map(|(k, v)| (k.as_ref(), Some(v.as_ref()))),
				child_delta,
				state_version,
			);
		state.genesis = transaction.clone().drain();
		state.genesis_root = root;
		state.commit(root, transaction, Vec::new(), Vec::new())?;
		state.record.take();
		Ok(state)
	}

	/// Get the proof recorder for this state
	pub fn recorder(&self) -> Option<sp_trie::recorder::Recorder<Hasher>> {
		self.proof_recorder.clone()
	}

	fn reopen(&self) -> Result<(), String> {
		*self.state.borrow_mut() = None;
		let db = match self.db.take() {
			Some(db) => db,
			None => Arc::new(kvdb_memorydb::create(1)),
		};
		self.db.set(Some(db.clone()));
		if let Some(recorder) = &self.proof_recorder {
			recorder.reset();
			self.proof_recorder_root.set(self.root.get());
		}
		let storage_db = Arc::new(StorageDb::<Hasher> { db, _phantom: Default::default() });
		*self.state.borrow_mut() = Some(
			DbStateBuilder::<Hasher>::new(storage_db, self.root.get())
				.with_optional_recorder(self.proof_recorder.clone())
				.with_cache(self.shared_trie_cache.local_cache_trusted())
				.build(),
		);
		Ok(())
	}

	fn add_whitelist_to_tracker(&self) {
		self.key_tracker.lock().add_whitelist(&self.whitelist.borrow());
	}

	fn wipe_tracker(&self) {
		let mut key_tracker = self.key_tracker.lock();
		key_tracker.main_keys = LinkedHashMap::new();
		key_tracker.child_keys = LinkedHashMap::new();
		key_tracker.add_whitelist(&self.whitelist.borrow());
	}

	fn add_read_key(&self, childtrie: Option<&[u8]>, key: &[u8]) {
		self.key_tracker.lock().add_read_key(childtrie, key);
	}

	fn add_write_key(&self, childtrie: Option<&[u8]>, key: &[u8]) {
		self.key_tracker.lock().add_write_key(childtrie, key);
	}

	fn all_trackers(&self) -> Vec<TrackedStorageKey> {
		self.key_tracker.lock().all_trackers()
	}
}

impl KeyTracker {
	/// Creates a new KeyTracker with tracking enabled or disabled.
	///
	/// When `enable_tracking` is false, all subsequent tracking calls become no-ops,
	/// minimizing benchmarking overhead when detailed tracking isn't needed.
	fn new(enable_tracking: bool) -> Self {
		Self {
			enable_tracking,
			main_keys: LinkedHashMap::new(),
			child_keys: LinkedHashMap::new(),
			whitelisted_prefixes: Vec::new(),
		}
	}

	/// Adds whitelist prefixes to exclude from weight calculations.
	///
	/// Whitelisted keys are still tracked (for completeness) but marked as such,
	/// so they can be filtered out when calculating storage access weights.
	/// This is crucial for accurate weight modeling of user transactions.
	fn add_whitelist(&mut self, whitelist: &[TrackedStorageKey]) {
		// Store the whitelisted prefixes for later checking
		self.whitelisted_prefixes.extend(whitelist.iter().map(|k| k.key.clone()));
	}

	/// Checks if a key matches any whitelisted prefix.
	///
	/// Used internally to determine if a key should be excluded from weight
	/// calculations. A key is considered whitelisted if it starts with any
	/// of the registered whitelist prefixes.
	fn is_whitelisted(&self, key: &[u8]) -> bool {
		self.whitelisted_prefixes.iter().any(|prefix| key.starts_with(prefix))
	}

	/// Records a read access to a storage key.
	///
	/// # Parameters
	/// - `childtrie`: Optional child trie identifier (`ChildInfo::storage_key()`)
	/// - `key`: The storage key being read
	///
	/// Whitelisted keys are tracked but marked as such, so they don't contribute
	/// to weight calculations. Each call increments the read counter, allowing
	/// analysis of repeated accesses within the same block.
	fn add_read_key(&mut self, childtrie: Option<&[u8]>, key: &[u8]) {
		if !self.enable_tracking {
			return
		}

		let is_whitelisted = self.is_whitelisted(key);

		let child_key_tracker = &mut self.child_keys;
		let main_key_tracker = &mut self.main_keys;

		let key_tracker = if let Some(childtrie) = childtrie {
			child_key_tracker.entry(childtrie.to_vec()).or_insert_with(LinkedHashMap::new)
		} else {
			main_key_tracker
		};

		let should_log = match key_tracker.get_mut(key) {
			None => {
				let mut tracker = TrackedStorageKey::new(key.to_vec());

				// Always count the operation internally
				tracker.add_read();

				if is_whitelisted {
					// Mark as whitelisted so it's excluded from weight calculation
					tracker.whitelist();
				}

				key_tracker.insert(key.to_vec(), tracker);

				// Log only if not whitelisted (first time we see this key)
				!is_whitelisted
			},
			Some(tracker) => {
				// Always count the operation internally
				tracker.add_read();

				// Update whitelist status if needed
				if is_whitelisted && !tracker.whitelisted {
					tracker.whitelist();
				}

				// Log only if this is the first read AND not whitelisted
				!tracker.has_been_read() && !tracker.whitelisted
			},
		};

		if should_log {
			if let Some(childtrie) = childtrie {
				log::trace!(
					target: "benchmark",
					"Childtrie Read: {} {}", HexDisplay::from(&childtrie), HexDisplay::from(&key)
				);
			} else {
				log::trace!(target: "benchmark", "Read: {}", HexDisplay::from(&key));
			}
		}
	}

	/// Records a write access to a storage key.
	///
	/// Writing to a key implicitly counts as reading it (for weight calculation
	/// purposes), since writes typically require reading the current value first
	/// in Substrate's storage model.
	fn add_write_key(&mut self, childtrie: Option<&[u8]>, key: &[u8]) {
		if !self.enable_tracking {
			return
		}

		// Check if key matches any whitelisted prefix
		let is_whitelisted = self.is_whitelisted(key);

		let child_key_tracker = &mut self.child_keys;
		let main_key_tracker = &mut self.main_keys;

		let key_tracker = if let Some(childtrie) = childtrie {
			child_key_tracker.entry(childtrie.to_vec()).or_insert_with(LinkedHashMap::new)
		} else {
			main_key_tracker
		};

		// If we have written to the key, we also consider that we have read from it.
		let should_log = match key_tracker.get_mut(key) {
			None => {
				let mut tracker = TrackedStorageKey::new(key.to_vec());

				// Always count the operation internally
				tracker.add_write();

				if is_whitelisted {
					tracker.whitelist();
				}

				key_tracker.insert(key.to_vec(), tracker);

				!is_whitelisted
			},
			Some(tracker) => {
				// Always count the operation internally
				tracker.add_write();

				if is_whitelisted && !tracker.whitelisted {
					tracker.whitelist();
				}

				// Log only if this is the first write AND not whitelisted
				!tracker.has_been_written() && !tracker.whitelisted
			},
		};

		if should_log {
			if let Some(childtrie) = childtrie {
				log::trace!(
					target: "benchmark",
					"Childtrie Write: {} {}", HexDisplay::from(&childtrie), HexDisplay::from(&key)
				);
			} else {
				log::trace!(target: "benchmark", "Write: {}", HexDisplay::from(&key));
			}
		}
	}

	/// Returns all tracked keys from both main and child tries.
	///
	/// This flattened view is useful for generating comprehensive storage
	/// access reports and calculating total read/write statistics.
	fn all_trackers(&self) -> Vec<TrackedStorageKey> {
		let mut all_trackers = Vec::new();

		self.main_keys.iter().for_each(|(_, tracker)| {
			all_trackers.push(tracker.clone());
		});

		self.child_keys.iter().for_each(|(_, child_tracker)| {
			child_tracker.iter().for_each(|(_, tracker)| {
				all_trackers.push(tracker.clone());
			});
		});

		all_trackers
	}
}

fn state_err() -> String {
	"State is not open".into()
}

impl<Hasher: Hash> StateBackend<Hasher> for BenchmarkingState<Hasher> {
	type Error = <DbState<Hasher> as StateBackend<Hasher>>::Error;
	type TrieBackendStorage = <DbState<Hasher> as StateBackend<Hasher>>::TrieBackendStorage;
	type RawIter = RawIter<Hasher>;

	fn storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
		self.add_read_key(None, key);
		self.state.borrow().as_ref().ok_or_else(state_err)?.storage(key)
	}

	fn storage_hash(&self, key: &[u8]) -> Result<Option<Hasher::Output>, Self::Error> {
		self.add_read_key(None, key);
		self.state.borrow().as_ref().ok_or_else(state_err)?.storage_hash(key)
	}

	fn child_storage(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, Self::Error> {
		self.add_read_key(Some(child_info.storage_key()), key);
		self.state
			.borrow()
			.as_ref()
			.ok_or_else(state_err)?
			.child_storage(child_info, key)
	}

	fn child_storage_hash(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<Hasher::Output>, Self::Error> {
		self.add_read_key(Some(child_info.storage_key()), key);
		self.state
			.borrow()
			.as_ref()
			.ok_or_else(state_err)?
			.child_storage_hash(child_info, key)
	}

	fn closest_merkle_value(
		&self,
		key: &[u8],
	) -> Result<Option<MerkleValue<Hasher::Output>>, Self::Error> {
		self.add_read_key(None, key);
		self.state.borrow().as_ref().ok_or_else(state_err)?.closest_merkle_value(key)
	}

	fn child_closest_merkle_value(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<MerkleValue<Hasher::Output>>, Self::Error> {
		self.add_read_key(None, key);
		self.state
			.borrow()
			.as_ref()
			.ok_or_else(state_err)?
			.child_closest_merkle_value(child_info, key)
	}

	fn exists_storage(&self, key: &[u8]) -> Result<bool, Self::Error> {
		self.add_read_key(None, key);
		self.state.borrow().as_ref().ok_or_else(state_err)?.exists_storage(key)
	}

	fn exists_child_storage(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<bool, Self::Error> {
		self.add_read_key(Some(child_info.storage_key()), key);
		self.state
			.borrow()
			.as_ref()
			.ok_or_else(state_err)?
			.exists_child_storage(child_info, key)
	}

	fn next_storage_key(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
		self.add_read_key(None, key);
		self.state.borrow().as_ref().ok_or_else(state_err)?.next_storage_key(key)
	}

	fn next_child_storage_key(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, Self::Error> {
		self.add_read_key(Some(child_info.storage_key()), key);
		self.state
			.borrow()
			.as_ref()
			.ok_or_else(state_err)?
			.next_child_storage_key(child_info, key)
	}

	fn storage_root<'a>(
		&self,
		delta: impl Iterator<Item = (&'a [u8], Option<&'a [u8]>)>,
		state_version: StateVersion,
	) -> (Hasher::Output, BackendTransaction<Hasher>) {
		self.state
			.borrow()
			.as_ref()
			.map_or(Default::default(), |s| s.storage_root(delta, state_version))
	}

	fn child_storage_root<'a>(
		&self,
		child_info: &ChildInfo,
		delta: impl Iterator<Item = (&'a [u8], Option<&'a [u8]>)>,
		state_version: StateVersion,
	) -> (Hasher::Output, bool, BackendTransaction<Hasher>) {
		self.state
			.borrow()
			.as_ref()
			.map_or(Default::default(), |s| s.child_storage_root(child_info, delta, state_version))
	}

	fn raw_iter(&self, args: IterArgs) -> Result<Self::RawIter, Self::Error> {
		let child_trie =
			args.child_info.as_ref().map(|child_info| child_info.storage_key().to_vec());
		self.state
			.borrow()
			.as_ref()
			.map(|s| s.raw_iter(args))
			.unwrap_or(Ok(Default::default()))
			.map(|raw_iter| RawIter {
				inner: raw_iter,
				key_tracker: self.key_tracker.clone(),
				child_trie,
			})
	}

	fn commit(
		&self,
		storage_root: <Hasher as DbHasher>::Out,
		mut transaction: BackendTransaction<Hasher>,
		main_storage_changes: StorageCollection,
		child_storage_changes: ChildStorageCollection,
	) -> Result<(), Self::Error> {
		if let Some(db) = self.db.take() {
			let mut db_transaction = DBTransaction::new();
			let changes = transaction.drain();
			let mut keys = Vec::with_capacity(changes.len());
			for (key, (val, rc)) in changes {
				if rc > 0 {
					db_transaction.put(0, &key, &val);
				} else if rc < 0 {
					db_transaction.delete(0, &key);
				}
				keys.push(key);
			}
			let mut record = self.record.take();
			record.extend(keys);
			self.record.set(record);
			db.write(db_transaction)
				.map_err(|_| String::from("Error committing transaction"))?;
			self.root.set(storage_root);
			self.db.set(Some(db));

			// Track DB Writes
			main_storage_changes.iter().for_each(|(key, _)| {
				self.add_write_key(None, key);
			});
			child_storage_changes.iter().for_each(|(child_storage_key, storage_changes)| {
				storage_changes.iter().for_each(|(key, _)| {
					self.add_write_key(Some(child_storage_key), key);
				})
			});
		} else {
			return Err("Trying to commit to a closed db".into())
		}
		self.reopen()
	}

	fn wipe(&self) -> Result<(), Self::Error> {
		// Restore to genesis
		let record = self.record.take();
		if let Some(db) = self.db.take() {
			let mut db_transaction = DBTransaction::new();
			for key in record {
				match self.genesis.get(&key) {
					Some((v, _)) => db_transaction.put(0, &key, v),
					None => db_transaction.delete(0, &key),
				}
			}
			db.write(db_transaction)
				.map_err(|_| String::from("Error committing transaction"))?;
			self.db.set(Some(db));
		}

		self.root.set(self.genesis_root);
		self.reopen()?;
		self.wipe_tracker();
		Ok(())
	}

	/// Get the key tracking information for the state db.
	/// 1. `reads` - Total number of DB reads.
	/// 2. `repeat_reads` - Total number of in-memory reads.
	/// 3. `writes` - Total number of DB writes.
	/// 4. `repeat_writes` - Total number of in-memory writes.
	fn read_write_count(&self) -> (u32, u32, u32, u32) {
		let mut reads = 0;
		let mut repeat_reads = 0;
		let mut writes = 0;
		let mut repeat_writes = 0;

		self.all_trackers().iter().for_each(|tracker| {
			if !tracker.whitelisted {
				if tracker.reads > 0 {
					reads += 1;
					repeat_reads += tracker.reads - 1;
				}

				if tracker.writes > 0 {
					writes += 1;
					repeat_writes += tracker.writes - 1;
				}
			}
		});
		(reads, repeat_reads, writes, repeat_writes)
	}

	/// Reset the key tracking information for the state db.
	fn reset_read_write_count(&self) {
		self.wipe_tracker()
	}

	fn get_whitelist(&self) -> Vec<TrackedStorageKey> {
		self.whitelist.borrow().to_vec()
	}

	fn set_whitelist(&self, new: Vec<TrackedStorageKey>) {
		*self.whitelist.borrow_mut() = new;
	}

	fn get_read_and_written_keys(&self) -> Vec<(Vec<u8>, u32, u32, bool)> {
		// We only track at the level of a key-prefix and not whitelisted for now for memory size.
		// TODO: Refactor to enable full storage key transparency, where we can remove the
		// `prefix_key_tracker`.
		let mut prefix_key_tracker = LinkedHashMap::<Vec<u8>, (u32, u32, bool)>::new();
		self.all_trackers().iter().for_each(|tracker| {
			if !tracker.whitelisted {
				let prefix_length = tracker.key.len().min(32);
				let prefix = tracker.key[0..prefix_length].to_vec();
				// each read / write of a specific key is counted at most one time, since
				// additional reads / writes happen in the memory overlay.
				let reads = tracker.reads.min(1);
				let writes = tracker.writes.min(1);
				if let Some(prefix_tracker) = prefix_key_tracker.get_mut(&prefix) {
					prefix_tracker.0 += reads;
					prefix_tracker.1 += writes;
				} else {
					prefix_key_tracker.insert(prefix, (reads, writes, tracker.whitelisted));
				}
			}
		});

		prefix_key_tracker
			.iter()
			.map(|(key, tracker)| -> (Vec<u8>, u32, u32, bool) {
				(key.to_vec(), tracker.0, tracker.1, tracker.2)
			})
			.collect::<Vec<_>>()
	}

	fn register_overlay_stats(&self, stats: &sp_state_machine::StateMachineStats) {
		self.state.borrow().as_ref().map(|s| s.register_overlay_stats(stats));
	}

	fn usage_info(&self) -> sp_state_machine::UsageInfo {
		self.state
			.borrow()
			.as_ref()
			.map_or(sp_state_machine::UsageInfo::empty(), |s| s.usage_info())
	}

	fn proof_size(&self) -> Option<u32> {
		self.proof_recorder.as_ref().map(|recorder| {
			let proof_size = recorder.estimate_encoded_size() as u32;

			let proof = recorder.to_storage_proof();

			let proof_recorder_root = self.proof_recorder_root.get();
			if proof_recorder_root == Default::default() || proof_size == 1 {
				// empty trie
				log::debug!(target: "benchmark", "Some proof size: {}", &proof_size);
				proof_size
			} else {
				if let Some(size) = proof.encoded_compact_size::<Hasher>(proof_recorder_root) {
					size as u32
				} else if proof_recorder_root == self.root.get() {
					log::debug!(target: "benchmark", "No changes - no proof");
					0
				} else {
					panic!(
						"proof rec root {:?}, root {:?}, genesis {:?}, rec_len {:?}",
						self.proof_recorder_root.get(),
						self.root.get(),
						self.genesis_root,
						proof_size,
					);
				}
			}
		})
	}
}

impl<Hasher: Hash> std::fmt::Debug for BenchmarkingState<Hasher> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "Bench DB")
	}
}

#[cfg(test)]
mod test {
	use crate::bench::{BenchmarkingState, KeyTracker, TrackedStorageKey};
	use sp_runtime::traits::{BlakeTwo256, HashingFor};
	use sp_state_machine::backend::Backend as _;

	fn hex(hex: &str) -> Vec<u8> {
		array_bytes::hex2bytes(hex).unwrap()
	}

	#[test]
	fn iteration_is_also_counted_in_rw_counts() {
		let storage = sp_runtime::Storage {
			top: vec![(
				hex("ce6e1397e668c7fcf47744350dc59688455a2c2dbd2e2a649df4e55d93cd7158"),
				hex("0102030405060708"),
			)]
			.into_iter()
			.collect(),
			..sp_runtime::Storage::default()
		};
		let bench_state =
			BenchmarkingState::<HashingFor<crate::tests::Block>>::new(storage, None, false, true)
				.unwrap();

		assert_eq!(bench_state.read_write_count(), (0, 0, 0, 0));
		assert_eq!(bench_state.keys(Default::default()).unwrap().count(), 1);
		assert_eq!(bench_state.read_write_count(), (1, 0, 0, 0));
	}

	#[test]
	fn read_to_main_and_child_tries() {
		let bench_state = BenchmarkingState::<HashingFor<crate::tests::Block>>::new(
			Default::default(),
			None,
			false,
			true,
		)
		.unwrap();

		for _ in 0..2 {
			let child1 = sp_core::storage::ChildInfo::new_default(b"child1");
			let child2 = sp_core::storage::ChildInfo::new_default(b"child2");

			bench_state.storage(b"foo").unwrap();
			bench_state.child_storage(&child1, b"foo").unwrap();
			bench_state.child_storage(&child2, b"foo").unwrap();

			bench_state.storage(b"bar").unwrap();
			bench_state.child_storage(&child1, b"bar").unwrap();
			bench_state.child_storage(&child2, b"bar").unwrap();

			bench_state
				.commit(
					Default::default(),
					Default::default(),
					vec![("foo".as_bytes().to_vec(), None)],
					vec![("child1".as_bytes().to_vec(), vec![("foo".as_bytes().to_vec(), None)])],
				)
				.unwrap();

			let rw_tracker = bench_state.read_write_count();
			assert_eq!(rw_tracker.0, 6);
			assert_eq!(rw_tracker.1, 0);
			assert_eq!(rw_tracker.2, 2);
			assert_eq!(rw_tracker.3, 0);
			bench_state.wipe().unwrap();
		}
	}

	#[test]
	fn test_storage_operations_with_whitelist() {
		let state =
			BenchmarkingState::<BlakeTwo256>::new(Default::default(), None, false, true).unwrap();

		// Set comprehensive whitelist
		state.set_whitelist(vec![
			TrackedStorageKey::new(b"system".to_vec()),
			TrackedStorageKey::new(b"timestamp".to_vec()),
		]);

		state.reset_read_write_count();

		// Perform various storage operations
		state.storage(b"system_account").unwrap();
		state.storage(b"timestamp_now").unwrap();
		state.storage(b"balances_account").unwrap();

		let (reads, repeat_reads, writes, _) = state.read_write_count();

		// Only non-whitelisted keys should count toward reads
		assert_eq!(reads, 1); // balances_account
		assert_eq!(repeat_reads, 0);
	}

	#[test]
	fn test_whitelisted_prefix_matching() {
		let mut tracker = KeyTracker::new(true);

		// Set up whitelisted prefixes
		let whitelist = vec![
			TrackedStorageKey::new(b"system".to_vec()),
			TrackedStorageKey::new(b"balances".to_vec()),
		];
		tracker.add_whitelist(&whitelist);

		// Test various key scenarios
		tracker.add_read_key(None, b"system_account"); // Should be whitelisted
		tracker.add_read_key(None, b"balances_total"); // Should be whitelisted
		tracker.add_read_key(None, b"timestamp_now"); // Should NOT be whitelisted
		tracker.add_read_key(None, b"vesting_account"); // Should NOT be whitelisted

		let all_trackers = tracker.all_trackers();
		let whitelisted: Vec<_> = all_trackers.iter().filter(|t| t.whitelisted).collect();
		let non_whitelisted: Vec<_> = all_trackers.iter().filter(|t| !t.whitelisted).collect();

		assert_eq!(whitelisted.len(), 2);
		assert_eq!(non_whitelisted.len(), 2);
	}

	#[test]
	fn test_read_write_counting_with_whitelist() {
		let mut tracker = KeyTracker::new(true);

		let whitelist = vec![TrackedStorageKey::new(b"system".to_vec())];
		tracker.add_whitelist(&whitelist);

		// Add multiple reads/writes to same keys
		tracker.add_read_key(None, b"system_account");
		tracker.add_read_key(None, b"system_account");
		tracker.add_read_key(None, b"system_balance");
		tracker.add_write_key(None, b"uniques_account");
		tracker.add_write_key(None, b"uniques_account");

		let trackers = tracker.all_trackers();

		let system_account_tracker =
			trackers.iter().find(|t| t.key == b"system_account".to_vec()).unwrap();
		let system_balance_tracker =
			trackers.iter().find(|t| t.key == b"system_balance".to_vec()).unwrap();
		let uniques_account_tracker =
			trackers.iter().find(|t| t.key == b"uniques_account".to_vec()).unwrap();

		// Whitelisted key should have reads/writes but marked as whitelisted
		assert!(system_account_tracker.whitelisted);
		assert_eq!(system_account_tracker.reads, 2);
		assert_eq!(system_account_tracker.writes, 0);

		assert!(system_balance_tracker.whitelisted);
		assert_eq!(system_balance_tracker.reads, 1);

		// Normal key should have reads/writes and NOT be whitelisted
		assert!(!uniques_account_tracker.whitelisted);
		assert_eq!(uniques_account_tracker.reads, 0);
		assert_eq!(uniques_account_tracker.writes, 2);
	}

	#[test]
	fn test_child_trie_tracking_with_whitelist() {
		let mut tracker = KeyTracker::new(true);

		let whitelist = vec![TrackedStorageKey::new(b"child_whitelist".to_vec())];
		tracker.add_whitelist(&whitelist);

		let child_info = b"child_trie_1";

		// Whitelisted child trie key
		tracker.add_read_key(Some(child_info), b"child_whitelist_key");
		tracker.add_write_key(Some(child_info), b"child_whitelist_key");

		// Normal child trie key
		tracker.add_read_key(Some(child_info), b"child_normal_key");
		tracker.add_write_key(Some(child_info), b"child_normal_key");

		let all_trackers = tracker.all_trackers();

		let whitelisted_child =
			all_trackers.iter().find(|t| t.key == b"child_whitelist_key".to_vec()).unwrap();
		let normal_child =
			all_trackers.iter().find(|t| t.key == b"child_normal_key".to_vec()).unwrap();

		assert!(whitelisted_child.whitelisted);
		assert!(!normal_child.whitelisted);
	}

	#[test]
	fn test_prefix_based_whitelisting() {
		let mut tracker = KeyTracker::new(true);

		// Test with various prefix lengths
		let whitelist = vec![
			TrackedStorageKey::new(b"sys".to_vec()),     // 3 byte prefix
			TrackedStorageKey::new(b"balance".to_vec()), // 7 byte prefix
			TrackedStorageKey::new(b"timestamp_".to_vec()), // 10 byte prefix
		];
		tracker.add_whitelist(&whitelist);

		// Test keys that should match prefixes
		assert!(tracker.is_whitelisted(b"system_account"));
		assert!(tracker.is_whitelisted(b"balances_total"));
		assert!(tracker.is_whitelisted(b"timestamp_now"));

		// Test keys that should NOT match
		assert!(!tracker.is_whitelisted(b"account_info"));
		assert!(!tracker.is_whitelisted(b"total_balance"));
		assert!(!tracker.is_whitelisted(b"now_timestamp"));
	}

	#[test]
	fn test_late_whitelist_update() {
		let mut tracker = KeyTracker::new(true);

		// 1. Read a key when it's NOT whitelisted
		tracker.add_read_key(None, b"timestamp_now");

		// 2. Later add it to whitelist
		tracker.add_whitelist(&[TrackedStorageKey::new(b"timestamp".to_vec())]);

		// 3. Read it again - should now be whitelisted
		tracker.add_read_key(None, b"timestamp_now");

		let all_trackers = tracker.all_trackers();

		let timestamp = all_trackers.iter().find(|t| t.key == b"timestamp_now".to_vec()).unwrap();

		assert!(timestamp.whitelisted);
		assert_eq!(timestamp.reads, 2);
	}

	#[test]
	fn test_tracking_disabled() {
		let mut tracker = KeyTracker::new(false);
		// These should be no-ops
		tracker.add_read_key(None, b"some_key");
		tracker.add_write_key(None, b"another_key");
		assert!(tracker.main_keys.is_empty());
		assert!(tracker.child_keys.is_empty());
	}

	#[test]
	fn test_overlapping_whitelist_prefixes() {
		let mut tracker = KeyTracker::new(true);

		// Short prefix first
		tracker.add_whitelist(&[TrackedStorageKey::new(b"sys".to_vec())]);
		tracker.add_read_key(None, b"system_account");

		// Add longer overlapping prefix
		tracker.add_whitelist(&[TrackedStorageKey::new(b"system".to_vec())]);
		tracker.add_read_key(None, b"system_account");

		let key = b"system_account".as_slice();

		// Get the tracked key
		let tracker_entry = tracker.main_keys.get(key).unwrap();

		assert!(tracker_entry.whitelisted, "Key should be whitelisted");
		assert_eq!(tracker_entry.reads, 2, "Both reads should be counted");

		// No merging/deduplication logic, see [substrate::frame::benchmarking::utils]
		assert_eq!(tracker.whitelisted_prefixes.len(), 2, "Should have both prefixes in whitelist");

		// Verify both prefixes match
		assert!(tracker.is_whitelisted(b"system_account"), "Key should match both prefixes");
		assert!(tracker.is_whitelisted(b"system_balance"), "Should also match 'sys' prefix");
		assert!(tracker.is_whitelisted(b"sys_other"), "Should match short prefix");
		assert!(
			!tracker.is_whitelisted(b"ystematic_error"), // Doesn't start with "sys" or "system"
			"Should NOT match - starts with 'ystemat', not 'sys' or 'system'"
		);
	}
}
