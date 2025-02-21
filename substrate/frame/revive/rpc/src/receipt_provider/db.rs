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

use super::*;
use crate::{
	Address, AddressOrAddresses, BlockInfoProvider, Bytes, FilterTopic, ReceiptExtractor,
	LOG_TARGET,
};
use jsonrpsee::core::async_trait;
use pallet_revive::evm::{Filter, Log, ReceiptInfo, TransactionSigned};
use sp_core::{H256, U256};
use sqlx::{query, QueryBuilder, Row, Sqlite, SqlitePool};
use std::{collections::HashMap, sync::Arc};

/// A `[ReceiptProvider]` that stores receipts in a SQLite database.
#[derive(Clone)]
pub struct DBReceiptProvider {
	/// The database pool.
	pool: SqlitePool,
	/// The block provider used to fetch blocks, and reconstruct receipts.
	block_provider: Arc<dyn BlockInfoProvider>,
	/// A means to extract receipts from extrinsics.
	receipt_extractor: ReceiptExtractor,
	/// Whether to prune old blocks.
	prune_old_blocks: bool,
}

impl DBReceiptProvider {
	/// Create a new `DBReceiptProvider` with the given database URL and block provider.
	pub async fn new(
		database_url: &str,
		block_provider: Arc<dyn BlockInfoProvider>,
		receipt_extractor: ReceiptExtractor,
		prune_old_blocks: bool,
	) -> Result<Self, sqlx::Error> {
		let pool = SqlitePool::connect(database_url).await?;
		sqlx::migrate!().run(&pool).await?;
		Ok(Self { pool, block_provider, receipt_extractor, prune_old_blocks })
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
}

#[async_trait]
impl ReceiptProvider for DBReceiptProvider {
	async fn remove(&self, block_hash: &H256) {
		if !self.prune_old_blocks {
			return;
		}

		let block_hash = block_hash.as_ref();

		let delete_transaction_hashes = query!(
			r#"
        DELETE FROM transaction_hashes
        WHERE block_hash = $1
        "#,
			block_hash
		)
		.execute(&self.pool);

		let delete_logs = query!(
			r#"
        DELETE FROM logs
        WHERE block_hash = $1
        "#,
			block_hash
		)
		.execute(&self.pool);

		let (tx_result, logs_result) = tokio::join!(delete_transaction_hashes, delete_logs);

		if let Err(err) = tx_result {
			log::error!(target: LOG_TARGET, "Error removing transaction hashes for block hash {block_hash:?}: {err:?}");
		}

		if let Err(err) = logs_result {
			log::error!(target: LOG_TARGET, "Error removing logs for block hash {block_hash:?}: {err:?}");
		}
	}

	async fn archive(&self, block_hash: &H256, receipts: &[(TransactionSigned, ReceiptInfo)]) {
		self.insert(block_hash, receipts).await;
	}

