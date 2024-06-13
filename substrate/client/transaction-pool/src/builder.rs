// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Utitlity for building substrate transaction pool trait object.

use sp_runtime::traits::Block as BlockT;
use std::{marker::PhantomData, sync::Arc, time::Duration};

use crate::{
	fork_aware_txpool::fork_aware_txpool::FullPool as ForkAwareFullPool, graph::IsValidator,
	single_state_txpool::single_state_txpool::FullPool as SingleStateFullPool,
};
use prometheus_endpoint::Registry as PrometheusRegistry;
use sc_transaction_pool_api::{LocalTransactionPool, MaintainedTransactionPool};
use sp_core::traits::SpawnEssentialNamed;

/// The type of transaction pool.
#[derive(Debug, Clone)]
pub enum TransactionPoolType {
	/// Single-state transaction pool
	SingleState,
	/// Fork-aware transaction pool
	ForkAware,
}

/// Transaction pool options.
#[derive(Debug, Clone)]
pub struct TransactionPoolOptions {
	txpool_type: TransactionPoolType,
	options: crate::graph::Options,
}

impl Default for TransactionPoolOptions {
	fn default() -> Self {
		Self { txpool_type: TransactionPoolType::SingleState, options: Default::default() }
	}
}

impl TransactionPoolOptions {
	/// Creates the options for the transaction pool using given parameters.
	pub fn new_with_params(
		pool_limit: usize,
		pool_kbytes: usize,
		tx_ban_seconds: Option<u64>,
		txpool_type: TransactionPoolType,
		is_dev: bool,
	) -> TransactionPoolOptions {
		let mut options = crate::graph::Options::default();

		// ready queue
		options.ready.count = pool_limit;
		options.ready.total_bytes = pool_kbytes * 1024;

		// future queue
		let factor = 10;
		options.future.count = pool_limit / factor;
		options.future.total_bytes = pool_kbytes * 1024 / factor;

		options.ban_time = if let Some(ban_seconds) = tx_ban_seconds {
			std::time::Duration::from_secs(ban_seconds)
		} else if is_dev {
			std::time::Duration::from_secs(0)
		} else {
			std::time::Duration::from_secs(30 * 60)
		};

		TransactionPoolOptions { options, txpool_type }
	}

	/// Creates predefined options for benchmarking
	pub fn new_for_benchmarks() -> TransactionPoolOptions {
		TransactionPoolOptions {
			options: crate::graph::Options {
				ready: crate::graph::base_pool::Limit {
					count: 100_000,
					total_bytes: 100 * 1024 * 1024,
				},
				future: crate::graph::base_pool::Limit {
					count: 100_000,
					total_bytes: 100 * 1024 * 1024,
				},
				reject_future_transactions: false,
				ban_time: Duration::from_secs(30 * 60),
			},
			txpool_type: TransactionPoolType::SingleState,
		}
	}
}

use crate::{common::api::FullChainApi, graph::ChainApi};

/// `FullClientTransactionPool` is a trait that combines the functionality of
/// `MaintainedTransactionPool` and `LocalTransactionPool` for a given `Client` and `Block`.
///
/// This trait defines the requirements for a full client transaction pool, ensuring
/// that it can handle transactions submission and maintenance.
pub trait FullClientTransactionPool<Block, Client>:
	MaintainedTransactionPool<
		Block = Block,
		Hash = crate::graph::ExtrinsicHash<FullChainApi<Client, Block>>,
		InPoolTransaction = crate::graph::base_pool::Transaction<
			crate::graph::ExtrinsicHash<FullChainApi<Client, Block>>,
			<Block as BlockT>::Extrinsic,
		>,
		Error = <FullChainApi<Client, Block> as ChainApi>::Error,
	> + LocalTransactionPool<
		Block = Block,
		Hash = crate::graph::ExtrinsicHash<FullChainApi<Client, Block>>,
		Error = <FullChainApi<Client, Block> as ChainApi>::Error,
	>
where
	Block: BlockT,
	Client: sp_api::ProvideRuntimeApi<Block>
		+ sc_client_api::BlockBackend<Block>
		+ sc_client_api::blockchain::HeaderBackend<Block>
		+ sp_runtime::traits::BlockIdTo<Block>
		+ sp_blockchain::HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ 'static,
	Client::Api: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>,
{
}

impl<Block, Client, P> FullClientTransactionPool<Block, Client> for P
where
	Block: BlockT,
	Client: sp_api::ProvideRuntimeApi<Block>
		+ sc_client_api::BlockBackend<Block>
		+ sc_client_api::blockchain::HeaderBackend<Block>
		+ sp_runtime::traits::BlockIdTo<Block>
		+ sp_blockchain::HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ 'static,
	Client::Api: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>,
	P: MaintainedTransactionPool<
			Block = Block,
			Hash = crate::graph::ExtrinsicHash<FullChainApi<Client, Block>>,
			InPoolTransaction = crate::graph::base_pool::Transaction<
				crate::graph::ExtrinsicHash<FullChainApi<Client, Block>>,
				<Block as BlockT>::Extrinsic,
			>,
			Error = <FullChainApi<Client, Block> as ChainApi>::Error,
		> + LocalTransactionPool<
			Block = Block,
			Hash = crate::graph::ExtrinsicHash<FullChainApi<Client, Block>>,
			Error = <FullChainApi<Client, Block> as ChainApi>::Error,
		>,
{
}

/// The type alias for a dynamic trait object implementing
/// `FullClientTransactionPool` with the given `Client` and `Block` types.
///
/// This trait object abstracts away the specific implementations of the transaction pool.
pub type TransactionPoolImpl<Block, Client> = dyn FullClientTransactionPool<Block, Client>;

/// Builder allowing to create specific instance of transaction pool.
pub struct Builder<Block, Client> {
	options: TransactionPoolOptions,
	_phantom: PhantomData<(Client, Block)>,
}

impl<Client, Block> Builder<Block, Client>
where
	Block: BlockT,
	Client: sp_api::ProvideRuntimeApi<Block>
		+ sc_client_api::BlockBackend<Block>
		+ sc_client_api::blockchain::HeaderBackend<Block>
		+ sp_runtime::traits::BlockIdTo<Block>
		+ sc_client_api::ExecutorProvider<Block>
		+ sc_client_api::UsageProvider<Block>
		+ sp_blockchain::HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ Send
		+ Sync
		+ 'static,
	<Block as BlockT>::Hash: std::marker::Unpin,
	Client::Api: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>,
{
	/// Creates new instance of `Builder`
	pub fn new() -> Builder<Block, Client> {
		Builder { options: Default::default(), _phantom: Default::default() }
	}

	/// Sets the options used for creating a transaction pool instance.
	pub fn with_options(mut self, options: TransactionPoolOptions) -> Self {
		self.options = options;
		self
	}

	/// Creates an instance of transaction pool.
	pub fn build(
		self,
		is_validator: IsValidator,
		prometheus: Option<&PrometheusRegistry>,
		spawner: impl SpawnEssentialNamed,
		client: Arc<Client>,
	) -> Arc<TransactionPoolImpl<Block, Client>> {
		match self.options.txpool_type {
			TransactionPoolType::SingleState => SingleStateFullPool::new_full(
				self.options.options,
				is_validator,
				prometheus,
				spawner,
				client,
			),
			TransactionPoolType::ForkAware => ForkAwareFullPool::new_full(
				self.options.options,
				is_validator,
				prometheus,
				spawner,
				client,
			),
		}
	}
}
