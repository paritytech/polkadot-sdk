// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
use crate::{
	client::{SubstrateBlock, SubstrateBlockNumber},
	Address, AddressOrAddresses, BlockInfoProvider, BlockNumberOrTag, BlockTag, Bytes, ClientError,
	FilterTopic, ReceiptExtractor, SubxtBlockInfoProvider,
};
use pallet_revive::evm::{Filter, Log, ReceiptInfo, TransactionSigned};
use sp_core::{H256, U256};
use sqlx::{query, QueryBuilder, Row, Sqlite, SqlitePool};
use std::{
	collections::{BTreeMap, HashMap},
	sync::Arc,
};
use tokio::sync::Mutex;

const LOG_TARGET: &str = "eth-rpc::receipt_provider";

/// ReceiptProvider stores transaction receipts and logs in a SQLite database.
#[derive(Clone)]
pub struct ReceiptProvider<B: BlockInfoProvider = SubxtBlockInfoProvider> {
	/// The database pool.
	pool: SqlitePool,
	/// The block provider used to fetch blocks, and reconstruct receipts.
	block_provider: B,
	/// A means to extract receipts from extrinsics.
	receipt_extractor: ReceiptExtractor,
	/// When `false`, old blocks will be pruned.
	archive_mode: bool,
	/// A Map of the latest block numbers to block hashes (substrate_hash, ethereum_hash).
	/// In Archive mode: This is a cache for recent blocks.
	/// Else: This is the only source of block mappings.
	block_number_to_hash: Arc<Mutex<BTreeMap<SubstrateBlockNumber, (H256, H256)>>>,
	cache_size: usize,
}

/// Provides information about a block,
/// This is an abstratction on top of [`SubstrateBlock`] that can't be mocked in tests.
/// Can be removed once <https://github.com/paritytech/subxt/issues/1883> is fixed.
pub trait BlockInfo {
	/// Returns the block hash.
	fn hash(&self) -> H256;
	/// Returns the block number.
	fn number(&self) -> SubstrateBlockNumber;
}

impl BlockInfo for SubstrateBlock {
	fn hash(&self) -> H256 {
		SubstrateBlock::hash(self)
	}
	fn number(&self) -> SubstrateBlockNumber {
		SubstrateBlock::number(self)
	}
}

impl<B: BlockInfoProvider> ReceiptProvider<B> {
	/// Create a new `ReceiptProvider` with the given database URL and block provider.
	pub async fn new(
		pool: SqlitePool,
		block_provider: B,
		receipt_extractor: ReceiptExtractor,
		archive_mode: bool,
		cache_size: usize,
	) -> Result<Self, sqlx::Error> {
		sqlx::migrate!().run(&pool).await?;
		Ok(Self {
			pool,
			block_provider,
			receipt_extractor,
			archive_mode,
			block_number_to_hash: Default::default(),
			cache_size,
		})
	}

	async fn fetch_row(&self, transaction_hash: &H256) -> Option<(H256, usize)> {
		let transaction_hash = transaction_hash.as_ref();
		let result = query!(
			r#"
			SELECT block_hash, transaction_index
			FROM transaction_hashes
			WHERE transaction_hash = $1
			"#,
			transaction_hash
		)
		.fetch_optional(&self.pool)
		.await
		.ok()??;

		let block_hash = H256::from_slice(&result.block_hash[..]);
		let transaction_index = result.transaction_index.try_into().ok()?;
		Some((block_hash, transaction_index))
	}

	/// Insert a block mapping from Ethereum block hash to Substrate block hash.
	/// This method:
	/// 1. Updates the cache with the new mapping
	/// 2. Detects forks (same block number, different substrate hash)
	/// 3. Detects pruning needs (cache exceeds size limit)
	/// 4. Writes to database
	/// 5. Returns list of substrate and ethereum block hash tuples that should be removed
	async fn insert_block_mapping(
		&self,
		block: &impl BlockInfo,
		ethereum_hash: &H256,
	) -> Result<Vec<(H256, H256)>, ClientError> {
		let substrate_hash = block.hash();
		let block_number = block.number();

		// Step 1: Update cache and detect forks/pruning
		let mut to_remove = Vec::new();
		{
			let mut cache = self.block_number_to_hash.lock().await;

			// Insert into cache and check for fork
			match cache.insert(block_number, (substrate_hash, *ethereum_hash)) {
				Some((old_substrate_hash, old_ethereum_hash))
					if old_substrate_hash != substrate_hash =>
				{
					log::debug!(target: LOG_TARGET, "Fork detected at block #{block_number}: old={old_substrate_hash:?}, new={substrate_hash:?}");
					to_remove.push((old_substrate_hash, old_ethereum_hash));
				},
				_ => {},
			}

			// Check if cache exceeds limit and needs pruning
			while cache.len() > self.cache_size {
				if let Some((_, (old_substrate_hash, old_ethereum_hash))) = cache.pop_first() {
					log::trace!(target: LOG_TARGET, "Pruning block from cache: {old_substrate_hash:?}");
					to_remove.push((old_substrate_hash, old_ethereum_hash));
				}
			}
		}

		// Step 2: Write to database
		let ethereum_hash_ref = ethereum_hash.as_ref();
		let substrate_hash_ref = substrate_hash.as_ref();
		let block_number_i64 = block_number as i64;

		query!(
			r#"
			INSERT OR REPLACE INTO eth_to_substrate_blocks (ethereum_block_hash, substrate_block_hash, block_number)
			VALUES ($1, $2, $3)
			"#,
			ethereum_hash_ref,
			substrate_hash_ref,
			block_number_i64
		)
		.execute(&self.pool)
		.await?;

		log::trace!(target: LOG_TARGET, "Insert block mapping: ethereum block: {ethereum_hash:?} -> substrate block: {substrate_hash:?}");

		// Step 3: Return blocks to remove
		Ok(to_remove)
	}

