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

use std::{
	collections::{hash_map::Entry, HashMap, HashSet},
	sync::Arc,
};

use itertools::Itertools;
use jsonrpsee::core::RpcResult;
use sc_client_api::{Backend, ChildInfo, StorageKey, StorageProvider};
use sp_runtime::traits::Block as BlockT;

use super::error::Error as ArchiveError;
use crate::common::{
	events::{
		ArchiveStorageDiffItem, ArchiveStorageDiffOperationType, ArchiveStorageDiffResult,
		ArchiveStorageDiffType, ArchiveStorageResult, PaginatedStorageQuery, StorageQueryType,
		StorageResult,
	},
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

/// Parse hex-encoded string parameter as raw bytes.
///
/// If the parsing fails, returns an error propagated to the RPC method.
pub fn parse_hex_param(param: String) -> Result<Vec<u8>, ArchiveError> {
	// Methods can accept empty parameters.
	if param.is_empty() {
		return Ok(Default::default())
	}

	array_bytes::hex2bytes(&param).map_err(|_| ArchiveError::InvalidParam(param))
}

#[derive(Debug, PartialEq, Clone)]
pub struct DiffDetails {
	key: StorageKey,
	return_type: ArchiveStorageDiffType,
	child_trie_key: Option<ChildInfo>,
	child_trie_key_string: Option<String>,
}

/// The type of storage query.
#[derive(Debug, PartialEq, Clone, Copy)]
enum FetchStorageType {
	/// Only fetch the value.
	Value,
	/// Only fetch the hash.
	Hash,
	/// Fetch both the value and the hash.
	Both,
}

pub struct ArchiveStorageDiff<Client, Block, BE> {
	client: Storage<Client, Block, BE>,
}

impl<Client, Block, BE> ArchiveStorageDiff<Client, Block, BE> {
	pub fn new(client: Arc<Client>) -> Self {
		Self { client: Storage::new(client) }
	}
}

impl<Client, Block, BE> ArchiveStorageDiff<Client, Block, BE>
where
	Block: BlockT + 'static,
	BE: Backend<Block> + 'static,
	Client: StorageProvider<Block, BE> + 'static,
{
	/// Deduplicate the provided items and return a list of `DiffDetails`.
	///
	/// Each list corresponds to a single child trie or the main trie.
	pub fn deduplicate_items(
		&self,
		items: Vec<ArchiveStorageDiffItem<String>>,
	) -> RpcResult<Vec<Vec<DiffDetails>>> {
		let mut deduplicated: HashMap<Option<ChildInfo>, Vec<DiffDetails>> = HashMap::new();

		for diff_item in items {
			// Ensure the provided hex keys are valid before deduplication.
			let key = StorageKey(parse_hex_param(diff_item.key)?);
			let child_trie_key_string = diff_item.child_trie_key.clone();
			let child_trie_key = diff_item
				.child_trie_key
				.map(|child_trie_key| parse_hex_param(child_trie_key))
				.transpose()?
				.map(ChildInfo::new_default_from_vec);

			let diff_item = DiffDetails {
				key,
				return_type: diff_item.return_type,
				child_trie_key: child_trie_key.clone(),
				child_trie_key_string,
			};

			match deduplicated.entry(child_trie_key.clone()) {
				Entry::Occupied(mut entry) => {
					let mut should_insert = true;

					for existing in entry.get() {
						// This points to a different return type.
						if existing.return_type != diff_item.return_type {
							continue
						}
						// Keys and return types are identical.
						if existing.key == diff_item.key {
							should_insert = false;
							break
						}
						// The current key is a longer prefix of the existing key.
						if diff_item.key.as_ref().starts_with(&existing.key.as_ref()) {
							should_insert = false;
							break
						}

						if diff_item.key.as_ref().starts_with(&existing.key.as_ref()) {
							let to_remove = existing.clone();
							entry.get_mut().retain(|item| item != &to_remove);
							break;
						}
					}

					if should_insert {
						entry.get_mut().push(diff_item);
					}
				},
				Entry::Vacant(entry) => {
					entry.insert(vec![diff_item]);
				},
			}
		}

		Ok(deduplicated.into_values().collect())
	}

	/// This calls into the database.
	fn fetch_storage(
		&self,
		hash: Block::Hash,
		key: StorageKey,
		maybe_child_trie: Option<ChildInfo>,
		ty: FetchStorageType,
	) -> RpcResult<Option<(StorageResult, Option<StorageResult>)>> {
		let convert_err = |error| ArchiveError::InvalidParam(error);

		match ty {
			FetchStorageType::Value => {
				let result = self
					.client
					.query_value(hash, &key, maybe_child_trie.as_ref())
					.map_err(convert_err)?;

				Ok(result.map(|res| (res, None)))
			},

			FetchStorageType::Hash => {
				let result = self
					.client
					.query_hash(hash, &key, maybe_child_trie.as_ref())
					.map_err(convert_err)?;

				Ok(result.map(|res| (res, None)))
			},

			FetchStorageType::Both => {
				let value = self
					.client
					.query_value(hash, &key, maybe_child_trie.as_ref())
					.map_err(convert_err)?;

				let Some(value) = value else {
					return Ok(None);
				};

				let hash = self
					.client
					.query_hash(hash, &key, maybe_child_trie.as_ref())
					.map_err(convert_err)?;

				let Some(hash) = hash else {
					return Ok(None);
				};

				Ok(Some((value, Some(hash))))
			},
		}
	}

	/// Check if the key starts with any of the provided items.
	///
	/// Returns a `FetchStorage` to indicate if the key should be fetched.
	fn starts_with(key: &StorageKey, items: &[DiffDetails]) -> Option<FetchStorageType> {
		// User has requested all keys, by default this fallbacks to fetching the value.
		if items.is_empty() {
			return Some(FetchStorageType::Value)
		}

		let mut value = false;
		let mut hash = false;

		for item in items {
			if key.as_ref().starts_with(&item.key.as_ref()) {
				match item.return_type {
					ArchiveStorageDiffType::Value => value = true,
					ArchiveStorageDiffType::Hash => hash = true,
				}
			}
		}

		match (value, hash) {
			(true, true) => Some(FetchStorageType::Both),
			(true, false) => Some(FetchStorageType::Value),
			(false, true) => Some(FetchStorageType::Hash),
			(false, false) => None,
		}
	}

	/// It is guaranteed that all entries correspond to the same child trie or main trie.
	pub fn handle_trie_queries(
		&self,
		hash: Block::Hash,
		previous_hash: Block::Hash,
		items: Vec<DiffDetails>,
	) -> RpcResult<Vec<ArchiveStorageDiffResult>> {
		let mut results = Vec::with_capacity(items.len());
		let mut keys_set = HashSet::new();

		let maybe_child_trie = items.first().map(|item| item.child_trie_key.clone()).flatten();
		let maybe_child_trie_str =
			items.first().map(|item| item.child_trie_key_string.clone()).flatten();

		let keys_iter = self
			.client
			.raw_keys_iter(hash, maybe_child_trie.clone())
			.map_err(|error| ArchiveError::InvalidParam(error))?;

		let mut previous_keys_iter = self
			.client
			.raw_keys_iter(previous_hash, maybe_child_trie.clone())
			.map_err(|error| ArchiveError::InvalidParam(error))?;

		for key in keys_iter {
			let Some(fetch_type) = Self::starts_with(&key, &items) else {
				continue;
			};

			let Some(storage_result) =
				self.fetch_storage(hash, key.clone(), maybe_child_trie.clone(), fetch_type)?
			else {
				continue
			};

			// The key is not present in the previous state.
			if !previous_keys_iter.contains(&key) {
				keys_set.insert(key.clone());

				results.push(ArchiveStorageDiffResult {
					key: storage_result.0.key.clone(),
					result: storage_result.0.result.clone(),
					operation_type: ArchiveStorageDiffOperationType::Added,
					child_trie_key: maybe_child_trie_str.clone(),
				});

				if let Some(second) = storage_result.1 {
					results.push(ArchiveStorageDiffResult {
						key: second.key.clone(),
						result: second.result.clone(),
						operation_type: ArchiveStorageDiffOperationType::Added,
						child_trie_key: maybe_child_trie_str.clone(),
					});
				}

				continue
			}

			// Report the result only if the value has changed.
			let Some(previous_storage_result) = self.fetch_storage(
				previous_hash,
				key.clone(),
				maybe_child_trie.clone(),
				fetch_type,
			)?
			else {
				continue
			};

			if storage_result.0.result != previous_storage_result.0.result {
				keys_set.insert(key.clone());

				results.push(ArchiveStorageDiffResult {
					key: storage_result.0.key.clone(),
					result: storage_result.0.result.clone(),
					operation_type: ArchiveStorageDiffOperationType::Modified,
					child_trie_key: maybe_child_trie_str.clone(),
				});

				if let Some(second) = storage_result.1 {
					results.push(ArchiveStorageDiffResult {
						key: second.key.clone(),
						result: second.result.clone(),
						operation_type: ArchiveStorageDiffOperationType::Modified,
						child_trie_key: maybe_child_trie_str.clone(),
					});
				}
			}
		}

		for previous_key in previous_keys_iter {
			let Some(fetch_type) = Self::starts_with(&previous_key, &items) else {
				continue;
			};

			let Some(previous_storage_result) = self.fetch_storage(
				previous_hash,
				previous_key.clone(),
				maybe_child_trie.clone(),
				fetch_type,
			)?
			else {
				continue
			};

			results.push(ArchiveStorageDiffResult {
				key: previous_storage_result.0.key,
				result: previous_storage_result.0.result,
				operation_type: ArchiveStorageDiffOperationType::Deleted,
				child_trie_key: maybe_child_trie_str.clone(),
			});

			if let Some(second) = previous_storage_result.1 {
				results.push(ArchiveStorageDiffResult {
					key: second.key.clone(),
					result: second.result.clone(),
					operation_type: ArchiveStorageDiffOperationType::Deleted,
					child_trie_key: maybe_child_trie_str.clone(),
				});
			}
		}

		Ok(results)
	}
}
