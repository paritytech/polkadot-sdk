// Copyright 2018-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Chain api required for the transaction pool.

use std::{marker::PhantomData, pin::Pin, sync::Arc};
use codec::{Decode, Encode};
use futures::{
	channel::oneshot, executor::{ThreadPool, ThreadPoolBuilder}, future::{Future, FutureExt, ready, Ready},
};

use sc_client_api::{
	blockchain::HeaderBackend,
	light::{Fetcher, RemoteCallRequest, RemoteBodyRequest},
	BlockBody,
};
use sp_runtime::{
	generic::BlockId, traits::{self, Block as BlockT, BlockIdTo, Header as HeaderT, Hash as HashT},
	transaction_validity::TransactionValidity,
};
use sp_transaction_pool::runtime_api::TaggedTransactionQueue;
use sp_api::ProvideRuntimeApi;

use crate::error::{self, Error};

/// The transaction pool logic for full client.
pub struct FullChainApi<Client, Block> {
	client: Arc<Client>,
	pool: ThreadPool,
	_marker: PhantomData<Block>,
}

impl<Client, Block> FullChainApi<Client, Block> where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + BlockIdTo<Block>,
{
	/// Create new transaction pool logic.
	pub fn new(client: Arc<Client>) -> Self {
		FullChainApi {
			client,
			pool: ThreadPoolBuilder::new()
				.pool_size(2)
				.name_prefix("txpool-verifier")
				.create()
				.expect("Failed to spawn verifier threads, that are critical for node operation."),
			_marker: Default::default()
		}
	}
}

impl<Client, Block> sc_transaction_graph::ChainApi for FullChainApi<Client, Block> where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + BlockBody<Block> + BlockIdTo<Block>,
	Client: Send + Sync + 'static,
	Client::Api: TaggedTransactionQueue<Block>,
	sp_api::ApiErrorFor<Client, Block>: Send,
{
	type Block = Block;
	type Hash = Block::Hash;
	type Error = error::Error;
	type ValidationFuture = Pin<Box<dyn Future<Output = error::Result<TransactionValidity>> + Send>>;
	type BodyFuture = Ready<error::Result<Option<Vec<<Self::Block as BlockT>::Extrinsic>>>>;

	fn block_body(&self, id: &BlockId<Self::Block>) -> Self::BodyFuture {
		ready(self.client.block_body(&id).map_err(|e| error::Error::from(e)))
	}

	fn validate_transaction(
		&self,
		at: &BlockId<Self::Block>,
		uxt: sc_transaction_graph::ExtrinsicFor<Self>,
	) -> Self::ValidationFuture {
		let (tx, rx) = oneshot::channel();
		let client = self.client.clone();
		let at = at.clone();

		self.pool.spawn_ok(futures_diagnose::diagnose("validate-transaction", async move {
			let res = client.runtime_api().validate_transaction(&at, uxt)
				.map_err(|e| Error::RuntimeApi(format!("{:?}", e)));
			if let Err(e) = tx.send(res) {
				log::warn!("Unable to send a validate transaction result: {:?}", e);
			}
		}));

		Box::pin(async move {
			match rx.await {
				Ok(r) => r,
				Err(_) => Err(Error::RuntimeApi("Validation was canceled".into())),
			}
		})
	}

	fn block_id_to_number(
		&self,
		at: &BlockId<Self::Block>,
	) -> error::Result<Option<sc_transaction_graph::NumberFor<Self>>> {
		self.client.to_number(at).map_err(|e| Error::BlockIdConversion(format!("{:?}", e)))
	}

	fn block_id_to_hash(
		&self,
		at: &BlockId<Self::Block>,
	) -> error::Result<Option<sc_transaction_graph::BlockHash<Self>>> {
		self.client.to_hash(at).map_err(|e| Error::BlockIdConversion(format!("{:?}", e)))
	}

	fn hash_and_length(&self, ex: &sc_transaction_graph::ExtrinsicFor<Self>) -> (Self::Hash, usize) {
		ex.using_encoded(|x| {
			(<traits::HashFor::<Block> as traits::Hash>::hash(x), x.len())
		})
	}
}