	/// Get the Substrate block hash for the given Ethereum block hash.
	/// Checks cache first, then falls back to DB in archive mode.
	pub async fn get_substrate_hash(&self, ethereum_block_hash: &H256) -> Option<H256> {
		// Check cache first
		{
			let cache = self.block_number_to_hash.lock().await;
			for (substrate_hash, eth_hash) in cache.values() {
				if eth_hash == ethereum_block_hash {
					log::trace!(target: LOG_TARGET, "Cache hit: ethereum block: {ethereum_block_hash:?} -> substrate block: {substrate_hash:?}");
					return Some(*substrate_hash);
				}
			}
		}

		// Not archive mode - it's been pruned
		if self.archive_mode {
			log::trace!(target: LOG_TARGET, "No block mapping found in cache (Mode 1): {ethereum_block_hash:?}");
			return None;
		}

		// Archive mode: Fall back to database
		let ethereum_hash = ethereum_block_hash.as_ref();
		let result = query!(
			r#"
			SELECT substrate_block_hash
			FROM eth_to_substrate_blocks
			WHERE ethereum_block_hash = $1
			"#,
			ethereum_hash
		)
		.fetch_optional(&self.pool)
		.await
		.inspect_err(|e| {
			log::error!(target: LOG_TARGET, "failed to get block mapping for ethereum block {ethereum_block_hash:?}, err: {e:?}");
		})
		.ok()?
		.or_else(||{
			log::trace!(target: LOG_TARGET, "No block mapping found in DB (Mode 2): {ethereum_block_hash:?}");
			None
		})?;

		let substrate_hash = H256::from_slice(&result.substrate_block_hash[..]);
		log::trace!(target: LOG_TARGET, "DB hit: ethereum block: {ethereum_block_hash:?} -> substrate block: {substrate_hash:?}");
		Some(substrate_hash)
	}

	/// Get the Ethereum block hash for the given Substrate block hash.
	/// Checks cache first, then falls back to DB in archive mode.
	pub async fn get_ethereum_hash(&self, substrate_block_hash: &H256) -> Option<H256> {
		// Check cache first
		{
			let cache = self.block_number_to_hash.lock().await;
			for (sub_hash, eth_hash) in cache.values() {
				if sub_hash == substrate_block_hash {
					log::trace!(target: LOG_TARGET, "Cache hit: substrate block: {substrate_block_hash:?} -> ethereum block: {eth_hash:?}");
					return Some(*eth_hash);
				}
			}
		}

		// Not archive mode - it's been pruned
		if !self.archive_mode {
			log::trace!(target: LOG_TARGET, "No block mapping found in cache (Mode 1): {substrate_block_hash:?}");
			return None;
		}

		// Archive mode: Fall back to database
		let substrate_hash = substrate_block_hash.as_ref();
		let result = query!(
			r#"
			SELECT ethereum_block_hash
			FROM eth_to_substrate_blocks
			WHERE substrate_block_hash = $1
			"#,
			substrate_hash
		)
		.fetch_optional(&self.pool)
		.await
		.inspect_err(|e| {
			log::error!(target: LOG_TARGET, "failed to get block mapping for substrate block {substrate_block_hash:?}, err: {e:?}");
		})
		.ok()?
		.or_else(||{
			log::trace!(target: LOG_TARGET, "No block mapping found in DB (Mode 2): {substrate_block_hash:?}");
			None
		})?;

		let ethereum_hash = H256::from_slice(&result.ethereum_block_hash[..]);
		log::trace!(target: LOG_TARGET, "DB hit: substrate block: {substrate_block_hash:?} -> ethereum block: {ethereum_hash:?}");
		Some(ethereum_hash)
	}

	/// Remove block mappings for the given list of substrate and ethereum hash tuples.
	pub async fn remove_block_mappings(
		&self,
		block_hashes: &[(H256, H256)],
	) -> Result<(), ClientError> {
		log::trace!(target: LOG_TARGET, "Removing block mappings: {block_hashes:?}");
		if block_hashes.is_empty() {
			return Ok(());
		}

		let mut cache = self.block_number_to_hash.lock().await;
		for (substrate_hash, _) in block_hashes {
			if let Some((&key, _)) =
				cache.iter().find(|(_, (sub_hash, _))| sub_hash == substrate_hash)
			{
				cache.remove(&key);
			}
		}
		// Archive mode: Keep eth_to_substrate_blocks intact (persistent storage)
		// Else: Also delete from eth_to_substrate_blocks
		if !self.archive_mode {
			let placeholders = vec!["?"; block_hashes.len()].join(", ");
			let sql_mappings = format!(
				"DELETE FROM eth_to_substrate_blocks WHERE substrate_block_hash in ({placeholders})"
			);
			let mut delete_mappings_query = sqlx::query(&sql_mappings);
			for (substrate_hash, _) in block_hashes {
				delete_mappings_query = delete_mappings_query.bind(substrate_hash.as_ref());
			}
			delete_mappings_query.execute(&self.pool).await?;
		}

		Ok(())
	}

