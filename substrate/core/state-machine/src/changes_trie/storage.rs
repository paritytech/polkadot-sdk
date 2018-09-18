// Copyright 2017 Parity Technologies (UK) Ltd.
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

//! Changes trie storage utilities.

use std::collections::HashMap;
use hashdb::{Hasher, HashDB, DBValue};
use heapsize::HeapSizeOf;
use memorydb::MemoryDB;
use parking_lot::RwLock;
use changes_trie::Storage;
use trie_backend_essence::TrieBackendStorage;

#[cfg(test)]
use backend::insert_into_memory_db;
#[cfg(test)]
use patricia_trie::NodeCodec;
#[cfg(test)]
use changes_trie::input::InputPair;

/// In-memory implementation of changes trie storage.
pub struct InMemoryStorage<H: Hasher> where H::Out: HeapSizeOf {
	data: RwLock<InMemoryStorageData<H>>,
}

/// Adapter for using changes trie storage as a TrieBackendEssence' storage.
pub struct TrieBackendAdapter<'a, H: Hasher, S: 'a + Storage<H>> {
	storage: &'a S,
	_hasher: ::std::marker::PhantomData<H>,
}

struct InMemoryStorageData<H: Hasher> where H::Out: HeapSizeOf {
	roots: HashMap<u64, H::Out>,
	mdb: MemoryDB<H>,
}

impl<H: Hasher> InMemoryStorage<H> where H::Out: HeapSizeOf {
	/// Create the storage from given in-memory database.
	pub fn with_db(mdb: MemoryDB<H>) -> Self {
		Self {
			data: RwLock::new(InMemoryStorageData {
				roots: HashMap::new(),
				mdb,
			}),
		}
	}

	/// Create the storage with empty database.
	pub fn new() -> Self {
		Self::with_db(Default::default())
	}

	#[cfg(test)]
	pub fn with_inputs<C: NodeCodec<H>>(inputs: Vec<(u64, Vec<InputPair>)>) -> Self {
		let mut mdb = MemoryDB::default();
		let mut roots = HashMap::new();
		for (block, pairs) in inputs {
			let root = insert_into_memory_db::<H, C, _>(&mut mdb, pairs.into_iter().map(Into::into));
			if let Some(root) = root {
				roots.insert(block, root);
			}
		}

		InMemoryStorage {
			data: RwLock::new(InMemoryStorageData {
				roots,
				mdb,
			}),
		}
	}

	#[cfg(test)]
	pub fn clear_storage(&self) {
		self.data.write().mdb = MemoryDB::new();
	}

	/// Insert changes trie for given block.
	pub fn insert(&self, block: u64, changes_trie_root: H::Out, trie: MemoryDB<H>) {
		let mut data = self.data.write();
		data.roots.insert(block, changes_trie_root);
		data.mdb.consolidate(trie);
	}
}

impl<H: Hasher> Storage<H> for InMemoryStorage<H> where H::Out: HeapSizeOf {
	fn root(&self, block: u64) -> Result<Option<H::Out>, String> {
		Ok(self.data.read().roots.get(&block).cloned())
	}

	fn get(&self, key: &H::Out) -> Result<Option<DBValue>, String> {
		Ok(HashDB::<H>::get(&self.data.read().mdb, key))
	}
}

impl<'a, H: Hasher, S: 'a + Storage<H>> TrieBackendAdapter<'a, H, S> {
	pub fn new(storage: &'a S) -> Self {
		Self { storage, _hasher: Default::default() }
	}
}

impl<'a, H: Hasher, S: 'a + Storage<H>> TrieBackendStorage<H> for TrieBackendAdapter<'a, H, S> {
	fn get(&self, key: &H::Out) -> Result<Option<DBValue>, String> {
		self.storage.get(key)
	}
}
