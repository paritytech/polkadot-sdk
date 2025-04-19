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
	subxt_client::SrcChainConfig,
	ClientError,
};
use jsonrpsee::core::async_trait;
use sp_core::H256;
use std::{
	collections::{HashMap, VecDeque},
	sync::Arc,
};
use subxt::{backend::legacy::LegacyRpcMethods, OnlineClient};
use tokio::sync::RwLock;

/// BlockInfoProvider cache and retrieves information about blocks.
#[async_trait]
pub trait BlockInfoProvider: Send + Sync {
	/// Cache a new block and return the pruned block hash.
	async fn cache_block(&self, block: SubstrateBlock) -> Option<H256>;

	/// Return the latest ingested block.
	async fn latest_block(&self) -> Option<Arc<SubstrateBlock>>;

	/// Return the latest block number
	async fn latest_block_number(&self) -> Option<SubstrateBlockNumber> {
		return self.latest_block().await.map(|block| block.number());
	}

	/// Get block by block_number.
	async fn block_by_number(
		&self,
		block_number: SubstrateBlockNumber,
	) -> Result<Option<Arc<SubstrateBlock>>, ClientError>;

	/// Get block by block hash.
	async fn block_by_hash(&self, hash: &H256) -> Result<Option<Arc<SubstrateBlock>>, ClientError>;
}

/// Provides information about blocks.
#[derive(Clone)]
pub struct BlockInfoProviderImpl {
	/// The shared in memory cache.
	cache: Arc<RwLock<BlockCache<SubstrateBlock>>>,

	/// The rpc client, used to fetch blocks not in the cache.
	rpc: LegacyRpcMethods<SrcChainConfig>,

	/// The api client, used to fetch blocks not in the cache.
	api: OnlineClient<SrcChainConfig>,
}

impl BlockInfoProviderImpl {
	pub fn new(
		cache_size: usize,
		api: OnlineClient<SrcChainConfig>,
		rpc: LegacyRpcMethods<SrcChainConfig>,
	) -> Self {
		Self { api, rpc, cache: Arc::new(RwLock::new(BlockCache::new(cache_size))) }
	}
}

#[async_trait]
impl BlockInfoProvider for BlockInfoProviderImpl {
	async fn cache_block(&self, block: SubstrateBlock) -> Option<H256> {
		self.cache.write().await.insert(block)
	}

	async fn latest_block(&self) -> Option<Arc<SubstrateBlock>> {
		self.cache.read().await.buffer.back().cloned()
	}

	async fn block_by_number(
		&self,
		block_number: SubstrateBlockNumber,
	) -> Result<Option<Arc<SubstrateBlock>>, ClientError> {
		if let Some(block) = self.cache.read().await.blocks_by_number.get(&block_number).cloned() {
			return Ok(Some(block));
		}

		let Some(hash) = self.rpc.chain_get_block_hash(Some(block_number.into())).await? else {
			return Ok(None);
		};

		self.block_by_hash(&hash).await
	}

	async fn block_by_hash(&self, hash: &H256) -> Result<Option<Arc<SubstrateBlock>>, ClientError> {
		if let Some(block) = self.cache.read().await.blocks_by_hash.get(hash).cloned() {
			return Ok(Some(block));
		}

		match self.api.blocks().at(*hash).await {
			Ok(block) => Ok(Some(Arc::new(block))),
			Err(subxt::Error::Block(subxt::error::BlockError::NotFound(_))) => Ok(None),
			Err(err) => Err(err.into()),
		}
	}
}

/// The cache maintains a buffer of the last N blocks,
struct BlockCache<Block> {
	/// The maximum buffer's size.
	max_cache_size: usize,

	/// A double-ended queue of the last N blocks.
	/// The most recent block is at the back of the queue, and the oldest block is at the front.
	buffer: VecDeque<Arc<Block>>,