	/// Deletes older records from the database.
	/// Archive mode: Removes from transaction_hashes and logs only.
	/// Else: Removes from all tables.
	pub async fn remove(&self, block_hashes: &[(H256, H256)]) -> Result<(), ClientError> {
		if block_hashes.is_empty() {
			return Ok(());
		}
		log::debug!(target: LOG_TARGET, "Removing block hashes (substrate): {block_hashes:?}");

		// Delete from transaction_hashes (uses substrate hash)
		let placeholders = vec!["?"; block_hashes.len()].join(", ");
		let sql_tx = format!("DELETE FROM transaction_hashes WHERE block_hash in ({placeholders})");
		let mut delete_tx_query = sqlx::query(&sql_tx);
		for (substrate_hash, _) in block_hashes {
			delete_tx_query = delete_tx_query.bind(substrate_hash.as_ref());
		}
		delete_tx_query.execute(&self.pool).await?;

		// Delete from logs (uses ethereum hash)
		let sql_logs = format!("DELETE FROM logs WHERE block_hash in ({placeholders})");
		let mut delete_logs_query = sqlx::query(&sql_logs);
		for (_, ethereum_hash) in block_hashes {
			delete_logs_query = delete_logs_query.bind(ethereum_hash.as_ref());
		}
		delete_logs_query.execute(&self.pool).await?;

		self.remove_block_mappings(block_hashes).await?;

		Ok(())
	}

	/// Check if the block is before the earliest block.
	pub fn is_before_earliest_block(&self, at: &BlockNumberOrTag) -> bool {
		match at {
			BlockNumberOrTag::U256(block_number) =>
				self.receipt_extractor.is_before_earliest_block(block_number.as_u32()),
			BlockNumberOrTag::BlockTag(_) => false,
		}
	}

	/// Fetch receipts from the given block.
	pub async fn receipts_from_block(
		&self,
		block: &SubstrateBlock,
	) -> Result<Vec<(TransactionSigned, ReceiptInfo)>, ClientError> {
		self.receipt_extractor.extract_from_block(block).await
	}

	/// Extract and insert receipts from the given block.
	pub async fn insert_block_receipts(
		&self,
		block: &SubstrateBlock,
		ethereum_hash: &H256,
	) -> Result<Vec<(TransactionSigned, ReceiptInfo)>, ClientError> {
		let receipts = self.receipts_from_block(block).await?;
		self.insert(block, &receipts, ethereum_hash).await?;
		Ok(receipts)
	}

	/// Insert receipts into the provider.
	///
	/// Note: Can be merged into `insert_block_receipts` once <https://github.com/paritytech/subxt/issues/1883> is fixed and subxt let
	/// us create Mock `SubstrateBlock`
	async fn insert(
		&self,
		block: &impl BlockInfo,
		receipts: &[(TransactionSigned, ReceiptInfo)],
		ethereum_hash: &H256,
	) -> Result<(), ClientError> {
		let block_hash = block.hash();
		let block_hash_ref = block_hash.as_ref();
		let ethereum_hash_ref = ethereum_hash.as_ref();
		let block_number = block.number() as i64;

		log::trace!(target: LOG_TARGET, "Insert receipts for substrate block #{block_number} {:?}", block_hash);

		// Step 1: Insert block mapping (updates cache, detects forks/pruning, writes to DB)
		let blocks_to_remove = self.insert_block_mapping(block, ethereum_hash).await?;

		// Step 2: Remove forked/pruned blocks (cleans up transaction_hashes, logs, and optionally
		// eth_to_substrate_blocks)
		if !blocks_to_remove.is_empty() {
			log::trace!(target: LOG_TARGET, "Removing forked/pruned blocks: {blocks_to_remove:?}");
			self.remove(&blocks_to_remove).await?;
		}

		// Step 3: Check if receipts already exist for this block
		let result = sqlx::query!(
			r#"SELECT EXISTS(SELECT 1 FROM transaction_hashes WHERE block_hash = $1) AS "exists!: bool""#,
			block_hash_ref
		)
		.fetch_one(&self.pool)
		.await?;

		// Step 4: Insert receipts and logs for the new block
		if !result.exists {
			for (_, receipt) in receipts {
				let transaction_hash: &[u8] = receipt.transaction_hash.as_ref();
				let transaction_index = receipt.transaction_index.as_u32() as i32;

				query!(
					r#"
					INSERT OR REPLACE INTO transaction_hashes (transaction_hash, block_hash, transaction_index)
					VALUES ($1, $2, $3)
					"#,
					transaction_hash,
					block_hash_ref,
					transaction_index
				)
				.execute(&self.pool)
				.await?;

				for log in &receipt.logs {
					let log_index = log.log_index.as_u32() as i32;
					let address: &[u8] = log.address.as_ref();

					let topic_0 = log.topics.first().as_ref().map(|v| &v[..]);
					let topic_1 = log.topics.get(1).as_ref().map(|v| &v[..]);
					let topic_2 = log.topics.get(2).as_ref().map(|v| &v[..]);
					let topic_3 = log.topics.get(3).as_ref().map(|v| &v[..]);
					let data = log.data.as_ref().map(|v| &v.0[..]);

					query!(
						r#"
						INSERT OR REPLACE INTO logs(
							block_hash,
							transaction_index,
							log_index,
							address,
							block_number,
							transaction_hash,
							topic_0, topic_1, topic_2, topic_3,
							data)
						VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
						"#,
						ethereum_hash_ref,
						transaction_index,
						log_index,
						address,
						block_number,
						transaction_hash,
						topic_0,
						topic_1,
						topic_2,
						topic_3,
						data
					)
					.execute(&self.pool)
					.await?;
				}
			}
		}

		Ok(())
	}

