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

// tag::description[]
//! Client backend that uses RocksDB database as storage.
// end::description[]

extern crate substrate_client as client;
extern crate kvdb_rocksdb;
extern crate kvdb;
extern crate hashdb;
extern crate memorydb;
extern crate parking_lot;
extern crate substrate_state_machine as state_machine;
extern crate substrate_primitives as primitives;
extern crate sr_primitives as runtime_primitives;
extern crate parity_codec as codec;
extern crate substrate_executor as executor;
extern crate substrate_state_db as state_db;

#[macro_use]
extern crate log;

#[macro_use]
extern crate parity_codec_derive;

#[cfg(test)]
extern crate kvdb_memorydb;

pub mod light;

mod cache;
mod utils;

use std::sync::Arc;
use std::path::PathBuf;
use std::io;

use codec::{Decode, Encode};
use hashdb::Hasher;
use kvdb::{KeyValueDB, DBTransaction};
use memorydb::MemoryDB;
use parking_lot::RwLock;
use primitives::{H256, AuthorityId, Blake2Hasher, RlpCodec};
use runtime_primitives::generic::BlockId;
use runtime_primitives::bft::Justification;
use runtime_primitives::traits::{Block as BlockT, Header as HeaderT, As, Hash, HashFor,
	NumberFor, Zero, Digest, DigestItem};
use runtime_primitives::BuildStorage;
use state_machine::backend::Backend as StateBackend;
use executor::RuntimeInfo;
use state_machine::{CodeExecutor, DBValue, ExecutionStrategy};
use utils::{Meta, db_err, meta_keys, number_to_db_key, db_key_to_number, open_database,
	read_db, read_id, read_meta};
use state_db::StateDb;
pub use state_db::PruningMode;

const FINALIZATION_WINDOW: u64 = 32;

/// DB-backed patricia trie state, transaction type is an overlay of changes to commit.
pub type DbState = state_machine::TrieBackend<Arc<state_machine::Storage<Blake2Hasher>>, Blake2Hasher, RlpCodec>;

/// Database settings.
pub struct DatabaseSettings {
	/// Cache size in bytes. If `None` default is used.
	pub cache_size: Option<usize>,
	/// Path to the database.
	pub path: PathBuf,
	/// Pruning mode.
	pub pruning: PruningMode,
}

/// Create an instance of db-backed client.
pub fn new_client<E, S, Block>(
	settings: DatabaseSettings,
	executor: E,
	genesis_storage: S,
	execution_strategy: ExecutionStrategy,
) -> Result<client::Client<Backend<Block>, client::LocalCallExecutor<Backend<Block>, E>, Block>, client::error::Error>
	where
		Block: BlockT,
		E: CodeExecutor<Blake2Hasher> + RuntimeInfo,
		S: BuildStorage,
{
	let backend = Arc::new(Backend::new(settings, FINALIZATION_WINDOW)?);
	let executor = client::LocalCallExecutor::new(backend.clone(), executor);
	Ok(client::Client::new(backend, executor, genesis_storage, execution_strategy)?)
}

mod columns {
	pub const META: Option<u32> = Some(0);
	pub const STATE: Option<u32> = Some(1);
	pub const STATE_META: Option<u32> = Some(2);
	pub const BLOCK_INDEX: Option<u32> = Some(3);
	pub const HEADER: Option<u32> = Some(4);
	pub const BODY: Option<u32> = Some(5);
	pub const JUSTIFICATION: Option<u32> = Some(6);
	pub const CHANGES_TRIE: Option<u32> = Some(7);
}

struct PendingBlock<Block: BlockT> {
	header: Block::Header,
	justification: Option<Justification<Block::Hash>>,
	body: Option<Vec<Block::Extrinsic>>,
	is_best: bool,
}

// wrapper that implements trait required for state_db
struct StateMetaDb<'a>(&'a KeyValueDB);

impl<'a> state_db::MetaDb for StateMetaDb<'a> {
	type Error = io::Error;

	fn get_meta(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
		self.0.get(columns::STATE_META, key).map(|r| r.map(|v| v.to_vec()))
	}
}

/// Block database
pub struct BlockchainDb<Block: BlockT> {
	db: Arc<KeyValueDB>,
	meta: RwLock<Meta<<Block::Header as HeaderT>::Number, Block::Hash>>,
}

