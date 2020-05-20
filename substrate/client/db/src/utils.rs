// This file is part of Substrate.

// Copyright (C) 2017-2020 Parity Technologies (UK) Ltd.
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

//! Db-based backend utility structures and functions, used by both
//! full and light storages.

use std::sync::Arc;
use std::convert::TryInto;

use log::debug;

use codec::Decode;
use sp_trie::DBValue;
use sp_database::Transaction;
use sp_runtime::generic::BlockId;
use sp_runtime::traits::{
	Block as BlockT, Header as HeaderT, Zero,
	UniqueSaturatedFrom, UniqueSaturatedInto,
};
use crate::{DatabaseSettings, DatabaseSettingsSrc, Database, DbHash};

/// Number of columns in the db. Must be the same for both full && light dbs.
/// Otherwise RocksDb will fail to open database && check its type.
#[cfg(any(feature = "kvdb-rocksdb", feature = "test-helpers", test))]
pub const NUM_COLUMNS: u32 = 11;
/// Meta column. The set of keys in the column is shared by full && light storages.
pub const COLUMN_META: u32 = 0;

/// Keys of entries in COLUMN_META.
pub mod meta_keys {
	/// Type of storage (full or light).
	pub const TYPE: &[u8; 4] = b"type";
	/// Best block key.
	pub const BEST_BLOCK: &[u8; 4] = b"best";
	/// Last finalized block key.
	pub const FINALIZED_BLOCK: &[u8; 5] = b"final";
	/// Meta information prefix for list-based caches.
	pub const CACHE_META_PREFIX: &[u8; 5] = b"cache";
	/// Meta information for changes tries key.
	pub const CHANGES_TRIES_META: &[u8; 5] = b"ctrie";
	/// Genesis block hash.
	pub const GENESIS_HASH: &[u8; 3] = b"gen";
	/// Leaves prefix list key.
	pub const LEAF_PREFIX: &[u8; 4] = b"leaf";
	/// Children prefix list key.
	pub const CHILDREN_PREFIX: &[u8; 8] = b"children";
}

/// Database metadata.
#[derive(Debug)]
pub struct Meta<N, H> {
	/// Hash of the best known block.
	pub best_hash: H,
	/// Number of the best known block.
	pub best_number: N,
	/// Hash of the best finalized block.
	pub finalized_hash: H,
	/// Number of the best finalized block.
	pub finalized_number: N,
	/// Hash of the genesis block.
	pub genesis_hash: H,
}

/// A block lookup key: used for canonical lookup from block number to hash
pub type NumberIndexKey = [u8; 4];

/// Database type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DatabaseType {
	/// Full node database.
	Full,
	/// Light node database.
	Light,
}

/// Convert block number into short lookup key (LE representation) for
/// blocks that are in the canonical chain.
///
/// In the current database schema, this kind of key is only used for
/// lookups into an index, NOT for storing header data or others.
pub fn number_index_key<N: TryInto<u32>>(n: N) -> sp_blockchain::Result<NumberIndexKey> {
	let n = n.try_into().map_err(|_|
		sp_blockchain::Error::Backend("Block number cannot be converted to u32".into())
	)?;

	Ok([
		(n >> 24) as u8,
		((n >> 16) & 0xff) as u8,
		((n >> 8) & 0xff) as u8,
		(n & 0xff) as u8
	])
}

/// Convert number and hash into long lookup key for blocks that are
/// not in the canonical chain.
pub fn number_and_hash_to_lookup_key<N, H>(
	number: N,
	hash: H,
) -> sp_blockchain::Result<Vec<u8>>	where
	N: TryInto<u32>,
	H: AsRef<[u8]>,
{
	let mut lookup_key = number_index_key(number)?.to_vec();
	lookup_key.extend_from_slice(hash.as_ref());
	Ok(lookup_key)
}

/// Convert block lookup key into block number.
/// all block lookup keys start with the block number.
pub fn lookup_key_to_number<N>(key: &[u8]) -> sp_blockchain::Result<N> where
	N: From<u32>
{
	if key.len() < 4 {
		return Err(sp_blockchain::Error::Backend("Invalid block key".into()));
	}
	Ok((key[0] as u32) << 24
		| (key[1] as u32) << 16
		| (key[2] as u32) << 8
		| (key[3] as u32)).map(Into::into)
}

/// Delete number to hash mapping in DB transaction.
pub fn remove_number_to_key_mapping<N: TryInto<u32>>(
	transaction: &mut Transaction<DbHash>,
	key_lookup_col: u32,
	number: N,
) -> sp_blockchain::Result<()> {
	transaction.remove(key_lookup_col, number_index_key(number)?.as_ref());
	Ok(())
}

/// Remove key mappings.
pub fn remove_key_mappings<N: TryInto<u32>, H: AsRef<[u8]>>(
	transaction: &mut Transaction<DbHash>,
	key_lookup_col: u32,
	number: N,
	hash: H,
) -> sp_blockchain::Result<()> {
	remove_number_to_key_mapping(transaction, key_lookup_col, number)?;
	transaction.remove(key_lookup_col, hash.as_ref());
	Ok(())
}

