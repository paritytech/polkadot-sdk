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

//! Client backend that is backed by a database.
//!
//! # Canonicality vs. Finality
//!
//! Finality indicates that a block will not be reverted, according to the consensus algorithm,
//! while canonicality indicates that the block may be reverted, but we will be unable to do so,
//! having discarded heavy state that will allow a chain reorganization.
//!
//! Finality implies canonicality but not vice-versa.

#![warn(missing_docs)]

pub mod offchain;

pub mod bench;

mod children;
mod parity_db;
mod pinned_blocks_cache;
mod record_stats_state;
mod state_importer;
mod stats;
#[cfg(test)]
mod tests;
#[cfg(any(feature = "rocksdb", test))]
mod upgrade;
mod utils;

use linked_hash_map::LinkedHashMap;
use log::{debug, trace, warn};
use parking_lot::{Mutex, RwLock};
use std::{
	collections::{HashMap, HashSet},
	io,
	path::{Path, PathBuf},
	sync::Arc,
};

use crate::{
	pinned_blocks_cache::PinnedBlocksCache,
	record_stats_state::RecordStatsState,
	state_importer::StateImporter,
	stats::StateUsageStats,
	utils::{meta_keys, read_db, read_meta, DatabaseType, Meta},
};
use codec::{Decode, Encode};
use hash_db::Prefix;
use sc_client_api::{
	backend::NewBlockState,
	blockchain::{BlockGap, BlockGapType},
	leaves::{FinalizationOutcome, LeafSet},
	utils::is_descendent_of,
	IoInfo, MemoryInfo, MemorySize, UsageInfo,
};
use sc_state_db::{IsPruned, LastCanonicalized, StateDb};
use sp_arithmetic::traits::Saturating;
use sp_blockchain::{
	Backend as _, CachedHeaderMetadata, DisplacedLeavesAfterFinalization, Error as ClientError,
	HeaderBackend, HeaderMetadata, HeaderMetadataCache, Result as ClientResult,
};
use sp_core::{
	offchain::OffchainOverlayedChange,
	storage::{well_known_keys, ChildInfo},
};
use sp_database::Transaction;
use sp_runtime::{
	generic::BlockId,
	traits::{
		Block as BlockT, Hash, HashingFor, Header as HeaderT, NumberFor, One, SaturatedConversion,
		Zero,
	},
	Justification, Justifications, StateVersion, Storage,
};
use sp_state_machine::{
	backend::{AsTrieBackend, Backend as StateBackend},
	BackendTransaction, ChildStorageCollection, DBValue, IndexOperation, IterArgs,
	OffchainChangesCollection, StateMachineStats, StorageCollection, StorageIterator, StorageKey,
	StorageValue, UsageInfo as StateUsageInfo,
};
use sp_trie::{
	cache::SharedTrieCache, prefixed_key, MemoryDB, MerkleValue, PrefixedMemoryDB, TrieError,
};
use utils::BLOCK_GAP_CURRENT_VERSION;

// Re-export the Database trait so that one can pass an implementation of it.
pub use sc_state_db::PruningMode;
pub use sp_database::Database;

pub use bench::BenchmarkingState;

const CACHE_HEADERS: usize = 8;

/// DB-backed patricia trie state, transaction type is an overlay of changes to commit.
pub type DbState<H> = sp_state_machine::TrieBackend<Arc<dyn sp_state_machine::Storage<H>>, H>;

/// Builder for [`DbState`].
pub type DbStateBuilder<Hasher> =
	sp_state_machine::TrieBackendBuilder<Arc<dyn sp_state_machine::Storage<Hasher>>, Hasher>;

/// Length of a [`DbHash`].
const DB_HASH_LEN: usize = 32;

/// Hash type that this backend uses for the database.
pub type DbHash = sp_core::H256;

type LayoutV0<Block> = sp_trie::LayoutV0<HashingFor<Block>>;
type LayoutV1<Block> = sp_trie::LayoutV1<HashingFor<Block>>;

/// An extrinsic entry in the database.
#[derive(Debug, Encode, Decode)]
enum DbExtrinsic<B: BlockT> {
	/// Extrinsic that contains indexed data.
	Indexed {
		/// Hash of the indexed part.
		hash: DbHash,
		/// Extrinsic header.
		header: Vec<u8>,
	},
	/// Complete extrinsic data.
	Full(B::Extrinsic),
}

/// A reference tracking state.
///
/// It makes sure that the hash we are using stays pinned in storage
/// until this structure is dropped.
pub struct RefTrackingState<Block: BlockT> {
	state: DbState<HashingFor<Block>>,
	storage: Arc<StorageDb<Block>>,
	parent_hash: Option<Block::Hash>,
}

impl<B: BlockT> RefTrackingState<B> {
	fn new(
		state: DbState<HashingFor<B>>,
		storage: Arc<StorageDb<B>>,
		parent_hash: Option<B::Hash>,
	) -> Self {
		RefTrackingState { state, parent_hash, storage }
	}
}

impl<B: BlockT> Drop for RefTrackingState<B> {
	fn drop(&mut self) {
		if let Some(hash) = &self.parent_hash {
			self.storage.state_db.unpin(hash);
		}
	}
}

impl<Block: BlockT> std::fmt::Debug for RefTrackingState<Block> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "Block {:?}", self.parent_hash)
	}
}

/// A raw iterator over the `RefTrackingState`.
pub struct RawIter<B: BlockT> {
	inner: <DbState<HashingFor<B>> as StateBackend<HashingFor<B>>>::RawIter,
}

impl<B: BlockT> StorageIterator<HashingFor<B>> for RawIter<B> {
	type Backend = RefTrackingState<B>;
	type Error = <DbState<HashingFor<B>> as StateBackend<HashingFor<B>>>::Error;

	fn next_key(&mut self, backend: &Self::Backend) -> Option<Result<StorageKey, Self::Error>> {
		self.inner.next_key(&backend.state)
	}

	fn next_pair(
		&mut self,
		backend: &Self::Backend,
	) -> Option<Result<(StorageKey, StorageValue), Self::Error>> {
		self.inner.next_pair(&backend.state)
	}

	fn was_complete(&self) -> bool {
		self.inner.was_complete()
	}
}

impl<B: BlockT> StateBackend<HashingFor<B>> for RefTrackingState<B> {
	type Error = <DbState<HashingFor<B>> as StateBackend<HashingFor<B>>>::Error;
	type TrieBackendStorage =
		<DbState<HashingFor<B>> as StateBackend<HashingFor<B>>>::TrieBackendStorage;
	type RawIter = RawIter<B>;

	fn storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
		self.state.storage(key)
	}

	fn storage_hash(&self, key: &[u8]) -> Result<Option<B::Hash>, Self::Error> {
		self.state.storage_hash(key)
	}

	fn child_storage(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, Self::Error> {
		self.state.child_storage(child_info, key)
	}

	fn child_storage_hash(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<B::Hash>, Self::Error> {
		self.state.child_storage_hash(child_info, key)
	}

	fn closest_merkle_value(
		&self,
		key: &[u8],
	) -> Result<Option<MerkleValue<B::Hash>>, Self::Error> {
		self.state.closest_merkle_value(key)
	}

	fn child_closest_merkle_value(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<MerkleValue<B::Hash>>, Self::Error> {
		self.state.child_closest_merkle_value(child_info, key)
	}

	fn exists_storage(&self, key: &[u8]) -> Result<bool, Self::Error> {
		self.state.exists_storage(key)
	}

	fn exists_child_storage(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<bool, Self::Error> {
		self.state.exists_child_storage(child_info, key)
	}

	fn next_storage_key(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
		self.state.next_storage_key(key)
	}

	fn next_child_storage_key(
		&self,
		child_info: &ChildInfo,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, Self::Error> {
		self.state.next_child_storage_key(child_info, key)
	}

	fn storage_root<'a>(
		&self,
		delta: impl Iterator<Item = (&'a [u8], Option<&'a [u8]>)>,
		state_version: StateVersion,
	) -> (B::Hash, BackendTransaction<HashingFor<B>>) {
		self.state.storage_root(delta, state_version)
	}

	fn child_storage_root<'a>(
		&self,
		child_info: &ChildInfo,
		delta: impl Iterator<Item = (&'a [u8], Option<&'a [u8]>)>,
		state_version: StateVersion,
	) -> (B::Hash, bool, BackendTransaction<HashingFor<B>>) {
		self.state.child_storage_root(child_info, delta, state_version)
	}

	fn raw_iter(&self, args: IterArgs) -> Result<Self::RawIter, Self::Error> {
		self.state.raw_iter(args).map(|inner| RawIter { inner })
	}

	fn register_overlay_stats(&self, stats: &StateMachineStats) {
		self.state.register_overlay_stats(stats);
	}

	fn usage_info(&self) -> StateUsageInfo {
		self.state.usage_info()
	}
}

impl<B: BlockT> AsTrieBackend<HashingFor<B>> for RefTrackingState<B> {
	type TrieBackendStorage =
		<DbState<HashingFor<B>> as StateBackend<HashingFor<B>>>::TrieBackendStorage;

	fn as_trie_backend(
		&self,
	) -> &sp_state_machine::TrieBackend<Self::TrieBackendStorage, HashingFor<B>> {
		&self.state.as_trie_backend()
	}
}

/// Database settings.
pub struct DatabaseSettings {
	/// The maximum trie cache size in bytes.
	///
	/// If `None` is given, the cache is disabled.
	pub trie_cache_maximum_size: Option<usize>,
	/// Requested state pruning mode.
	pub state_pruning: Option<PruningMode>,
	/// Where to find the database.
	pub source: DatabaseSource,
	/// Block pruning mode.
	///
	/// NOTE: only finalized blocks are subject for removal!
	pub blocks_pruning: BlocksPruning,
}

/// Block pruning settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlocksPruning {
	/// Keep full block history, of every block that was ever imported.
	KeepAll,
	/// Keep full finalized block history.
	KeepFinalized,
	/// Keep N recent finalized blocks.
	Some(u32),
}

impl BlocksPruning {
	/// True if this is an archive pruning mode (either KeepAll or KeepFinalized).
	pub fn is_archive(&self) -> bool {
		match *self {
			BlocksPruning::KeepAll | BlocksPruning::KeepFinalized => true,
			BlocksPruning::Some(_) => false,
		}
	}
}

/// Where to find the database..
#[derive(Debug, Clone)]
pub enum DatabaseSource {
	/// Check given path, and see if there is an existing database there. If it's either `RocksDb`
	/// or `ParityDb`, use it. If there is none, create a new instance of `ParityDb`.
	Auto {
		/// Path to the paritydb database.
		paritydb_path: PathBuf,
		/// Path to the rocksdb database.
		rocksdb_path: PathBuf,
		/// Cache size in MiB. Used only by `RocksDb` variant of `DatabaseSource`.
		cache_size: usize,
	},
	/// Load a RocksDB database from a given path. Recommended for most uses.
	#[cfg(feature = "rocksdb")]
	RocksDb {
		/// Path to the database.
		path: PathBuf,
		/// Cache size in MiB.
		cache_size: usize,
	},

	/// Load a ParityDb database from a given path.
	ParityDb {
		/// Path to the database.
		path: PathBuf,
	},

	/// Use a custom already-open database.
	Custom {
		/// the handle to the custom storage
		db: Arc<dyn Database<DbHash>>,

		/// if set, the `create` flag will be required to open such datasource
		require_create_flag: bool,
	},
}

impl DatabaseSource {
	/// Return path for databases that are stored on disk.
	pub fn path(&self) -> Option<&Path> {
		match self {
			// as per https://github.com/paritytech/substrate/pull/9500#discussion_r684312550
			//
			// IIUC this is needed for polkadot to create its own dbs, so until it can use parity db
			// I would think rocksdb, but later parity-db.
			DatabaseSource::Auto { paritydb_path, .. } => Some(paritydb_path),
			#[cfg(feature = "rocksdb")]
			DatabaseSource::RocksDb { path, .. } => Some(path),
			DatabaseSource::ParityDb { path } => Some(path),
			DatabaseSource::Custom { .. } => None,
		}
	}

	/// Set path for databases that are stored on disk.
	pub fn set_path(&mut self, p: &Path) -> bool {
		match self {
			DatabaseSource::Auto { ref mut paritydb_path, .. } => {
				*paritydb_path = p.into();
				true
			},
			#[cfg(feature = "rocksdb")]
			DatabaseSource::RocksDb { ref mut path, .. } => {
				*path = p.into();
				true
			},
			DatabaseSource::ParityDb { ref mut path } => {
				*path = p.into();
				true
			},
			DatabaseSource::Custom { .. } => false,
		}
	}
}

impl std::fmt::Display for DatabaseSource {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let name = match self {
			DatabaseSource::Auto { .. } => "Auto",
			#[cfg(feature = "rocksdb")]
			DatabaseSource::RocksDb { .. } => "RocksDb",
			DatabaseSource::ParityDb { .. } => "ParityDb",
			DatabaseSource::Custom { .. } => "Custom",
		};
		write!(f, "{}", name)
	}
}

