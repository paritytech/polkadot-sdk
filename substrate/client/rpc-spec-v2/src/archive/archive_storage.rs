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

//! Implementation of the `archive_storage` method.

use std::sync::Arc;

use sc_client_api::{Backend, ChildInfo, StorageKey, StorageProvider};
use sp_runtime::traits::Block as BlockT;

use crate::common::{
	events::{ArchiveStorageResult, PaginatedStorageQuery, StorageQueryType},
	storage::{IterQueryType, QueryIter, Storage},
};

/// Generates the events of the `archive_storage` method.
pub struct ArchiveStorage<Client, Block, BE> {
	/// Storage client.
	client: Storage<Client, Block, BE>,
	/// The maximum number of responses the API can return for a descendant query at a time.
	storage_max_descendant_responses: usize,
	/// The maximum number of queried items allowed for the `archive_storage` at a time.
	storage_max_queried_items: usize,
}

impl<Client, Block, BE> ArchiveStorage<Client, Block, BE> {
	/// Constructs a new [`ArchiveStorage`].
	pub fn new(
		client: Arc<Client>,
		storage_max_descendant_responses: usize,
		storage_max_queried_items: usize,
	) -> Self {
		Self {
			client: Storage::new(client),
			storage_max_descendant_responses,
			storage_max_queried_items,
		}
	}
}

impl<Client, Block, BE> ArchiveStorage<Client, Block, BE>
where
	Block: BlockT + 'static,
	BE: Backend<Block> + 'static,
	Client: StorageProvider<Block, BE> + 'static,
{
	/// Generate the response of the `archive_storage` method.
	pub fn handle_query(
		&self,
		hash: Block::Hash,
		mut items: Vec<PaginatedStorageQuery<StorageKey>>,
		child_key: Option<ChildInfo>,
	) -> ArchiveStorageResult {
		let discarded_items = items.len().saturating_sub(self.storage_max_queried_items);
		items.truncate(self.storage_max_queried_items);

		let mut storage_results = Vec::with_capacity(items.len());
		for item in items {
			match item.query_type {
				StorageQueryType::Value => {
					match self.client.query_value(hash, &item.key, child_key.as_ref()) {
						Ok(Some(value)) => storage_results.push(value),
						Ok(None) => continue,
						Err(error) => return ArchiveStorageResult::err(error),
					}
				},
				StorageQueryType::Hash =>
					match self.client.query_hash(hash, &item.key, child_key.as_ref()) {
						Ok(Some(value)) => storage_results.push(value),
						Ok(None) => continue,
						Err(error) => return ArchiveStorageResult::err(error),
					},
				StorageQueryType::ClosestDescendantMerkleValue =>
					match self.client.query_merkle_value(hash, &item.key, child_key.as_ref()) {
						Ok(Some(value)) => storage_results.push(value),
						Ok(None) => continue,
						Err(error) => return ArchiveStorageResult::err(error),
					},
				StorageQueryType::DescendantsValues => {
					match self.client.query_iter_pagination(
						QueryIter {
							query_key: item.key,
							ty: IterQueryType::Value,
							pagination_start_key: item.pagination_start_key,
						},
						hash,
						child_key.as_ref(),
						self.storage_max_descendant_responses,
					) {
						Ok((results, _)) => storage_results.extend(results),
						Err(error) => return ArchiveStorageResult::err(error),
					}
				},
				StorageQueryType::DescendantsHashes => {
					match self.client.query_iter_pagination(
						QueryIter {
							query_key: item.key,
							ty: IterQueryType::Hash,
							pagination_start_key: item.pagination_start_key,
						},
						hash,
						child_key.as_ref(),
						self.storage_max_descendant_responses,
					) {
						Ok((results, _)) => storage_results.extend(results),
						Err(error) => return ArchiveStorageResult::err(error),
					}
				},
			};
		}

		ArchiveStorageResult::ok(storage_results, discarded_items)
	}
}
