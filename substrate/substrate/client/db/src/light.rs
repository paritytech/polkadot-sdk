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

//! RocksDB-based light client blockchain storage.

use std::sync::Arc;
use parking_lot::RwLock;

use kvdb::{KeyValueDB, DBTransaction};

use client::blockchain::{BlockStatus, Cache as BlockchainCache,
	HeaderBackend as BlockchainHeaderBackend, Info as BlockchainInfo};
use client::cht;
use client::error::{ErrorKind as ClientErrorKind, Result as ClientResult};
use client::light::blockchain::Storage as LightBlockchainStorage;
use codec::{Decode, Encode};
use primitives::{AuthorityId, H256, KeccakHasher};
use runtime_primitives::generic::BlockId;
use runtime_primitives::traits::{Block as BlockT, Header as HeaderT, Hash, HashFor,
	Zero, One, As, NumberFor};
use cache::DbCache;
use utils::{meta_keys, Meta, db_err, number_to_db_key, db_key_to_number, open_database,
	read_db, read_id, read_meta};
use DatabaseSettings;

pub(crate) mod columns {
	pub const META: Option<u32> = ::utils::COLUMN_META;
	pub const BLOCK_INDEX: Option<u32> = Some(1);
	pub const HEADER: Option<u32> = Some(2);
	pub const AUTHORITIES: Option<u32> = Some(3);
	pub const CHT: Option<u32> = Some(4);
}

/// Keep authorities for last 'AUTHORITIES_ENTRIES_TO_KEEP' blocks.
pub(crate) const AUTHORITIES_ENTRIES_TO_KEEP: u64 = cht::SIZE;

/// Light blockchain storage. Stores most recent headers + CHTs for older headers.
pub struct LightStorage<Block: BlockT> {
	db: Arc<KeyValueDB>,
	meta: RwLock<Meta<<<Block as BlockT>::Header as HeaderT>::Number, Block::Hash>>,
	cache: DbCache<Block>,
}

#[derive(Clone, PartialEq, Debug)]
struct BestAuthorities<N> {
	/// first block, when this set became actual
	valid_from: N,
	/// None means that we do not know the set starting from `valid_from` block
	authorities: Option<Vec<AuthorityId>>,
}

impl<Block> LightStorage<Block>
	where
		Block: BlockT,
{
	/// Create new storage with given settings.
	pub fn new(config: DatabaseSettings) -> ClientResult<Self> {
		let db = open_database(&config, "light")?;

		Self::from_kvdb(db as Arc<_>)
	}

	#[cfg(test)]
	pub(crate) fn new_test() -> Self {
		use utils::NUM_COLUMNS;

		let db = Arc::new(::kvdb_memorydb::create(NUM_COLUMNS));

		Self::from_kvdb(db as Arc<_>).expect("failed to create test-db")
	}

	fn from_kvdb(db: Arc<KeyValueDB>) -> ClientResult<Self> {
		let cache = DbCache::new(db.clone(), columns::BLOCK_INDEX, columns::AUTHORITIES)?;
		let meta = RwLock::new(read_meta::<Block>(&*db, columns::HEADER)?);

		Ok(LightStorage {
			db,
			meta,
			cache,
		})
	}

	#[cfg(test)]
	pub(crate) fn db(&self) -> &Arc<KeyValueDB> {
		&self.db
	}

	#[cfg(test)]
	pub(crate) fn cache(&self) -> &DbCache<Block> {
		&self.cache
	}

	fn update_meta(&self, hash: Block::Hash, number: <<Block as BlockT>::Header as HeaderT>::Number, is_best: bool) {
		if is_best {
			let mut meta = self.meta.write();
			if number == <<Block as BlockT>::Header as HeaderT>::Number::zero() {
				meta.genesis_hash = hash;
			}

			meta.best_number = number;
			meta.best_hash = hash;
		}
	}
}