pub(crate) mod columns {
	pub const META: u32 = crate::utils::COLUMN_META;
	pub const STATE: u32 = 1;
	pub const STATE_META: u32 = 2;
	/// maps hashes to lookup keys and numbers to canon hashes.
	pub const KEY_LOOKUP: u32 = 3;
	pub const HEADER: u32 = 4;
	pub const BODY: u32 = 5;
	pub const JUSTIFICATIONS: u32 = 6;
	pub const AUX: u32 = 8;
	/// Offchain workers local storage
	pub const OFFCHAIN: u32 = 9;
	/// Transactions
	pub const TRANSACTION: u32 = 11;
	pub const BODY_INDEX: u32 = 12;
}

struct PendingBlock<Block: BlockT> {
	header: Block::Header,
	justifications: Option<Justifications>,
	body: Option<Vec<Block::Extrinsic>>,
	indexed_body: Option<Vec<Vec<u8>>>,
	leaf_state: NewBlockState,
}

// wrapper that implements trait required for state_db
#[derive(Clone)]
struct StateMetaDb(Arc<dyn Database<DbHash>>);

impl sc_state_db::MetaDb for StateMetaDb {
	type Error = sp_database::error::DatabaseError;

	fn get_meta(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
		Ok(self.0.get(columns::STATE_META, key))
	}
}

struct MetaUpdate<Block: BlockT> {
	pub hash: Block::Hash,
	pub number: NumberFor<Block>,
	pub is_best: bool,
	pub is_finalized: bool,
	pub with_state: bool,
}

fn cache_header<Hash: std::cmp::Eq + std::hash::Hash, Header>(
	cache: &mut LinkedHashMap<Hash, Option<Header>>,
	hash: Hash,
	header: Option<Header>,
) {
	cache.insert(hash, header);
	while cache.len() > CACHE_HEADERS {
		cache.pop_front();
	}
}

/// Block database
pub struct BlockchainDb<Block: BlockT> {
	db: Arc<dyn Database<DbHash>>,
	meta: Arc<RwLock<Meta<NumberFor<Block>, Block::Hash>>>,
	leaves: RwLock<LeafSet<Block::Hash, NumberFor<Block>>>,
	header_metadata_cache: Arc<HeaderMetadataCache<Block>>,
	header_cache: Mutex<LinkedHashMap<Block::Hash, Option<Block::Header>>>,
	pinned_blocks_cache: Arc<RwLock<PinnedBlocksCache<Block>>>,
}

impl<Block: BlockT> BlockchainDb<Block> {
	fn new(db: Arc<dyn Database<DbHash>>) -> ClientResult<Self> {
		let meta = read_meta::<Block>(&*db, columns::HEADER)?;
		let leaves = LeafSet::read_from_db(&*db, columns::META, meta_keys::LEAF_PREFIX)?;
		Ok(BlockchainDb {
			db,
			leaves: RwLock::new(leaves),
			meta: Arc::new(RwLock::new(meta)),
			header_metadata_cache: Arc::new(HeaderMetadataCache::default()),
			header_cache: Default::default(),
			pinned_blocks_cache: Arc::new(RwLock::new(PinnedBlocksCache::new())),
		})
	}

	fn update_meta(&self, update: MetaUpdate<Block>) {
		let MetaUpdate { hash, number, is_best, is_finalized, with_state } = update;
		let mut meta = self.meta.write();
		if number.is_zero() {
			meta.genesis_hash = hash;
		}

		if is_best {
			meta.best_number = number;
			meta.best_hash = hash;
		}

		if is_finalized {
			if with_state {
				meta.finalized_state = Some((hash, number));
			}
			meta.finalized_number = number;
			meta.finalized_hash = hash;
		}
	}

	fn update_block_gap(&self, gap: Option<BlockGap<NumberFor<Block>>>) {
		let mut meta = self.meta.write();
		meta.block_gap = gap;
	}

	/// Empty the cache of pinned items.
	fn clear_pinning_cache(&self) {
		self.pinned_blocks_cache.write().clear();
	}

	/// Load a justification into the cache of pinned items.
	/// Reference count of the item will not be increased. Use this
	/// to load values for items into the cache which have already been pinned.
	fn insert_justifications_if_pinned(&self, hash: Block::Hash, justification: Justification) {
		let mut cache = self.pinned_blocks_cache.write();
		if !cache.contains(hash) {
			return;
		}

		let justifications = Justifications::from(justification);
		cache.insert_justifications(hash, Some(justifications));
	}

	/// Load a justification from the db into the cache of pinned items.
	/// Reference count of the item will not be increased. Use this
	/// to load values for items into the cache which have already been pinned.
	fn insert_persisted_justifications_if_pinned(&self, hash: Block::Hash) -> ClientResult<()> {
		let mut cache = self.pinned_blocks_cache.write();
		if !cache.contains(hash) {
			return Ok(());
		}

		let justifications = self.justifications_uncached(hash)?;
		cache.insert_justifications(hash, justifications);
		Ok(())
	}

	/// Load a block body from the db into the cache of pinned items.
	/// Reference count of the item will not be increased. Use this
	/// to load values for items items into the cache which have already been pinned.
	fn insert_persisted_body_if_pinned(&self, hash: Block::Hash) -> ClientResult<()> {
		let mut cache = self.pinned_blocks_cache.write();
		if !cache.contains(hash) {
			return Ok(());
		}

		let body = self.body_uncached(hash)?;
		cache.insert_body(hash, body);
		Ok(())
	}

	/// Bump reference count for pinned item.
	fn bump_ref(&self, hash: Block::Hash) {
		self.pinned_blocks_cache.write().pin(hash);
	}

	/// Decrease reference count for pinned item and remove if reference count is 0.
	fn unpin(&self, hash: Block::Hash) {
		self.pinned_blocks_cache.write().unpin(hash);
	}

	fn justifications_uncached(&self, hash: Block::Hash) -> ClientResult<Option<Justifications>> {
		match read_db(
			&*self.db,
			columns::KEY_LOOKUP,
			columns::JUSTIFICATIONS,
			BlockId::<Block>::Hash(hash),
		)? {
			Some(justifications) => match Decode::decode(&mut &justifications[..]) {
				Ok(justifications) => Ok(Some(justifications)),
				Err(err) =>
					return Err(sp_blockchain::Error::Backend(format!(
						"Error decoding justifications: {err}"
					))),
			},
			None => Ok(None),
		}
	}

	fn body_uncached(&self, hash: Block::Hash) -> ClientResult<Option<Vec<Block::Extrinsic>>> {
		if let Some(body) =
			read_db(&*self.db, columns::KEY_LOOKUP, columns::BODY, BlockId::Hash::<Block>(hash))?
		{
			// Plain body
			match Decode::decode(&mut &body[..]) {
				Ok(body) => return Ok(Some(body)),
				Err(err) =>
					return Err(sp_blockchain::Error::Backend(format!("Error decoding body: {err}"))),
			}
		}

		if let Some(index) = read_db(
			&*self.db,
			columns::KEY_LOOKUP,
			columns::BODY_INDEX,
			BlockId::Hash::<Block>(hash),
		)? {
			match Vec::<DbExtrinsic<Block>>::decode(&mut &index[..]) {
				Ok(index) => {
					let mut body = Vec::new();
					for ex in index {
						match ex {
							DbExtrinsic::Indexed { hash, header } => {
								match self.db.get(columns::TRANSACTION, hash.as_ref()) {
									Some(t) => {
										let mut input =
											utils::join_input(header.as_ref(), t.as_ref());
										let ex = Block::Extrinsic::decode(&mut input).map_err(
											|err| {
												sp_blockchain::Error::Backend(format!(
													"Error decoding indexed extrinsic: {err}"
												))
											},
										)?;
										body.push(ex);
									},
									None =>
										return Err(sp_blockchain::Error::Backend(format!(
											"Missing indexed transaction {hash:?}"
										))),
								};
							},
							DbExtrinsic::Full(ex) => {
								body.push(ex);
							},
						}
					}
					return Ok(Some(body));
				},
				Err(err) =>
					return Err(sp_blockchain::Error::Backend(format!(
						"Error decoding body list: {err}",
					))),
			}
		}
		Ok(None)
	}
}

impl<Block: BlockT> sc_client_api::blockchain::HeaderBackend<Block> for BlockchainDb<Block> {
	fn header(&self, hash: Block::Hash) -> ClientResult<Option<Block::Header>> {
		let mut cache = self.header_cache.lock();
		if let Some(result) = cache.get_refresh(&hash) {
			return Ok(result.clone());
		}
		let header = utils::read_header(
			&*self.db,
			columns::KEY_LOOKUP,
			columns::HEADER,
			BlockId::<Block>::Hash(hash),
		)?;
		cache_header(&mut cache, hash, header.clone());
		Ok(header)
	}

	fn info(&self) -> sc_client_api::blockchain::Info<Block> {
		let meta = self.meta.read();
		sc_client_api::blockchain::Info {
			best_hash: meta.best_hash,
			best_number: meta.best_number,
			genesis_hash: meta.genesis_hash,
			finalized_hash: meta.finalized_hash,
			finalized_number: meta.finalized_number,
			finalized_state: meta.finalized_state,
			number_leaves: self.leaves.read().count(),
			block_gap: meta.block_gap,
		}
	}

