// Copyright 2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! State backend that's useful for benchmarking

use std::sync::Arc;
use std::path::PathBuf;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use rand::Rng;

use hash_db::{Prefix, Hasher};
use sp_trie::{MemoryDB, prefixed_key};
use sp_core::storage::ChildInfo;
use sp_runtime::traits::{Block as BlockT, HashFor};
use sp_runtime::Storage;
use sp_state_machine::{DBValue, backend::Backend as StateBackend};
use kvdb::{KeyValueDB, DBTransaction};
use kvdb_rocksdb::{Database, DatabaseConfig};

type DbState<B> = sp_state_machine::TrieBackend<
	Arc<dyn sp_state_machine::Storage<HashFor<B>>>, HashFor<B>
>;

struct StorageDb<Block: BlockT> {
	db: Arc<dyn KeyValueDB>,
	_block: std::marker::PhantomData<Block>,
}

impl<Block: BlockT> sp_state_machine::Storage<HashFor<Block>> for StorageDb<Block> {
	fn get(&self, key: &Block::Hash, prefix: Prefix) -> Result<Option<DBValue>, String> {
		let key = prefixed_key::<HashFor<Block>>(key, prefix);
		self.db.get(0, &key)
			.map_err(|e| format!("Database backend error: {:?}", e))
	}
}

/// State that manages the backend database reference. Allows runtime to control the database.
pub struct BenchmarkingState<B: BlockT> {
	path: PathBuf,
	root: Cell<B::Hash>,
	genesis_root: B::Hash,
	state: RefCell<Option<DbState<B>>>,
	db: Cell<Option<Arc<dyn KeyValueDB>>>,
	genesis: HashMap<Vec<u8>, (Vec<u8>, i32)>,
	record: Cell<Vec<Vec<u8>>>,
	cache_size_mb: Option<usize>,
}

impl<B: BlockT> BenchmarkingState<B> {
	/// Create a new instance that creates a database in a temporary dir.
	pub fn new(genesis: Storage, cache_size_mb: Option<usize>) -> Result<Self, String> {
		let temp_dir = PathBuf::from(std::env::temp_dir());
		let name: String = rand::thread_rng().sample_iter(&rand::distributions::Alphanumeric).take(10).collect();
		let path = temp_dir.join(&name);

		let mut root = B::Hash::default();
		let mut mdb = MemoryDB::<HashFor<B>>::default();
		sp_state_machine::TrieDBMut::<HashFor<B>>::new(&mut mdb, &mut root);

		std::fs::create_dir(&path).map_err(|_| String::from("Error creating temp dir"))?;
		let mut state = BenchmarkingState {
			state: RefCell::new(None),
			db: Cell::new(None),
			path,
			root: Cell::new(root),
			genesis: Default::default(),
			genesis_root: Default::default(),
			record: Default::default(),
			cache_size_mb,
		};

		state.reopen()?;
		let child_delta = genesis.children.into_iter().map(|(storage_key, child_content)| (
			storage_key,
			child_content.data.into_iter().map(|(k, v)| (k, Some(v))),
			child_content.child_info
		));
		let (root, transaction): (B::Hash, _) = state.state.borrow_mut().as_mut().unwrap().full_storage_root(
			genesis.top.into_iter().map(|(k, v)| (k, Some(v))),
			child_delta,
		);
		state.genesis = transaction.clone().drain();
		state.genesis_root = root.clone();
		state.commit(root, transaction)?;
		Ok(state)
	}

	fn reopen(&self) -> Result<(), String> {
		*self.state.borrow_mut() = None;
		self.db.set(None);
		let mut db_config = DatabaseConfig::with_columns(1);
		if let Some(size) = &self.cache_size_mb {
			db_config.memory_budget.insert(0, *size);
		}
		let path = self.path.to_str()
			.ok_or_else(|| String::from("Invalid database path"))?;
		let db = Arc::new(Database::open(&db_config, &path).map_err(|e| format!("Error opening database: {:?}", e))?);
		self.db.set(Some(db.clone()));
		let storage_db = Arc::new(StorageDb::<B> { db, _block: Default::default() });
		*self.state.borrow_mut() = Some(DbState::<B>::new(storage_db, self.root.get()));
		Ok(())
	}

	fn kill(&self) -> Result<(), String> {
		self.db.set(None);
		*self.state.borrow_mut() = None;
		let mut root = B::Hash::default();
		let mut mdb = MemoryDB::<HashFor<B>>::default();
		sp_state_machine::TrieDBMut::<HashFor<B>>::new(&mut mdb, &mut root);
		self.root.set(root);

		std::fs::remove_dir_all(&self.path).map_err(|_| "Error removing database dir".into())
	}
}

impl<B: BlockT> Drop for BenchmarkingState<B> {
	fn drop(&mut self) {
		self.kill().ok();
	}
}

fn state_err() -> String {
	"State is not open".into()
}