/// The transaction pool logic for light client.
pub struct LightChainApi<Client, F, Block> {
	client: Arc<Client>,
	fetcher: Arc<F>,
	_phantom: PhantomData<Block>,
}

impl<Client, F, Block> LightChainApi<Client, F, Block> where
	Block: BlockT,
	Client: HeaderBackend<Block>,
	F: Fetcher<Block>,
{
	/// Create new transaction pool logic.
	pub fn new(client: Arc<Client>, fetcher: Arc<F>) -> Self {
		LightChainApi {
			client,
			fetcher,
			_phantom: Default::default(),
		}
	}
}

impl<Client, F, Block> sc_transaction_graph::ChainApi for LightChainApi<Client, F, Block> where
	Block: BlockT,
	Client: HeaderBackend<Block> + 'static,
	F: Fetcher<Block> + 'static,
{
	type Block = Block;
	type Hash = Block::Hash;
	type Error = error::Error;
	type ValidationFuture = Box<dyn Future<Output = error::Result<TransactionValidity>> + Send + Unpin>;
	type BodyFuture = Pin<Box<dyn Future<Output = error::Result<Option<Vec<<Self::Block as BlockT>::Extrinsic>>>> + Send>>;

	fn validate_transaction(
		&self,
		at: &BlockId<Self::Block>,
		uxt: sc_transaction_graph::ExtrinsicFor<Self>,
	) -> Self::ValidationFuture {
		let header_hash = self.client.expect_block_hash_from_id(at);
		let header_and_hash = header_hash
			.and_then(|header_hash| self.client.expect_header(BlockId::Hash(header_hash))
				.map(|header| (header_hash, header)));
		let (block, header) = match header_and_hash {
			Ok((header_hash, header)) => (header_hash, header),
			Err(err) => return Box::new(ready(Err(err.into()))),
		};
		let remote_validation_request = self.fetcher.remote_call(RemoteCallRequest {
			block,
			header,
			method: "TaggedTransactionQueue_validate_transaction".into(),
			call_data: uxt.encode(),
			retry_count: None,
		});
		let remote_validation_request = remote_validation_request.then(move |result| {
			let result: error::Result<TransactionValidity> = result
				.map_err(Into::into)
				.and_then(|result| Decode::decode(&mut &result[..])
					.map_err(|e| Error::RuntimeApi(
						format!("Error decoding tx validation result: {:?}", e)
					))
				);
			ready(result)
		});

		Box::new(remote_validation_request)
	}

	fn block_id_to_number(&self, at: &BlockId<Self::Block>) -> error::Result<Option<sc_transaction_graph::NumberFor<Self>>> {
		Ok(self.client.block_number_from_id(at)?)
	}

	fn block_id_to_hash(&self, at: &BlockId<Self::Block>) -> error::Result<Option<sc_transaction_graph::BlockHash<Self>>> {
		Ok(self.client.block_hash_from_id(at)?)
	}

	fn hash_and_length(&self, ex: &sc_transaction_graph::ExtrinsicFor<Self>) -> (Self::Hash, usize) {
		ex.using_encoded(|x| {
			(<<Block::Header as HeaderT>::Hashing as HashT>::hash(x), x.len())
		})
	}

	fn block_body(&self, id: &BlockId<Self::Block>) -> Self::BodyFuture {
		let header = self.client.header(*id)
			.and_then(|h| h.ok_or(sp_blockchain::Error::UnknownBlock(format!("{}", id))));
		let header = match header {
			Ok(header) => header,
			Err(err) => {
				log::warn!(target: "txpool", "Failed to query header: {:?}", err);
				return Box::pin(ready(Ok(None)));
			}
		};

		let fetcher = self.fetcher.clone();
		async move {
			let transactions = fetcher.remote_body({
					RemoteBodyRequest {
						header,
						retry_count: None,
					}
				})
				.await
				.unwrap_or_else(|e| {
					log::warn!(target: "txpool", "Failed to fetch block body: {:?}", e);
					Vec::new()
				});

			Ok(Some(transactions))
		}.boxed()
	}
}