impl<Block: BlockT> BlockchainDb<Block> {
	fn new(db: Arc<KeyValueDB>) -> Result<Self, client::error::Error> {
		let meta = read_meta::<Block>(&*db, columns::HEADER)?;
		Ok(BlockchainDb {
			db,
			meta: RwLock::new(meta)
		})
	}

	fn update_meta(&self, hash: Block::Hash, number: <Block::Header as HeaderT>::Number, is_best: bool) {
		if is_best {
			let mut meta = self.meta.write();
			if number == Zero::zero() {
				meta.genesis_hash = hash;
			}
			meta.best_number = number;
			meta.best_hash = hash;
		}
	}
}

impl<Block: BlockT> client::blockchain::HeaderBackend<Block> for BlockchainDb<Block> {
	fn header(&self, id: BlockId<Block>) -> Result<Option<Block::Header>, client::error::Error> {
		match read_db(&*self.db, columns::BLOCK_INDEX, columns::HEADER, id)? {
			Some(header) => match Block::Header::decode(&mut &header[..]) {
				Some(header) => Ok(Some(header)),
				None => return Err(client::error::ErrorKind::Backend("Error decoding header".into()).into()),
			}
			None => Ok(None),
		}
	}

	fn info(&self) -> Result<client::blockchain::Info<Block>, client::error::Error> {
		let meta = self.meta.read();
		Ok(client::blockchain::Info {
			best_hash: meta.best_hash,
			best_number: meta.best_number,
			genesis_hash: meta.genesis_hash,
		})
	}

	fn status(&self, id: BlockId<Block>) -> Result<client::blockchain::BlockStatus, client::error::Error> {
		let exists = match id {
			BlockId::Hash(_) => read_id(&*self.db, columns::BLOCK_INDEX, id)?.is_some(),
			BlockId::Number(n) => n <= self.meta.read().best_number,
		};
		match exists {
			true => Ok(client::blockchain::BlockStatus::InChain),
			false => Ok(client::blockchain::BlockStatus::Unknown),
		}
	}

	fn number(&self, hash: Block::Hash) -> Result<Option<<Block::Header as HeaderT>::Number>, client::error::Error> {
		read_id::<Block>(&*self.db, columns::BLOCK_INDEX, BlockId::Hash(hash))
			.and_then(|key| match key {
				Some(key) => Ok(Some(db_key_to_number(&key)?)),
				None => Ok(None),
			})
	}

	fn hash(&self, number: <Block::Header as HeaderT>::Number) -> Result<Option<Block::Hash>, client::error::Error> {
		read_db::<Block>(&*self.db, columns::BLOCK_INDEX, columns::HEADER, BlockId::Number(number)).map(|x|
			x.map(|raw| HashFor::<Block>::hash(&raw[..])).map(Into::into)
		)
	}
}

impl<Block: BlockT> client::blockchain::Backend<Block> for BlockchainDb<Block> {
	fn body(&self, id: BlockId<Block>) -> Result<Option<Vec<Block::Extrinsic>>, client::error::Error> {
		match read_db(&*self.db, columns::BLOCK_INDEX, columns::BODY, id)? {
			Some(body) => match Decode::decode(&mut &body[..]) {
				Some(body) => Ok(Some(body)),
				None => return Err(client::error::ErrorKind::Backend("Error decoding body".into()).into()),
			}
			None => Ok(None),
		}
	}

	fn justification(&self, id: BlockId<Block>) -> Result<Option<Justification<Block::Hash>>, client::error::Error> {
		match read_db(&*self.db, columns::BLOCK_INDEX, columns::JUSTIFICATION, id)? {
			Some(justification) => match Decode::decode(&mut &justification[..]) {
				Some(justification) => Ok(Some(justification)),
				None => return Err(client::error::ErrorKind::Backend("Error decoding justification".into()).into()),
			}
			None => Ok(None),
		}
	}

	fn cache(&self) -> Option<&client::blockchain::Cache<Block>> {
		None
	}
}

/// Database transaction
pub struct BlockImportOperation<Block: BlockT, H: Hasher> {
	old_state: DbState,
	updates: MemoryDB<H>,
	changes_trie_updates: MemoryDB<H>,
	pending_block: Option<PendingBlock<Block>>,
}