	/// A map of blocks by block number.
	blocks_by_number: HashMap<SubstrateBlockNumber, Arc<Block>>,

	/// A map of blocks by block hash.
	blocks_by_hash: HashMap<H256, Arc<Block>>,
}

/// Provides information about a block,
/// This is an abstratction on top of [`SubstrateBlock`] used to test the [`BlockCache`].
/// Can be removed once https://github.com/paritytech/subxt/issues/1883 is fixed.
trait BlockInfo {
	/// Returns the block hash.
	fn hash(&self) -> H256;
	/// Returns the block number.
	fn number(&self) -> SubstrateBlockNumber;
}

impl BlockInfo for SubstrateBlock {
	fn hash(&self) -> H256 {
		SubstrateBlock::hash(self)
	}
	fn number(&self) -> u32 {
		SubstrateBlock::number(self)
	}
}

impl<B: BlockInfo> BlockCache<B> {
	/// Create a new cache with the given maximum buffer size.
	pub fn new(max_cache_size: usize) -> Self {
		Self {
			max_cache_size,
			buffer: Default::default(),
			blocks_by_number: Default::default(),
			blocks_by_hash: Default::default(),
		}
	}

	/// Insert an entry into the cache, and prune the oldest entry if the cache is full.
	pub fn insert(&mut self, block: B) -> Option<H256> {
		let mut pruned_block_hash = None;
		if self.buffer.len() >= self.max_cache_size {
			if let Some(block) = self.buffer.pop_front() {
				let hash = block.hash();
				self.blocks_by_hash.remove(&hash);
				self.blocks_by_number.remove(&block.number());
				pruned_block_hash = Some(hash);
			}
		}

		let block = Arc::new(block);
		self.buffer.push_back(block.clone());
		self.blocks_by_number.insert(block.number(), block.clone());
		self.blocks_by_hash.insert(block.hash(), block);
		pruned_block_hash
	}
}

#[cfg(test)]
pub mod test {
	use super::*;

	struct MockBlock {
		block_number: SubstrateBlockNumber,
		block_hash: H256,
	}

	impl BlockInfo for MockBlock {
		fn hash(&self) -> H256 {
			self.block_hash
		}

		fn number(&self) -> u32 {
			self.block_number
		}
	}

	#[test]
	fn cache_insert_works() {
		let mut cache = BlockCache::<MockBlock>::new(2);

		let pruned = cache.insert(MockBlock { block_number: 1, block_hash: H256::from([1; 32]) });
		assert_eq!(pruned, None);

		let pruned = cache.insert(MockBlock { block_number: 2, block_hash: H256::from([2; 32]) });
		assert_eq!(pruned, None);

		let pruned = cache.insert(MockBlock { block_number: 3, block_hash: H256::from([3; 32]) });
		assert_eq!(pruned, Some(H256::from([1; 32])));

		assert_eq!(cache.buffer.len(), 2);
		assert_eq!(cache.blocks_by_number.len(), 2);
		assert_eq!(cache.blocks_by_hash.len(), 2);
	}

	/// A Noop BlockInfoProvider used to test [`db::DBReceiptProvider`].
	pub struct MockBlockInfoProvider;

	#[async_trait]
	impl BlockInfoProvider for MockBlockInfoProvider {
		async fn cache_block(&self, _block: SubstrateBlock) -> Option<H256> {
			None
		}

		async fn latest_block(&self) -> Option<Arc<SubstrateBlock>> {
			None
		}

		async fn latest_block_number(&self) -> Option<SubstrateBlockNumber> {
			Some(2u32)
		}

		async fn block_by_number(
			&self,
			_block_number: SubstrateBlockNumber,
		) -> Result<Option<Arc<SubstrateBlock>>, ClientError> {
			Ok(None)
		}

		async fn block_by_hash(
			&self,
			_hash: &H256,
		) -> Result<Option<Arc<SubstrateBlock>>, ClientError> {
			Ok(None)
		}
	}
}
