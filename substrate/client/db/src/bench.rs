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
use linked_hash_map::LinkedHashMap;
use parking_lot::Mutex;
use sp_core::{
	hexdisplay::HexDisplay,
	storage::{ChildInfo, TrackedStorageKey},
};
use sp_runtime::{traits::Hash, StateVersion, Storage};
use sp_state_machine::{
	backend::{Backend as StateBackend, DBLocation},
	BackendTransaction, ChildStorageCollection, IterArgs, StorageCollection, StorageIterator,
	StorageKey, StorageValue,
};
use sp_trie::{
	cache::{CacheSize, SharedTrieCache},
	ChildChangeset, MemoryDB, MerkleValue,
};
use std::{
	cell::{Cell, RefCell},
	sync::Arc,
};

type State<H> = DbState<H>;

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
}

/// State that manages the backend database reference. Allows runtime to control the database.
pub struct BenchmarkingState<Hasher: Hash> {
	genesis_root: Hasher::Output,
	genesis: MemoryDB<Hasher>,
	state: RefCell<Option<State<Hasher>>>,
	key_tracker: Arc<Mutex<KeyTracker>>,
	whitelist: RefCell<Vec<TrackedStorageKey>>,
	proof_recorder: Option<sp_trie::recorder::Recorder<Hasher, DBLocation>>,
	proof_recorder_root: Cell<Hasher::Output>,
	shared_trie_cache: SharedTrieCache<Hasher, DBLocation>,
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
		let mdb = MemoryDB::<Hasher>::default();
		let root = sp_trie::trie_types::TrieDBMutBuilderV1::<Hasher>::new(&mdb)
			.build()
			.commit()
			.root_hash();

		let mut state = BenchmarkingState {
			state: RefCell::new(None),
			genesis: Default::default(),
			genesis_root: Default::default(),
			key_tracker: Arc::new(Mutex::new(KeyTracker {
				main_keys: Default::default(),
				child_keys: Default::default(),
				enable_tracking,
			})),
			whitelist: Default::default(),
			proof_recorder: record_proof.then(Default::default),
			proof_recorder_root: Cell::new(root),
			// Enable the cache, but do not sync anything to the shared state.
			shared_trie_cache: SharedTrieCache::new(CacheSize::new(0)),
		};

		state.add_whitelist_to_tracker();

		*state.state.borrow_mut() =
			Some(DbStateBuilder::<Hasher>::new(Box::new(mdb), root).build());