	fn status(&self, hash: Block::Hash) -> ClientResult<sc_client_api::blockchain::BlockStatus> {
		match self.header(hash)?.is_some() {
			true => Ok(sc_client_api::blockchain::BlockStatus::InChain),
			false => Ok(sc_client_api::blockchain::BlockStatus::Unknown),
		}
	}

	fn number(&self, hash: Block::Hash) -> ClientResult<Option<NumberFor<Block>>> {
		Ok(self.header_metadata(hash).ok().map(|header_metadata| header_metadata.number))
	}

	fn hash(&self, number: NumberFor<Block>) -> ClientResult<Option<Block::Hash>> {
		Ok(utils::read_header::<Block>(
			&*self.db,
			columns::KEY_LOOKUP,
			columns::HEADER,
			BlockId::Number(number),
		)?
		.map(|header| header.hash()))
	}
}

impl<Block: BlockT> sc_client_api::blockchain::Backend<Block> for BlockchainDb<Block> {
	fn body(&self, hash: Block::Hash) -> ClientResult<Option<Vec<Block::Extrinsic>>> {
		let cache = self.pinned_blocks_cache.read();
		if let Some(result) = cache.body(&hash) {
			return Ok(result.clone());
		}

		self.body_uncached(hash)
	}

	fn justifications(&self, hash: Block::Hash) -> ClientResult<Option<Justifications>> {
		let cache = self.pinned_blocks_cache.read();
		if let Some(result) = cache.justifications(&hash) {
			return Ok(result.clone());
		}

		self.justifications_uncached(hash)
	}

	fn last_finalized(&self) -> ClientResult<Block::Hash> {
		Ok(self.meta.read().finalized_hash)
	}

	fn leaves(&self) -> ClientResult<Vec<Block::Hash>> {
		Ok(self.leaves.read().hashes())
	}

	fn children(&self, parent_hash: Block::Hash) -> ClientResult<Vec<Block::Hash>> {
		children::read_children(&*self.db, columns::META, meta_keys::CHILDREN_PREFIX, parent_hash)
	}

	fn indexed_transaction(&self, hash: Block::Hash) -> ClientResult<Option<Vec<u8>>> {
		Ok(self.db.get(columns::TRANSACTION, hash.as_ref()))
	}

	fn has_indexed_transaction(&self, hash: Block::Hash) -> ClientResult<bool> {
		Ok(self.db.contains(columns::TRANSACTION, hash.as_ref()))
	}

	fn block_indexed_body(&self, hash: Block::Hash) -> ClientResult<Option<Vec<Vec<u8>>>> {
		let body = match read_db(
			&*self.db,
			columns::KEY_LOOKUP,
			columns::BODY_INDEX,
			BlockId::<Block>::Hash(hash),
		)? {
			Some(body) => body,
			None => return Ok(None),
		};
		match Vec::<DbExtrinsic<Block>>::decode(&mut &body[..]) {
			Ok(index) => {
				let mut transactions = Vec::new();
				for ex in index.into_iter() {
					if let DbExtrinsic::Indexed { hash, .. } = ex {
						match self.db.get(columns::TRANSACTION, hash.as_ref()) {
							Some(t) => transactions.push(t),
							None =>
								return Err(sp_blockchain::Error::Backend(format!(
									"Missing indexed transaction {hash:?}",
								))),
						}
					}
				}
				Ok(Some(transactions))
			},
			Err(err) =>
				Err(sp_blockchain::Error::Backend(format!("Error decoding body list: {err}"))),
		}
	}
}

impl<Block: BlockT> HeaderMetadata<Block> for BlockchainDb<Block> {
	type Error = sp_blockchain::Error;

	fn header_metadata(
		&self,
		hash: Block::Hash,
	) -> Result<CachedHeaderMetadata<Block>, Self::Error> {
		self.header_metadata_cache.header_metadata(hash).map_or_else(
			|| {
				self.header(hash)?
					.map(|header| {
						let header_metadata = CachedHeaderMetadata::from(&header);
						self.header_metadata_cache
							.insert_header_metadata(header_metadata.hash, header_metadata.clone());
						header_metadata
					})
					.ok_or_else(|| {
						ClientError::UnknownBlock(format!(
							"Header was not found in the database: {hash:?}",
						))
					})
			},
			Ok,
		)
	}

	fn insert_header_metadata(&self, hash: Block::Hash, metadata: CachedHeaderMetadata<Block>) {
		self.header_metadata_cache.insert_header_metadata(hash, metadata)
	}

	fn remove_header_metadata(&self, hash: Block::Hash) {
		self.header_cache.lock().remove(&hash);
		self.header_metadata_cache.remove_header_metadata(hash);
	}
}

/// Database transaction
pub struct BlockImportOperation<Block: BlockT> {
	old_state: RecordStatsState<RefTrackingState<Block>, Block>,
	db_updates: PrefixedMemoryDB<HashingFor<Block>>,
	storage_updates: StorageCollection,
	child_storage_updates: ChildStorageCollection,
	offchain_storage_updates: OffchainChangesCollection,
	pending_block: Option<PendingBlock<Block>>,
	aux_ops: Vec<(Vec<u8>, Option<Vec<u8>>)>,
	finalized_blocks: Vec<(Block::Hash, Option<Justification>)>,
	set_head: Option<Block::Hash>,
	commit_state: bool,
	create_gap: bool,
	index_ops: Vec<IndexOperation>,
}

impl<Block: BlockT> BlockImportOperation<Block> {
	fn apply_offchain(&mut self, transaction: &mut Transaction<DbHash>) {
		let mut count = 0;
		for ((prefix, key), value_operation) in self.offchain_storage_updates.drain(..) {
			count += 1;
			let key = crate::offchain::concatenate_prefix_and_key(&prefix, &key);
			match value_operation {
				OffchainOverlayedChange::SetValue(val) =>
					transaction.set_from_vec(columns::OFFCHAIN, &key, val),
				OffchainOverlayedChange::Remove => transaction.remove(columns::OFFCHAIN, &key),
			}
		}

		if count > 0 {
			log::debug!(target: "sc_offchain", "Applied {count} offchain indexing changes.");
		}
	}

	fn apply_aux(&mut self, transaction: &mut Transaction<DbHash>) {
		for (key, maybe_val) in self.aux_ops.drain(..) {
			match maybe_val {
				Some(val) => transaction.set_from_vec(columns::AUX, &key, val),
				None => transaction.remove(columns::AUX, &key),
			}
		}
	}

	fn apply_new_state(
		&mut self,
		storage: Storage,
		state_version: StateVersion,
	) -> ClientResult<Block::Hash> {
		if storage.top.keys().any(|k| well_known_keys::is_child_storage_key(k)) {
			return Err(sp_blockchain::Error::InvalidState);
		}

		let child_delta = storage.children_default.values().map(|child_content| {
			(
				&child_content.child_info,
				child_content.data.iter().map(|(k, v)| (&k[..], Some(&v[..]))),
			)
		});

		let (root, transaction) = self.old_state.full_storage_root(
			storage.top.iter().map(|(k, v)| (&k[..], Some(&v[..]))),
			child_delta,
			state_version,
		);

		self.db_updates = transaction;
		Ok(root)
	}
}

impl<Block: BlockT> sc_client_api::backend::BlockImportOperation<Block>
	for BlockImportOperation<Block>
{
	type State = RecordStatsState<RefTrackingState<Block>, Block>;

	fn state(&self) -> ClientResult<Option<&Self::State>> {
		Ok(Some(&self.old_state))
	}

	fn set_block_data(
		&mut self,
		header: Block::Header,
		body: Option<Vec<Block::Extrinsic>>,
		indexed_body: Option<Vec<Vec<u8>>>,
		justifications: Option<Justifications>,
		leaf_state: NewBlockState,
	) -> ClientResult<()> {
		assert!(self.pending_block.is_none(), "Only one block per operation is allowed");
		self.pending_block =
			Some(PendingBlock { header, body, indexed_body, justifications, leaf_state });
		Ok(())
	}

	fn update_db_storage(
		&mut self,
		update: PrefixedMemoryDB<HashingFor<Block>>,
	) -> ClientResult<()> {
		self.db_updates = update;
		Ok(())
	}

	fn reset_storage(
		&mut self,
		storage: Storage,
		state_version: StateVersion,
	) -> ClientResult<Block::Hash> {
		let root = self.apply_new_state(storage, state_version)?;
		self.commit_state = true;
		Ok(root)
	}

	fn set_genesis_state(
		&mut self,
		storage: Storage,
		commit: bool,
		state_version: StateVersion,
	) -> ClientResult<Block::Hash> {
		let root = self.apply_new_state(storage, state_version)?;
		self.commit_state = commit;
		Ok(root)
	}

	fn insert_aux<I>(&mut self, ops: I) -> ClientResult<()>
	where
		I: IntoIterator<Item = (Vec<u8>, Option<Vec<u8>>)>,
	{
		self.aux_ops.append(&mut ops.into_iter().collect());
		Ok(())
	}

	fn update_storage(
		&mut self,
		update: StorageCollection,
		child_update: ChildStorageCollection,
	) -> ClientResult<()> {
		self.storage_updates = update;
		self.child_storage_updates = child_update;
		Ok(())
	}

	fn update_offchain_storage(
		&mut self,
		offchain_update: OffchainChangesCollection,
	) -> ClientResult<()> {
		self.offchain_storage_updates = offchain_update;
		Ok(())
	}

	fn mark_finalized(
		&mut self,
		block: Block::Hash,
		justification: Option<Justification>,
	) -> ClientResult<()> {
		self.finalized_blocks.push((block, justification));
		Ok(())
	}

	fn mark_head(&mut self, hash: Block::Hash) -> ClientResult<()> {
		assert!(self.set_head.is_none(), "Only one set head per operation is allowed");
		self.set_head = Some(hash);
		Ok(())
	}

	fn update_transaction_index(&mut self, index_ops: Vec<IndexOperation>) -> ClientResult<()> {
		self.index_ops = index_ops;
		Ok(())
	}

	fn set_create_gap(&mut self, create_gap: bool) {
		self.create_gap = create_gap;
	}

	fn set_commit_state(&mut self, commit: bool) {
		self.commit_state = commit;
	}
}

struct StorageDb<Block: BlockT> {
	pub db: Arc<dyn Database<DbHash>>,
	pub state_db: StateDb<Block::Hash, Vec<u8>, StateMetaDb>,
	prefix_keys: bool,
}

impl<Block: BlockT> sp_state_machine::Storage<HashingFor<Block>> for StorageDb<Block> {
	fn get(&self, key: &Block::Hash, prefix: Prefix) -> Result<Option<DBValue>, String> {
		if self.prefix_keys {
			let key = prefixed_key::<HashingFor<Block>>(key, prefix);
			self.state_db.get(&key, self)
		} else {
			self.state_db.get(key.as_ref(), self)
		}
		.map_err(|e| format!("Database backend error: {e:?}"))
	}
}

impl<Block: BlockT> sc_state_db::NodeDb for StorageDb<Block> {
	type Error = io::Error;
	type Key = [u8];

	fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
		Ok(self.db.get(columns::STATE, key))
	}
}

struct DbGenesisStorage<Block: BlockT> {
	root: Block::Hash,
	storage: PrefixedMemoryDB<HashingFor<Block>>,
}

