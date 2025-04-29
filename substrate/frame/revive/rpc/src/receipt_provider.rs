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
	FilterTopic, ReceiptExtractor, SubxtBlockInfoProvider, LOG_TARGET,
};
use pallet_revive::evm::{Filter, Log, ReceiptInfo, TransactionSigned};
use sp_core::{H256, U256};
use sqlx::{query, QueryBuilder, Row, Sqlite, SqlitePool};
use std::{
	collections::{BTreeMap, HashMap},
	sync::Arc,
};
use tokio::sync::Mutex;

/// ReceiptProvider stores transaction receipts and logs in a SQLite database.
#[derive(Clone)]
pub struct ReceiptProvider<B: BlockInfoProvider = SubxtBlockInfoProvider> {
	/// The database pool.
	pool: SqlitePool,
	/// The block provider used to fetch blocks, and reconstruct receipts.
	block_provider: B,
	/// A means to extract receipts from extrinsics.
	receipt_extractor: ReceiptExtractor,
	/// When `Some`, old blocks will be pruned.
	keep_latest_n_blocks: Option<usize>,
	/// A Map of the latest block numbers to block hashes.
	block_number_to_hash: Arc<Mutex<BTreeMap<SubstrateBlockNumber, H256>>>,
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
		database_url: &str,
		block_provider: B,
		receipt_extractor: ReceiptExtractor,
		keep_latest_n_blocks: Option<usize>,
	) -> Result<Self, sqlx::Error> {
		let pool = SqlitePool::connect(database_url).await?;
		sqlx::migrate!().run(&pool).await?;
		Ok(Self {
			pool,
			block_provider,
			receipt_extractor,
			keep_latest_n_blocks,
			block_number_to_hash: Default::default(),
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

	/// Deletes older records from the database.
	pub async fn remove(&self, block_hashes: &[H256]) -> Result<(), ClientError> {
		if block_hashes.is_empty() {
			return Ok(());
		}
		log::debug!(target: LOG_TARGET, "Removing block hashes: {block_hashes:?}");

		let placeholders = vec!["?"; block_hashes.len()].join(", ");
		let sql = format!("DELETE FROM transaction_hashes WHERE block_hash in ({})", placeholders);
		let mut delete_tx_query = sqlx::query(&sql);

		let sql = format!("DELETE FROM logs WHERE block_hash in ({})", placeholders);
		let mut delete_logs_query = sqlx::query(&sql);

		for block_hash in block_hashes {
			delete_tx_query = delete_tx_query.bind(block_hash.as_ref());
			delete_logs_query = delete_logs_query.bind(block_hash.as_ref());
		}

		let delete_transaction_hashes = delete_tx_query.execute(&self.pool);
		let delete_logs = delete_logs_query.execute(&self.pool);
		tokio::try_join!(delete_transaction_hashes, delete_logs)?;
		Ok(())
	}

	/// Fetch receipts from the given block.
	pub async fn receipts_from_block(
		&self,
		block: &SubstrateBlock,
	) -> Result<Vec<(TransactionSigned, ReceiptInfo)>, ClientError> {
		self.receipt_extractor.extract_from_block(block).await
	}

	/// Extract and insert receipts from the given block.
	pub async fn insert_block_receipts(&self, block: &SubstrateBlock) -> Result<(), ClientError> {
		let receipts = self.receipts_from_block(block).await?;
		self.insert(block, &receipts).await
	}

	/// Insert receipts into the provider.
	///
	/// Note: Can be merged into `insert_block_receipts` once https://github.com/paritytech/subxt/issues/1883 is fixed and subxt let
	/// us create Mock `SubstrateBlock`
	async fn insert(
		&self,
		block: &impl BlockInfo,
		receipts: &[(TransactionSigned, ReceiptInfo)],
	) -> Result<(), ClientError> {
		if receipts.is_empty() {
			return Ok(());
		}

		let block_hash = block.hash();
		let block_hash_ref = block_hash.as_ref();
		let block_number = block.number() as i64;

		let result = sqlx::query!(
			r#"SELECT EXISTS(SELECT 1 FROM transaction_hashes WHERE block_hash = $1) AS "exists!: bool""#,
			block_hash_ref
		)
		.fetch_one(&self.pool)
		.await?;

		if result.exists {
			return Ok(());
		}

		// Keep track of the latest block hashes, so we can prune older blocks.
		if let Some(keep_latest_n_blocks) = self.keep_latest_n_blocks {
			let latest = block.number();
			let mut block_number_to_hash = self.block_number_to_hash.lock().await;

			let oldest_block = latest.saturating_sub(keep_latest_n_blocks as _);
			let mut to_remove = block_number_to_hash
				.iter()
				.take_while(|(n, _)| **n <= oldest_block)
				.map(|(_, hash)| *hash)
				.collect::<Vec<_>>();

			block_number_to_hash.retain(|&n, _| n > oldest_block);
			match block_number_to_hash.insert(block.number(), block_hash) {
				Some(old_hash) if old_hash != block_hash => {
					to_remove.push(old_hash);
				},
				_ => {},
			}

			log::trace!(target: LOG_TARGET, "Pruning old blocks: {to_remove:?}");
			self.remove(&to_remove).await?;
		}

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
					block_hash_ref,
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
					removed: None,
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
			keep_latest_n_blocks: Some(10),
			block_number_to_hash: Default::default(),
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

		provider.insert(&block, &receipts).await?;
		let row = provider.fetch_row(&receipts[0].1.transaction_hash).await;
		assert_eq!(row, Some((block.hash, 0)));

		provider.remove(&[block.hash()]).await?;
		assert_eq!(count(&provider.pool, "transaction_hashes", Some(block.hash())).await, 0);
		assert_eq!(count(&provider.pool, "logs", Some(block.hash())).await, 0);
		Ok(())
	}

	#[sqlx::test]
	async fn test_prune(pool: SqlitePool) -> anyhow::Result<()> {
		let provider = setup_sqlite_provider(pool).await;
		let n = provider.keep_latest_n_blocks.unwrap();

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
			provider.insert(&block, &receipts).await?;
		}
		assert_eq!(count(&provider.pool, "transaction_hashes", None).await, n);
		assert_eq!(count(&provider.pool, "logs", None).await, n);
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
			provider.insert(&block, &receipts).await?;
		}
		assert_eq!(count(&provider.pool, "transaction_hashes", None).await, 1);
		assert_eq!(count(&provider.pool, "logs", None).await, 1);
		assert_eq!(
			provider.block_number_to_hash.lock().await.clone(),
			[(1, H256::from([2u8; 32]))].into(),
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

		provider.insert(&block, &receipts).await?;
		let count = provider.receipts_count_per_block(&block.hash).await;
		assert_eq!(count, Some(2));
		Ok(())
	}

	#[sqlx::test]
	async fn test_query_logs(pool: SqlitePool) -> anyhow::Result<()> {
		let provider = setup_sqlite_provider(pool).await;
		let block1 = MockBlockInfo { hash: H256::from([1u8; 32]), number: 1 };
		let block2 = MockBlockInfo { hash: H256::from([2u8; 32]), number: 2 };
		let log1 = Log {
			block_hash: block1.hash,
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
			block_hash: block2.hash,
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
}