impl<Block> client::backend::BlockImportOperation<Block, Blake2Hasher, RlpCodec>
for BlockImportOperation<Block, Blake2Hasher>
where Block: BlockT,
{
	type State = DbState;

	fn state(&self) -> Result<Option<&Self::State>, client::error::Error> {
		Ok(Some(&self.old_state))
	}

	fn set_block_data(&mut self, header: Block::Header, body: Option<Vec<Block::Extrinsic>>, justification: Option<Justification<Block::Hash>>, is_best: bool) -> Result<(), client::error::Error> {
		assert!(self.pending_block.is_none(), "Only one block per operation is allowed");
		self.pending_block = Some(PendingBlock {
			header,
			body,
			justification,
			is_best,
		});
		Ok(())
	}

	fn update_authorities(&mut self, _authorities: Vec<AuthorityId>) {
		// currently authorities are not cached on full nodes
	}

	fn update_storage(&mut self, update: MemoryDB<Blake2Hasher>) -> Result<(), client::error::Error> {
		self.updates = update;
		Ok(())
	}

	fn reset_storage<I: Iterator<Item=(Vec<u8>, Vec<u8>)>>(&mut self, iter: I) -> Result<(), client::error::Error> {
		// TODO: wipe out existing trie.
		let (_, update) = self.old_state.storage_root(iter.into_iter().map(|(k, v)| (k, Some(v))));
		self.updates = update;
		Ok(())
	}

	fn update_changes_trie(&mut self, update: MemoryDB<Blake2Hasher>) -> Result<(), client::error::Error> {
		self.changes_trie_updates = update;
		Ok(())
	}
}

struct StorageDb<Block: BlockT> {
	pub db: Arc<KeyValueDB>,
	pub state_db: StateDb<Block::Hash, H256>,
}

impl<Block: BlockT> state_machine::Storage<Blake2Hasher> for StorageDb<Block> {
	fn get(&self, key: &H256) -> Result<Option<DBValue>, String> {
		self.state_db.get(&key.0.into(), self).map(|r| r.map(|v| DBValue::from_slice(&v)))
			.map_err(|e| format!("Database backend error: {:?}", e))
	}
}

impl<Block: BlockT> state_db::HashDb for StorageDb<Block> {
	type Error = io::Error;
	type Hash = H256;

	fn get(&self, key: &H256) -> Result<Option<Vec<u8>>, Self::Error> {
		self.db.get(columns::STATE, &key[..]).map(|r| r.map(|v| v.to_vec()))
	}
}

struct DbGenesisStorage(pub H256);

impl DbGenesisStorage {
	pub fn new() -> Self {
		let mut root = H256::default();
		let mut mdb = MemoryDB::<Blake2Hasher>::new();
		state_machine::TrieDBMut::<Blake2Hasher, RlpCodec>::new(&mut mdb, &mut root);
		DbGenesisStorage(root)
	}
}

impl state_machine::Storage<Blake2Hasher> for DbGenesisStorage {
	fn get(&self, _key: &H256) -> Result<Option<DBValue>, String> {
		Ok(None)
	}
}

pub struct DbChangesTrieStorage<Block: BlockT> {
	db: Arc<KeyValueDB>,
	_phantom: ::std::marker::PhantomData<Block>,
}

impl<Block: BlockT> state_machine::ChangesTrieStorage<Blake2Hasher> for DbChangesTrieStorage<Block> {
	fn root(&self, block: u64) -> Result<Option<H256>, String> {
		Ok(read_db::<Block>(&*self.db, columns::BLOCK_INDEX, columns::HEADER, BlockId::Number(As::sa(block)))
			.map_err(|err| format!("{}", err))
			.and_then(|header| match header {
				Some(header) => Block::Header::decode(&mut &header[..])
					.ok_or_else(|| format!("Failed to parse header of block {}", block))
					.map(Some),
				None => Ok(None)
			})?
			.and_then(|header| header.digest().logs().iter()
				.find(|log| log.as_changes_trie_root().is_some())
				.and_then(DigestItem::as_changes_trie_root)
				.map(|root| H256::from_slice(root.as_ref()))))
	}

	fn get(&self, key: &H256) -> Result<Option<DBValue>, String> {
		self.db.get(columns::CHANGES_TRIE, &key[..])
			.map_err(|err| format!("{}", err))
	}
}

/// Disk backend. Keeps data in a key-value store. In archive mode, trie nodes are kept from all blocks.
/// Otherwise, trie nodes are kept only from the most recent block.
pub struct Backend<Block: BlockT> {
	storage: Arc<StorageDb<Block>>,
	tries_change_storage: DbChangesTrieStorage<Block>,
	blockchain: BlockchainDb<Block>,
	finalization_window: u64,
}