/// Place a number mapping into the database. This maps number to current perceived
/// block hash at that position.
pub fn insert_number_to_key_mapping<N: TryInto<u32> + Clone, H: AsRef<[u8]>>(
	transaction: &mut Transaction<DbHash>,
	key_lookup_col: u32,
	number: N,
	hash: H,
) -> sp_blockchain::Result<()> {
	transaction.set_from_vec(
		key_lookup_col,
		number_index_key(number.clone())?.as_ref(),
		number_and_hash_to_lookup_key(number, hash)?,
	);
	Ok(())
}

/// Insert a hash to key mapping in the database.
pub fn insert_hash_to_key_mapping<N: TryInto<u32>, H: AsRef<[u8]> + Clone>(
	transaction: &mut Transaction<DbHash>,
	key_lookup_col: u32,
	number: N,
	hash: H,
) -> sp_blockchain::Result<()> {
	transaction.set_from_vec(
		key_lookup_col,
		hash.clone().as_ref(),
		number_and_hash_to_lookup_key(number, hash)?,
	);
	Ok(())
}

/// Convert block id to block lookup key.
/// block lookup key is the DB-key header, block and justification are stored under.
/// looks up lookup key by hash from DB as necessary.
pub fn block_id_to_lookup_key<Block>(
	db: &dyn Database<DbHash>,
	key_lookup_col: u32,
	id: BlockId<Block>
) -> Result<Option<Vec<u8>>, sp_blockchain::Error> where
	Block: BlockT,
	::sp_runtime::traits::NumberFor<Block>: UniqueSaturatedFrom<u64> + UniqueSaturatedInto<u64>,
{
	Ok(match id {
		BlockId::Number(n) => db.get(
			key_lookup_col,
			number_index_key(n)?.as_ref(),
		),
		BlockId::Hash(h) => db.get(key_lookup_col, h.as_ref())
	})
}

/// Opens the configured database.
pub fn open_database<Block: BlockT>(
	config: &DatabaseSettings,
	db_type: DatabaseType,
) -> sp_blockchain::Result<Arc<dyn Database<DbHash>>> {
	let db_open_error = |feat| Err(
		sp_blockchain::Error::Backend(
			format!("`{}` feature not enabled, database can not be opened", feat),
		),
	);

	let db: Arc<dyn Database<DbHash>> = match &config.source {
		#[cfg(any(feature = "kvdb-rocksdb", test))]
		DatabaseSettingsSrc::RocksDb { path, cache_size } => {
			// first upgrade database to required version
			crate::upgrade::upgrade_db::<Block>(&path, db_type)?;

			// and now open database assuming that it has the latest version
			let mut db_config = kvdb_rocksdb::DatabaseConfig::with_columns(NUM_COLUMNS);
			let state_col_budget = (*cache_size as f64 * 0.9) as usize;
			let other_col_budget = (cache_size - state_col_budget) / (NUM_COLUMNS as usize - 1);
			let mut memory_budget = std::collections::HashMap::new();
			let path = path.to_str()
				.ok_or_else(|| sp_blockchain::Error::Backend("Invalid database path".into()))?;

			for i in 0..NUM_COLUMNS {
				if i == crate::columns::STATE {
					memory_budget.insert(i, state_col_budget);
				} else {
					memory_budget.insert(i, other_col_budget);
				}
			}

			db_config.memory_budget = memory_budget;

			log::trace!(
				target: "db",
				"Open RocksDB database at {}, state column budget: {} MiB, others({}) column cache: {} MiB",
				path,
				state_col_budget,
				NUM_COLUMNS,
				other_col_budget,
			);

			let db = kvdb_rocksdb::Database::open(&db_config, &path)
				.map_err(|err| sp_blockchain::Error::Backend(format!("{}", err)))?;
			sp_database::as_database(db)
		},
		#[cfg(not(any(feature = "kvdb-rocksdb", test)))]
		DatabaseSettingsSrc::RocksDb { .. } => {
			return db_open_error("kvdb-rocksdb");
		},
		#[cfg(feature = "subdb")]
		DatabaseSettingsSrc::SubDb { path } => {
			crate::subdb::open(&path, NUM_COLUMNS)
				.map_err(|e| sp_blockchain::Error::Backend(format!("{:?}", e)))?
		},
		#[cfg(not(feature = "subdb"))]
		DatabaseSettingsSrc::SubDb { .. } => {
			return db_open_error("subdb");
		},
		#[cfg(feature = "parity-db")]
		DatabaseSettingsSrc::ParityDb { path } => {
			crate::parity_db::open(&path)
				.map_err(|e| sp_blockchain::Error::Backend(format!("{:?}", e)))?
		},
		#[cfg(not(feature = "parity-db"))]
		DatabaseSettingsSrc::ParityDb { .. } => {
			return db_open_error("parity-db");
		},
		DatabaseSettingsSrc::Custom(db) => db.clone(),
	};

	check_database_type(&*db, db_type)?;

	Ok(db)
}

