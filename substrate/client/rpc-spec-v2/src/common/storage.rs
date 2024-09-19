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

//! Storage queries for the RPC-V2 spec.

use std::{marker::PhantomData, sync::Arc};

use futures::{stream::FuturesUnordered, FutureExt, StreamExt};
use sc_client_api::{Backend, ChildInfo, StorageKey, StorageProvider};
use sc_rpc::SubscriptionTaskExecutor;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use tokio::sync::mpsc;

use super::events::{StorageResult, StorageResultType};
use crate::hex_string;

/// The buffer capacity for `Storage::query_iter_pagination`.
///
/// This is small because the underlying JSON-RPC server has
/// its down buffer capacity per connection as well.
const QUERY_ITER_PAGINATED_BUF_CAP: usize = 16;

/// Call into the storage of blocks.
pub struct Storage<Client, Block, BE> {
	/// Substrate client.
	client: Arc<Client>,
	executor: SubscriptionTaskExecutor,
	_phandom: PhantomData<(BE, Block)>,
}

impl<Client, Block, BE> Clone for Storage<Client, Block, BE> {
	fn clone(&self) -> Self {
		Self { client: self.client.clone(), executor: self.executor.clone(), _phandom: PhantomData }
	}
}

impl<Client, Block, BE> Storage<Client, Block, BE> {
	/// Constructs a new [`Storage`].
	pub fn new(client: Arc<Client>, executor: SubscriptionTaskExecutor) -> Self {
		Self { client, _phandom: PhantomData, executor }
	}
}

/// Query to iterate over storage.
#[derive(Debug)]
pub struct QueryIter {
	/// The key from which the iteration was started.
	pub query_key: StorageKey,
	/// The key after which pagination should resume.
	pub pagination_start_key: Option<StorageKey>,
	/// The type of the query (either value or hash).
	pub ty: IterQueryType,
}

/// The query type of an iteration.
#[derive(Debug)]
pub enum IterQueryType {
	/// Iterating over (key, value) pairs.
	Value,
	/// Iterating over (key, hash) pairs.
	Hash,
}

/// The result of making a query call.
pub type QueryResult = Result<Option<StorageResult>, String>;