impl<Block: BlockT> Backend<Block> {
	/// Create a new instance of database backend.
	pub fn new(config: DatabaseSettings, finalization_window: u64) -> Result<Self, client::error::Error> {
		let db = open_database(&config, "full")?;

		Backend::from_kvdb(db as Arc<_>, config.pruning, finalization_window)
	}

	#[cfg(test)]
	fn new_test(keep_blocks: u32) -> Self {
		use utils::NUM_COLUMNS;

		let db = Arc::new(::kvdb_memorydb::create(NUM_COLUMNS));

		Backend::from_kvdb(db as Arc<_>, PruningMode::keep_blocks(keep_blocks), 0).expect("failed to create test-db")
	}

	fn from_kvdb(db: Arc<KeyValueDB>, pruning: PruningMode, finalization_window: u64) -> Result<Self, client::error::Error> {
		let blockchain = BlockchainDb::new(db.clone())?;
		let map_e = |e: state_db::Error<io::Error>| ::client::error::Error::from(format!("State database error: {:?}", e));
		let state_db: StateDb<Block::Hash, H256> = StateDb::new(pruning, &StateMetaDb(&*db)).map_err(map_e)?;
		let storage_db = StorageDb {
			db: db.clone(),
			state_db,
		};
		let tries_change_storage = DbChangesTrieStorage {
			db,
			_phantom: Default::default(),
		};

		Ok(Backend {
			storage: Arc::new(storage_db),
			tries_change_storage: tries_change_storage,
			blockchain,
			finalization_window,
		})
	}
}

fn apply_state_commit(transaction: &mut DBTransaction, commit: state_db::CommitSet<H256>) {
	for (key, val) in commit.data.inserted.into_iter() {
		transaction.put(columns::STATE, &key[..], &val);
	}
	for key in commit.data.deleted.into_iter() {
		transaction.delete(columns::STATE, &key[..]);
	}
	for (key, val) in commit.meta.inserted.into_iter() {
		transaction.put(columns::STATE_META, &key[..], &val);
	}
	for key in commit.meta.deleted.into_iter() {
		transaction.delete(columns::STATE_META, &key[..]);
	}
}

fn apply_changes_trie_commit(transaction: &mut DBTransaction, mut commit: MemoryDB<Blake2Hasher>) {
	for (key, (val, _)) in commit.drain() {
		transaction.put(columns::CHANGES_TRIE, &key[..], &val);
	}
}

impl<Block> client::backend::Backend<Block, Blake2Hasher, RlpCodec> for Backend<Block> where Block: BlockT {
	type BlockImportOperation = BlockImportOperation<Block, Blake2Hasher>;
	type Blockchain = BlockchainDb<Block>;
	type State = DbState;
	type ChangesTrieStorage = DbChangesTrieStorage<Block>;

	fn begin_operation(&self, block: BlockId<Block>) -> Result<Self::BlockImportOperation, client::error::Error> {
		let state = self.state_at(block)?;
		Ok(BlockImportOperation {
			pending_block: None,
			old_state: state,
			updates: MemoryDB::default(),
			changes_trie_updates: MemoryDB::default(),
		})
	}

