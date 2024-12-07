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
	LOG_TARGET,
};
use pallet_revive::evm::{ReceiptInfo, TransactionSigned, H256, U256};
use std::{
	collections::{HashMap, VecDeque},
	sync::Arc,
};
use tokio::sync::RwLock;

/// The number of recent blocks maintained by the cache.
/// For each block in the cache, we also store the EVM transaction receipts.
pub const CACHE_SIZE: usize = 256;

/// The cache maintains a buffer of the last N blocks,
#[derive(Default)]
pub struct BlockCache<const N: usize = CACHE_SIZE> {
	/// A double-ended queue of the last N blocks.
	/// The most recent block is at the back of the queue, and the oldest block is at the front.
	buffer: VecDeque<Arc<SubstrateBlock>>,

	/// A map of blocks by block number.
	blocks_by_number: HashMap<SubstrateBlockNumber, Arc<SubstrateBlock>>,

	/// A map of blocks by block hash.
	blocks_by_hash: HashMap<H256, Arc<SubstrateBlock>>,

	/// A map of receipts by hash.
	receipts_by_hash: HashMap<H256, ReceiptInfo>,

	/// A map of Signed transaction by hash.
	signed_tx_by_hash: HashMap<H256, TransactionSigned>,

	/// A map of receipt hashes by block hash.
	tx_hashes_by_block_and_index: HashMap<H256, HashMap<U256, H256>>,
}

pub struct BlockDataProvider {
	cache: Arc<RwLock<BlockCache<CACHE_SIZE>>>,
}

impl Default for BlockDataProvider {
	fn default() -> Self {
		Self { cache: Arc::new(RwLock::new(BlockCache::default())) }
	}
}

impl BlockDataProvider {
	async fn cache(&self) -> tokio::sync::RwLockReadGuard<'_, BlockCache<CACHE_SIZE>> {
		self.cache.read().await
	}

	pub async fn latest_block(&self) -> Option<Arc<SubstrateBlock>> {
		let cache = self.cache().await;
		cache.buffer.back().cloned()
	}

	pub async fn block_by_number(
		&self,
		number: SubstrateBlockNumber,
	) -> Option<Arc<SubstrateBlock>> {
		let cache = self.cache().await;
		cache.blocks_by_number.get(&number).cloned()
	}

	pub async fn block_by_hash(&self, hash: &H256) -> Option<Arc<SubstrateBlock>> {
		let cache = self.cache().await;
		cache.blocks_by_hash.get(hash).cloned()
	}

	pub async fn receipt_by_hash(&self, hash: &H256) -> Option<ReceiptInfo> {
		let cache = self.cache().await;
		cache.receipts_by_hash.get(hash).cloned()
	}

	pub async fn signed_tx_by_hash(&self, hash: &H256) -> Option<TransactionSigned> {
		let cache = self.cache().await;
		cache.signed_tx_by_hash.get(hash).cloned()
	}
}

impl<const N: usize> BlockCache<N> {
	/// Insert an entry into the cache, and prune the oldest entry if the cache is full.
	pub fn insert(
		&mut self,
		block: SubstrateBlock,
		receipts: Vec<(TransactionSigned, ReceiptInfo)>,
	) {
		if self.buffer.len() >= N {
			if let Some(block) = self.buffer.pop_front() {
				log::trace!(target: LOG_TARGET, "Pruning block: {}", block.number());
				let hash = block.hash();
				self.blocks_by_hash.remove(&hash);
				self.blocks_by_number.remove(&block.number());
				self.signed_tx_by_hash.remove(&hash);
				if let Some(entries) = self.tx_hashes_by_block_and_index.remove(&hash) {
					for hash in entries.values() {
						self.receipts_by_hash.remove(hash);
					}
				}
			}
		}
		if !receipts.is_empty() {
			let values = receipts
				.iter()
				.map(|(_, receipt)| (receipt.transaction_index, receipt.transaction_hash))
				.collect::<HashMap<_, _>>();

			self.tx_hashes_by_block_and_index.insert(block.hash(), values);

			self.receipts_by_hash.extend(
				receipts.iter().map(|(_, receipt)| (receipt.transaction_hash, receipt.clone())),
			);

			self.signed_tx_by_hash.extend(
				receipts
					.iter()
					.map(|(signed_tx, receipt)| (receipt.transaction_hash, signed_tx.clone())),
			)
		}

		let block = Arc::new(block);
		self.buffer.push_back(block.clone());
		self.blocks_by_number.insert(block.number(), block.clone());
		self.blocks_by_hash.insert(block.hash(), block);
	}
}