impl<Client, Block, BE> Storage<Client, Block, BE>
where
	Block: BlockT + Send + 'static,
	BE: Backend<Block> + Send + 'static,
	Client: StorageProvider<Block, BE> + Send + Sync + 'static,
	<<BE as sc_client_api::Backend<Block>>::State as sc_client_api::StateBackend<<<Block as BlockT>::Header as HeaderT>::Hashing>>::RawIter: Send,
{
	/// Fetch the value from storage.
	pub fn query_value(
		&self,
		hash: Block::Hash,
		key: &StorageKey,
		child_key: Option<&ChildInfo>,
	) -> QueryResult {
		let result = if let Some(child_key) = child_key {
			self.client.child_storage(hash, child_key, key)
		} else {
			self.client.storage(hash, key)
		};

		result
			.map(|opt| {
				QueryResult::Ok(opt.map(|storage_data| StorageResult {
					key: hex_string(&key.0),
					result: StorageResultType::Value(hex_string(&storage_data.0)),
				}))
			})
			.unwrap_or_else(|error| QueryResult::Err(error.to_string()))
	}

	/// Fetch the hash of a value from storage.
	pub fn query_hash(
		&self,
		hash: Block::Hash,
		key: &StorageKey,
		child_key: Option<&ChildInfo>,
	) -> QueryResult {
		let result = if let Some(child_key) = child_key {
			self.client.child_storage_hash(hash, child_key, key)
		} else {
			self.client.storage_hash(hash, key)
		};

		result
			.map(|opt| {
				QueryResult::Ok(opt.map(|storage_data| StorageResult {
					key: hex_string(&key.0),
					result: StorageResultType::Hash(hex_string(&storage_data.as_ref())),
				}))
			})
			.unwrap_or_else(|error| QueryResult::Err(error.to_string()))
	}

	/// Fetch the closest merkle value.
	pub fn query_merkle_value(
		&self,
		hash: Block::Hash,
		key: &StorageKey,
		child_key: Option<&ChildInfo>,
	) -> QueryResult {
		let result = if let Some(ref child_key) = child_key {
			self.client.child_closest_merkle_value(hash, child_key, key)
		} else {
			self.client.closest_merkle_value(hash, key)
		};

		result
			.map(|opt| {
				QueryResult::Ok(opt.map(|storage_data| {
					let result = match &storage_data {
						sc_client_api::MerkleValue::Node(data) => hex_string(&data.as_slice()),
						sc_client_api::MerkleValue::Hash(hash) => hex_string(&hash.as_ref()),
					};

					StorageResult {
						key: hex_string(&key.0),
						result: StorageResultType::ClosestDescendantMerkleValue(result),
					}
				}))
			})
			.unwrap_or_else(|error| QueryResult::Err(error.to_string()))
	}

	/// Iterate over the storage which returns a stream that receive the results of the
	/// query.
	///
	/// Internally this relies on a bounded channel which provides backpressure which needs
	/// propagated down the underlying client.
	///
	/// For users of this API, if you can't rely on backpressure then you can
	/// use `max_iterations` to limit the number of iterations.
	pub fn query_iter_pagination(
		&self,
		queries: Vec<QueryIter>,
		hash: Block::Hash,
		child_key: Option<ChildInfo>,
		max_iterations: Option<usize>,
	) -> mpsc::Receiver<QueryResult> {
		let (tx, rx) = mpsc::channel(QUERY_ITER_PAGINATED_BUF_CAP);
		let storage = self.clone();

		let pending_queries = async move {
			let queries: FuturesUnordered<_> = queries
				.into_iter()
				.map(|query| {
					query_iter_pagination_one(
						&storage,
						query,
						hash,
						child_key.as_ref(),
						&tx,
						max_iterations,
					)
				})
				.collect();

			queries.for_each(|_| async {}).await;
		};

		self.executor.spawn_blocking(
			"substrate-rpc-subscription",
			Some("rpc"),
			Box::pin(pending_queries),
		);

		rx
	}
}

async fn query_iter_pagination_one<Client, Block, BE>(
	storage: &Storage<Client, Block, BE>,
	query: QueryIter,
	hash: Block::Hash,
	child_key: Option<&ChildInfo>,
	tx: &mpsc::Sender<QueryResult>,
	max_iterations: Option<usize>,
) where
	Block: BlockT + Send + 'static,
	BE: Backend<Block> + Send + 'static,
	Client: StorageProvider<Block, BE> + Send + Sync + 'static,
	<<BE as sc_client_api::Backend<Block>>::State as sc_client_api::StateBackend<<<Block as BlockT>::Header as HeaderT>::Hashing>>::RawIter: std::marker::Send,

{
	let QueryIter { ty, query_key, pagination_start_key } = query;

	let maybe_storage = if let Some(child_key) = child_key {
		storage.client.child_storage_keys(
			hash,
			child_key.to_owned(),
			Some(&query_key),
			pagination_start_key.as_ref(),
		)
	} else {
		storage
			.client
			.storage_keys(hash, Some(&query_key), pagination_start_key.as_ref())
	};

	let keys_iter = match maybe_storage {
		Ok(keys_iter) => keys_iter,
		Err(error) => {
			_ = tx.send(Err(error.to_string())).await;
			return;
		},
	};

	for (count, key) in keys_iter.into_iter().enumerate() {
		if let Some(max_iterations) = max_iterations {
			if count >= max_iterations {
				break;
			}
		}

		let result = match ty {
			IterQueryType::Value => storage.query_value(hash, &key, child_key),
			IterQueryType::Hash => storage.query_hash(hash, &key, child_key),
		};

		if tx.send(result).await.is_err() {
			break;
		}
	}
}
