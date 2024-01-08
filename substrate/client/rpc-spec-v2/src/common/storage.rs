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

use sc_client_api::{Backend, ChildInfo, StorageKey, StorageProvider};
use sp_runtime::traits::Block as BlockT;

use super::events::{StorageResult, StorageResultType};
use crate::hex_string;

/// Call into the storage of blocks.
pub struct Storage<Client, Block, BE> {
	/// Substrate client.
	client: Arc<Client>,
	_phandom: PhantomData<(BE, Block)>,
}

impl<Client, Block, BE> Storage<Client, Block, BE> {
	/// Constructs a new [`Storage`].
	pub fn new(client: Arc<Client>) -> Self {
		Self { client, _phandom: PhantomData }
	}
}

/// Query to iterate over storage.
pub struct QueryIter {
	/// The key from which the iteration was started.
	pub query_key: StorageKey,
	/// The key after which pagination should resume.
	pub pagination_start_key: Option<StorageKey>,
	/// The type of the query (either value or hash).
	pub ty: IterQueryType,
}

/// The query type of an iteration.
pub enum IterQueryType {
	/// Iterating over (key, value) pairs.
	Value,
	/// Iterating over (key, hash) pairs.
	Hash,
}

/// The result of making a query call.
pub type QueryResult = Result<Option<StorageResult>, String>;

/// The result of iterating over keys.
pub type QueryIterResult = Result<(Vec<StorageResult>, Option<QueryIter>), String>;

impl<Client, Block, BE> Storage<Client, Block, BE>
where
	Block: BlockT + 'static,
	BE: Backend<Block> + 'static,
	Client: StorageProvider<Block, BE> + 'static,
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
		let result = if let Some(child_key) = child_key {
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

	/// Iterate over at most the provided number of keys.
	///
	/// Returns the storage result with a potential next key to resume iteration.
	pub fn query_iter_pagination(
		&self,
		query: QueryIter,
		hash: Block::Hash,
		child_key: Option<&ChildInfo>,
		count: usize,
	) -> QueryIterResult {
		let QueryIter { ty, query_key, pagination_start_key } = query;

		let mut keys_iter = if let Some(child_key) = child_key {
			self.client.child_storage_keys(
				hash,
				child_key.to_owned(),
				Some(&query_key),
				pagination_start_key.as_ref(),
			)
		} else {
			self.client.storage_keys(hash, Some(&query_key), pagination_start_key.as_ref())
		}
		.map_err(|err| err.to_string())?;

		let mut ret = Vec::with_capacity(count);
		let mut next_pagination_key = None;
		for _ in 0..count {
			let Some(key) = keys_iter.next() else { break };

			next_pagination_key = Some(key.clone());

			let result = match ty {
				IterQueryType::Value => self.query_value(hash, &key, child_key),
				IterQueryType::Hash => self.query_hash(hash, &key, child_key),
			}?;

			if let Some(value) = result {
				ret.push(value);
			}
		}

		// Save the next key if any to continue the iteration.
		let maybe_next_query = keys_iter.next().map(|_| QueryIter {
			ty,
			query_key,
			pagination_start_key: next_pagination_key,
		});
		Ok((ret, maybe_next_query))
	}
}