	/// Get logs that match the given filter.
	pub async fn logs(&self, filter: Option<Filter>) -> anyhow::Result<Vec<Log>> {
		let mut qb = QueryBuilder::<Sqlite>::new("SELECT logs.* FROM logs WHERE 1=1");
		let filter = filter.unwrap_or_default();

		let latest_block = U256::from(self.block_provider.latest_block_number().await);

		let as_block_number = |block_param| match block_param {
			None => Ok(None),
			Some(BlockNumberOrTag::U256(v)) => Ok(Some(v)),
			Some(BlockNumberOrTag::BlockTag(BlockTag::Latest)) => Ok(Some(latest_block)),
			Some(BlockNumberOrTag::BlockTag(tag)) => anyhow::bail!("Unsupported tag: {tag:?}"),
		};

		let from_block = as_block_number(filter.from_block)?;
		let to_block = as_block_number(filter.to_block)?;

		match (from_block, to_block, filter.block_hash) {
			(Some(_), _, Some(_)) | (_, Some(_), Some(_)) => {
				anyhow::bail!("block number and block hash cannot be used together");
			},

			(Some(block), _, _) | (_, Some(block), _) if block > latest_block => {
				anyhow::bail!("block number exceeds latest block");
			},
			(Some(from_block), Some(to_block), None) if from_block > to_block => {
				anyhow::bail!("invalid block range params");
			},
			(Some(from_block), Some(to_block), None) if from_block == to_block => {
				qb.push(" AND block_number = ").push_bind(from_block.as_u64() as i64);
			},
			(Some(from_block), Some(to_block), None) => {
				qb.push(" AND block_number BETWEEN ")
					.push_bind(from_block.as_u64() as i64)
					.push(" AND ")
					.push_bind(to_block.as_u64() as i64);
			},
			(Some(from_block), None, None) => {
				qb.push(" AND block_number >= ").push_bind(from_block.as_u64() as i64);
			},
			(None, Some(to_block), None) => {
				qb.push(" AND block_number <= ").push_bind(to_block.as_u64() as i64);
			},
			(None, None, Some(hash)) => {
				qb.push(" AND block_hash = ").push_bind(hash.0.to_vec());
			},
			(None, None, None) => {
				qb.push(" AND block_number = ").push_bind(latest_block.as_u64() as i64);
			},
		}

		if let Some(addresses) = filter.address {
			match addresses {
				AddressOrAddresses::Address(addr) => {
					qb.push(" AND address = ").push_bind(addr.0.to_vec());
				},
				AddressOrAddresses::Addresses(addrs) => {
					qb.push(" AND address IN (");
					let mut separated = qb.separated(", ");
					for addr in addrs {
						separated.push_bind(addr.0.to_vec());
					}
					separated.push_unseparated(")");
				},
			}
		}

		if let Some(topics) = filter.topics {
			if topics.len() > 4 {
				return Err(anyhow::anyhow!("exceed max topics"));
			}

			for (i, topic) in topics.into_iter().enumerate() {
				match topic {
					FilterTopic::Single(hash) => {
						qb.push(format_args!(" AND topic_{i} = ")).push_bind(hash.0.to_vec());
					},
					FilterTopic::Multiple(hashes) => {
						qb.push(format_args!(" AND topic_{i} IN ("));
						let mut separated = qb.separated(", ");
						for hash in hashes {
							separated.push_bind(hash.0.to_vec());
						}
						separated.push_unseparated(")");
					},
				}
			}
		}

		qb.push(" LIMIT 10000");

		let logs = qb
			.build()
			.try_map(|row| {
				let block_hash: Vec<u8> = row.try_get("block_hash")?;
				let transaction_index: i64 = row.try_get("transaction_index")?;
				let log_index: i64 = row.try_get("log_index")?;
				let address: Vec<u8> = row.try_get("address")?;
				let block_number: i64 = row.try_get("block_number")?;
				let transaction_hash: Vec<u8> = row.try_get("transaction_hash")?;
				let topic_0: Option<Vec<u8>> = row.try_get("topic_0")?;
				let topic_1: Option<Vec<u8>> = row.try_get("topic_1")?;
				let topic_2: Option<Vec<u8>> = row.try_get("topic_2")?;
				let topic_3: Option<Vec<u8>> = row.try_get("topic_3")?;
				let data: Option<Vec<u8>> = row.try_get("data")?;

				let topics = [topic_0, topic_1, topic_2, topic_3]
					.iter()
					.filter_map(|t| t.as_ref().map(|t| H256::from_slice(t)))
					.collect::<Vec<_>>();

				Ok(Log {
					address: Address::from_slice(&address),
					block_hash: H256::from_slice(&block_hash),
					block_number: U256::from(block_number as u64),
					data: data.map(Bytes::from),
					log_index: U256::from(log_index as u64),
					topics,
					transaction_hash: H256::from_slice(&transaction_hash),
					transaction_index: U256::from(transaction_index as u64),
					removed: false,
				})
			})
			.fetch_all(&self.pool)
			.await?;

		Ok(logs)
	}

