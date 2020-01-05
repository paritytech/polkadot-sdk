// Copyright 2017-2020 Parity Technologies (UK) Ltd.
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

//! DB-backed cache of blockchain data.

use std::{sync::Arc, collections::HashMap};
use parking_lot::RwLock;

use kvdb::{KeyValueDB, DBTransaction};

use sc_client_api::blockchain::{well_known_cache_keys::{self, Id as CacheKeyId}, Cache as BlockchainCache};
use sp_blockchain::Result as ClientResult;
use codec::{Encode, Decode};
use sp_runtime::generic::BlockId;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, NumberFor, Zero};
use crate::utils::{self, COLUMN_META, db_err};

use self::list_cache::{ListCache, PruningStrategy};

mod list_cache;
mod list_entry;
mod list_storage;

/// Minimal post-finalization age of finalized blocks before they'll pruned.
const PRUNE_DEPTH: u32 = 1024;

/// The type of entry that is inserted to the cache.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EntryType {
	/// Non-final entry.
	NonFinal,
	/// Final entry.
	Final,
	/// Genesis entry (inserted during cache initialization).
	Genesis,
}

/// Block identifier that holds both hash and number.
#[derive(Clone, Debug, Encode, Decode, PartialEq)]
pub struct ComplexBlockId<Block: BlockT> {
	hash: Block::Hash,
	number: NumberFor<Block>,
}

impl<Block: BlockT> ComplexBlockId<Block> {
	/// Create new complex block id.
	pub fn new(hash: Block::Hash, number: NumberFor<Block>) -> Self {
		ComplexBlockId { hash, number }
	}
}

impl<Block: BlockT> ::std::cmp::PartialOrd for ComplexBlockId<Block> {
	fn partial_cmp(&self, other: &ComplexBlockId<Block>) -> Option<::std::cmp::Ordering> {
		self.number.partial_cmp(&other.number)
	}
}

/// All cache items must implement this trait.
pub trait CacheItemT: Clone + Decode + Encode + PartialEq {}

impl<T> CacheItemT for T where T: Clone + Decode + Encode + PartialEq {}

/// Database-backed blockchain data cache.
pub struct DbCache<Block: BlockT> {
	cache_at: HashMap<CacheKeyId, ListCache<Block, Vec<u8>, self::list_storage::DbStorage>>,
	db: Arc<dyn KeyValueDB>,
	key_lookup_column: u32,
	header_column: u32,
	authorities_column: u32,
	genesis_hash: Block::Hash,
	best_finalized_block: ComplexBlockId<Block>,
}

impl<Block: BlockT> DbCache<Block> {
	/// Create new cache.
	pub fn new(
		db: Arc<dyn KeyValueDB>,
		key_lookup_column: u32,
		header_column: u32,
		authorities_column: u32,
		genesis_hash: Block::Hash,
		best_finalized_block: ComplexBlockId<Block>,
	) -> Self {
		Self {
			cache_at: HashMap::new(),
			db,
			key_lookup_column,
			header_column,
			authorities_column,
			genesis_hash,
			best_finalized_block,
		}
	}

	/// Set genesis block hash.
	pub fn set_genesis_hash(&mut self, genesis_hash: Block::Hash) {
		self.genesis_hash = genesis_hash;
	}

	/// Begin cache transaction.
	pub fn transaction<'a>(&'a mut self, tx: &'a mut DBTransaction) -> DbCacheTransaction<'a, Block> {
		DbCacheTransaction {
			cache: self,
			tx,
			cache_at_op: HashMap::new(),
			best_finalized_block: None,
		}
	}

	/// Run post-commit cache operations.
	pub fn commit(&mut self, ops: DbCacheTransactionOps<Block>) {
		for (name, op) in ops.cache_at_op.into_iter() {
			self.get_cache(name).on_transaction_commit(op);
		}
		if let Some(best_finalized_block) = ops.best_finalized_block {
			self.best_finalized_block = best_finalized_block;
		}
	}