impl<Block: BlockT> DbGenesisStorage<Block> {
	pub fn new(root: Block::Hash, storage: PrefixedMemoryDB<HashingFor<Block>>) -> Self {
		DbGenesisStorage { root, storage }
	}
}

impl<Block: BlockT> sp_state_machine::Storage<HashingFor<Block>> for DbGenesisStorage<Block> {
	fn get(&self, key: &Block::Hash, prefix: Prefix) -> Result<Option<DBValue>, String> {
		use hash_db::HashDB;
		Ok(self.storage.get(key, prefix))
	}
}

struct EmptyStorage<Block: BlockT>(pub Block::Hash);

impl<Block: BlockT> EmptyStorage<Block> {
	pub fn new() -> Self {
		let mut root = Block::Hash::default();
		let mut mdb = MemoryDB::<HashingFor<Block>>::default();
		// both triedbmut are the same on empty storage.
		sp_trie::trie_types::TrieDBMutBuilderV1::<HashingFor<Block>>::new(&mut mdb, &mut root)
			.build();
		EmptyStorage(root)
	}
}

impl<Block: BlockT> sp_state_machine::Storage<HashingFor<Block>> for EmptyStorage<Block> {
	fn get(&self, _key: &Block::Hash, _prefix: Prefix) -> Result<Option<DBValue>, String> {
		Ok(None)
	}
}

/// Frozen `value` at time `at`.
///
/// Used as inner structure under lock in `FrozenForDuration`.
struct Frozen<T: Clone> {
	at: std::time::Instant,
	value: Option<T>,
}

/// Some value frozen for period of time.
///
/// If time `duration` not passed since the value was instantiated,
/// current frozen value is returned. Otherwise, you have to provide
/// a new value which will be again frozen for `duration`.
pub(crate) struct FrozenForDuration<T: Clone> {
	duration: std::time::Duration,
	value: parking_lot::Mutex<Frozen<T>>,
}

impl<T: Clone> FrozenForDuration<T> {
	fn new(duration: std::time::Duration) -> Self {
		Self { duration, value: Frozen { at: std::time::Instant::now(), value: None }.into() }
	}

	fn take_or_else<F>(&self, f: F) -> T
	where
		F: FnOnce() -> T,
	{
		let mut lock = self.value.lock();
		let now = std::time::Instant::now();
		if now.saturating_duration_since(lock.at) > self.duration || lock.value.is_none() {
			let new_value = f();
			lock.at = now;
			lock.value = Some(new_value.clone());
			new_value
		} else {
			lock.value.as_ref().expect("Checked with in branch above; qed").clone()
		}
	}
}

/// Disk backend.
///
/// Disk backend keeps data in a key-value store. In archive mode, trie nodes are kept from all
/// blocks. Otherwise, trie nodes are kept only from some recent blocks.
pub struct Backend<Block: BlockT> {
	storage: Arc<StorageDb<Block>>,
	offchain_storage: offchain::LocalStorage,
	blockchain: BlockchainDb<Block>,
	canonicalization_delay: u64,
	import_lock: Arc<RwLock<()>>,
	is_archive: bool,
	blocks_pruning: BlocksPruning,
	io_stats: FrozenForDuration<(kvdb::IoStats, StateUsageInfo)>,
	state_usage: Arc<StateUsageStats>,
	genesis_state: RwLock<Option<Arc<DbGenesisStorage<Block>>>>,
	shared_trie_cache: Option<sp_trie::cache::SharedTrieCache<HashingFor<Block>>>,
}

impl<Block: BlockT> Backend<Block> {
	/// Create a new instance of database backend.
	///
	/// The pruning window is how old a block must be before the state is pruned.
	pub fn new(db_config: DatabaseSettings, canonicalization_delay: u64) -> ClientResult<Self> {
		use utils::OpenDbError;

		let db_source = &db_config.source;

		let (needs_init, db) =
			match crate::utils::open_database::<Block>(db_source, DatabaseType::Full, false) {
				Ok(db) => (false, db),
				Err(OpenDbError::DoesNotExist) => {
					let db =
						crate::utils::open_database::<Block>(db_source, DatabaseType::Full, true)?;
					(true, db)
				},
				Err(as_is) => return Err(as_is.into()),
			};

		Self::from_database(db as Arc<_>, canonicalization_delay, &db_config, needs_init)
	}

	/// Reset the shared trie cache.
	pub fn reset_trie_cache(&self) {
		if let Some(cache) = &self.shared_trie_cache {
			cache.reset();
		}
	}

	/// Create new memory-backed client backend for tests.
	#[cfg(any(test, feature = "test-helpers"))]
	pub fn new_test(blocks_pruning: u32, canonicalization_delay: u64) -> Self {
		Self::new_test_with_tx_storage(BlocksPruning::Some(blocks_pruning), canonicalization_delay)
	}

	/// Create new memory-backed client backend for tests.
	#[cfg(any(test, feature = "test-helpers"))]
	pub fn new_test_with_tx_storage(
		blocks_pruning: BlocksPruning,
		canonicalization_delay: u64,
	) -> Self {
		let db = kvdb_memorydb::create(crate::utils::NUM_COLUMNS);
		let db = sp_database::as_database(db);
		let state_pruning = match blocks_pruning {
			BlocksPruning::KeepAll => PruningMode::ArchiveAll,
			BlocksPruning::KeepFinalized => PruningMode::ArchiveCanonical,
			BlocksPruning::Some(n) => PruningMode::blocks_pruning(n),
		};
		let db_setting = DatabaseSettings {
			trie_cache_maximum_size: Some(16 * 1024 * 1024),
			state_pruning: Some(state_pruning),
			source: DatabaseSource::Custom { db, require_create_flag: true },
			blocks_pruning,
		};

		Self::new(db_setting, canonicalization_delay).expect("failed to create test-db")
	}

	/// Expose the Database that is used by this backend.
	/// The second argument is the Column that stores the State.
	///
	/// Should only be needed for benchmarking and testing.
	#[cfg(any(feature = "runtime-benchmarks", test))]
	pub fn expose_db(&self) -> (Arc<dyn sp_database::Database<DbHash>>, sp_database::ColumnId) {
		(self.storage.db.clone(), columns::STATE)
	}

	/// Expose the Storage that is used by this backend.
	///
	/// Should only be needed for benchmarking and testing.
	#[cfg(any(feature = "runtime-benchmarks", test))]
	pub fn expose_storage(&self) -> Arc<dyn sp_state_machine::Storage<HashingFor<Block>>> {
		self.storage.clone()
	}

	fn from_database(
		db: Arc<dyn Database<DbHash>>,
		canonicalization_delay: u64,
		config: &DatabaseSettings,
		should_init: bool,
	) -> ClientResult<Self> {
		let mut db_init_transaction = Transaction::new();

		let requested_state_pruning = config.state_pruning.clone();
		let state_meta_db = StateMetaDb(db.clone());
		let map_e = sp_blockchain::Error::from_state_db;

		let (state_db_init_commit_set, state_db) = StateDb::open(
			state_meta_db,
			requested_state_pruning,
			!db.supports_ref_counting(),
			should_init,
		)
		.map_err(map_e)?;

		apply_state_commit(&mut db_init_transaction, state_db_init_commit_set);

		let state_pruning_used = state_db.pruning_mode();
		let is_archive_pruning = state_pruning_used.is_archive();
		let blockchain = BlockchainDb::new(db.clone())?;

		let storage_db =
			StorageDb { db: db.clone(), state_db, prefix_keys: !db.supports_ref_counting() };

		let offchain_storage = offchain::LocalStorage::new(db.clone());

		let backend = Backend {
			storage: Arc::new(storage_db),
			offchain_storage,
			blockchain,
			canonicalization_delay,
			import_lock: Default::default(),
			is_archive: is_archive_pruning,
			io_stats: FrozenForDuration::new(std::time::Duration::from_secs(1)),
			state_usage: Arc::new(StateUsageStats::new()),
			blocks_pruning: config.blocks_pruning,
			genesis_state: RwLock::new(None),
			shared_trie_cache: config.trie_cache_maximum_size.map(|maximum_size| {
				SharedTrieCache::new(sp_trie::cache::CacheSize::new(maximum_size))
			}),
		};

		// Older DB versions have no last state key. Check if the state is available and set it.
		let info = backend.blockchain.info();
		if info.finalized_state.is_none() &&
			info.finalized_hash != Default::default() &&
			sc_client_api::Backend::have_state_at(
				&backend,
				info.finalized_hash,
				info.finalized_number,
			) {
			backend.blockchain.update_meta(MetaUpdate {
				hash: info.finalized_hash,
				number: info.finalized_number,
				is_best: info.finalized_hash == info.best_hash,
				is_finalized: true,
				with_state: true,
			});
		}

		db.commit(db_init_transaction)?;

		Ok(backend)
	}

	/// Handle setting head within a transaction. `route_to` should be the last
	/// block that existed in the database. `best_to` should be the best block
	/// to be set.
	///
	/// In the case where the new best block is a block to be imported, `route_to`
	/// should be the parent of `best_to`. In the case where we set an existing block
	/// to be best, `route_to` should equal to `best_to`.
	fn set_head_with_transaction(
		&self,
		transaction: &mut Transaction<DbHash>,
		route_to: Block::Hash,
		best_to: (NumberFor<Block>, Block::Hash),
	) -> ClientResult<(Vec<Block::Hash>, Vec<Block::Hash>)> {
		let mut enacted = Vec::default();
		let mut retracted = Vec::default();

		let (best_number, best_hash) = best_to;

		let meta = self.blockchain.meta.read();

		if meta.best_number.saturating_sub(best_number).saturated_into::<u64>() >
			self.canonicalization_delay
		{
			return Err(sp_blockchain::Error::SetHeadTooOld);
		}

		let parent_exists =
			self.blockchain.status(route_to)? == sp_blockchain::BlockStatus::InChain;

		// Cannot find tree route with empty DB or when imported a detached block.
		if meta.best_hash != Default::default() && parent_exists {
			let tree_route = sp_blockchain::tree_route(&self.blockchain, meta.best_hash, route_to)?;

			// uncanonicalize: check safety violations and ensure the numbers no longer
			// point to these block hashes in the key mapping.
			for r in tree_route.retracted() {
				if r.hash == meta.finalized_hash {
					warn!(
						"Potential safety failure: reverting finalized block {:?}",
						(&r.number, &r.hash)
					);

					return Err(sp_blockchain::Error::NotInFinalizedChain);
				}

				retracted.push(r.hash);
				utils::remove_number_to_key_mapping(transaction, columns::KEY_LOOKUP, r.number)?;
			}

			// canonicalize: set the number lookup to map to this block's hash.
			for e in tree_route.enacted() {
				enacted.push(e.hash);
				utils::insert_number_to_key_mapping(
					transaction,
					columns::KEY_LOOKUP,
					e.number,
					e.hash,
				)?;
			}
		}

		let lookup_key = utils::number_and_hash_to_lookup_key(best_number, &best_hash)?;
		transaction.set_from_vec(columns::META, meta_keys::BEST_BLOCK, lookup_key);
		utils::insert_number_to_key_mapping(
			transaction,
			columns::KEY_LOOKUP,
			best_number,
			best_hash,
		)?;

		Ok((enacted, retracted))
	}