	async fn insert(&self, block_hash: &H256, receipts: &[(TransactionSigned, ReceiptInfo)]) {
		let block_hash = block_hash.as_ref();
		for (_, receipt) in receipts {
			let transaction_hash: &[u8] = receipt.transaction_hash.as_ref();
			let transaction_index = receipt.transaction_index.as_u32() as i32;

			let result = query!(
				r#"
				INSERT OR REPLACE INTO transaction_hashes (transaction_hash, block_hash, transaction_index)
				VALUES ($1, $2, $3)
				"#,
				transaction_hash,
				block_hash,
				transaction_index
			)
			.execute(&self.pool)
			.await;

			if let Err(err) = result {
				log::error!("Error inserting transaction for block hash {block_hash:?}: {err:?}");
			}

			for log in &receipt.logs {
				let block_hash = log.block_hash.as_ref();
				let transaction_index = log.transaction_index.as_u64() as i64;
				let log_index = log.log_index.as_u32() as i32;
				let address = log.address.as_ref();
				let block_number = log.block_number.as_u64() as i64;
				let transaction_hash = log.transaction_hash.as_ref();

				let topic_0 = log.topics.first().as_ref().map(|v| &v[..]);
				let topic_1 = log.topics.get(1).as_ref().map(|v| &v[..]);
				let topic_2 = log.topics.get(2).as_ref().map(|v| &v[..]);
				let topic_3 = log.topics.get(3).as_ref().map(|v| &v[..]);
				let data = log.data.as_ref().map(|v| &v.0[..]);

				let result = query!(
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
					block_hash,
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
				.await;

				if let Err(err) = result {
					log::error!("Error inserting log {log:?}: {err:?}");
				}
			}
		}
	}

	async fn logs(&self, filter: Option<Filter>) -> anyhow::Result<Vec<Log>> {
		let mut qb = QueryBuilder::<Sqlite>::new("SELECT logs.* FROM logs WHERE 1=1");
		let filter = filter.unwrap_or_default();

		let latest_block =
			U256::from(self.block_provider.latest_block_number().await.unwrap_or_default());

		match (filter.from_block, filter.to_block, filter.block_hash) {
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

	async fn receipts_count_per_block(&self, block_hash: &H256) -> Option<usize> {
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

	async fn block_transaction_hashes(&self, block_hash: &H256) -> Option<HashMap<usize, H256>> {
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

	async fn receipt_by_block_hash_and_index(
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

	async fn receipt_by_hash(&self, transaction_hash: &H256) -> Option<ReceiptInfo> {
		let (block_hash, transaction_index) = self.fetch_row(transaction_hash).await?;

		let block = self.block_provider.block_by_hash(&block_hash).await.ok()??;
		let (_, receipt) = self
			.receipt_extractor
			.extract_from_transaction(&block, transaction_index)
			.await
			.ok()?;
		Some(receipt)
	}

	async fn signed_tx_by_hash(&self, transaction_hash: &H256) -> Option<TransactionSigned> {
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
	use crate::test::MockBlockInfoProvider;
	use pallet_revive::evm::{ReceiptInfo, TransactionSigned};
	use pretty_assertions::assert_eq;
	use sp_core::{H160, H256};
	use sqlx::SqlitePool;

	async fn setup_sqlite_provider(pool: SqlitePool) -> DBReceiptProvider {
		DBReceiptProvider {
			pool,
			block_provider: Arc::new(MockBlockInfoProvider {}),
			receipt_extractor: ReceiptExtractor::new(1_000_000, None),
			prune_old_blocks: true,
		}
	}

	#[sqlx::test]
	async fn test_insert_remove(pool: SqlitePool) {
		let provider = setup_sqlite_provider(pool).await;
		let block_hash = H256::default();
		let receipts = vec![(
			TransactionSigned::default(),
			ReceiptInfo {
				logs: vec![Log { block_hash, ..Default::default() }],
				..Default::default()
			},
		)];

		provider.insert(&block_hash, &receipts).await;
		let row = provider.fetch_row(&receipts[0].1.transaction_hash).await;
		assert_eq!(row, Some((block_hash, 0)));

		provider.remove(&block_hash).await;

		let transaction_count: i64 = sqlx::query_scalar(
			r#"
        SELECT COUNT(*)
        FROM transaction_hashes
        WHERE block_hash = ?
        "#,
		)
		.bind(block_hash.as_ref())
		.fetch_one(&provider.pool)
		.await
		.unwrap();
		assert_eq!(transaction_count, 0);

		let logs_count: i64 = sqlx::query_scalar(
			r#"
        SELECT COUNT(*)
        FROM logs
        WHERE block_hash = ?
        "#,
		)
		.bind(block_hash.as_ref())
		.fetch_one(&provider.pool)
		.await
		.unwrap();
		assert_eq!(logs_count, 0);
	}

	#[sqlx::test]
	async fn test_receipts_count_per_block(pool: SqlitePool) {
		let provider = setup_sqlite_provider(pool).await;
		let block_hash = H256::default();
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

		provider.insert(&block_hash, &receipts).await;
		let count = provider.receipts_count_per_block(&block_hash).await;
		assert_eq!(count, Some(2));
	}

	#[sqlx::test]
	async fn test_query_logs(pool: SqlitePool) -> anyhow::Result<()> {
		let provider = setup_sqlite_provider(pool).await;
		let log1 = Log {
			block_hash: H256::from([1u8; 32]),
			block_number: U256::from(1),
			address: H160::from([1u8; 20]),
			topics: vec![H256::from([1u8; 32]), H256::from([2u8; 32])],
			data: Some(vec![0u8; 32].into()),
			transaction_hash: H256::default(),
			transaction_index: U256::from(1),
			log_index: U256::from(1),
			..Default::default()
		};
		let log2 = Log {
			block_hash: H256::from([2u8; 32]),
			block_number: U256::from(2),
			address: H160::from([2u8; 20]),
			topics: vec![H256::from([2u8; 32]), H256::from([3u8; 32])],
			transaction_hash: H256::from([1u8; 32]),
			transaction_index: U256::from(2),
			log_index: U256::from(1),
			..Default::default()
		};

		provider
			.insert(
				&log1.block_hash,
				&vec![(
					TransactionSigned::default(),
					ReceiptInfo { logs: vec![log1.clone()], ..Default::default() },
				)],
			)
			.await;
		provider
			.insert(
				&log2.block_hash,
				&vec![(
					TransactionSigned::default(),
					ReceiptInfo { logs: vec![log2.clone()], ..Default::default() },
				)],
			)
			.await;

		// Empty filter
		let logs = provider.logs(None).await?;
		assert_eq!(logs, vec![log2.clone()]);

		// from_block filter
		let logs = provider
			.logs(Some(Filter { from_block: Some(log2.block_number), ..Default::default() }))
			.await?;
		assert_eq!(logs, vec![log2.clone()]);

		// to_block filter
		let logs = provider
			.logs(Some(Filter { to_block: Some(log1.block_number), ..Default::default() }))
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
				from_block: Some(U256::from(0)),
				address: Some(log1.address.into()),
				..Default::default()
			}))
			.await?;
		assert_eq!(logs, vec![log1.clone()]);

		// multiple addresses
		let logs = provider
			.logs(Some(Filter {
				from_block: Some(U256::from(0)),
				address: Some(vec![log1.address, log2.address].into()),
				..Default::default()
			}))
			.await?;
		assert_eq!(logs, vec![log1.clone(), log2.clone()]);

		// single topic
		let logs = provider
			.logs(Some(Filter {
				from_block: Some(U256::from(0)),
				topics: Some(vec![FilterTopic::Single(log1.topics[0])]),
				..Default::default()
			}))
			.await?;
		assert_eq!(logs, vec![log1.clone()]);

		// multiple topic
		let logs = provider
			.logs(Some(Filter {
				from_block: Some(U256::from(0)),
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
				from_block: Some(U256::from(0)),
				topics: Some(vec![FilterTopic::Multiple(vec![log1.topics[0], log2.topics[0]])]),
				..Default::default()
			}))
			.await?;
		assert_eq!(logs, vec![log1.clone(), log2.clone()]);

		// Altogether
		let logs = provider
			.logs(Some(Filter {
				from_block: Some(log1.block_number),
				to_block: Some(log2.block_number),
				block_hash: None,
				address: Some(vec![log1.address, log2.address].into()),
				topics: Some(vec![FilterTopic::Multiple(vec![log1.topics[0], log2.topics[0]])]),
			}))
			.await?;
		assert_eq!(logs, vec![log1.clone(), log2.clone()]);
		Ok(())
	}
}
