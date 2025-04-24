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
	client::{SubscriptionType, SubstrateBlock, SubstrateBlockNumber},
	subxt_client::SrcChainConfig,
	ClientError,
};
use jsonrpsee::core::async_trait;
use sp_core::H256;
use std::sync::Arc;
use subxt::{backend::legacy::LegacyRpcMethods, OnlineClient};
use tokio::sync::RwLock;

/// BlockInfoProvider cache and retrieves information about blocks.
#[async_trait]
pub trait BlockInfoProvider: Send + Sync {
	/// Update the latest block
	async fn update_latest(&self, block: SubstrateBlock, subscription_type: SubscriptionType);

	/// Return the latest finalized block.
	async fn latest_finalized_block(&self) -> Arc<SubstrateBlock>;

	/// Return the latest block.
	async fn latest_block(&self) -> Arc<SubstrateBlock>;

	/// Return the latest block number
	async fn latest_block_number(&self) -> SubstrateBlockNumber {
		return self.latest_block().await.number()
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
pub struct SubxtBlockInfoProvider {
	/// The latest block.
	latest_block: Arc<RwLock<Arc<SubstrateBlock>>>,

	/// The latest finalized block.
	latest_finalized_block: Arc<RwLock<Arc<SubstrateBlock>>>,

	/// The rpc client, used to fetch blocks not in the cache.
	rpc: LegacyRpcMethods<SrcChainConfig>,

	/// The api client, used to fetch blocks not in the cache.
	api: OnlineClient<SrcChainConfig>,
}

impl SubxtBlockInfoProvider {
	pub async fn new(
		api: OnlineClient<SrcChainConfig>,
		rpc: LegacyRpcMethods<SrcChainConfig>,
	) -> Result<Self, ClientError> {
		let latest = Arc::new(api.blocks().at_latest().await?);
		Ok(Self {
			api,
			rpc,
			latest_block: Arc::new(RwLock::new(latest.clone())),
			latest_finalized_block: Arc::new(RwLock::new(latest)),
		})
	}
}

#[async_trait]
impl BlockInfoProvider for SubxtBlockInfoProvider {
	async fn update_latest(&self, block: SubstrateBlock, subscription_type: SubscriptionType) {
		let mut latest = match subscription_type {
			SubscriptionType::FinalizedBlocks => self.latest_finalized_block.write().await,
			SubscriptionType::BestBlocks => self.latest_block.write().await,
		};
		*latest = Arc::new(block);
	}

	async fn latest_block(&self) -> Arc<SubstrateBlock> {
		self.latest_block.read().await.clone()
	}

	async fn latest_finalized_block(&self) -> Arc<SubstrateBlock> {
		self.latest_finalized_block.read().await.clone()
	}

	async fn block_by_number(
		&self,
		block_number: SubstrateBlockNumber,
	) -> Result<Option<Arc<SubstrateBlock>>, ClientError> {
		let Some(hash) = self.rpc.chain_get_block_hash(Some(block_number.into())).await? else {
			return Ok(None);
		};

		self.block_by_hash(&hash).await
	}

	async fn block_by_hash(&self, hash: &H256) -> Result<Option<Arc<SubstrateBlock>>, ClientError> {
		if hash == &self.latest_block().await.hash() {
			return Ok(Some(self.latest_block().await));
		} else if hash == &self.latest_finalized_block().await.hash() {
			return Ok(Some(self.latest_finalized_block().await));
		}

		match self.api.blocks().at(*hash).await {
			Ok(block) => Ok(Some(Arc::new(block))),
			Err(subxt::Error::Block(subxt::error::BlockError::NotFound(_))) => Ok(None),
			Err(err) => Err(err.into()),
		}
	}
}

#[cfg(test)]
pub mod test {
	use super::*;
	use crate::BlockInfo;

	/// A Noop BlockInfoProvider used to test [`db::ReceiptProvider`].
	pub struct MockBlockInfoProvider;

	pub struct MockBlockInfo {
		pub number: SubstrateBlockNumber,
		pub hash: H256,
	}

	impl BlockInfo for MockBlockInfo {
		fn hash(&self) -> H256 {
			self.hash
		}
		fn number(&self) -> SubstrateBlockNumber {
			self.number
		}
	}

	#[async_trait]
	impl BlockInfoProvider for MockBlockInfoProvider {
		async fn update_latest(
			&self,
			_block: SubstrateBlock,
			_subscription_type: SubscriptionType,
		) {
		}

		async fn latest_finalized_block(&self) -> Arc<SubstrateBlock> {
			unimplemented!()
		}

		async fn latest_block(&self) -> Arc<SubstrateBlock> {
			unimplemented!()
		}

		async fn latest_block_number(&self) -> SubstrateBlockNumber {
			2u32
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