	fn ensure_sequential_finalization(
		&self,
		header: &Block::Header,
		last_finalized: Option<Block::Hash>,
	) -> ClientResult<()> {
		let last_finalized =
			last_finalized.unwrap_or_else(|| self.blockchain.meta.read().finalized_hash);
		if last_finalized != self.blockchain.meta.read().genesis_hash &&
			*header.parent_hash() != last_finalized
		{
			return Err(sp_blockchain::Error::NonSequentialFinalization(format!(
				"Last finalized {last_finalized:?} not parent of {:?}",
				header.hash()
			)));
		}
		Ok(())
	}

	/// `remove_displaced` can be set to `false` if this is not the last of many subsequent calls
	/// for performance reasons.
	fn finalize_block_with_transaction(
		&self,
		transaction: &mut Transaction<DbHash>,
		hash: Block::Hash,
		header: &Block::Header,
		last_finalized: Option<Block::Hash>,
		justification: Option<Justification>,
		current_transaction_justifications: &mut HashMap<Block::Hash, Justification>,
		remove_displaced: bool,
	) -> ClientResult<MetaUpdate<Block>> {
		// TODO: ensure best chain contains this block.
		let number = *header.number();
		self.ensure_sequential_finalization(header, last_finalized)?;
		let with_state = sc_client_api::Backend::have_state_at(self, hash, number);

		self.note_finalized(
			transaction,
			header,
			hash,
			with_state,
			current_transaction_justifications,
			remove_displaced,
		)?;

		if let Some(justification) = justification {
			transaction.set_from_vec(
				columns::JUSTIFICATIONS,
				&utils::number_and_hash_to_lookup_key(number, hash)?,
				Justifications::from(justification.clone()).encode(),
			);
			current_transaction_justifications.insert(hash, justification);
		}
		Ok(MetaUpdate { hash, number, is_best: false, is_finalized: true, with_state })
	}

	// performs forced canonicalization with a delay after importing a non-finalized block.
	fn force_delayed_canonicalize(
		&self,
		transaction: &mut Transaction<DbHash>,
	) -> ClientResult<()> {
		let best_canonical = match self.storage.state_db.last_canonicalized() {
			LastCanonicalized::None => 0,
			LastCanonicalized::Block(b) => b,
			// Nothing needs to be done when canonicalization is not happening.
			LastCanonicalized::NotCanonicalizing => return Ok(()),
		};

		let info = self.blockchain.info();
		let best_number: u64 = self.blockchain.info().best_number.saturated_into();

		for to_canonicalize in
			best_canonical + 1..=best_number.saturating_sub(self.canonicalization_delay)
		{
			let hash_to_canonicalize = sc_client_api::blockchain::HeaderBackend::hash(
				&self.blockchain,
				to_canonicalize.saturated_into(),
			)?
			.ok_or_else(|| {
				let best_hash = info.best_hash;

				sp_blockchain::Error::Backend(format!(
					"Can't canonicalize missing block number #{to_canonicalize} when for best block {best_hash:?} (#{best_number})",
				))
			})?;

			if !sc_client_api::Backend::have_state_at(
				self,
				hash_to_canonicalize,
				to_canonicalize.saturated_into(),
			) {
				return Ok(());
			}

			trace!(target: "db", "Canonicalize block #{to_canonicalize} ({hash_to_canonicalize:?})");
			let commit = self.storage.state_db.canonicalize_block(&hash_to_canonicalize).map_err(
				sp_blockchain::Error::from_state_db::<
					sc_state_db::Error<sp_database::error::DatabaseError>,
				>,
			)?;
			apply_state_commit(transaction, commit);
		}

		Ok(())
	}

	fn try_commit_operation(&self, mut operation: BlockImportOperation<Block>) -> ClientResult<()> {
		let mut transaction = Transaction::new();

		operation.apply_aux(&mut transaction);
		operation.apply_offchain(&mut transaction);

		let mut meta_updates = Vec::with_capacity(operation.finalized_blocks.len());
		let (best_num, mut last_finalized_hash, mut last_finalized_num, mut block_gap) = {
			let meta = self.blockchain.meta.read();
			(meta.best_number, meta.finalized_hash, meta.finalized_number, meta.block_gap)
		};

		let mut block_gap_updated = false;

		let mut current_transaction_justifications: HashMap<Block::Hash, Justification> =
			HashMap::new();
		let mut finalized_blocks = operation.finalized_blocks.into_iter().peekable();
		while let Some((block_hash, justification)) = finalized_blocks.next() {
			let block_header = self.blockchain.expect_header(block_hash)?;
			meta_updates.push(self.finalize_block_with_transaction(
				&mut transaction,
				block_hash,
				&block_header,
				Some(last_finalized_hash),
				justification,
				&mut current_transaction_justifications,
				finalized_blocks.peek().is_none(),
			)?);
			last_finalized_hash = block_hash;
			last_finalized_num = *block_header.number();
		}

		let imported = if let Some(pending_block) = operation.pending_block {
			let hash = pending_block.header.hash();

			let parent_hash = *pending_block.header.parent_hash();
			let number = *pending_block.header.number();
			let highest_leaf = self
				.blockchain
				.leaves
				.read()
				.highest_leaf()
				.map(|(n, _)| n)
				.unwrap_or(Zero::zero());
			let existing_header = number <= highest_leaf && self.blockchain.header(hash)?.is_some();
			let existing_body = pending_block.body.is_some();

			// blocks are keyed by number + hash.
			let lookup_key = utils::number_and_hash_to_lookup_key(number, hash)?;

			if pending_block.leaf_state.is_best() {
				self.set_head_with_transaction(&mut transaction, parent_hash, (number, hash))?;
			};

			utils::insert_hash_to_key_mapping(&mut transaction, columns::KEY_LOOKUP, number, hash)?;

			transaction.set_from_vec(columns::HEADER, &lookup_key, pending_block.header.encode());
			if let Some(body) = pending_block.body {
				// If we have any index operations we save block in the new format with indexed
				// extrinsic headers Otherwise we save the body as a single blob.
				if operation.index_ops.is_empty() {
					transaction.set_from_vec(columns::BODY, &lookup_key, body.encode());
				} else {
					let body =
						apply_index_ops::<Block>(&mut transaction, body, operation.index_ops);
					transaction.set_from_vec(columns::BODY_INDEX, &lookup_key, body);
				}
			}
			if let Some(body) = pending_block.indexed_body {
				apply_indexed_body::<Block>(&mut transaction, body);
			}
			if let Some(justifications) = pending_block.justifications {
				transaction.set_from_vec(
					columns::JUSTIFICATIONS,
					&lookup_key,
					justifications.encode(),
				);
			}

			if number.is_zero() {
				transaction.set(columns::META, meta_keys::GENESIS_HASH, hash.as_ref());

				if operation.commit_state {
					transaction.set_from_vec(columns::META, meta_keys::FINALIZED_STATE, lookup_key);
				} else {
					// When we don't want to commit the genesis state, we still preserve it in
					// memory to bootstrap consensus. It is queried for an initial list of
					// authorities, etc.
					*self.genesis_state.write() = Some(Arc::new(DbGenesisStorage::new(
						*pending_block.header.state_root(),
						operation.db_updates.clone(),
					)));
				}
			}

			let finalized = if operation.commit_state {
				let mut changeset: sc_state_db::ChangeSet<Vec<u8>> =
					sc_state_db::ChangeSet::default();
				let mut ops: u64 = 0;
				let mut bytes: u64 = 0;
				let mut removal: u64 = 0;
				let mut bytes_removal: u64 = 0;
				for (mut key, (val, rc)) in operation.db_updates.drain() {
					self.storage.db.sanitize_key(&mut key);
					if rc > 0 {
						ops += 1;
						bytes += key.len() as u64 + val.len() as u64;
						if rc == 1 {
							changeset.inserted.push((key, val.to_vec()));
						} else {
							changeset.inserted.push((key.clone(), val.to_vec()));
							for _ in 0..rc - 1 {
								changeset.inserted.push((key.clone(), Default::default()));
							}
						}
					} else if rc < 0 {
						removal += 1;
						bytes_removal += key.len() as u64;
						if rc == -1 {
							changeset.deleted.push(key);
						} else {
							for _ in 0..-rc {
								changeset.deleted.push(key.clone());
							}
						}
					}
				}
				self.state_usage.tally_writes_nodes(ops, bytes);
				self.state_usage.tally_removed_nodes(removal, bytes_removal);

				let mut ops: u64 = 0;
				let mut bytes: u64 = 0;
				for (key, value) in operation
					.storage_updates
					.iter()
					.chain(operation.child_storage_updates.iter().flat_map(|(_, s)| s.iter()))
				{
					ops += 1;
					bytes += key.len() as u64;
					if let Some(v) = value.as_ref() {
						bytes += v.len() as u64;
					}
				}
				self.state_usage.tally_writes(ops, bytes);
				let number_u64 = number.saturated_into::<u64>();
				let commit = self
					.storage
					.state_db
					.insert_block(&hash, number_u64, pending_block.header.parent_hash(), changeset)
					.map_err(|e: sc_state_db::Error<sp_database::error::DatabaseError>| {
						sp_blockchain::Error::from_state_db(e)
					})?;
				apply_state_commit(&mut transaction, commit);
				if number <= last_finalized_num {
					// Canonicalize in the db when re-importing existing blocks with state.
					let commit = self.storage.state_db.canonicalize_block(&hash).map_err(
						sp_blockchain::Error::from_state_db::<
							sc_state_db::Error<sp_database::error::DatabaseError>,
						>,
					)?;
					apply_state_commit(&mut transaction, commit);
					meta_updates.push(MetaUpdate {
						hash,
						number,
						is_best: false,
						is_finalized: true,
						with_state: true,
					});
				}

				// Check if need to finalize. Genesis is always finalized instantly.
				let finalized = number_u64 == 0 || pending_block.leaf_state.is_final();
				finalized
			} else {
				(number.is_zero() && last_finalized_num.is_zero()) ||
					pending_block.leaf_state.is_final()
			};

			let header = &pending_block.header;
			let is_best = pending_block.leaf_state.is_best();
			debug!(
				target: "db",
				"DB Commit {hash:?} ({number}), best={is_best}, state={}, existing={existing_header}, finalized={finalized}",
				operation.commit_state,
			);

			self.state_usage.merge_sm(operation.old_state.usage_info());

			// release state reference so that it can be finalized
			// VERY IMPORTANT
			drop(operation.old_state);

			if finalized {
				// TODO: ensure best chain contains this block.
				self.ensure_sequential_finalization(header, Some(last_finalized_hash))?;
				let mut current_transaction_justifications = HashMap::new();
				self.note_finalized(
					&mut transaction,
					header,
					hash,
					operation.commit_state,
					&mut current_transaction_justifications,
					true,
				)?;
			} else {
				// canonicalize blocks which are old enough, regardless of finality.
				self.force_delayed_canonicalize(&mut transaction)?
			}

			if !existing_header {
				// Add a new leaf if the block has the potential to be finalized.
				if number > last_finalized_num || last_finalized_num.is_zero() {
					let mut leaves = self.blockchain.leaves.write();
					leaves.import(hash, number, parent_hash);
					leaves.prepare_transaction(
						&mut transaction,
						columns::META,
						meta_keys::LEAF_PREFIX,
					);
				}

				let mut children = children::read_children(
					&*self.storage.db,
					columns::META,
					meta_keys::CHILDREN_PREFIX,
					parent_hash,
				)?;
				if !children.contains(&hash) {
					children.push(hash);
					children::write_children(
						&mut transaction,
						columns::META,
						meta_keys::CHILDREN_PREFIX,
						parent_hash,
						children,
					);
				}
			}

			let should_check_block_gap = !existing_header || !existing_body;

			if should_check_block_gap {
				let insert_new_gap =
					|transaction: &mut Transaction<DbHash>,
					 new_gap: BlockGap<NumberFor<Block>>,
					 block_gap: &mut Option<BlockGap<NumberFor<Block>>>| {
						transaction.set(columns::META, meta_keys::BLOCK_GAP, &new_gap.encode());
						transaction.set(
							columns::META,
							meta_keys::BLOCK_GAP_VERSION,
							&BLOCK_GAP_CURRENT_VERSION.encode(),
						);
						block_gap.replace(new_gap);
					};

				if let Some(mut gap) = block_gap {
					match gap.gap_type {
						BlockGapType::MissingHeaderAndBody =>
							if number == gap.start {
								gap.start += One::one();
								utils::insert_number_to_key_mapping(
									&mut transaction,
									columns::KEY_LOOKUP,
									number,
									hash,
								)?;
								if gap.start > gap.end {
									transaction.remove(columns::META, meta_keys::BLOCK_GAP);
									transaction.remove(columns::META, meta_keys::BLOCK_GAP_VERSION);
									block_gap = None;
									debug!(target: "db", "Removed block gap.");
								} else {
									insert_new_gap(&mut transaction, gap, &mut block_gap);
									debug!(target: "db", "Update block gap. {block_gap:?}");
								}
								block_gap_updated = true;
							},
						BlockGapType::MissingBody => {
							// Gap increased when syncing the header chain during fast sync.
							if number == gap.end + One::one() && !existing_body {
								gap.end += One::one();
								utils::insert_number_to_key_mapping(
									&mut transaction,
									columns::KEY_LOOKUP,
									number,
									hash,
								)?;
								insert_new_gap(&mut transaction, gap, &mut block_gap);
								debug!(target: "db", "Update block gap. {block_gap:?}");
								block_gap_updated = true;
							// Gap decreased when downloading the full blocks.
							} else if number == gap.start && existing_body {
								gap.start += One::one();
								if gap.start > gap.end {
									transaction.remove(columns::META, meta_keys::BLOCK_GAP);
									transaction.remove(columns::META, meta_keys::BLOCK_GAP_VERSION);
									block_gap = None;
									debug!(target: "db", "Removed block gap.");
								} else {
									insert_new_gap(&mut transaction, gap, &mut block_gap);
									debug!(target: "db", "Update block gap. {block_gap:?}");
								}
								block_gap_updated = true;
							}
						},
					}
				} else if operation.create_gap {
					if number > best_num + One::one() &&
						self.blockchain.header(parent_hash)?.is_none()
					{
						let gap = BlockGap {
							start: best_num + One::one(),
							end: number - One::one(),
							gap_type: BlockGapType::MissingHeaderAndBody,
						};
						insert_new_gap(&mut transaction, gap, &mut block_gap);
						block_gap_updated = true;
						debug!(target: "db", "Detected block gap (warp sync) {block_gap:?}");
					} else if number == best_num + One::one() &&
						self.blockchain.header(parent_hash)?.is_some() &&
						!existing_body
					{
						let gap = BlockGap {
							start: number,
							end: number,
							gap_type: BlockGapType::MissingBody,
						};
						insert_new_gap(&mut transaction, gap, &mut block_gap);
						block_gap_updated = true;
						debug!(target: "db", "Detected block gap (fast sync) {block_gap:?}");
					}
				}
			}

			meta_updates.push(MetaUpdate {
				hash,
				number,
				is_best: pending_block.leaf_state.is_best(),
				is_finalized: finalized,
				with_state: operation.commit_state,
			});
			Some((pending_block.header, hash))
		} else {
			None
		};

		if let Some(set_head) = operation.set_head {
			if let Some(header) =
				sc_client_api::blockchain::HeaderBackend::header(&self.blockchain, set_head)?
			{
				let number = header.number();
				let hash = header.hash();

				self.set_head_with_transaction(&mut transaction, hash, (*number, hash))?;

				meta_updates.push(MetaUpdate {
					hash,
					number: *number,
					is_best: true,
					is_finalized: false,
					with_state: false,
				});
			} else {
				return Err(sp_blockchain::Error::UnknownBlock(format!(
					"Cannot set head {set_head:?}",
				)));
			}
		}

		self.storage.db.commit(transaction)?;

		// Apply all in-memory state changes.
		// Code beyond this point can't fail.

		if let Some((header, hash)) = imported {
			trace!(target: "db", "DB Commit done {hash:?}");
			let header_metadata = CachedHeaderMetadata::from(&header);
			self.blockchain.insert_header_metadata(header_metadata.hash, header_metadata);
			cache_header(&mut self.blockchain.header_cache.lock(), hash, Some(header));
		}

		for m in meta_updates {
			self.blockchain.update_meta(m);
		}
		if block_gap_updated {
			self.blockchain.update_block_gap(block_gap);
		}

		Ok(())
	}

