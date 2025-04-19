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

use jsonrpsee::core::async_trait;
use pallet_revive::evm::{Filter, Log, ReceiptInfo, TransactionSigned, H256};
use std::collections::HashMap;
use tokio::join;

mod cache;
pub use cache::CacheReceiptProvider;

mod db;
pub use db::DBReceiptProvider;

/// Provide means to store and retrieve receipts.
#[async_trait]
pub trait ReceiptProvider: Send + Sync {
	/// Insert receipts into the provider.
	async fn insert(&self, block_hash: &H256, receipts: &[(TransactionSigned, ReceiptInfo)]);

	/// Similar to `insert`, but intended for archiving receipts from historical blocks.
	async fn archive(&self, block_hash: &H256, receipts: &[(TransactionSigned, ReceiptInfo)]);

	/// Get logs that match the given filter.
	async fn logs(&self, filter: Option<Filter>) -> anyhow::Result<Vec<Log>>;

	/// Deletes receipts associated with the specified block hash.
	async fn remove(&self, block_hash: &H256);

	/// Return all transaction hashes for the given block hash.
	async fn block_transaction_hashes(&self, block_hash: &H256) -> Option<HashMap<usize, H256>>;

	/// Get the receipt for the given block hash and transaction index.
	async fn receipt_by_block_hash_and_index(
		&self,
		block_hash: &H256,
		transaction_index: usize,
	) -> Option<ReceiptInfo>;

	/// Get the number of receipts per block.
	async fn receipts_count_per_block(&self, block_hash: &H256) -> Option<usize>;

	/// Get the receipt for the given transaction hash.
	async fn receipt_by_hash(&self, transaction_hash: &H256) -> Option<ReceiptInfo>;

	/// Get the signed transaction for the given transaction hash.
	async fn signed_tx_by_hash(&self, transaction_hash: &H256) -> Option<TransactionSigned>;
}

#[async_trait]
impl<Cache: ReceiptProvider, Archive: ReceiptProvider> ReceiptProvider for (Cache, Archive) {
	async fn insert(&self, block_hash: &H256, receipts: &[(TransactionSigned, ReceiptInfo)]) {
		join!(self.0.insert(block_hash, receipts), self.1.insert(block_hash, receipts));
	}

	async fn archive(&self, block_hash: &H256, receipts: &[(TransactionSigned, ReceiptInfo)]) {
		self.1.insert(block_hash, receipts).await;
	}

	async fn remove(&self, block_hash: &H256) {
		join!(self.0.remove(block_hash), self.1.remove(block_hash));
	}

	async fn receipt_by_block_hash_and_index(
		&self,
		block_hash: &H256,
		transaction_index: usize,
	) -> Option<ReceiptInfo> {
		if let Some(receipt) =
			self.0.receipt_by_block_hash_and_index(block_hash, transaction_index).await
		{
			return Some(receipt);
		}

		self.1.receipt_by_block_hash_and_index(block_hash, transaction_index).await
	}

	async fn receipts_count_per_block(&self, block_hash: &H256) -> Option<usize> {
		if let Some(count) = self.0.receipts_count_per_block(block_hash).await {
			return Some(count);
		}
		self.1.receipts_count_per_block(block_hash).await
	}

	async fn block_transaction_hashes(&self, block_hash: &H256) -> Option<HashMap<usize, H256>> {
		if let Some(hashes) = self.0.block_transaction_hashes(block_hash).await {
			return Some(hashes);
		}
		self.1.block_transaction_hashes(block_hash).await
	}

	async fn receipt_by_hash(&self, hash: &H256) -> Option<ReceiptInfo> {
		if let Some(receipt) = self.0.receipt_by_hash(hash).await {
			return Some(receipt);
		}
		self.1.receipt_by_hash(hash).await
	}

	async fn signed_tx_by_hash(&self, hash: &H256) -> Option<TransactionSigned> {
		if let Some(tx) = self.0.signed_tx_by_hash(hash).await {
			return Some(tx);
		}
		self.1.signed_tx_by_hash(hash).await
	}

	async fn logs(&self, filter: Option<Filter>) -> anyhow::Result<Vec<Log>> {
		self.1.logs(filter).await
	}
}