	/// Creates `ListCache` with the given name or returns a reference to the existing.
	fn get_cache(&mut self, name: CacheKeyId) -> &mut ListCache<Block, Vec<u8>, self::list_storage::DbStorage> {
		get_cache_helper(
			&mut self.cache_at,
			name,
			&self.db,
			self.key_lookup_column,
			self.header_column,
			self.authorities_column,
			&self.best_finalized_block
		)
	}
}

// This helper is needed because otherwise the borrow checker will require to
// clone all parameters outside of the closure.
fn get_cache_helper<'a, Block: BlockT>(
	cache_at: &'a mut HashMap<CacheKeyId, ListCache<Block, Vec<u8>, self::list_storage::DbStorage>>,
	name: CacheKeyId,
	db: &Arc<dyn KeyValueDB>,
	key_lookup: u32,
	header: u32,
	cache: u32,
	best_finalized_block: &ComplexBlockId<Block>,
) -> &'a mut ListCache<Block, Vec<u8>, self::list_storage::DbStorage> {
	cache_at.entry(name).or_insert_with(|| {
		ListCache::new(
			self::list_storage::DbStorage::new(name.to_vec(), db.clone(),
				self::list_storage::DbColumns {
					meta: COLUMN_META,
					key_lookup,
					header,
					cache,
				},
			),
			cache_pruning_strategy(name),
			best_finalized_block.clone(),
		)
	})
}

/// Cache operations that are to be committed after database transaction is committed.
pub struct DbCacheTransactionOps<Block: BlockT> {
	cache_at_op: HashMap<CacheKeyId, self::list_cache::CommitOperation<Block, Vec<u8>>>,
	best_finalized_block: Option<ComplexBlockId<Block>>,
}

/// Database-backed blockchain data cache transaction valid for single block import.
pub struct DbCacheTransaction<'a, Block: BlockT> {
	cache: &'a mut DbCache<Block>,
	tx: &'a mut DBTransaction,
	cache_at_op: HashMap<CacheKeyId, self::list_cache::CommitOperation<Block, Vec<u8>>>,
	best_finalized_block: Option<ComplexBlockId<Block>>,
}

impl<'a, Block: BlockT> DbCacheTransaction<'a, Block> {
	/// Convert transaction into post-commit operations set.
	pub fn into_ops(self) -> DbCacheTransactionOps<Block> {
		DbCacheTransactionOps {
			cache_at_op: self.cache_at_op,
			best_finalized_block: self.best_finalized_block,
		}
	}

	/// When new block is inserted into database.
	pub fn on_block_insert(
		mut self,
		parent: ComplexBlockId<Block>,
		block: ComplexBlockId<Block>,
		data_at: HashMap<CacheKeyId, Vec<u8>>,
		entry_type: EntryType,
	) -> ClientResult<Self> {
		assert!(self.cache_at_op.is_empty());

		// prepare list of caches that are not update
		// (we might still need to do some cache maintenance in this case)
		let missed_caches = self.cache.cache_at.keys()
			.filter(|cache| !data_at.contains_key(*cache))
			.cloned()
			.collect::<Vec<_>>();

		let mut insert_op = |name: CacheKeyId, value: Option<Vec<u8>>| -> Result<(), sp_blockchain::Error> {
			let cache = self.cache.get_cache(name);
			let op = cache.on_block_insert(
				&mut self::list_storage::DbStorageTransaction::new(
					cache.storage(),
					&mut self.tx,
				),
				parent.clone(),
				block.clone(),
				value,
				entry_type,
			)?;
			if let Some(op) = op {
				self.cache_at_op.insert(name, op);
			}
			Ok(())
		};

		data_at.into_iter().try_for_each(|(name, data)| insert_op(name, Some(data)))?;
		missed_caches.into_iter().try_for_each(|name| insert_op(name, None))?;

		match entry_type {
			EntryType::Final | EntryType::Genesis =>
				self.best_finalized_block = Some(block),
			EntryType::NonFinal => (),
		}

		Ok(self)
	}