impl<Block> BlockchainHeaderBackend<Block> for LightStorage<Block>
	where
		Block: BlockT,
{
	fn header(&self, id: BlockId<Block>) -> ClientResult<Option<Block::Header>> {
		match read_db(&*self.db, columns::BLOCK_INDEX, columns::HEADER, id)? {
			Some(header) => match Block::Header::decode(&mut &header[..]) {
				Some(header) => Ok(Some(header)),
				None => return Err(ClientErrorKind::Backend("Error decoding header".into()).into()),
			}
			None => Ok(None),
		}
	}

	fn info(&self) -> ClientResult<BlockchainInfo<Block>> {
		let meta = self.meta.read();
		Ok(BlockchainInfo {
			best_hash: meta.best_hash,
			best_number: meta.best_number,
			genesis_hash: meta.genesis_hash,
		})
	}

	fn status(&self, id: BlockId<Block>) -> ClientResult<BlockStatus> {
		let exists = match id {
			BlockId::Hash(_) => read_id(&*self.db, columns::BLOCK_INDEX, id)?.is_some(),
			BlockId::Number(n) => n <= self.meta.read().best_number,
		};
		match exists {
			true => Ok(BlockStatus::InChain),
			false => Ok(BlockStatus::Unknown),
		}
	}

	fn number(&self, hash: Block::Hash) -> ClientResult<Option<<<Block as BlockT>::Header as HeaderT>::Number>> {
		read_id::<Block>(&*self.db, columns::BLOCK_INDEX, BlockId::Hash(hash))
			.and_then(|key| match key {
				Some(key) => Ok(Some(db_key_to_number(&key)?)),
				None => Ok(None),
			})
	}

	fn hash(&self, number: <<Block as BlockT>::Header as HeaderT>::Number) -> ClientResult<Option<Block::Hash>> {
		read_db::<Block>(&*self.db, columns::BLOCK_INDEX, columns::HEADER, BlockId::Number(number)).map(|x|
			x.map(|raw| HashFor::<Block>::hash(&raw[..])).map(Into::into)
		)
	}
}

