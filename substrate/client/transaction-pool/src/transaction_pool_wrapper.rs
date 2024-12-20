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

//! Transaction pool wrapper. Provides a type for wrapping object providing actual implementation of
//! transaction pool.

use crate::{
	builder::FullClientTransactionPool,
	graph::{base_pool::Transaction, ExtrinsicFor, ExtrinsicHash},
	ChainApi, FullChainApi,
};
use async_trait::async_trait;
use sc_transaction_pool_api::{
	ChainEvent, ImportNotificationStream, LocalTransactionFor, LocalTransactionPool,
	MaintainedTransactionPool, PoolFuture, PoolStatus, ReadyTransactions, TransactionFor,
	TransactionPool, TransactionSource, TransactionStatusStreamFor, TxHash,
};
use sp_runtime::traits::Block as BlockT;
use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

/// The wrapper for actual object providing implementation of TransactionPool.
///
/// This wraps actual implementation of the TransactionPool, e.g. fork-aware or single-state.
pub struct TransactionPoolWrapper<Block, Client>(
	pub Box<dyn FullClientTransactionPool<Block, Client>>,
)
where
	Block: BlockT,
	Client: sp_api::ProvideRuntimeApi<Block>
		+ sc_client_api::BlockBackend<Block>
		+ sc_client_api::blockchain::HeaderBackend<Block>
		+ sp_runtime::traits::BlockIdTo<Block>
		+ sp_blockchain::HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ 'static,
	Client::Api: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>;

impl<Block, Client> TransactionPool for TransactionPoolWrapper<Block, Client>
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
	type Block = Block;
	type Hash = ExtrinsicHash<FullChainApi<Client, Block>>;
	type InPoolTransaction = Transaction<
		ExtrinsicHash<FullChainApi<Client, Block>>,
		ExtrinsicFor<FullChainApi<Client, Block>>,
	>;
	type Error = <FullChainApi<Client, Block> as ChainApi>::Error;

	fn submit_at(
		&self,
		at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xts: Vec<TransactionFor<Self>>,
	) -> PoolFuture<Vec<Result<TxHash<Self>, Self::Error>>, Self::Error> {
		self.0.submit_at(at, source, xts)
	}

	fn submit_one(
		&self,
		at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xt: TransactionFor<Self>,
	) -> PoolFuture<TxHash<Self>, Self::Error> {
		self.0.submit_one(at, source, xt)
	}

	fn submit_and_watch(
		&self,
		at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xt: TransactionFor<Self>,
	) -> PoolFuture<Pin<Box<TransactionStatusStreamFor<Self>>>, Self::Error> {
		self.0.submit_and_watch(at, source, xt)
	}

	fn ready_at(
		&self,
		at: <Self::Block as BlockT>::Hash,
	) -> Pin<
		Box<
			dyn Future<
					Output = Box<dyn ReadyTransactions<Item = Arc<Self::InPoolTransaction>> + Send>,
				> + Send,
		>,
	> {
		self.0.ready_at(at)
	}

	fn ready(&self) -> Box<dyn ReadyTransactions<Item = Arc<Self::InPoolTransaction>> + Send> {
		self.0.ready()
	}

	fn remove_invalid(&self, hashes: &[TxHash<Self>]) -> Vec<Arc<Self::InPoolTransaction>> {
		self.0.remove_invalid(hashes)
	}

	fn futures(&self) -> Vec<Self::InPoolTransaction> {
		self.0.futures()
	}

	fn status(&self) -> PoolStatus {
		self.0.status()
	}

	fn import_notification_stream(&self) -> ImportNotificationStream<TxHash<Self>> {
		self.0.import_notification_stream()
	}

	fn on_broadcasted(&self, propagations: HashMap<TxHash<Self>, Vec<String>>) {
		self.0.on_broadcasted(propagations)
	}

	fn hash_of(&self, xt: &TransactionFor<Self>) -> TxHash<Self> {
		self.0.hash_of(xt)
	}

	fn ready_transaction(&self, hash: &TxHash<Self>) -> Option<Arc<Self::InPoolTransaction>> {
		self.0.ready_transaction(hash)
	}

	fn ready_at_with_timeout(
		&self,
		at: <Self::Block as BlockT>::Hash,
		timeout: std::time::Duration,
	) -> Pin<
		Box<
			dyn Future<
					Output = Box<dyn ReadyTransactions<Item = Arc<Self::InPoolTransaction>> + Send>,
				> + Send
				+ '_,
		>,
	> {
		self.0.ready_at_with_timeout(at, timeout)
	}
}

#[async_trait]
impl<Block, Client> MaintainedTransactionPool for TransactionPoolWrapper<Block, Client>
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
	async fn maintain(&self, event: ChainEvent<Self::Block>) {
		self.0.maintain(event).await;
	}
}

impl<Block, Client> LocalTransactionPool for TransactionPoolWrapper<Block, Client>
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
	type Block = Block;
	type Hash = ExtrinsicHash<FullChainApi<Client, Block>>;
	type Error = <FullChainApi<Client, Block> as ChainApi>::Error;

	fn submit_local(
		&self,
		at: <Self::Block as BlockT>::Hash,
		xt: LocalTransactionFor<Self>,
	) -> Result<Self::Hash, Self::Error> {
		self.0.submit_local(at, xt)
	}
}