	fn commit_operation(&self, mut operation: Self::BlockImportOperation) -> Result<(), client::error::Error> {
		use client::blockchain::HeaderBackend;
		let mut transaction = DBTransaction::new();
		if let Some(pending_block) = operation.pending_block {
			let hash = pending_block.header.hash();
			let number = pending_block.header.number().clone();
			let key = number_to_db_key(number.clone());
			transaction.put(columns::HEADER, &key, &pending_block.header.encode());
			if let Some(body) = pending_block.body {
				transaction.put(columns::BODY, &key, &body.encode());
			}
			if let Some(justification) = pending_block.justification {
				transaction.put(columns::JUSTIFICATION, &key, &justification.encode());
			}
			transaction.put(columns::BLOCK_INDEX, hash.as_ref(), &key);
			if pending_block.is_best {
				transaction.put(columns::META, meta_keys::BEST_BLOCK, &key);
			}
			let mut changeset: state_db::ChangeSet<H256> = state_db::ChangeSet::default();
			for (key, (val, rc)) in operation.updates.drain() {
				if rc > 0 {
					changeset.inserted.push((key.0.into(), val.to_vec()));
				} else if rc < 0 {
					changeset.deleted.push(key.0.into());
				}
			}
			let number_u64 = number.as_().into();
			let commit = self.storage.state_db.insert_block(&hash, number_u64, &pending_block.header.parent_hash(), changeset);
			apply_state_commit(&mut transaction, commit);
			apply_changes_trie_commit(&mut transaction, operation.changes_trie_updates);

			//finalize an older block
			if number_u64 > self.finalization_window {
				let finalizing_hash = if self.finalization_window == 0 {
					Some(hash)
				} else {
					let finalizing = number_u64 - self.finalization_window;
					if finalizing > self.storage.state_db.best_finalized() {
						self.blockchain.hash(As::sa(finalizing))?
					} else {
						None
					}
				};
				if let Some(finalizing_hash) = finalizing_hash {
					trace!(target: "db", "Finalizing block #{} ({:?})", number_u64 - self.finalization_window, finalizing_hash);
					let commit = self.storage.state_db.finalize_block(&finalizing_hash);
					apply_state_commit(&mut transaction, commit);
				}
			}

			debug!(target: "db", "DB Commit {:?} ({}), best = {}", hash, number, pending_block.is_best);
			self.storage.db.write(transaction).map_err(db_err)?;
			self.blockchain.update_meta(hash, number, pending_block.is_best);
		}
		Ok(())
	}

	fn changes_trie_storage(&self) -> Option<&Self::ChangesTrieStorage> {
		Some(&self.tries_change_storage)
	}

	fn revert(&self, n: NumberFor<Block>) -> Result<NumberFor<Block>, client::error::Error> {
		use client::blockchain::HeaderBackend;
		let mut best = self.blockchain.info()?.best_number;
		for c in 0 .. n.as_() {
			if best == As::sa(0) {
				return Ok(As::sa(c))
			}
			let mut transaction = DBTransaction::new();
			match self.storage.state_db.revert_one() {
				Some(commit) => {
					apply_state_commit(&mut transaction, commit);
					let removed = self.blockchain.hash(best)?.ok_or_else(
						|| client::error::ErrorKind::UnknownBlock(
							format!("Error reverting to {}. Block hash not found.", best)))?;
					best -= As::sa(1);
					let key = number_to_db_key(best.clone());
					let hash = self.blockchain.hash(best)?.ok_or_else(
						|| client::error::ErrorKind::UnknownBlock(
							format!("Error reverting to {}. Block hash not found.", best)))?;
					transaction.put(columns::META, meta_keys::BEST_BLOCK, &key);
					transaction.delete(columns::BLOCK_INDEX, removed.as_ref());
					self.storage.db.write(transaction).map_err(db_err)?;
					self.blockchain.update_meta(hash, best, true);
				}
				None => return Ok(As::sa(c))
			}
		}
		Ok(n)
	}

	fn blockchain(&self) -> &BlockchainDb<Block> {
		&self.blockchain
	}

	fn state_at(&self, block: BlockId<Block>) -> Result<Self::State, client::error::Error> {
		use client::blockchain::HeaderBackend as BcHeaderBackend;

		// special case for genesis initialization
		match block {
			BlockId::Hash(h) if h == Default::default() => {
				let genesis_storage = DbGenesisStorage::new();
				let root = genesis_storage.0.clone();
				return Ok(DbState::new(Arc::new(genesis_storage), root));
			},
			_ => {}
		}

		match self.blockchain.header(block) {
			Ok(Some(ref hdr)) if !self.storage.state_db.is_pruned(hdr.number().as_()) => {
				let root = H256::from_slice(hdr.state_root().as_ref());
				Ok(DbState::new(self.storage.clone(), root))
			},
			Err(e) => Err(e),
			_ => Err(client::error::ErrorKind::UnknownBlock(format!("{:?}", block)).into()),
		}
	}
}

impl<Block> client::backend::LocalBackend<Block, Blake2Hasher, RlpCodec> for Backend<Block>
where Block: BlockT {}

#[cfg(test)]
mod tests {
	use hashdb::HashDB;
	use super::*;
	use client::backend::Backend as BTrait;
	use client::backend::BlockImportOperation as Op;
	use client::blockchain::HeaderBackend as BlockchainHeaderBackend;
	use runtime_primitives::testing::{Header, Block as RawBlock};
	use state_machine::{TrieMut, TrieDBMut, ChangesTrieStorage};

	type Block = RawBlock<u64>;