	/// When previously inserted block is finalized.
	pub fn on_block_finalize(
		mut self,
		parent: ComplexBlockId<Block>,
		block: ComplexBlockId<Block>
	) -> ClientResult<Self> {
		assert!(self.cache_at_op.is_empty());

		for (name, cache_at) in self.cache.cache_at.iter() {
			let op = cache_at.on_block_finalize(
				&mut self::list_storage::DbStorageTransaction::new(
					cache_at.storage(),
					&mut self.tx
				),
				parent.clone(),
				block.clone(),
			)?;

			if let Some(op) = op {
				self.cache_at_op.insert(name.to_owned(), op);
			}
		}

		self.best_finalized_block = Some(block);

		Ok(self)
	}

	/// When block is reverted.
	pub fn on_block_revert(
		mut self,
		reverted_block: &ComplexBlockId<Block>,
	) -> ClientResult<Self> {
		for (name, cache) in self.cache.cache_at.iter() {
			let op = cache.on_block_revert(
				&mut self::list_storage::DbStorageTransaction::new(
					cache.storage(),
					&mut self.tx
				),
				reverted_block,
			)?;

			assert!(!self.cache_at_op.contains_key(name));
			self.cache_at_op.insert(name.to_owned(), op);
		}

		Ok(self)
	}
}

/// Synchronous implementation of database-backed blockchain data cache.
pub struct DbCacheSync<Block: BlockT>(pub RwLock<DbCache<Block>>);

impl<Block: BlockT> BlockchainCache<Block> for DbCacheSync<Block> {
	fn initialize(&self, key: &CacheKeyId, data: Vec<u8>) -> ClientResult<()> {
		let mut cache = self.0.write();
		let genesis_hash = cache.genesis_hash;
		let cache_contents = vec![(*key, data)].into_iter().collect();
		let db = cache.db.clone();
		let mut dbtx = DBTransaction::new();
		let tx = cache.transaction(&mut dbtx);
		let tx = tx.on_block_insert(
			ComplexBlockId::new(Default::default(), Zero::zero()),
			ComplexBlockId::new(genesis_hash, Zero::zero()),
			cache_contents,
			EntryType::Genesis,
		)?;
		let tx_ops = tx.into_ops();
		db.write(dbtx).map_err(db_err)?;
		cache.commit(tx_ops);
		Ok(())
	}

	fn get_at(
		&self,
		key: &CacheKeyId,
		at: &BlockId<Block>,
	) -> Option<((NumberFor<Block>, Block::Hash), Option<(NumberFor<Block>, Block::Hash)>, Vec<u8>)> {
		let mut cache = self.0.write();
		let storage = cache.get_cache(*key).storage();
		let db = storage.db();
		let columns = storage.columns();
		let at = match *at {
			BlockId::Hash(hash) => {
				let header = utils::read_header::<Block>(
					&**db,
					columns.key_lookup,
					columns.header,
					BlockId::Hash(hash.clone())).ok()??;
				ComplexBlockId::new(hash, *header.number())
			},
			BlockId::Number(number) => {
				let hash = utils::read_header::<Block>(
					&**db,
					columns.key_lookup,
					columns.header,
					BlockId::Number(number.clone())).ok()??.hash();
				ComplexBlockId::new(hash, number)
			},
		};

		cache.cache_at
			.get(key)?
			.value_at_block(&at)
			.map(|block_and_value| block_and_value.map(|(begin_block, end_block, value)|
				(
					(begin_block.number, begin_block.hash),
					end_block.map(|end_block| (end_block.number, end_block.hash)),
					value,
				)))
			.ok()?
	}
}

/// Get pruning strategy for given cache.
fn cache_pruning_strategy<N: From<u32>>(cache: CacheKeyId) -> PruningStrategy<N> {
	// the cache is mostly used to store data from consensus engines
	// this kind of data is only required for non-finalized blocks
	// => by default we prune finalized cached entries

	match cache {
		// we need to keep changes tries configurations forever (or at least until changes tries,
		// that were built using this configuration, are pruned) to make it possible to refer
		// to old changes tries
		well_known_cache_keys::CHANGES_TRIE_CONFIG => PruningStrategy::NeverPrune,
		_ => PruningStrategy::ByDepth(PRUNE_DEPTH.into()),
	}
}