	/// Get the number of receipts per block.
	pub async fn receipts_count_per_block(&self, block_hash: &H256) -> Option<usize> {
		let block_hash = block_hash.as_ref();
		let row = query!(
			r#"
            SELECT COUNT(*) as count
            FROM transaction_hashes
            WHERE block_hash = $1
            "#,
			block_hash
		)
		.fetch_one(&self.pool)
		.await
		.ok()?;

		let count = row.count as usize;
		Some(count)
	}

	/// Return all transaction hashes for the given block hash.
	pub async fn block_transaction_hashes(
		&self,
		block_hash: &H256,
	) -> Option<HashMap<usize, H256>> {
		let block_hash = block_hash.as_ref();
		let rows = query!(
			r#"
		      SELECT transaction_index, transaction_hash
		      FROM transaction_hashes
		      WHERE block_hash = $1
		      "#,
			block_hash
		)
		.map(|row| {
			let transaction_index = row.transaction_index as usize;
			let transaction_hash = H256::from_slice(&row.transaction_hash);
			(transaction_index, transaction_hash)
		})
		.fetch_all(&self.pool)
		.await
		.ok()?;

		Some(rows.into_iter().collect())
	}

	/// Get the receipt for the given block hash and transaction index.
	pub async fn receipt_by_block_hash_and_index(
		&self,
		block_hash: &H256,
		transaction_index: usize,
	) -> Option<ReceiptInfo> {
		let block = self.block_provider.block_by_hash(block_hash).await.ok()??;
		let (_, receipt) = self
			.receipt_extractor
			.extract_from_transaction(&block, transaction_index)
			.await
			.ok()?;
		Some(receipt)
	}

	/// Get the receipt for the given transaction hash.
	pub async fn receipt_by_hash(&self, transaction_hash: &H256) -> Option<ReceiptInfo> {
		let (block_hash, transaction_index) = self.fetch_row(transaction_hash).await?;

		let block = self.block_provider.block_by_hash(&block_hash).await.ok()??;
		let (_, receipt) = self
			.receipt_extractor
			.extract_from_transaction(&block, transaction_index)
			.await
			.ok()?;
		Some(receipt)
	}