	#[test]
	fn block_hash_inserted_correctly() {
		let db = Backend::<Block>::new_test(1);
		for i in 0..10 {
			assert!(db.blockchain().hash(i).unwrap().is_none());

			{
				let id = if i == 0 {
					BlockId::Hash(Default::default())
				} else {
					BlockId::Number(i - 1)
				};

				let mut op = db.begin_operation(id).unwrap();
				let header = Header {
					number: i,
					parent_hash: if i == 0 {
						Default::default()
					} else {
						db.blockchain.hash(i - 1).unwrap().unwrap()
					},
					state_root: Default::default(),
					digest: Default::default(),
					extrinsics_root: Default::default(),
				};

				op.set_block_data(
					header,
					Some(vec![]),
					None,
					true,
				).unwrap();
				db.commit_operation(op).unwrap();
			}

			assert!(db.blockchain().hash(i).unwrap().is_some())
		}
	}

	#[test]
	fn set_state_data() {
		let db = Backend::<Block>::new_test(2);
		{
			let mut op = db.begin_operation(BlockId::Hash(Default::default())).unwrap();
			let mut header = Header {
				number: 0,
				parent_hash: Default::default(),
				state_root: Default::default(),
				digest: Default::default(),
				extrinsics_root: Default::default(),
			};

			let storage = vec![
				(vec![1, 3, 5], vec![2, 4, 6]),
				(vec![1, 2, 3], vec![9, 9, 9]),
			];

			header.state_root = op.old_state.storage_root(storage
				.iter()
				.cloned()
				.map(|(x, y)| (x, Some(y)))
			).0.into();

			op.reset_storage(storage.iter().cloned()).unwrap();
			op.set_block_data(
				header,
				Some(vec![]),
				None,
				true
			).unwrap();

			db.commit_operation(op).unwrap();

			let state = db.state_at(BlockId::Number(0)).unwrap();

			assert_eq!(state.storage(&[1, 3, 5]).unwrap(), Some(vec![2, 4, 6]));
			assert_eq!(state.storage(&[1, 2, 3]).unwrap(), Some(vec![9, 9, 9]));
			assert_eq!(state.storage(&[5, 5, 5]).unwrap(), None);

		}

		{
			let mut op = db.begin_operation(BlockId::Number(0)).unwrap();
			let mut header = Header {
				number: 1,
				parent_hash: Default::default(),
				state_root: Default::default(),
				digest: Default::default(),
				extrinsics_root: Default::default(),
			};

			let storage = vec![
				(vec![1, 3, 5], None),
				(vec![5, 5, 5], Some(vec![4, 5, 6])),
			];

			let (root, overlay) = op.old_state.storage_root(storage.iter().cloned());
			op.update_storage(overlay).unwrap();
			header.state_root = root.into();

			op.set_block_data(
				header,
				Some(vec![]),
				None,
				true
			).unwrap();

			db.commit_operation(op).unwrap();

			let state = db.state_at(BlockId::Number(1)).unwrap();

			assert_eq!(state.storage(&[1, 3, 5]).unwrap(), None);
			assert_eq!(state.storage(&[1, 2, 3]).unwrap(), Some(vec![9, 9, 9]));
			assert_eq!(state.storage(&[5, 5, 5]).unwrap(), Some(vec![4, 5, 6]));
		}
	}