	// Write stuff to a transaction after a new block is finalized. This canonicalizes finalized
	// blocks. Fails if called with a block which was not a child of the last finalized block.
	/// `remove_displaced` can be set to `false` if this is not the last of many subsequent calls
	/// for performance reasons.
	fn note_finalized(
		&self,
		transaction: &mut Transaction<DbHash>,
		f_header: &Block::Header,
		f_hash: Block::Hash,
		with_state: bool,
		current_transaction_justifications: &mut HashMap<Block::Hash, Justification>,
		remove_displaced: bool,
	) -> ClientResult<()> {
		let f_num = *f_header.number();

		let lookup_key = utils::number_and_hash_to_lookup_key(f_num, f_hash)?;
		if with_state {
			transaction.set_from_vec(columns::META, meta_keys::FINALIZED_STATE, lookup_key.clone());
		}
		transaction.set_from_vec(columns::META, meta_keys::FINALIZED_BLOCK, lookup_key);

		let requires_canonicalization = match self.storage.state_db.last_canonicalized() {
			LastCanonicalized::None => true,
			LastCanonicalized::Block(b) => f_num.saturated_into::<u64>() > b,
			LastCanonicalized::NotCanonicalizing => false,
		};

		if requires_canonicalization && sc_client_api::Backend::have_state_at(self, f_hash, f_num) {
			let commit = self.storage.state_db.canonicalize_block(&f_hash).map_err(
				sp_blockchain::Error::from_state_db::<
					sc_state_db::Error<sp_database::error::DatabaseError>,
				>,
			)?;
			apply_state_commit(transaction, commit);
		}

		if remove_displaced {
			let new_displaced = self.blockchain.displaced_leaves_after_finalizing(f_hash, f_num)?;

			self.blockchain.leaves.write().remove_displaced_leaves(FinalizationOutcome::new(
				new_displaced.displaced_leaves.iter().copied(),
			));

			if !matches!(self.blocks_pruning, BlocksPruning::KeepAll) {
				self.prune_displaced_branches(transaction, &new_displaced)?;
			}
		}

		self.prune_blocks(transaction, f_num, current_transaction_justifications)?;

		Ok(())
	}

	fn prune_blocks(
		&self,
		transaction: &mut Transaction<DbHash>,
		finalized_number: NumberFor<Block>,
		current_transaction_justifications: &mut HashMap<Block::Hash, Justification>,
	) -> ClientResult<()> {
		if let BlocksPruning::Some(blocks_pruning) = self.blocks_pruning {
			// Always keep the last finalized block
			let keep = std::cmp::max(blocks_pruning, 1);
			if finalized_number >= keep.into() {
				let number = finalized_number.saturating_sub(keep.into());

				// Before we prune a block, check if it is pinned
				if let Some(hash) = self.blockchain.hash(number)? {
					self.blockchain.insert_persisted_body_if_pinned(hash)?;

					// If the block was finalized in this transaction, it will not be in the db
					// yet.
					if let Some(justification) = current_transaction_justifications.remove(&hash) {
						self.blockchain.insert_justifications_if_pinned(hash, justification);
					} else {
						self.blockchain.insert_persisted_justifications_if_pinned(hash)?;
					}
				};

				self.prune_block(transaction, BlockId::<Block>::number(number))?;
			}
		}
		Ok(())
	}

	fn prune_displaced_branches(
		&self,
		transaction: &mut Transaction<DbHash>,
		displaced: &DisplacedLeavesAfterFinalization<Block>,
	) -> ClientResult<()> {
		// Discard all blocks from displaced branches
		for &hash in displaced.displaced_blocks.iter() {
			self.blockchain.insert_persisted_body_if_pinned(hash)?;
			self.prune_block(transaction, BlockId::<Block>::hash(hash))?;
		}
		Ok(())
	}

	fn prune_block(
		&self,
		transaction: &mut Transaction<DbHash>,
		id: BlockId<Block>,
	) -> ClientResult<()> {
		debug!(target: "db", "Removing block #{id}");
		utils::remove_from_db(
			transaction,
			&*self.storage.db,
			columns::KEY_LOOKUP,
			columns::BODY,
			id,
		)?;
		utils::remove_from_db(
			transaction,
			&*self.storage.db,
			columns::KEY_LOOKUP,
			columns::JUSTIFICATIONS,
			id,
		)?;
		if let Some(index) =
			read_db(&*self.storage.db, columns::KEY_LOOKUP, columns::BODY_INDEX, id)?
		{
			utils::remove_from_db(
				transaction,
				&*self.storage.db,
				columns::KEY_LOOKUP,
				columns::BODY_INDEX,
				id,
			)?;
			match Vec::<DbExtrinsic<Block>>::decode(&mut &index[..]) {
				Ok(index) =>
					for ex in index {
						if let DbExtrinsic::Indexed { hash, .. } = ex {
							transaction.release(columns::TRANSACTION, hash);
						}
					},
				Err(err) =>
					return Err(sp_blockchain::Error::Backend(format!(
						"Error decoding body list: {err}",
					))),
			}
		}
		Ok(())
	}

	fn empty_state(&self) -> RecordStatsState<RefTrackingState<Block>, Block> {
		let root = EmptyStorage::<Block>::new().0; // Empty trie
		let db_state = DbStateBuilder::<HashingFor<Block>>::new(self.storage.clone(), root)
			.with_optional_cache(self.shared_trie_cache.as_ref().map(|c| c.local_cache()))
			.build();
		let state = RefTrackingState::new(db_state, self.storage.clone(), None);
		RecordStatsState::new(state, None, self.state_usage.clone())
	}
}

fn apply_state_commit(
	transaction: &mut Transaction<DbHash>,
	commit: sc_state_db::CommitSet<Vec<u8>>,
) {
	for (key, val) in commit.data.inserted.into_iter() {
		transaction.set_from_vec(columns::STATE, &key[..], val);
	}
	for key in commit.data.deleted.into_iter() {
		transaction.remove(columns::STATE, &key[..]);
	}
	for (key, val) in commit.meta.inserted.into_iter() {
		transaction.set_from_vec(columns::STATE_META, &key[..], val);
	}
	for key in commit.meta.deleted.into_iter() {
		transaction.remove(columns::STATE_META, &key[..]);
	}
}