	/// Get the signed transaction for the given transaction hash.
	pub async fn signed_tx_by_hash(&self, transaction_hash: &H256) -> Option<TransactionSigned> {
		let transaction_hash = transaction_hash.as_ref();
		let result = query!(
			r#"
			SELECT block_hash, transaction_index
			FROM transaction_hashes
			WHERE transaction_hash = $1
			"#,
			transaction_hash
		)
		.fetch_optional(&self.pool)
		.await
		.ok()??;

		let block_hash = H256::from_slice(&result.block_hash[..]);
		let transaction_index = result.transaction_index.try_into().ok()?;

		let block = self.block_provider.block_by_hash(&block_hash).await.ok()??;
		let (signed_tx, _) = self
			.receipt_extractor
			.extract_from_transaction(&block, transaction_index)
			.await
			.ok()?;
		Some(signed_tx)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test::{MockBlockInfo, MockBlockInfoProvider};
	use pallet_revive::evm::{ReceiptInfo, TransactionSigned};
	use pretty_assertions::assert_eq;
	use sp_core::{H160, H256};
	use sqlx::SqlitePool;

	async fn count(pool: &SqlitePool, table: &str, block_hash: Option<H256>) -> usize {
		let count: i64 = match block_hash {
			None =>
				sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
					.fetch_one(pool)
					.await,
			Some(hash) =>
				sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE block_hash = ?"))
					.bind(hash.as_ref())
					.fetch_one(pool)
					.await,
		}
		.unwrap();

		count as _
	}

	async fn setup_sqlite_provider(pool: SqlitePool) -> ReceiptProvider<MockBlockInfoProvider> {
		ReceiptProvider {
			pool,
			block_provider: MockBlockInfoProvider {},
			receipt_extractor: ReceiptExtractor::new_mock(),
			archive_mode: false,
			block_number_to_hash: Default::default(),
			cache_size: 10,
		}
	}

	#[sqlx::test]
	async fn test_insert_remove(pool: SqlitePool) -> anyhow::Result<()> {
		let provider = setup_sqlite_provider(pool).await;
		let block = MockBlockInfo { hash: H256::default(), number: 0 };
		let receipts = vec![(
			TransactionSigned::default(),
			ReceiptInfo {
				logs: vec![Log { block_hash: block.hash, ..Default::default() }],
				..Default::default()
			},
		)];
		let ethereum_hash = H256::from([1_u8; 32]);

		provider.insert(&block, &receipts, &ethereum_hash).await?;
		let row = provider.fetch_row(&receipts[0].1.transaction_hash).await;
		assert_eq!(row, Some((block.hash, 0)));

		provider.remove(&[(block.hash(), ethereum_hash)]).await?;
		assert_eq!(count(&provider.pool, "transaction_hashes", Some(block.hash())).await, 0);
		assert_eq!(count(&provider.pool, "logs", Some(block.hash())).await, 0);
		Ok(())
	}

	#[sqlx::test]
	async fn test_prune(pool: SqlitePool) -> anyhow::Result<()> {
		let provider = setup_sqlite_provider(pool).await;
		let n = provider.cache_size;

		for i in 0..2 * n {
			let block = MockBlockInfo { hash: H256::from([i as u8; 32]), number: i as _ };
			let transaction_hash = H256::from([i as u8; 32]);
			let receipts = vec![(
				TransactionSigned::default(),
				ReceiptInfo {
					transaction_hash,
					logs: vec![Log {
						block_hash: block.hash,
						transaction_hash,
						..Default::default()
					}],
					..Default::default()
				},
			)];
			let ethereum_hash = H256::from([(i + 1) as u8; 32]);
			provider.insert(&block, &receipts, &ethereum_hash).await?;
		}
		assert_eq!(count(&provider.pool, "transaction_hashes", None).await, n);
		assert_eq!(count(&provider.pool, "logs", None).await, n);
		assert_eq!(count(&provider.pool, "eth_to_substrate_blocks", None).await, n);
		assert_eq!(provider.block_number_to_hash.lock().await.len(), n);

		return Ok(());
	}

	#[sqlx::test]
	async fn test_fork(pool: SqlitePool) -> anyhow::Result<()> {
		let provider = setup_sqlite_provider(pool).await;

		for i in [1u8, 2u8] {
			let block = MockBlockInfo { hash: H256::from([i; 32]), number: 1 };
			let transaction_hash = H256::from([i; 32]);
			let receipts = vec![(
				TransactionSigned::default(),
				ReceiptInfo {
					transaction_hash,
					logs: vec![Log {
						block_hash: block.hash,
						transaction_hash,
						..Default::default()
					}],
					..Default::default()
				},
			)];
			let ethereum_hash = H256::from([i + 1; 32]);
			provider.insert(&block, &receipts, &ethereum_hash).await?;
		}
		assert_eq!(count(&provider.pool, "transaction_hashes", None).await, 1);
		assert_eq!(count(&provider.pool, "logs", None).await, 1);
		assert_eq!(count(&provider.pool, "eth_to_substrate_blocks", None).await, 1);
		assert_eq!(
			provider.block_number_to_hash.lock().await.clone(),
			[(1, (H256::from([2u8; 32]), H256::from([3u8; 32])))].into(),
			"New receipt for block #1 should replace the old one"
		);

		return Ok(());
	}

	#[sqlx::test]
	async fn test_receipts_count_per_block(pool: SqlitePool) -> anyhow::Result<()> {
		let provider = setup_sqlite_provider(pool).await;
		let block = MockBlockInfo { hash: H256::default(), number: 0 };
		let receipts = vec![
			(
				TransactionSigned::default(),
				ReceiptInfo { transaction_hash: H256::from([0u8; 32]), ..Default::default() },
			),
			(
				TransactionSigned::default(),
				ReceiptInfo { transaction_hash: H256::from([1u8; 32]), ..Default::default() },
			),
		];
		let ethereum_hash = H256::from([2u8; 32]);

		provider.insert(&block, &receipts, &ethereum_hash).await?;
		let count = provider.receipts_count_per_block(&block.hash).await;
		assert_eq!(count, Some(2));
		Ok(())
	}

	#[sqlx::test]
	async fn test_query_logs(pool: SqlitePool) -> anyhow::Result<()> {
		let provider = setup_sqlite_provider(pool).await;
		let block1 = MockBlockInfo { hash: H256::from([1u8; 32]), number: 1 };
		let block2 = MockBlockInfo { hash: H256::from([2u8; 32]), number: 2 };
		let ethereum_hash1 = H256::from([3u8; 32]);
		let ethereum_hash2 = H256::from([4u8; 32]);
		let log1 = Log {
			block_hash: ethereum_hash1,
			block_number: block1.number.into(),
			address: H160::from([1u8; 20]),
			topics: vec![H256::from([1u8; 32]), H256::from([2u8; 32])],
			data: Some(vec![0u8; 32].into()),
			transaction_hash: H256::default(),
			transaction_index: U256::from(1),
			log_index: U256::from(1),
			..Default::default()
		};
		let log2 = Log {
			block_hash: ethereum_hash2,
			block_number: block2.number.into(),
			address: H160::from([2u8; 20]),
			topics: vec![H256::from([2u8; 32]), H256::from([3u8; 32])],
			transaction_hash: H256::from([1u8; 32]),
			transaction_index: U256::from(2),
			log_index: U256::from(1),
			..Default::default()
		};

		provider
			.insert(
				&block1,
				&vec![(
					TransactionSigned::default(),
					ReceiptInfo {
						logs: vec![log1.clone()],
						transaction_hash: log1.transaction_hash,
						transaction_index: log1.transaction_index,
						..Default::default()
					},
				)],
				&ethereum_hash1,
			)
			.await?;
		provider
			.insert(
				&block2,
				&vec![(
					TransactionSigned::default(),
					ReceiptInfo {
						logs: vec![log2.clone()],
						transaction_hash: log2.transaction_hash,
						transaction_index: log2.transaction_index,
						..Default::default()
					},
				)],
				&ethereum_hash2,
			)
			.await?;

		// Empty filter
		let logs = provider.logs(None).await?;
		assert_eq!(logs, vec![log2.clone()]);

		// from_block filter
		let logs = provider
			.logs(Some(Filter { from_block: Some(log2.block_number.into()), ..Default::default() }))
			.await?;
		assert_eq!(logs, vec![log2.clone()]);

		// from_block filter (using latest block)
		let logs = provider
			.logs(Some(Filter { from_block: Some(BlockTag::Latest.into()), ..Default::default() }))
			.await?;
		assert_eq!(logs, vec![log2.clone()]);

		// to_block filter
		let logs = provider
			.logs(Some(Filter { to_block: Some(log1.block_number.into()), ..Default::default() }))
			.await?;
		assert_eq!(logs, vec![log1.clone()]);

		// block_hash filter
		let logs = provider
			.logs(Some(Filter { block_hash: Some(log1.block_hash), ..Default::default() }))
			.await?;
		assert_eq!(logs, vec![log1.clone()]);

		// single address
		let logs = provider
			.logs(Some(Filter {
				from_block: Some(U256::from(0).into()),
				address: Some(log1.address.into()),
				..Default::default()
			}))
			.await?;
		assert_eq!(logs, vec![log1.clone()]);

		// multiple addresses
		let logs = provider
			.logs(Some(Filter {
				from_block: Some(U256::from(0).into()),
				address: Some(vec![log1.address, log2.address].into()),
				..Default::default()
			}))
			.await?;
		assert_eq!(logs, vec![log1.clone(), log2.clone()]);

		// single topic
		let logs = provider
			.logs(Some(Filter {
				from_block: Some(U256::from(0).into()),
				topics: Some(vec![FilterTopic::Single(log1.topics[0])]),
				..Default::default()
			}))
			.await?;
		assert_eq!(logs, vec![log1.clone()]);

		// multiple topic
		let logs = provider
			.logs(Some(Filter {
				from_block: Some(U256::from(0).into()),
				topics: Some(vec![
					FilterTopic::Single(log1.topics[0]),
					FilterTopic::Single(log1.topics[1]),
				]),
				..Default::default()
			}))
			.await?;
		assert_eq!(logs, vec![log1.clone()]);

		// multiple topic for topic_0
		let logs = provider
			.logs(Some(Filter {
				from_block: Some(U256::from(0).into()),
				topics: Some(vec![FilterTopic::Multiple(vec![log1.topics[0], log2.topics[0]])]),
				..Default::default()
			}))
			.await?;
		assert_eq!(logs, vec![log1.clone(), log2.clone()]);

		// Altogether
		let logs = provider
			.logs(Some(Filter {
				from_block: Some(log1.block_number.into()),
				to_block: Some(log2.block_number.into()),
				block_hash: None,
				address: Some(vec![log1.address, log2.address].into()),
				topics: Some(vec![FilterTopic::Multiple(vec![log1.topics[0], log2.topics[0]])]),
			}))
			.await?;
		assert_eq!(logs, vec![log1.clone(), log2.clone()]);
		Ok(())
	}

	#[sqlx::test]
	async fn test_block_mapping_insert_get(pool: SqlitePool) -> anyhow::Result<()> {
		let provider = setup_sqlite_provider(pool).await;
		let ethereum_hash = H256::from([1u8; 32]);
		let block1 = MockBlockInfo { hash: H256::from([2u8; 32]), number: 42 };

		// Insert mapping
		provider.insert_block_mapping(&block1, &ethereum_hash).await?;

		// Test forward lookup
		let resolved = provider.get_substrate_hash(&ethereum_hash).await;
		assert_eq!(resolved, Some(block1.hash));

		// Test reverse lookup
		let resolved = provider.get_ethereum_hash(&block1.hash).await;
		assert_eq!(resolved, Some(ethereum_hash));

		Ok(())
	}

	#[sqlx::test]
	async fn test_block_mapping_overwrite(pool: SqlitePool) -> anyhow::Result<()> {
		let provider = setup_sqlite_provider(pool).await;
		let block1 = MockBlockInfo { hash: H256::from([2u8; 32]), number: 42 };
		let block2 = MockBlockInfo { hash: H256::from([3u8; 32]), number: 42 };
		let ethereum_hash = H256::from([1u8; 32]);

		// Insert first mapping
		provider.insert_block_mapping(&block1, &ethereum_hash).await?;
		assert_eq!(provider.get_substrate_hash(&ethereum_hash).await, Some(block1.hash));

		// Insert second mapping (should overwrite)
		provider.insert_block_mapping(&block2, &ethereum_hash).await?;
		assert_eq!(provider.get_substrate_hash(&ethereum_hash).await, Some(block2.hash));

		// Old mapping should be gone
		assert_eq!(provider.get_ethereum_hash(&block1.hash).await, None);

		Ok(())
	}

	#[sqlx::test]
	async fn test_block_mapping_remove(pool: SqlitePool) -> anyhow::Result<()> {
		let provider = setup_sqlite_provider(pool).await;
		let ethereum_hash1 = H256::from([1u8; 32]);
		let ethereum_hash2 = H256::from([2u8; 32]);
		let block1 = MockBlockInfo { hash: H256::from([3u8; 32]), number: 42 };
		let block2 = MockBlockInfo { hash: H256::from([4u8; 32]), number: 43 };

		// Insert mappings
		provider.insert_block_mapping(&block1, &ethereum_hash1).await?;
		provider.insert_block_mapping(&block2, &ethereum_hash2).await?;

		// Verify they exist
		assert_eq!(provider.get_substrate_hash(&ethereum_hash1).await, Some(block1.hash));
		assert_eq!(provider.get_substrate_hash(&ethereum_hash2).await, Some(block2.hash));

		// Remove one mapping
		provider.remove_block_mappings(&[(block1.hash, H256::default())]).await?;

		// Verify removal
		assert_eq!(provider.get_substrate_hash(&ethereum_hash1).await, None);
		assert_eq!(provider.get_substrate_hash(&ethereum_hash2).await, Some(block2.hash));

		Ok(())
	}

	#[sqlx::test]
	async fn test_block_mapping_pruning_integration(pool: SqlitePool) -> anyhow::Result<()> {
		let provider = setup_sqlite_provider(pool).await;
		let block = MockBlockInfo { hash: H256::from([1u8; 32]), number: 42 };
		let ethereum_hash = H256::from([2u8; 32]);

		// Insert mapping
		provider.insert_block_mapping(&block, &ethereum_hash).await?;
		assert_eq!(provider.get_substrate_hash(&ethereum_hash).await, Some(block.hash));

		// Remove substrate block (this should also remove the mapping)
		provider.remove(&[(block.hash, ethereum_hash)]).await?;

		// Mapping should be gone
		assert_eq!(provider.get_substrate_hash(&ethereum_hash).await, None);

		Ok(())
	}

	#[sqlx::test]
	async fn test_logs_with_ethereum_block_hash_mapping(pool: SqlitePool) -> anyhow::Result<()> {
		let provider = setup_sqlite_provider(pool).await;
		let ethereum_hash = H256::from([1u8; 32]);
		let substrate_hash = H256::from([2u8; 32]);
		let block_number = 1u64;

		// Create a log - note that logs are now stored with ethereum hash
		let log = Log {
			block_hash: ethereum_hash, // Logs now use ethereum hash
			block_number: block_number.into(),
			address: H160::from([1u8; 20]),
			topics: vec![H256::from([1u8; 32])],
			transaction_hash: H256::from([3u8; 32]),
			transaction_index: U256::from(0),
			log_index: U256::from(0),
			data: Some(vec![0u8; 32].into()),
			..Default::default()
		};

		// Insert the log
		let block = MockBlockInfo { hash: substrate_hash, number: block_number as u32 };
		let receipts = vec![(
			TransactionSigned::default(),
			ReceiptInfo {
				logs: vec![log.clone()],
				transaction_hash: log.transaction_hash,
				transaction_index: log.transaction_index,
				..Default::default()
			},
		)];
		provider.insert(&block, &receipts, &ethereum_hash).await?;

		// Query logs using Ethereum block hash - logs should be returned with ethereum hash
		let logs = provider
			.logs(Some(Filter { block_hash: Some(ethereum_hash), ..Default::default() }))
			.await?;
		assert_eq!(logs, vec![log]);

		Ok(())
	}

	#[sqlx::test]
	async fn test_mapping_count(pool: SqlitePool) -> anyhow::Result<()> {
		let provider = setup_sqlite_provider(pool).await;

		// Initially no mappings
		assert_eq!(count(&provider.pool, "eth_to_substrate_blocks", None).await, 0);

		let block1 = MockBlockInfo { hash: H256::from([1u8; 32]), number: 1 };
		let block2 = MockBlockInfo { hash: H256::from([3u8; 32]), number: 2 };
		let ethereum_hash1 = H256::from([2u8; 32]);
		let ethereum_hash2 = H256::from([4u8; 32]);

		// Insert some mappings
		provider.insert_block_mapping(&block1, &ethereum_hash1).await?;
		provider.insert_block_mapping(&block2, &ethereum_hash2).await?;

		assert_eq!(count(&provider.pool, "eth_to_substrate_blocks", None).await, 2);

		// Remove one
		provider.remove_block_mappings(&[(block1.hash, H256::default())]).await?;
		assert_eq!(count(&provider.pool, "eth_to_substrate_blocks", None).await, 1);

		Ok(())
	}
}