/// Check database type.
pub fn check_database_type(db: &dyn Database<DbHash>, db_type: DatabaseType) -> sp_blockchain::Result<()> {
	match db.get(COLUMN_META, meta_keys::TYPE) {
		Some(stored_type) => {
			if db_type.as_str().as_bytes() != &*stored_type {
				return Err(sp_blockchain::Error::Backend(
					format!("Unexpected database type. Expected: {}", db_type.as_str())).into());
			}
		},
		None => {
			let mut transaction = Transaction::new();
			transaction.set(COLUMN_META, meta_keys::TYPE, db_type.as_str().as_bytes());
			db.commit(transaction)
		},
	}

	Ok(())
}

/// Read database column entry for the given block.
pub fn read_db<Block>(
	db: &dyn Database<DbHash>,
	col_index: u32,
	col: u32,
	id: BlockId<Block>
) -> sp_blockchain::Result<Option<DBValue>>
	where
		Block: BlockT,
{
	block_id_to_lookup_key(db, col_index, id).and_then(|key| match key {
		Some(key) => Ok(db.get(col, key.as_ref())),
		None => Ok(None),
	})
}

/// Read a header from the database.
pub fn read_header<Block: BlockT>(
	db: &dyn Database<DbHash>,
	col_index: u32,
	col: u32,
	id: BlockId<Block>,
) -> sp_blockchain::Result<Option<Block::Header>> {
	match read_db(db, col_index, col, id)? {
		Some(header) => match Block::Header::decode(&mut &header[..]) {
			Ok(header) => Ok(Some(header)),
			Err(_) => return Err(
				sp_blockchain::Error::Backend("Error decoding header".into())
			),
		}
		None => Ok(None),
	}
}

/// Required header from the database.
pub fn require_header<Block: BlockT>(
	db: &dyn Database<DbHash>,
	col_index: u32,
	col: u32,
	id: BlockId<Block>,
) -> sp_blockchain::Result<Block::Header> {
	read_header(db, col_index, col, id)
		.and_then(|header| header.ok_or_else(||
			sp_blockchain::Error::UnknownBlock(format!("Require header: {}", id))
		))
}

/// Read meta from the database.
pub fn read_meta<Block>(db: &dyn Database<DbHash>, col_header: u32) -> Result<
	Meta<<<Block as BlockT>::Header as HeaderT>::Number, Block::Hash>,
	sp_blockchain::Error,
>
	where
		Block: BlockT,
{
	let genesis_hash: Block::Hash = match read_genesis_hash(db)? {
		Some(genesis_hash) => genesis_hash,
		None => return Ok(Meta {
			best_hash: Default::default(),
			best_number: Zero::zero(),
			finalized_hash: Default::default(),
			finalized_number: Zero::zero(),
			genesis_hash: Default::default(),
		}),
	};

	let load_meta_block = |desc, key| -> Result<_, sp_blockchain::Error> {
		if let Some(Some(header)) = match db.get(COLUMN_META, key) {
				Some(id) => db.get(col_header, &id).map(|b| Block::Header::decode(&mut &b[..]).ok()),
				None => None,
			}
		{
			let hash = header.hash();
			debug!("DB Opened blockchain db, fetched {} = {:?} ({})", desc, hash, header.number());
			Ok((hash, *header.number()))
		} else {
			Ok((genesis_hash.clone(), Zero::zero()))
		}
	};

	let (best_hash, best_number) = load_meta_block("best", meta_keys::BEST_BLOCK)?;
	let (finalized_hash, finalized_number) = load_meta_block("final", meta_keys::FINALIZED_BLOCK)?;

	Ok(Meta {
		best_hash,
		best_number,
		finalized_hash,
		finalized_number,
		genesis_hash,
	})
}

/// Read genesis hash from database.
pub fn read_genesis_hash<Hash: Decode>(db: &dyn Database<DbHash>) -> sp_blockchain::Result<Option<Hash>> {
	match db.get(COLUMN_META, meta_keys::GENESIS_HASH) {
		Some(h) => match Decode::decode(&mut &h[..]) {
			Ok(h) => Ok(Some(h)),
			Err(err) => Err(sp_blockchain::Error::Backend(
				format!("Error decoding genesis hash: {}", err)
			)),
		},
		None => Ok(None),
	}
}

impl DatabaseType {
	/// Returns str representation of the type.
	pub fn as_str(&self) -> &'static str {
		match *self {
			DatabaseType::Full => "full",
			DatabaseType::Light => "light",
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_runtime::testing::{Block as RawBlock, ExtrinsicWrapper};
	type Block = RawBlock<ExtrinsicWrapper<u32>>;

	#[test]
	fn number_index_key_doesnt_panic() {
		let id = BlockId::<Block>::Number(72340207214430721);
		match id {
			BlockId::Number(n) => number_index_key(n).expect_err("number should overflow u32"),
			_ => unreachable!(),
		};
	}

	#[test]
	fn database_type_as_str_works() {
		assert_eq!(DatabaseType::Full.as_str(), "full");
		assert_eq!(DatabaseType::Light.as_str(), "light");
	}
}
