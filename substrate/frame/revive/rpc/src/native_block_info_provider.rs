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
//! A [`BlockInfoProvider`] implementation backed by the native in-process Substrate client.
//!
//! This avoids the need to open a `subxt` WebSocket connection when the ETH-RPC
//! server is embedded directly inside the node binary (e.g. Asset Hub).
use crate::{
	ClientError,
	block_info_provider::{BlockInfo, BlockInfoProvider},
	client::{SubscriptionType, SubstrateBlockNumber},
	native_client::OpaqueBlock,
};
use codec::Decode;
use jsonrpsee::core::async_trait;
use sc_client_api::{BlockBackend, HeaderBackend};
use sp_core::H256;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use std::sync::Arc;
use tokio::sync::RwLock;

/// A lightweight cached block record produced by the native client.
#[derive(Clone, Debug)]
pub struct NativeNormalizedBlock {
	pub hash: H256,
	pub number: SubstrateBlockNumber,
	pub parent_hash: H256,
	pub opaque: OpaqueBlock,
}

impl NativeNormalizedBlock {
	pub fn hash(&self) -> H256 {
		self.hash
	}
	pub fn number(&self) -> SubstrateBlockNumber {
		self.number
	}
	pub fn header(&self) -> &sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256> {
		&self.opaque.header
	}
}

impl BlockInfo for NativeNormalizedBlock {
	fn hash(&self) -> H256 {
		self.hash
	}

	fn number(&self) -> SubstrateBlockNumber {
		self.number
	}

	fn parent_hash(&self) -> H256 {
		self.parent_hash
	}
}

/// A [`BlockInfoProvider`] that fetches blocks directly from the in-process Substrate client.
pub struct NativeClientBlockInfoProvider<Client, Block = OpaqueBlock> {
	client: Arc<Client>,
	latest_block: Arc<RwLock<Arc<NativeNormalizedBlock>>>,
	latest_finalized_block: Arc<RwLock<Arc<NativeNormalizedBlock>>>,
	_block: std::marker::PhantomData<Block>,
}

impl<Client, Block> Clone for NativeClientBlockInfoProvider<Client, Block> {
	fn clone(&self) -> Self {
		Self {
			client: Arc::clone(&self.client),
			latest_block: Arc::clone(&self.latest_block),
			latest_finalized_block: Arc::clone(&self.latest_finalized_block),
			_block: std::marker::PhantomData,
		}
	}
}

impl<Client, Block> NativeClientBlockInfoProvider<Client, Block>
where
	Block: BlockT<Hash = H256> + Send + Sync + 'static,
	Block::Header: HeaderT<Number = u32, Hash = H256>,
	Client: HeaderBackend<Block> + BlockBackend<Block> + Send + Sync + 'static,
{
	/// Create a new provider, seeding the cache with the current best block.
	pub fn new(client: Arc<Client>) -> Result<Self, ClientError> {
		let info = client.info();
		let best =
			Self::fetch_block_inner(&client, info.best_hash)?.ok_or(ClientError::BlockNotFound)?;
		let finalized =
			Self::fetch_block_inner(&client, info.finalized_hash)?.unwrap_or_else(|| best.clone());

		Ok(Self {
			client,
			latest_block: Arc::new(RwLock::new(best)),
			latest_finalized_block: Arc::new(RwLock::new(finalized)),
			_block: std::marker::PhantomData,
		})
	}

	fn fetch_block_inner(
		client: &Arc<Client>,
		hash: H256,
	) -> Result<Option<Arc<NativeNormalizedBlock>>, ClientError> {
		let Some(signed) =
			client.block(hash).map_err(|e| ClientError::NativeClientError(e.to_string()))?
		else {
			return Ok(None);
		};

		let encoded = signed.block.encode();
		let opaque = OpaqueBlock::decode(&mut &encoded[..])
			.map_err(|e| ClientError::NativeClientError(e.to_string()))?;

		let number: u32 = (*signed.block.header().number()).into();
		Ok(Some(Arc::new(NativeNormalizedBlock {
			hash,
			number,
			parent_hash: *signed.block.header().parent_hash(),
			opaque,
		})))
	}
}

#[async_trait]
impl<Client, Block> BlockInfoProvider for NativeClientBlockInfoProvider<Client, Block>
where
	Block: BlockT<Hash = H256> + Send + Sync + 'static,
	Block::Header: HeaderT<Number = u32, Hash = H256> + Send + Sync,
	Client: HeaderBackend<Block> + BlockBackend<Block> + Send + Sync + 'static,
{
	type Block = NativeNormalizedBlock;

	async fn update_latest(
		&self,
		block: Arc<NativeNormalizedBlock>,
		subscription_type: SubscriptionType,
	) {
		let mut slot = match subscription_type {
			SubscriptionType::FinalizedBlocks => self.latest_finalized_block.write().await,
			SubscriptionType::BestBlocks => self.latest_block.write().await,
		};
		*slot = block;
	}

	async fn latest_block(&self) -> Arc<NativeNormalizedBlock> {
		self.latest_block.read().await.clone()
	}

	async fn latest_finalized_block(&self) -> Arc<NativeNormalizedBlock> {
		self.latest_finalized_block.read().await.clone()
	}

	async fn block_by_number(
		&self,
		block_number: SubstrateBlockNumber,
	) -> Result<Option<Arc<NativeNormalizedBlock>>, ClientError> {
		let latest = self.latest_block.read().await.clone();
		if latest.number() == block_number {
			return Ok(Some(latest));
		}
		let finalized = self.latest_finalized_block.read().await.clone();
		if finalized.number() == block_number {
			return Ok(Some(finalized));
		}

		let hash = self
			.client
			.block_hash(block_number.into())
			.map_err(|e| ClientError::NativeClientError(e.to_string()))?;

		match hash {
			Some(h) => Self::fetch_block_inner(&self.client, h),
			None => Ok(None),
		}
	}

	async fn block_by_hash(
		&self,
		hash: &H256,
	) -> Result<Option<Arc<NativeNormalizedBlock>>, ClientError> {
		let latest = self.latest_block.read().await.clone();
		if &latest.hash() == hash {
			return Ok(Some(latest));
		}
		let finalized = self.latest_finalized_block.read().await.clone();
		if &finalized.hash() == hash {
			return Ok(Some(finalized));
		}

		Self::fetch_block_inner(&self.client, *hash)
	}
}
