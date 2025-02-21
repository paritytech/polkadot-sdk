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
use super::ReceiptProvider;
use jsonrpsee::core::async_trait;
use pallet_revive::evm::{Filter, Log, ReceiptInfo, TransactionSigned, H256};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

/// A `[ReceiptProvider]` that caches receipts in memory.
#[derive(Clone, Default)]
pub struct CacheReceiptProvider {
	cache: Arc<RwLock<ReceiptCache>>,
}

impl CacheReceiptProvider {
	/// Get a read access on the shared cache.
	async fn cache(&self) -> tokio::sync::RwLockReadGuard<'_, ReceiptCache> {
		self.cache.read().await
	}
}

#[async_trait]
impl ReceiptProvider for CacheReceiptProvider {
	async fn archive(&self, _block_hash: &H256, _receipts: &[(TransactionSigned, ReceiptInfo)]) {}

	async fn logs(&self, _filter: Option<Filter>) -> anyhow::Result<Vec<Log>> {
		anyhow::bail!("Not implemented")
	}

	async fn insert(&self, block_hash: &H256, receipts: &[(TransactionSigned, ReceiptInfo)]) {
		let mut cache = self.cache.write().await;
		cache.insert(block_hash, receipts);
	}

	async fn remove(&self, block_hash: &H256) {
		let mut cache = self.cache.write().await;
		cache.remove(block_hash);
	}

	async fn receipt_by_block_hash_and_index(
		&self,
		block_hash: &H256,
		transaction_index: usize,
	) -> Option<ReceiptInfo> {
		let cache = self.cache().await;
		let receipt_hash = cache
			.transaction_hashes_by_block_and_index
			.get(block_hash)?
			.get(&transaction_index)?;
		let receipt = cache.receipts_by_hash.get(receipt_hash)?;
		Some(receipt.clone())
	}

	async fn receipts_count_per_block(&self, block_hash: &H256) -> Option<usize> {
		let cache = self.cache().await;
		cache.transaction_hashes_by_block_and_index.get(block_hash).map(|v| v.len())
	}

	async fn block_transaction_hashes(&self, block_hash: &H256) -> Option<HashMap<usize, H256>> {
		let cache = self.cache().await;
		cache.transaction_hashes_by_block_and_index.get(block_hash).cloned()
	}

	async fn receipt_by_hash(&self, hash: &H256) -> Option<ReceiptInfo> {
		let cache = self.cache().await;
		cache.receipts_by_hash.get(hash).cloned()
	}

	async fn signed_tx_by_hash(&self, hash: &H256) -> Option<TransactionSigned> {
		let cache = self.cache().await;
		cache.signed_tx_by_hash.get(hash).cloned()
	}
}

#[derive(Default)]
struct ReceiptCache {
	/// A map of receipts by transaction hash.
	receipts_by_hash: HashMap<H256, ReceiptInfo>,

	/// A map of Signed transaction by transaction hash.
	signed_tx_by_hash: HashMap<H256, TransactionSigned>,

	/// A map of receipt hashes by block hash.
	transaction_hashes_by_block_and_index: HashMap<H256, HashMap<usize, H256>>,
}

impl ReceiptCache {
	/// Insert new receipts into the cache.
	pub fn insert(&mut self, block_hash: &H256, receipts: &[(TransactionSigned, ReceiptInfo)]) {
		if !receipts.is_empty() {
			let values = receipts
				.iter()
				.map(|(_, receipt)| {
					(receipt.transaction_index.as_usize(), receipt.transaction_hash)
				})
				.collect::<HashMap<_, _>>();

			self.transaction_hashes_by_block_and_index.insert(*block_hash, values);

			self.receipts_by_hash.extend(
				receipts.iter().map(|(_, receipt)| (receipt.transaction_hash, receipt.clone())),
			);

			self.signed_tx_by_hash.extend(
				receipts
					.iter()
					.map(|(signed_tx, receipt)| (receipt.transaction_hash, signed_tx.clone())),
			)
		}
	}

	/// Remove entry from the cache.
	pub fn remove(&mut self, hash: &H256) {
		if let Some(entries) = self.transaction_hashes_by_block_and_index.remove(hash) {
			for hash in entries.values() {
				self.receipts_by_hash.remove(hash);
				self.signed_tx_by_hash.remove(hash);
			}
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn cache_insert_and_remove_works() {
		let mut cache = ReceiptCache::default();

		for i in 1u8..=3 {
			let hash = H256::from([i; 32]);
			cache.insert(
				&hash,
				&[(
					TransactionSigned::default(),
					ReceiptInfo { transaction_hash: hash, ..Default::default() },
				)],
			);
		}

		cache.remove(&H256::from([1u8; 32]));
		assert_eq!(cache.transaction_hashes_by_block_and_index.len(), 2);
		assert_eq!(cache.receipts_by_hash.len(), 2);
		assert_eq!(cache.signed_tx_by_hash.len(), 2);
	}
}
