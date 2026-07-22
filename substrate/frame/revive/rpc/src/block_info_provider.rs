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
	ClientError,
	client::{SubscriptionType, SubstrateBlockNumber},
};
use jsonrpsee::core::async_trait;
use sp_core::H256;
use std::sync::Arc;

/// Provides information about a block.
pub trait BlockInfo: Send + Sync + 'static {
	/// Returns the block hash.
	fn hash(&self) -> H256;
	/// Returns the block number.
	fn number(&self) -> SubstrateBlockNumber;
	/// Returns the parent hash.
	fn parent_hash(&self) -> H256;
}

/// A generic block-info provider that caches and retrieves block metadata.
#[async_trait]
pub trait BlockInfoProvider: Send + Sync + Clone + 'static {
	/// The concrete block type this provider returns.
	type Block: BlockInfo + Clone;

	/// Update the cached latest block.
	async fn update_latest(&self, block: Arc<Self::Block>, subscription_type: SubscriptionType);

	/// Return the latest finalized block.
	async fn latest_finalized_block(&self) -> Arc<Self::Block>;

	/// Return the latest best block.
	async fn latest_block(&self) -> Arc<Self::Block>;

	/// Return the latest block number.
	async fn latest_block_number(&self) -> SubstrateBlockNumber {
		self.latest_block().await.number()
	}

	/// Get a block by block number.
	async fn block_by_number(
		&self,
		block_number: SubstrateBlockNumber,
	) -> Result<Option<Arc<Self::Block>>, ClientError>;

	/// Get a block by hash.
	async fn block_by_hash(&self, hash: &H256) -> Result<Option<Arc<Self::Block>>, ClientError>;
}

/// The subxt-backed block info provider.
#[cfg(feature = "subxt")]
pub use crate::subxt_client::SubxtBlockInfoProvider;

pub mod test {
	use super::*;
	use crate::client::SubstrateBlockNumber;

	/// A noop [`BlockInfoProvider`] used in unit tests.
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
		fn parent_hash(&self) -> H256 {
			H256::default()
		}
	}

	impl Clone for MockBlockInfo {
		fn clone(&self) -> Self {
			Self { number: self.number, hash: self.hash }
		}
	}

	#[async_trait]
	impl BlockInfoProvider for MockBlockInfoProvider {
		type Block = MockBlockInfo;

		async fn update_latest(
			&self,
			_block: Arc<MockBlockInfo>,
			_subscription_type: SubscriptionType,
		) {
		}

		async fn latest_finalized_block(&self) -> Arc<MockBlockInfo> {
			unimplemented!()
		}

		async fn latest_block(&self) -> Arc<MockBlockInfo> {
			unimplemented!()
		}

		async fn latest_block_number(&self) -> SubstrateBlockNumber {
			2u32
		}

		async fn block_by_number(
			&self,
			_block_number: SubstrateBlockNumber,
		) -> Result<Option<Arc<MockBlockInfo>>, ClientError> {
			Ok(None)
		}

		async fn block_by_hash(
			&self,
			_hash: &H256,
		) -> Result<Option<Arc<MockBlockInfo>>, ClientError> {
			Ok(None)
		}
	}

	impl Clone for MockBlockInfoProvider {
		fn clone(&self) -> Self {
			MockBlockInfoProvider
		}
	}
}
