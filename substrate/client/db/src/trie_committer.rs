use crate::{columns, DbHash};
use hash_db::{AsHashDB, HashDB, HashDBRef, Hasher, Prefix};
use sp_database::{Change, Database, Transaction};
use sp_state_machine::TrieBackendStorage;
use sp_trie::DBValue;
use std::{marker::PhantomData, sync::Arc};

/// [`TrieCommitter`] is responsible for committing trie state changes
/// directly into the database, bypassing the in-memory intermediate storage
/// (`PrefixedMemoryDB`).
///
/// This approach avoids potential OOM issues that can arise when dealing with
/// large state imports, especially when importing the state downloaded from
/// fast sync or warp sync.
pub(crate) struct TrieCommitter<'a, S: 'a + TrieBackendStorage<H>, H: 'a + Hasher> {
	/// Old state storage backend.
	storage: &'a S,
	/// Handle to the trie database where changes will be committed.
	trie_database: Arc<dyn Database<DbHash>>,
	_phantom: PhantomData<H>,
}

impl<'a, S: TrieBackendStorage<H>, H: Hasher> TrieCommitter<'a, S, H> {
	pub fn new(storage: &'a S, trie_database: Arc<dyn Database<DbHash>>) -> Self {
		Self { storage, trie_database, _phantom: Default::default() }
	}
}

impl<'a, S: 'a + TrieBackendStorage<H>, H: Hasher> hash_db::HashDB<H, DBValue>
	for TrieCommitter<'a, S, H>
{
	fn get(&self, key: &H::Out, prefix: Prefix) -> Option<DBValue> {
		let db_key = sp_trie::prefixed_key::<H>(key, prefix);

		let res = self.trie_database.get(columns::STATE, &db_key).or_else(|| {
			self.storage.get(key, prefix).unwrap_or_else(|e| {
				log::warn!(target: "trie", "Failed to read from DB: {}", e);
				None
			})
		});

		// TODO: this feels like incorrect.
		if prefix == sp_trie::EMPTY_PREFIX && res.is_none() {
			Some([0u8].to_vec())
		} else {
			res
		}
	}

	fn contains(&self, key: &H::Out, prefix: Prefix) -> bool {
		HashDB::get(self, key, prefix).is_some()
	}

	fn insert(&mut self, prefix: Prefix, value: &[u8]) -> H::Out {
		let key = H::hash(value);

		let db_key = sp_trie::prefixed_key::<H>(&key, prefix);
		let tx = Transaction(vec![Change::Set(columns::STATE, db_key, value.to_vec())]);
		self.trie_database.commit(tx).expect("TODO: handle unwrap properly");

		key
	}

	fn emplace(&mut self, key: H::Out, prefix: Prefix, value: DBValue) {
		let key = sp_trie::prefixed_key::<H>(&key, prefix);
		let tx = Transaction(vec![Change::Set(columns::STATE, key, value)]);
		self.trie_database.commit(tx).expect("TODO: handle unwrap properly");
	}

	fn remove(&mut self, key: &H::Out, prefix: Prefix) {
		let key = sp_trie::prefixed_key::<H>(&key, prefix);
		let tx = Transaction(vec![Change::Remove(columns::STATE, key)]);
		self.trie_database.commit(tx).expect("TODO: handle unwrap properly");
	}
}

impl<'a, S: 'a + TrieBackendStorage<H>, H: Hasher> HashDBRef<H, DBValue>
	for TrieCommitter<'a, S, H>
{
	fn get(&self, key: &H::Out, prefix: Prefix) -> Option<DBValue> {
		HashDB::get(self, key, prefix)
	}

	fn contains(&self, key: &H::Out, prefix: Prefix) -> bool {
		HashDB::contains(self, key, prefix)
	}
}

impl<'a, S: 'a + TrieBackendStorage<H>, H: 'a + Hasher> AsHashDB<H, DBValue>
	for TrieCommitter<'a, S, H>
{
	fn as_hash_db<'b>(&'b self) -> &'b (dyn HashDB<H, DBValue> + 'b) {
		self
	}
	fn as_hash_db_mut<'b>(&'b mut self) -> &'b mut (dyn HashDB<H, DBValue> + 'b) {
		self
	}
}
