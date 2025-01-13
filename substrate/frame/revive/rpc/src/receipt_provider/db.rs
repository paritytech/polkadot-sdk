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
use crate::BlockInfoProvider;
use jsonrpsee::core::async_trait;
use pallet_revive::evm::{ReceiptInfo, TransactionSigned};
use sp_core::{H256, U256};
use sqlx::{query, SqlitePool};
use std::sync::Arc;

/// A `[ReceiptProvider]` that stores receipts in a SQLite database.
#[derive(Clone)]
pub struct DBReceiptProvider {
	/// The database pool.
	pool: SqlitePool,
	/// The block provider used to fetch blocks, and reconstruct receipts.
	block_provider: Arc<dyn BlockInfoProvider>,
	/// weather or not we should write to the DB.
	read_only: bool,
}

impl DBReceiptProvider {
	/// Create a new `DBReceiptProvider` with the given database URL and block provider.
	pub async fn new(
		database_url: &str,
		read_only: bool,
		block_provider: Arc<dyn BlockInfoProvider>,
	) -> Result<Self, sqlx::Error> {
		let pool = SqlitePool::connect(database_url).await?;
		Ok(Self { pool, block_provider, read_only })
	}

	async fn fetch_row(&self, transaction_hash: &H256) -> Option<(H256, usize)> {
		let transaction_hash = hex::encode(transaction_hash);
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

		let block_hash = result.block_hash.parse::<H256>().ok()?;
		let transaction_index = result.transaction_index.try_into().ok()?;
		Some((block_hash, transaction_index))
	}
}

#[async_trait]
impl ReceiptProvider for DBReceiptProvider {
	async fn insert(&self, block_hash: &H256, receipts: &[(TransactionSigned, ReceiptInfo)]) {
		if self.read_only {
			return
		}

		let block_hash_str = hex::encode(block_hash);
		for (_, receipt) in receipts {
			let transaction_hash = hex::encode(receipt.transaction_hash);
			let transaction_index = receipt.transaction_index.as_u32() as i32;

			let result = query!(
				r#"
				INSERT INTO transaction_hashes (transaction_hash, block_hash, transaction_index)
				VALUES ($1, $2, $3)

				ON CONFLICT(transaction_hash) DO UPDATE SET
				block_hash = EXCLUDED.block_hash,
				transaction_index = EXCLUDED.transaction_index
				"#,
				transaction_hash,
				block_hash_str,
				transaction_index
			)
			.execute(&self.pool)
			.await;

			if let Err(err) = result {
				log::error!(
					"Error inserting transaction for block hash {block_hash:?}:  {:?}",
					err
				);
			}
		}
	}

	async fn remove(&self, _block_hash: &H256) {}

	async fn receipts_count_per_block(&self, block_hash: &H256) -> Option<usize> {
		let block_hash = hex::encode(block_hash);
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

	async fn receipt_by_block_hash_and_index(
		&self,
		block_hash: &H256,
		transaction_index: &U256,
	) -> Option<ReceiptInfo> {
		let block = self.block_provider.block_by_hash(block_hash).await.ok()??;
		let transaction_index: usize = transaction_index.as_usize(); // TODO: check for overflow
		let (_, receipt) =
			extract_receipts_from_transaction(&block, transaction_index).await.ok()?;
		Some(receipt)
	}

	async fn receipt_by_hash(&self, transaction_hash: &H256) -> Option<ReceiptInfo> {
		let (block_hash, transaction_index) = self.fetch_row(transaction_hash).await?;

		let block = self.block_provider.block_by_hash(&block_hash).await.ok()??;
		let (_, receipt) =
			extract_receipts_from_transaction(&block, transaction_index).await.ok()?;
		Some(receipt)
	}

	async fn signed_tx_by_hash(&self, transaction_hash: &H256) -> Option<TransactionSigned> {
		let transaction_hash = hex::encode(transaction_hash);
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

		let block_hash = result.block_hash.parse::<H256>().ok()?;
		let transaction_index = result.transaction_index.try_into().ok()?;

		let block = self.block_provider.block_by_hash(&block_hash).await.ok()??;
		let (signed_tx, _) =
			extract_receipts_from_transaction(&block, transaction_index).await.ok()?;
		Some(signed_tx)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test::MockBlockInfoProvider;
	use pallet_revive::evm::{ReceiptInfo, TransactionSigned};
	use sp_core::H256;
	use sqlx::SqlitePool;

	async fn setup_sqlite_provider(pool: SqlitePool) -> DBReceiptProvider {
		DBReceiptProvider {
			pool,
			block_provider: Arc::new(MockBlockInfoProvider {}),
			read_only: false,
		}
	}

	#[sqlx::test]
	async fn test_insert(pool: SqlitePool) {
		let provider = setup_sqlite_provider(pool).await;
		let block_hash = H256::default();
		let receipts = vec![(TransactionSigned::default(), ReceiptInfo::default())];

		provider.insert(&block_hash, &receipts).await;
		let row = provider.fetch_row(&receipts[0].1.transaction_hash).await;
		assert_eq!(row, Some((block_hash, 0)));
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
}