fn apply_index_ops<Block: BlockT>(
	transaction: &mut Transaction<DbHash>,
	body: Vec<Block::Extrinsic>,
	ops: Vec<IndexOperation>,
) -> Vec<u8> {
	let mut extrinsic_index: Vec<DbExtrinsic<Block>> = Vec::with_capacity(body.len());
	let mut index_map = HashMap::new();
	let mut renewed_map = HashMap::new();
	for op in ops {
		match op {
			IndexOperation::Insert { extrinsic, hash, size } => {
				index_map.insert(extrinsic, (hash, size));
			},
			IndexOperation::Renew { extrinsic, hash } => {
				renewed_map.insert(extrinsic, DbHash::from_slice(hash.as_ref()));
			},
		}
	}
	for (index, extrinsic) in body.into_iter().enumerate() {
		let db_extrinsic = if let Some(hash) = renewed_map.get(&(index as u32)) {
			// Bump ref counter
			let extrinsic = extrinsic.encode();
			transaction.reference(columns::TRANSACTION, DbHash::from_slice(hash.as_ref()));
			DbExtrinsic::Indexed { hash: *hash, header: extrinsic }
		} else {
			match index_map.get(&(index as u32)) {
				Some((hash, size)) => {
					let encoded = extrinsic.encode();
					if *size as usize <= encoded.len() {
						let offset = encoded.len() - *size as usize;
						transaction.store(
							columns::TRANSACTION,
							DbHash::from_slice(hash.as_ref()),
							encoded[offset..].to_vec(),
						);
						DbExtrinsic::Indexed {
							hash: DbHash::from_slice(hash.as_ref()),
							header: encoded[..offset].to_vec(),
						}
					} else {
						// Invalid indexed slice. Just store full data and don't index anything.
						DbExtrinsic::Full(extrinsic)
					}
				},
				_ => DbExtrinsic::Full(extrinsic),
			}
		};
		extrinsic_index.push(db_extrinsic);
	}
	debug!(
		target: "db",
		"DB transaction index: {} inserted, {} renewed, {} full",
		index_map.len(),
		renewed_map.len(),
		extrinsic_index.len() - index_map.len() - renewed_map.len(),
	);
	extrinsic_index.encode()
}

fn apply_indexed_body<Block: BlockT>(transaction: &mut Transaction<DbHash>, body: Vec<Vec<u8>>) {
	for extrinsic in body {
		let hash = sp_runtime::traits::BlakeTwo256::hash(&extrinsic);
		transaction.store(columns::TRANSACTION, DbHash::from_slice(hash.as_ref()), extrinsic);
	}
}

impl<Block> sc_client_api::backend::AuxStore for Backend<Block>
where
	Block: BlockT,
{
	fn insert_aux<
		'a,
		'b: 'a,
		'c: 'a,
		I: IntoIterator<Item = &'a (&'c [u8], &'c [u8])>,
		D: IntoIterator<Item = &'a &'b [u8]>,
	>(
		&self,
		insert: I,
		delete: D,
	) -> ClientResult<()> {
		let mut transaction = Transaction::new();
		for (k, v) in insert {
			transaction.set(columns::AUX, k, v);
		}
		for k in delete {
			transaction.remove(columns::AUX, k);
		}
		self.storage.db.commit(transaction)?;
		Ok(())
	}

	fn get_aux(&self, key: &[u8]) -> ClientResult<Option<Vec<u8>>> {
		Ok(self.storage.db.get(columns::AUX, key))
	}
}

impl<Block: BlockT> sc_client_api::backend::Backend<Block> for Backend<Block> {
	type BlockImportOperation = BlockImportOperation<Block>;
	type Blockchain = BlockchainDb<Block>;
	type State = RecordStatsState<RefTrackingState<Block>, Block>;
	type OffchainStorage = offchain::LocalStorage;

	fn begin_operation(&self) -> ClientResult<Self::BlockImportOperation> {
		Ok(BlockImportOperation {
			pending_block: None,
			old_state: self.empty_state(),
			db_updates: PrefixedMemoryDB::default(),
			storage_updates: Default::default(),
			child_storage_updates: Default::default(),
			offchain_storage_updates: Default::default(),
			aux_ops: Vec::new(),
			finalized_blocks: Vec::new(),
			set_head: None,
			commit_state: false,
			create_gap: true,
			index_ops: Default::default(),
		})
	}

	fn begin_state_operation(
		&self,
		operation: &mut Self::BlockImportOperation,
		block: Block::Hash,
	) -> ClientResult<()> {
		if block == Default::default() {
			operation.old_state = self.empty_state();
		} else {
			operation.old_state = self.state_at(block)?;
		}

		operation.commit_state = true;
		Ok(())
	}

	fn commit_operation(&self, operation: Self::BlockImportOperation) -> ClientResult<()> {
		let usage = operation.old_state.usage_info();
		self.state_usage.merge_sm(usage);

		if let Err(e) = self.try_commit_operation(operation) {
			let state_meta_db = StateMetaDb(self.storage.db.clone());
			self.storage
				.state_db
				.reset(state_meta_db)
				.map_err(sp_blockchain::Error::from_state_db)?;
			self.blockchain.clear_pinning_cache();
			Err(e)
		} else {
			self.storage.state_db.sync();
			Ok(())
		}
	}

	fn finalize_block(
		&self,
		hash: Block::Hash,
		justification: Option<Justification>,
	) -> ClientResult<()> {
		let mut transaction = Transaction::new();
		let header = self.blockchain.expect_header(hash)?;

		let mut current_transaction_justifications = HashMap::new();
		let m = self.finalize_block_with_transaction(
			&mut transaction,
			hash,
			&header,
			None,
			justification,
			&mut current_transaction_justifications,
			true,
		)?;

		self.storage.db.commit(transaction)?;
		self.blockchain.update_meta(m);
		Ok(())
	}

	fn append_justification(
		&self,
		hash: Block::Hash,
		justification: Justification,
	) -> ClientResult<()> {
		let mut transaction: Transaction<DbHash> = Transaction::new();
		let header = self.blockchain.expect_header(hash)?;
		let number = *header.number();

		// Check if the block is finalized first.
		let is_descendent_of = is_descendent_of(&self.blockchain, None);
		let last_finalized = self.blockchain.last_finalized()?;

		// We can do a quick check first, before doing a proper but more expensive check
		if number > self.blockchain.info().finalized_number ||
			(hash != last_finalized && !is_descendent_of(&hash, &last_finalized)?)
		{
			return Err(ClientError::NotInFinalizedChain);
		}

		let justifications = if let Some(mut stored_justifications) =
			self.blockchain.justifications(hash)?
		{
			if !stored_justifications.append(justification) {
				return Err(ClientError::BadJustification("Duplicate consensus engine ID".into()));
			}
			stored_justifications
		} else {
			Justifications::from(justification)
		};

		transaction.set_from_vec(
			columns::JUSTIFICATIONS,
			&utils::number_and_hash_to_lookup_key(number, hash)?,
			justifications.encode(),
		);

		self.storage.db.commit(transaction)?;

		Ok(())
	}

	fn offchain_storage(&self) -> Option<Self::OffchainStorage> {
		Some(self.offchain_storage.clone())
	}

	fn usage_info(&self) -> Option<UsageInfo> {
		let (io_stats, state_stats) = self.io_stats.take_or_else(|| {
			(
				// TODO: implement DB stats and cache size retrieval
				kvdb::IoStats::empty(),
				self.state_usage.take(),
			)
		});
		let database_cache = MemorySize::from_bytes(0);
		let state_cache = MemorySize::from_bytes(
			self.shared_trie_cache.as_ref().map_or(0, |c| c.used_memory_size()),
		);

		Some(UsageInfo {
			memory: MemoryInfo { state_cache, database_cache },
			io: IoInfo {
				transactions: io_stats.transactions,
				bytes_read: io_stats.bytes_read,
				bytes_written: io_stats.bytes_written,
				writes: io_stats.writes,
				reads: io_stats.reads,
				average_transaction_size: io_stats.avg_transaction_size() as u64,
				state_reads: state_stats.reads.ops,
				state_writes: state_stats.writes.ops,
				state_writes_cache: state_stats.overlay_writes.ops,
				state_reads_cache: state_stats.cache_reads.ops,
				state_writes_nodes: state_stats.nodes_writes.ops,
			},
		})
	}

	fn revert(
		&self,
		n: NumberFor<Block>,
		revert_finalized: bool,
	) -> ClientResult<(NumberFor<Block>, HashSet<Block::Hash>)> {
		let mut reverted_finalized = HashSet::new();

		let info = self.blockchain.info();

		let highest_leaf = self
			.blockchain
			.leaves
			.read()
			.highest_leaf()
			.and_then(|(n, h)| h.last().map(|h| (n, *h)));

		let best_number = info.best_number;
		let best_hash = info.best_hash;

		let finalized = info.finalized_number;

		let revertible = best_number - finalized;
		let n = if !revert_finalized && revertible < n { revertible } else { n };

		let (n, mut number_to_revert, mut hash_to_revert) = match highest_leaf {
			Some((l_n, l_h)) => (n + (l_n - best_number), l_n, l_h),
			None => (n, best_number, best_hash),
		};

		let mut revert_blocks = || -> ClientResult<NumberFor<Block>> {
			for c in 0..n.saturated_into::<u64>() {
				if number_to_revert.is_zero() {
					return Ok(c.saturated_into::<NumberFor<Block>>());
				}
				let mut transaction = Transaction::new();
				let removed = self.blockchain.header(hash_to_revert)?.ok_or_else(|| {
					sp_blockchain::Error::UnknownBlock(format!(
						"Error reverting to {hash_to_revert}. Block header not found.",
					))
				})?;
				let removed_hash = removed.hash();

				let prev_number = number_to_revert.saturating_sub(One::one());
				let prev_hash =
					if prev_number == best_number { best_hash } else { *removed.parent_hash() };

				if !self.have_state_at(prev_hash, prev_number) {
					return Ok(c.saturated_into::<NumberFor<Block>>());
				}

				match self.storage.state_db.revert_one() {
					Some(commit) => {
						apply_state_commit(&mut transaction, commit);

						number_to_revert = prev_number;
						hash_to_revert = prev_hash;

						let update_finalized = number_to_revert < finalized;

						let key = utils::number_and_hash_to_lookup_key(
							number_to_revert,
							&hash_to_revert,
						)?;
						if update_finalized {
							transaction.set_from_vec(
								columns::META,
								meta_keys::FINALIZED_BLOCK,
								key.clone(),
							);

							reverted_finalized.insert(removed_hash);
							if let Some((hash, _)) = self.blockchain.info().finalized_state {
								if hash == hash_to_revert {
									if !number_to_revert.is_zero() &&
										self.have_state_at(
											prev_hash,
											number_to_revert - One::one(),
										) {
										let lookup_key = utils::number_and_hash_to_lookup_key(
											number_to_revert - One::one(),
											prev_hash,
										)?;
										transaction.set_from_vec(
											columns::META,
											meta_keys::FINALIZED_STATE,
											lookup_key,
										);
									} else {
										transaction
											.remove(columns::META, meta_keys::FINALIZED_STATE);
									}
								}
							}
						}
						transaction.set_from_vec(columns::META, meta_keys::BEST_BLOCK, key);
						transaction.remove(columns::KEY_LOOKUP, removed.hash().as_ref());
						children::remove_children(
							&mut transaction,
							columns::META,
							meta_keys::CHILDREN_PREFIX,
							hash_to_revert,
						);
						self.storage.db.commit(transaction)?;

						let is_best = number_to_revert < best_number;

						self.blockchain.update_meta(MetaUpdate {
							hash: hash_to_revert,
							number: number_to_revert,
							is_best,
							is_finalized: update_finalized,
							with_state: false,
						});
					},
					None => return Ok(c.saturated_into::<NumberFor<Block>>()),
				}
			}

			Ok(n)
		};

		let reverted = revert_blocks()?;

		let revert_leaves = || -> ClientResult<()> {
			let mut transaction = Transaction::new();
			let mut leaves = self.blockchain.leaves.write();

			leaves.revert(hash_to_revert, number_to_revert);
			leaves.prepare_transaction(&mut transaction, columns::META, meta_keys::LEAF_PREFIX);
			self.storage.db.commit(transaction)?;

			Ok(())
		};

		revert_leaves()?;

		Ok((reverted, reverted_finalized))
	}