	#[test]
	fn delete_only_when_negative_rc() {
		let key;
		let backend = Backend::<Block>::new_test(0);

		let hash = {
			let mut op = backend.begin_operation(BlockId::Hash(Default::default())).unwrap();
			let mut header = Header {
				number: 0,
				parent_hash: Default::default(),
				state_root: Default::default(),
				digest: Default::default(),
				extrinsics_root: Default::default(),
			};

			let storage: Vec<(_, _)> = vec![];

			header.state_root = op.old_state.storage_root(storage
				.iter()
				.cloned()
				.map(|(x, y)| (x, Some(y)))
			).0.into();
			let hash = header.hash();

			op.reset_storage(storage.iter().cloned()).unwrap();

			key = op.updates.insert(b"hello");
			op.set_block_data(
				header,
				Some(vec![]),
				None,
				true
			).unwrap();

			backend.commit_operation(op).unwrap();

			assert_eq!(backend.storage.db.get(::columns::STATE, &key.0[..]).unwrap().unwrap(), &b"hello"[..]);
			hash
		};

		let hash = {
			let mut op = backend.begin_operation(BlockId::Number(0)).unwrap();
			let mut header = Header {
				number: 1,
				parent_hash: hash,
				state_root: Default::default(),
				digest: Default::default(),
				extrinsics_root: Default::default(),
			};

			let storage: Vec<(_, _)> = vec![];

			header.state_root = op.old_state.storage_root(storage
				.iter()
				.cloned()
				.map(|(x, y)| (x, Some(y)))
			).0.into();
			let hash = header.hash();

			op.updates.insert(b"hello");
			op.updates.remove(&key);
			op.set_block_data(
				header,
				Some(vec![]),
				None,
				true
			).unwrap();

			backend.commit_operation(op).unwrap();

			assert_eq!(backend.storage.db.get(::columns::STATE, &key.0[..]).unwrap().unwrap(), &b"hello"[..]);
			hash
		};

		{
			let mut op = backend.begin_operation(BlockId::Number(1)).unwrap();
			let mut header = Header {
				number: 2,
				parent_hash: hash,
				state_root: Default::default(),
				digest: Default::default(),
				extrinsics_root: Default::default(),
			};

			let storage: Vec<(_, _)> = vec![];

			header.state_root = op.old_state.storage_root(storage
				.iter()
				.cloned()
				.map(|(x, y)| (x, Some(y)))
			).0.into();

			op.updates.remove(&key);
			op.set_block_data(
				header,
				Some(vec![]),
				None,
				true
			).unwrap();

			backend.commit_operation(op).unwrap();

			assert!(backend.storage.db.get(::columns::STATE, &key.0[..]).unwrap().is_none());
		}
	}

	#[test]
	fn changes_trie_storage_works() {
		let backend = Backend::<Block>::new_test(1000);

		let prepare_changes = |changes: Vec<(Vec<u8>, Vec<u8>)>| {
			let mut changes_root = H256::default();
			let mut changes_trie_update = MemoryDB::<Blake2Hasher>::new();
			{
				let mut trie = TrieDBMut::<Blake2Hasher, RlpCodec>::new(
					&mut changes_trie_update,
					&mut changes_root
				);
				for (key, value) in changes {
					trie.insert(&key, &value).unwrap();
				}
			}

			(changes_root, changes_trie_update)
		};

		let insert_header = |number: u64, parent_hash: H256, changes: Vec<(Vec<u8>, Vec<u8>)>| {
			use runtime_primitives::generic::DigestItem;
			use runtime_primitives::testing::Digest;

			let (changes_root, changes_trie_update) = prepare_changes(changes);
			let digest = Digest {
				logs: vec![
					DigestItem::ChangesTrieRoot(changes_root),
				],
			};
			let header = Header {
				number,
				parent_hash,
				state_root: Default::default(),
				digest,
				extrinsics_root: Default::default(),
			};
			let header_hash = header.hash();

			let block_id = if number == 0 {
				BlockId::Hash(Default::default())
			} else {
				BlockId::Number(number - 1)
			};
			let mut op = backend.begin_operation(block_id).unwrap();
			op.set_block_data(header, None, None, true).unwrap();
			op.update_changes_trie(changes_trie_update).unwrap();
			backend.commit_operation(op).unwrap();

			header_hash
		};

		let check_changes = |backend: &Backend<Block>, block: u64, changes: Vec<(Vec<u8>, Vec<u8>)>| {
			let (changes_root, mut changes_trie_update) = prepare_changes(changes);
			assert_eq!(backend.tries_change_storage.root(block), Ok(Some(changes_root)));

			for (key, (val, _)) in changes_trie_update.drain() {
				assert_eq!(backend.changes_trie_storage().unwrap().get(&key), Ok(Some(val)));
			}
		};

		let changes0 = vec![(b"key_at_0".to_vec(), b"val_at_0".to_vec())];
		let changes1 = vec![
			(b"key_at_1".to_vec(), b"val_at_1".to_vec()),
			(b"another_key_at_1".to_vec(), b"another_val_at_1".to_vec()),
		];
		let changes2 = vec![(b"key_at_2".to_vec(), b"val_at_2".to_vec())];

		let block0 = insert_header(0, Default::default(), changes0.clone());
		let block1 = insert_header(1, block0, changes1.clone());
		let _ = insert_header(2, block1, changes2.clone());

		// check that the storage contains tries for all blocks
		check_changes(&backend, 0, changes0);
		check_changes(&backend, 1, changes1);
		check_changes(&backend, 2, changes2);
	}
}