impl<B: BlockT> StateBackend<HashFor<B>> for BenchmarkingState<B> {
	type Error =  <DbState<B> as StateBackend<HashFor<B>>>::Error;
	type Transaction = <DbState<B> as StateBackend<HashFor<B>>>::Transaction;
	type TrieBackendStorage = <DbState<B> as StateBackend<HashFor<B>>>::TrieBackendStorage;

	fn storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
		self.state.borrow().as_ref().ok_or_else(state_err)?.storage(key)
	}

	fn storage_hash(&self, key: &[u8]) -> Result<Option<B::Hash>, Self::Error> {
		self.state.borrow().as_ref().ok_or_else(state_err)?.storage_hash(key)
	}

	fn child_storage(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, Self::Error> {
		self.state.borrow().as_ref().ok_or_else(state_err)?.child_storage(storage_key, child_info, key)
	}

	fn exists_storage(&self, key: &[u8]) -> Result<bool, Self::Error> {
		self.state.borrow().as_ref().ok_or_else(state_err)?.exists_storage(key)
	}

	fn exists_child_storage(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		key: &[u8],
	) -> Result<bool, Self::Error> {
		self.state.borrow().as_ref().ok_or_else(state_err)?.exists_child_storage(storage_key, child_info, key)
	}

	fn next_storage_key(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
		self.state.borrow().as_ref().ok_or_else(state_err)?.next_storage_key(key)
	}

	fn next_child_storage_key(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, Self::Error> {
		self.state.borrow().as_ref().ok_or_else(state_err)?.next_child_storage_key(storage_key, child_info, key)
	}

	fn for_keys_with_prefix<F: FnMut(&[u8])>(&self, prefix: &[u8], f: F) {
		if let Some(ref state) = *self.state.borrow() {
			state.for_keys_with_prefix(prefix, f)
		}
	}

	fn for_key_values_with_prefix<F: FnMut(&[u8], &[u8])>(&self, prefix: &[u8], f: F) {
		if let Some(ref state) = *self.state.borrow() {
			state.for_key_values_with_prefix(prefix, f)
		}
	}

	fn for_keys_in_child_storage<F: FnMut(&[u8])>(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		f: F,
	) {
		if let Some(ref state) = *self.state.borrow() {
			state.for_keys_in_child_storage(storage_key, child_info, f)
		}
	}

	fn for_child_keys_with_prefix<F: FnMut(&[u8])>(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		prefix: &[u8],
		f: F,
	) {
		if let Some(ref state) = *self.state.borrow() {
			state.for_child_keys_with_prefix(storage_key, child_info, prefix, f)
		}
	}

	fn storage_root<I>(&self, delta: I) -> (B::Hash, Self::Transaction) where
		I: IntoIterator<Item=(Vec<u8>, Option<Vec<u8>>)>
	{
		self.state.borrow().as_ref().map_or(Default::default(), |s| s.storage_root(delta))
	}

	fn child_storage_root<I>(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		delta: I,
	) -> (B::Hash, bool, Self::Transaction) where
		I: IntoIterator<Item=(Vec<u8>, Option<Vec<u8>>)>,
	{
		self.state.borrow().as_ref().map_or(Default::default(), |s| s.child_storage_root(storage_key, child_info, delta))
	}

	fn pairs(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
		self.state.borrow().as_ref().map_or(Default::default(), |s| s.pairs())
	}

	fn keys(&self, prefix: &[u8]) -> Vec<Vec<u8>> {
		self.state.borrow().as_ref().map_or(Default::default(), |s| s.keys(prefix))
	}

	fn child_keys(
		&self,
		storage_key: &[u8],
		child_info: ChildInfo,
		prefix: &[u8],
	) -> Vec<Vec<u8>> {
		self.state.borrow().as_ref().map_or(Default::default(), |s| s.child_keys(storage_key, child_info, prefix))
	}

	fn as_trie_backend(&mut self)
		-> Option<&sp_state_machine::TrieBackend<Self::TrieBackendStorage, HashFor<B>>>
	{
		None
	}

	fn commit(&self, storage_root: <HashFor<B> as Hasher>::Out, mut transaction: Self::Transaction)
		-> Result<(), Self::Error>
	{
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
			self.record.set(keys);
			db.write(db_transaction).map_err(|_| String::from("Error committing transaction"))?;
			self.root.set(storage_root);
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
			db.write(db_transaction).map_err(|_| String::from("Error committing transaction"))?;
		}

		self.db.set(None);
		*self.state.borrow_mut() = None;

		self.root.set(self.genesis_root.clone());
		self.reopen()?;
		Ok(())
	}

	fn register_overlay_stats(&mut self, stats: &sp_state_machine::StateMachineStats) {
		self.state.borrow_mut().as_mut().map(|s| s.register_overlay_stats(stats));
	}

	fn usage_info(&self) -> sp_state_machine::UsageInfo {
		self.state.borrow().as_ref().map_or(sp_state_machine::UsageInfo::empty(), |s| s.usage_info())
	}
}

impl<Block: BlockT> std::fmt::Debug for BenchmarkingState<Block> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "DB at {:?}", self.path)
	}
}