impl<Block> LightBlockchainStorage<Block> for LightStorage<Block>
	where
		Block: BlockT,
		Block::Hash: From<H256>,
{
	fn import_header(&self, is_new_best: bool, header: Block::Header, authorities: Option<Vec<AuthorityId>>) -> ClientResult<()> {
		let mut transaction = DBTransaction::new();

		let hash = header.hash();
		let number = *header.number();
		let key = number_to_db_key(number);

		transaction.put(columns::HEADER, &key, &header.encode());
		transaction.put(columns::BLOCK_INDEX, hash.as_ref(), &key);

		let best_authorities = if is_new_best {
			transaction.put(columns::META, meta_keys::BEST_BLOCK, &key);

			// cache authorities for previous block
			let number: u64 = number.as_();
			let previous_number = number.checked_sub(1);
			let best_authorities = previous_number
				.and_then(|previous_number| self.cache.authorities_at_cache()
					.commit_best_entry(&mut transaction, As::sa(previous_number), authorities));

			// prune authorities from 'ancient' blocks
			if let Some(ancient_number) = number.checked_sub(AUTHORITIES_ENTRIES_TO_KEEP) {
				self.cache.authorities_at_cache().prune_entries(&mut transaction, As::sa(ancient_number))?;
			}

			best_authorities
		} else {
			None
		};

		// build new CHT if required
		if let Some(new_cht_number) = cht::is_build_required(cht::SIZE, *header.number()) {
			let new_cht_start: NumberFor<Block> = cht::start_number(cht::SIZE, new_cht_number);
			let new_cht_root: Option<Block::Hash> = cht::compute_root::<Block::Header, KeccakHasher, _>(
				cht::SIZE, new_cht_number, (new_cht_start.as_()..)
				.map(|num| self.hash(As::sa(num)).unwrap_or_default()));

			if let Some(new_cht_root) = new_cht_root {
				transaction.put(columns::CHT, &number_to_db_key(new_cht_start), new_cht_root.as_ref());

				let mut prune_block = new_cht_start;
				let new_cht_end = cht::end_number(cht::SIZE, new_cht_number);
				trace!(target: "db", "Replacing blocks [{}..{}] with CHT#{}", new_cht_start, new_cht_end, new_cht_number);

				while prune_block <= new_cht_end {
					transaction.delete(columns::HEADER, &number_to_db_key(prune_block));
 					prune_block += <<Block as BlockT>::Header as HeaderT>::Number::one();
				}
			}
		}

		debug!("Light DB Commit {:?} ({})", hash, number);
		self.db.write(transaction).map_err(db_err)?;
		self.update_meta(hash, number, is_new_best);
		if let Some(best_authorities) = best_authorities {
			self.cache.authorities_at_cache().update_best_entry(Some(best_authorities));
		}

		Ok(())
	}

	fn cht_root(&self, cht_size: u64, block: <<Block as BlockT>::Header as HeaderT>::Number) -> ClientResult<Block::Hash> {
		let no_cht_for_block = || ClientErrorKind::Backend(format!("CHT for block {} not exists", block)).into();

		let cht_number = cht::block_to_cht_number(cht_size, block).ok_or_else(no_cht_for_block)?;
		let cht_start = cht::start_number(cht_size, cht_number);
		self.db.get(columns::CHT, &number_to_db_key(cht_start)).map_err(db_err)?
			.ok_or_else(no_cht_for_block)
			.and_then(|hash| Block::Hash::decode(&mut &*hash).ok_or_else(no_cht_for_block))
	}

	fn cache(&self) -> Option<&BlockchainCache<Block>> {
		Some(&self.cache)
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use client::cht;
	use runtime_primitives::testing::{H256 as Hash, Header, Block as RawBlock};
	use super::*;

	type Block = RawBlock<u32>;

	pub fn insert_block(
		db: &LightStorage<Block>,
		parent: &Hash,
		number: u64,
		authorities: Option<Vec<AuthorityId>>
	) -> Hash {
		let header = Header {
			number: number.into(),
			parent_hash: *parent,
			state_root: Default::default(),
			digest: Default::default(),
			extrinsics_root: Default::default(),
		};

		let hash = header.hash();
		db.import_header(true, header, authorities).unwrap();
		hash
	}

	#[test]
	fn returns_known_header() {
		let db = LightStorage::new_test();
		let known_hash = insert_block(&db, &Default::default(), 0, None);
		let header_by_hash = db.header(BlockId::Hash(known_hash)).unwrap().unwrap();
		let header_by_number = db.header(BlockId::Number(0)).unwrap().unwrap();
		assert_eq!(header_by_hash, header_by_number);
	}

	#[test]
	fn does_not_return_unknown_header() {
		let db = LightStorage::<Block>::new_test();
		assert!(db.header(BlockId::Hash(1.into())).unwrap().is_none());
		assert!(db.header(BlockId::Number(0)).unwrap().is_none());
	}

	#[test]
	fn returns_info() {
		let db = LightStorage::new_test();
		let genesis_hash = insert_block(&db, &Default::default(), 0, None);
		let info = db.info().unwrap();
		assert_eq!(info.best_hash, genesis_hash);
		assert_eq!(info.best_number, 0);
		assert_eq!(info.genesis_hash, genesis_hash);
		let best_hash = insert_block(&db, &genesis_hash, 1, None);
		let info = db.info().unwrap();
		assert_eq!(info.best_hash, best_hash);
		assert_eq!(info.best_number, 1);
		assert_eq!(info.genesis_hash, genesis_hash);
	}

	#[test]
	fn returns_block_status() {
		let db = LightStorage::new_test();
		let genesis_hash = insert_block(&db, &Default::default(), 0, None);
		assert_eq!(db.status(BlockId::Hash(genesis_hash)).unwrap(), BlockStatus::InChain);
		assert_eq!(db.status(BlockId::Number(0)).unwrap(), BlockStatus::InChain);
		assert_eq!(db.status(BlockId::Hash(1.into())).unwrap(), BlockStatus::Unknown);
		assert_eq!(db.status(BlockId::Number(1)).unwrap(), BlockStatus::Unknown);
	}

	#[test]
	fn returns_block_hash() {
		let db = LightStorage::new_test();
		let genesis_hash = insert_block(&db, &Default::default(), 0, None);
		assert_eq!(db.hash(0).unwrap(), Some(genesis_hash));
		assert_eq!(db.hash(1).unwrap(), None);
	}

	#[test]
	fn import_header_works() {
		let db = LightStorage::new_test();

		let genesis_hash = insert_block(&db, &Default::default(), 0, None);
		assert_eq!(db.db.iter(columns::HEADER).count(), 1);
		assert_eq!(db.db.iter(columns::BLOCK_INDEX).count(), 1);

		let _ = insert_block(&db, &genesis_hash, 1, None);
		assert_eq!(db.db.iter(columns::HEADER).count(), 2);
		assert_eq!(db.db.iter(columns::BLOCK_INDEX).count(), 2);
	}

	#[test]
	fn ancient_headers_are_replaced_with_cht() {
		let db = LightStorage::new_test();

		// insert genesis block header (never pruned)
		let mut prev_hash = insert_block(&db, &Default::default(), 0, None);

		// insert SIZE blocks && ensure that nothing is pruned
		for number in 0..cht::SIZE {
			prev_hash = insert_block(&db, &prev_hash, 1 + number, None);
		}
		assert_eq!(db.db.iter(columns::HEADER).count(), (1 + cht::SIZE) as usize);
		assert_eq!(db.db.iter(columns::CHT).count(), 0);

		// insert next SIZE blocks && ensure that nothing is pruned
		for number in 0..cht::SIZE {
			prev_hash = insert_block(&db, &prev_hash, 1 + cht::SIZE + number, None);
		}
		assert_eq!(db.db.iter(columns::HEADER).count(), (1 + cht::SIZE + cht::SIZE) as usize);
		assert_eq!(db.db.iter(columns::CHT).count(), 0);

		// insert block #{2 * cht::SIZE + 1} && check that new CHT is created + headers of this CHT are pruned
		insert_block(&db, &prev_hash, 1 + cht::SIZE + cht::SIZE, None);
		assert_eq!(db.db.iter(columns::HEADER).count(), (1 + cht::SIZE + 1) as usize);
		assert_eq!(db.db.iter(columns::CHT).count(), 1);
		assert!((0..cht::SIZE).all(|i| db.db.get(columns::HEADER, &number_to_db_key(1 + i)).unwrap().is_none()));
	}

	#[test]
	fn get_cht_fails_for_genesis_block() {
		assert!(LightStorage::<Block>::new_test().cht_root(cht::SIZE, 0).is_err());
	}

	#[test]
	fn get_cht_fails_for_non_existant_cht() {
		assert!(LightStorage::<Block>::new_test().cht_root(cht::SIZE, (cht::SIZE / 2) as u64).is_err());
	}

	#[test]
	fn get_cht_works() {
		let db = LightStorage::new_test();

		// insert 1 + SIZE + SIZE + 1 blocks so that CHT#0 is created
		let mut prev_hash = Default::default();
		for i in 0..1 + cht::SIZE + cht::SIZE + 1 {
			prev_hash = insert_block(&db, &prev_hash, i as u64, None);
		}

		let cht_root_1 = db.cht_root(cht::SIZE, cht::start_number(cht::SIZE, 0)).unwrap();
		let cht_root_2 = db.cht_root(cht::SIZE, (cht::start_number(cht::SIZE, 0) + cht::SIZE / 2) as u64).unwrap();
		let cht_root_3 = db.cht_root(cht::SIZE, cht::end_number(cht::SIZE, 0)).unwrap();
		assert_eq!(cht_root_1, cht_root_2);
		assert_eq!(cht_root_2, cht_root_3);
	}
}