		let child_delta = genesis.children_default.values().map(|child_content| {
			(
				&child_content.child_info,
				child_content.data.iter().map(|(k, v)| (k.as_ref(), Some(v.as_ref()))),
			)
		});
		let transaction = state.state.borrow().as_ref().unwrap().full_storage_root(
			genesis.top.iter().map(|(k, v)| (k.as_ref(), Some(v.as_ref()))),
			child_delta,
			state_version,
		);
		let mut genesis = MemoryDB::<Hasher>::default();
		let genesis_root = transaction.apply_to(&mut genesis);
		state.genesis = genesis;
		state.genesis_root = genesis_root;
		state.reopen()?;
		Ok(state)
	}

	fn reopen(&self) -> Result<(), String> {
		*self.state.borrow_mut() = None;
		let db = Box::new(self.genesis.clone());
		*self.state.borrow_mut() = Some(
			DbStateBuilder::<Hasher>::new(db, self.genesis_root)
				.with_optional_recorder(self.proof_recorder.clone())
				.with_cache(self.shared_trie_cache.local_cache())
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
	fn add_whitelist(&mut self, whitelist: &[TrackedStorageKey]) {
		whitelist.iter().for_each(|key| {
			let mut whitelisted = TrackedStorageKey::new(key.key.clone());
			whitelisted.whitelist();
			self.main_keys.insert(key.key.clone(), whitelisted);
		});
	}

	// Childtrie is identified by its storage key (i.e. `ChildInfo::storage_key`)
	fn add_read_key(&mut self, childtrie: Option<&[u8]>, key: &[u8]) {
		if !self.enable_tracking {
			return
		}

		let child_key_tracker = &mut self.child_keys;
		let main_key_tracker = &mut self.main_keys;

		let key_tracker = if let Some(childtrie) = childtrie {
			child_key_tracker.entry(childtrie.to_vec()).or_insert_with(LinkedHashMap::new)
		} else {
			main_key_tracker
		};

		let should_log = match key_tracker.get_mut(key) {
			None => {
				let mut has_been_read = TrackedStorageKey::new(key.to_vec());
				has_been_read.add_read();
				key_tracker.insert(key.to_vec(), has_been_read);
				true
			},
			Some(tracker) => {
				let should_log = !tracker.has_been_read();
				tracker.add_read();
				should_log
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

	// Childtrie is identified by its storage key (i.e. `ChildInfo::storage_key`)
	fn add_write_key(&mut self, childtrie: Option<&[u8]>, key: &[u8]) {
		if !self.enable_tracking {
			return
		}

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
				let mut has_been_written = TrackedStorageKey::new(key.to_vec());
				has_been_written.add_write();
				key_tracker.insert(key.to_vec(), has_been_written);
				true
			},
			Some(tracker) => {
				let should_log = !tracker.has_been_written();
				tracker.add_write();
				should_log
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

	// Return all the tracked storage keys among main and child trie.
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
		delta: impl Iterator<Item = (&'a [u8], Option<&'a [u8]>, ChildChangeset<Hasher::Out>)>,
		state_version: StateVersion,
	) -> BackendTransaction<Hasher::Out> {
		self.state
			.borrow()
			.as_ref()
			.map_or(BackendTransaction::unchanged(self.genesis_root, Default::default()), |s| {
				s.storage_root(delta, state_version)
			})
	}

	fn child_storage_root<'a>(
		&self,
		child_info: &ChildInfo,
		delta: impl Iterator<Item = (&'a [u8], Option<&'a [u8]>)>,
		state_version: StateVersion,
	) -> (BackendTransaction<Hasher::Out>, bool) {
		self.state.borrow().as_ref().map_or_else(
			|| (BackendTransaction::unchanged(self.genesis_root, Default::default()), true),
			|s| s.child_storage_root(child_info, delta, state_version),
		)
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
		transaction: BackendTransaction<Hasher::Out>,
		main_storage_changes: StorageCollection,
		child_storage_changes: ChildStorageCollection,
	) -> Result<(), Self::Error> {
		if let Some(state) = &mut *self.state.borrow_mut() {
			if let Some(mut db) = state.backend_storage_mut().as_mem_db_mut() {
				let root = transaction.apply_to(&mut db);
				state.set_root(root);
			} else if let Some(mut db) = state.backend_storage_mut().as_prefixed_mem_db_mut() {
				let root = transaction.apply_to(&mut db);
				state.set_root(root);
			} else {
				unreachable!()
			}

			// Track DB Writes
			main_storage_changes.iter().for_each(|(key, _)| {
				self.add_write_key(None, key);
			});
			child_storage_changes.iter().for_each(|(child_storage_key, storage_changes)| {
				storage_changes.iter().for_each(|(key, _)| {
					self.add_write_key(Some(child_storage_key), key);
				})
			});
		}
		Ok(())
	}

	fn wipe(&self) -> Result<(), Self::Error> {
		// Restore to genesis
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
				let root = if let Some(state) = self.state.borrow().as_ref().map(|s| *s.root()) {
					state
				} else {
					self.genesis_root
				};

				if let Some(size) = proof.encoded_compact_size::<Hasher>(proof_recorder_root) {
					size as u32
				} else if proof_recorder_root == root {
					log::debug!(target: "benchmark", "No changes - no proof");
					0
				} else {
					panic!(
						"proof rec root {:?}, root {:?}, genesis {:?}, rec_len {:?}",
						self.proof_recorder_root.get(),
						root,
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
	use crate::bench::BenchmarkingState;
	use sp_runtime::traits::HashingFor;
	use sp_state_machine::{backend::Backend as _, BackendTransaction};

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
					BackendTransaction::unchanged(Default::default(), Default::default()),
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
}