	fn remove_leaf_block(&self, hash: Block::Hash) -> ClientResult<()> {
		let best_hash = self.blockchain.info().best_hash;

		if best_hash == hash {
			return Err(sp_blockchain::Error::Backend(format!("Can't remove best block {hash:?}")));
		}

		let hdr = self.blockchain.header_metadata(hash)?;
		if !self.have_state_at(hash, hdr.number) {
			return Err(sp_blockchain::Error::UnknownBlock(format!(
				"State already discarded for {hash:?}",
			)));
		}

		let mut leaves = self.blockchain.leaves.write();
		if !leaves.contains(hdr.number, hash) {
			return Err(sp_blockchain::Error::Backend(format!(
				"Can't remove non-leaf block {hash:?}",
			)));
		}

		let mut transaction = Transaction::new();
		if let Some(commit) = self.storage.state_db.remove(&hash) {
			apply_state_commit(&mut transaction, commit);
		}
		transaction.remove(columns::KEY_LOOKUP, hash.as_ref());

		let children: Vec<_> = self
			.blockchain()
			.children(hdr.parent)?
			.into_iter()
			.filter(|child_hash| *child_hash != hash)
			.collect();
		let parent_leaf = if children.is_empty() {
			children::remove_children(
				&mut transaction,
				columns::META,
				meta_keys::CHILDREN_PREFIX,
				hdr.parent,
			);
			Some(hdr.parent)
		} else {
			children::write_children(
				&mut transaction,
				columns::META,
				meta_keys::CHILDREN_PREFIX,
				hdr.parent,
				children,
			);
			None
		};

		let remove_outcome = leaves.remove(hash, hdr.number, parent_leaf);
		leaves.prepare_transaction(&mut transaction, columns::META, meta_keys::LEAF_PREFIX);
		if let Err(e) = self.storage.db.commit(transaction) {
			if let Some(outcome) = remove_outcome {
				leaves.undo().undo_remove(outcome);
			}
			return Err(e.into());
		}
		self.blockchain().remove_header_metadata(hash);
		Ok(())
	}

	fn blockchain(&self) -> &BlockchainDb<Block> {
		&self.blockchain
	}

	fn state_at(&self, hash: Block::Hash) -> ClientResult<Self::State> {
		if hash == self.blockchain.meta.read().genesis_hash {
			if let Some(genesis_state) = &*self.genesis_state.read() {
				let root = genesis_state.root;
				let db_state =
					DbStateBuilder::<HashingFor<Block>>::new(genesis_state.clone(), root)
						.with_optional_cache(
							self.shared_trie_cache.as_ref().map(|c| c.local_cache()),
						)
						.build();

				let state = RefTrackingState::new(db_state, self.storage.clone(), None);
				return Ok(RecordStatsState::new(state, None, self.state_usage.clone()));
			}
		}

		match self.blockchain.header_metadata(hash) {
			Ok(ref hdr) => {
				let hint = || {
					sc_state_db::NodeDb::get(self.storage.as_ref(), hdr.state_root.as_ref())
						.unwrap_or(None)
						.is_some()
				};

				if let Ok(()) =
					self.storage.state_db.pin(&hash, hdr.number.saturated_into::<u64>(), hint)
				{
					let root = hdr.state_root;
					let db_state =
						DbStateBuilder::<HashingFor<Block>>::new(self.storage.clone(), root)
							.with_optional_cache(
								self.shared_trie_cache.as_ref().map(|c| c.local_cache()),
							)
							.build();
					let state = RefTrackingState::new(db_state, self.storage.clone(), Some(hash));
					Ok(RecordStatsState::new(state, Some(hash), self.state_usage.clone()))
				} else {
					Err(sp_blockchain::Error::UnknownBlock(format!(
						"State already discarded for {hash:?}",
					)))
				}
			},
			Err(e) => Err(e),
		}
	}

	fn have_state_at(&self, hash: Block::Hash, number: NumberFor<Block>) -> bool {
		if self.is_archive {
			match self.blockchain.header_metadata(hash) {
				Ok(header) => sp_state_machine::Storage::get(
					self.storage.as_ref(),
					&header.state_root,
					(&[], None),
				)
				.unwrap_or(None)
				.is_some(),
				_ => false,
			}
		} else {
			match self.storage.state_db.is_pruned(&hash, number.saturated_into::<u64>()) {
				IsPruned::Pruned => false,
				IsPruned::NotPruned => true,
				IsPruned::MaybePruned => match self.blockchain.header_metadata(hash) {
					Ok(header) => sp_state_machine::Storage::get(
						self.storage.as_ref(),
						&header.state_root,
						(&[], None),
					)
					.unwrap_or(None)
					.is_some(),
					_ => false,
				},
			}
		}
	}

	fn import_state(
		&self,
		at: Block::Hash,
		storage: sp_runtime::Storage,
		state_version: sp_runtime::StateVersion,
	) -> sp_blockchain::Result<Block::Hash> {
		let root = self.blockchain.header_metadata(at).map(|header| header.state_root)?;

		let storage_db: Arc<dyn sp_state_machine::Storage<HashingFor<Block>>> =
			self.storage.clone();
		let mut state_importer = StateImporter::new(&storage_db, self.storage.db.clone());

		let trie_err =
			|err: Box<TrieError<LayoutV0<Block>>>| sp_blockchain::Error::Application(err);

		let child_deltas = storage.children_default.values().map(|child_content| {
			(
				&child_content.child_info,
				child_content.data.iter().map(|(k, v)| (&k[..], Some(&v[..]))),
			)
		});

		let mut child_roots = Vec::new();

		// child first
		for (child_info, child_delta) in child_deltas {
			let default_root = match child_info.child_type() {
				sp_storage::ChildType::ParentKeyId =>
					sp_trie::empty_child_trie_root::<LayoutV1<Block>>(),
			};

			let new_child_root = match state_version {
				StateVersion::V0 => {
					let child_root = match crate::state_importer::read_child_root::<
						_,
						_,
						LayoutV0<Block>,
					>(&state_importer, &root, &child_info)
					{
						Ok(Some(hash)) => hash,
						Ok(None) => default_root,
						Err(e) => {
							warn!(target: "trie", "Failed to read child storage root: {}", e);
							default_root
						},
					};

					sp_trie::child_delta_trie_root::<LayoutV0<Block>, _, _, _, _, _, _>(
						child_info.keyspace(),
						&mut state_importer,
						child_root,
						child_delta,
						None,
						None,
					)
					.map_err(trie_err)?
				},
				StateVersion::V1 => {
					let child_root = match crate::state_importer::read_child_root::<
						_,
						_,
						LayoutV1<Block>,
					>(&state_importer, &root, &child_info)
					{
						Ok(Some(hash)) => hash,
						Ok(None) => default_root,
						Err(e) => {
							warn!(target: "trie", "Failed to read child storage root: {}", e);
							default_root
						},
					};

					sp_trie::child_delta_trie_root::<LayoutV1<Block>, _, _, _, _, _, _>(
						child_info.keyspace(),
						&mut state_importer,
						child_root,
						child_delta,
						None,
						None,
					)
					.map_err(trie_err)?
				},
			};

			let is_default = new_child_root == default_root;

			let prefixed_storage_key = child_info.prefixed_storage_key().into_inner();

			if is_default {
				child_roots.push((prefixed_storage_key, None));
			} else {
				child_roots.push((prefixed_storage_key, Some(new_child_root.encode())));
			}
		}

		let delta = storage
			.top
			.into_iter()
			.map(|(k, v)| (k, Some(v)))
			.chain(child_roots.into_iter());

		let state_root = match state_version {
			StateVersion::V0 => sp_trie::delta_trie_root::<LayoutV0<Block>, _, _, _, _, _>(
				&mut state_importer,
				root,
				delta,
				None,
				None,
			)
			.map_err(trie_err)?,
			StateVersion::V1 => sp_trie::delta_trie_root::<LayoutV1<Block>, _, _, _, _, _>(
				&mut state_importer,
				root,
				delta,
				None,
				None,
			)
			.map_err(trie_err)?,
		};

		Ok(state_root)
	}

	fn get_import_lock(&self) -> &RwLock<()> {
		&self.import_lock
	}

	fn requires_full_sync(&self) -> bool {
		matches!(
			self.storage.state_db.pruning_mode(),
			PruningMode::ArchiveAll | PruningMode::ArchiveCanonical
		)
	}

	fn pin_block(&self, hash: <Block as BlockT>::Hash) -> sp_blockchain::Result<()> {
		let hint = || {
			let header_metadata = self.blockchain.header_metadata(hash);
			header_metadata
				.map(|hdr| {
					sc_state_db::NodeDb::get(self.storage.as_ref(), hdr.state_root.as_ref())
						.unwrap_or(None)
						.is_some()
				})
				.unwrap_or(false)
		};

		if let Some(number) = self.blockchain.number(hash)? {
			self.storage.state_db.pin(&hash, number.saturated_into::<u64>(), hint).map_err(
				|_| {
					sp_blockchain::Error::UnknownBlock(format!(
						"Unable to pin: state already discarded for `{hash:?}`",
					))
				},
			)?;
		} else {
			return Err(ClientError::UnknownBlock(format!(
				"Can not pin block with hash `{hash:?}`. Block not found.",
			)));
		}

		if self.blocks_pruning != BlocksPruning::KeepAll {
			// Only increase reference count for this hash. Value is loaded once we prune.
			self.blockchain.bump_ref(hash);
		}
		Ok(())
	}

	fn unpin_block(&self, hash: <Block as BlockT>::Hash) {
		self.storage.state_db.unpin(&hash);

		if self.blocks_pruning != BlocksPruning::KeepAll {
			self.blockchain.unpin(hash);
		}
	}
}

impl<Block: BlockT> sc_client_api::backend::LocalBackend<Block> for Backend<Block> {}
