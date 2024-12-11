use crate::{columns, DbHash};
use hash_db::{AsHashDB, HashDB, HashDBRef, Hasher, Prefix};
use sp_database::{Change, Database, Transaction};
use sp_state_machine::TrieBackendStorage;
use sp_storage::ChildInfo;
use sp_trie::{DBValue, TrieError, TrieHash, TrieLayout};
use std::{marker::PhantomData, sync::Arc};

/// [`StateImporter`] is responsible for importing the state changes
/// directly into the database, bypassing the in-memory intermediate storage
/// (`PrefixedMemoryDB`).
///
/// This approach avoids potential OOM issues that can arise when dealing with
/// large state imports, especially when importing the state downloaded from
/// fast sync or warp sync.
pub(crate) struct StateImporter<'a, S: 'a + TrieBackendStorage<H>, H: 'a + Hasher> {
	/// Old state storage backend.
	storage: &'a S,
	/// Handle to the trie database where changes will be committed.
	trie_database: Arc<dyn Database<DbHash>>,
	/// Default child storage root.
	default_child_root: H::Out,
	_phantom: PhantomData<H>,
}

impl<'a, S: TrieBackendStorage<H>, H: Hasher> StateImporter<'a, S, H> {
	pub fn new(storage: &'a S, trie_database: Arc<dyn Database<DbHash>>) -> Self {
		let default_child_root = sp_trie::empty_child_trie_root::<sp_trie::LayoutV1<H>>();
		Self { storage, trie_database, default_child_root, _phantom: Default::default() }
	}
}

pub(crate) fn read_child_root<'a, S, H, L>(
	state_importer: &StateImporter<'a, S, H>,
	root: &TrieHash<L>,
	child_info: &ChildInfo,
) -> Result<Option<H::Out>, Box<TrieError<L>>>
where
	S: 'a + TrieBackendStorage<H>,
	H: Hasher,
	L: TrieLayout,
	StateImporter<'a, S, H>: HashDBRef<<L as TrieLayout>::Hash, Vec<u8>>,
{
	let key = child_info.prefixed_storage_key();
	Ok(sp_trie::read_trie_value::<L, _>(state_importer, root, key.as_slice(), None, None)?.map(
		|r| {
			let mut hash = H::Out::default();

			// root is fetched from DB, not writable by runtime, so it's always valid.
			hash.as_mut().copy_from_slice(&r[..]);

			hash
		},
	))
}

impl<'a, S: 'a + TrieBackendStorage<H>, H: Hasher> hash_db::HashDB<H, DBValue>
	for StateImporter<'a, S, H>
{
	fn get(&self, key: &H::Out, prefix: Prefix) -> Option<DBValue> {
		// TODO: we'll run into IncompleteDatabase error without this special handling.
		// Double check and provide an explanation.
		if *key == self.default_child_root {
			return Some([0u8].to_vec());
		}

		let db_key = sp_trie::prefixed_key::<H>(key, prefix);

		let res = self.trie_database.get(columns::STATE, &db_key).or_else(|| {
			self.storage.get(key, prefix).unwrap_or_else(|e| {
				log::warn!(target: "trie", "Failed to read from DB: {}", e);
				None
			})
		});

		// TODO: we'll run into IncompleteDatabase error without this special handling.
		// Double check and provide an explanation.
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
		self.emplace(key, prefix, value.to_vec());
		key
	}

	fn emplace(&mut self, key: H::Out, prefix: Prefix, value: DBValue) {
		let key = sp_trie::prefixed_key::<H>(&key, prefix);
		let tx = Transaction(vec![Change::Set(columns::STATE, key, value)]);
		// TODO: better error handling?
		self.trie_database
			.commit(tx)
			.unwrap_or_else(|err| panic!("Failed to put value into the state database: {err:?}"))
	}

	fn remove(&mut self, key: &H::Out, prefix: Prefix) {
		let key = sp_trie::prefixed_key::<H>(&key, prefix);
		let tx = Transaction(vec![Change::Remove(columns::STATE, key)]);
		// TODO: better error handling?
		self.trie_database
			.commit(tx)
			.unwrap_or_else(|err| panic!("Failed to remove value in the state database: {err:?}"))
	}
}

impl<'a, S: 'a + TrieBackendStorage<H>, H: Hasher> HashDBRef<H, DBValue>
	for StateImporter<'a, S, H>
{
	fn get(&self, key: &H::Out, prefix: Prefix) -> Option<DBValue> {
		HashDB::get(self, key, prefix)
	}

	fn contains(&self, key: &H::Out, prefix: Prefix) -> bool {
		HashDB::contains(self, key, prefix)
	}
}

impl<'a, S: 'a + TrieBackendStorage<H>, H: 'a + Hasher> AsHashDB<H, DBValue>
	for StateImporter<'a, S, H>
{
	fn as_hash_db<'b>(&'b self) -> &'b (dyn HashDB<H, DBValue> + 'b) {
		self
	}
	fn as_hash_db_mut<'b>(&'b mut self) -> &'b mut (dyn HashDB<H, DBValue> + 'b) {
		self
	}
}
