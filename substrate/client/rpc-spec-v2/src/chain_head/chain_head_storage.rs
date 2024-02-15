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

//! Implementation of the `chainHead_storage` method.

use std::{collections::VecDeque, marker::PhantomData, sync::Arc};

use sc_client_api::{Backend, ChildInfo, StorageKey, StorageProvider};
use sc_utils::mpsc::TracingUnboundedSender;
use sp_runtime::traits::Block as BlockT;

use crate::{
	chain_head::{
		event::{OperationError, OperationId, OperationStorageItems},
		subscription::BlockGuard,
		FollowEvent,
	},
	common::{
		events::{StorageQuery, StorageQueryType},
		storage::{IterQueryType, QueryIter, QueryIterResult, Storage},
	},
};

/// Generates the events of the `chainHead_storage` method.
pub struct ChainHeadStorage<Client, Block, BE> {
	/// Storage client.
	client: Storage<Client, Block, BE>,
	/// Queue of operations that may require pagination.
	iter_operations: VecDeque<QueryIter>,
	/// The maximum number of items reported by the `chainHead_storage` before
	/// pagination is required.
	operation_max_storage_items: usize,
	_phandom: PhantomData<(BE, Block)>,
}

impl<Client, Block, BE> ChainHeadStorage<Client, Block, BE> {
	/// Constructs a new [`ChainHeadStorage`].
	pub fn new(client: Arc<Client>, operation_max_storage_items: usize) -> Self {
		Self {
			client: Storage::new(client),
			iter_operations: VecDeque::new(),
			operation_max_storage_items,
			_phandom: PhantomData,
		}
	}
}

impl<Client, Block, BE> ChainHeadStorage<Client, Block, BE>
where
	Block: BlockT + 'static,
	BE: Backend<Block> + 'static,
	Client: StorageProvider<Block, BE> + 'static,
{
	/// Iterate over (key, hash) and (key, value) generating the `WaitingForContinue` event if
	/// necessary.
	async fn generate_storage_iter_events(
		&mut self,
		mut block_guard: BlockGuard<Block, BE>,
		hash: Block::Hash,
		child_key: Option<ChildInfo>,
	) {
		let sender = block_guard.response_sender();
		let operation = block_guard.operation();

		while let Some(query) = self.iter_operations.pop_front() {
			if operation.was_stopped() {
				return
			}

			let result = self.client.query_iter_pagination(
				query,
				hash,
				child_key.as_ref(),
				self.operation_max_storage_items,
			);
			let (events, maybe_next_query) = match result {
				QueryIterResult::Ok(result) => result,
				QueryIterResult::Err(error) => {
					send_error::<Block>(&sender, operation.operation_id(), error.to_string());
					return
				},
			};

			if !events.is_empty() {
				// Send back the results of the iteration produced so far.
				let _ = sender.unbounded_send(FollowEvent::<Block::Hash>::OperationStorageItems(
					OperationStorageItems { operation_id: operation.operation_id(), items: events },
				));
			}

			if let Some(next_query) = maybe_next_query {
				let _ =
					sender.unbounded_send(FollowEvent::<Block::Hash>::OperationWaitingForContinue(
						OperationId { operation_id: operation.operation_id() },
					));

				// The operation might be continued or cancelled only after the
				// `OperationWaitingForContinue` is generated above.
				operation.wait_for_continue().await;

				// Give a chance for the other items to advance next time.
				self.iter_operations.push_back(next_query);
			}
		}

		if operation.was_stopped() {
			return
		}

		let _ =
			sender.unbounded_send(FollowEvent::<Block::Hash>::OperationStorageDone(OperationId {
				operation_id: operation.operation_id(),
			}));
	}

	/// Generate the block events for the `chainHead_storage` method.
	pub async fn generate_events(
		&mut self,
		mut block_guard: BlockGuard<Block, BE>,
		hash: Block::Hash,
		items: Vec<StorageQuery<StorageKey>>,
		child_key: Option<ChildInfo>,
	) {
		let sender = block_guard.response_sender();
		let operation = block_guard.operation();

		let mut storage_results = Vec::with_capacity(items.len());
		for item in items {
			match item.query_type {
				StorageQueryType::Value => {
					match self.client.query_value(hash, &item.key, child_key.as_ref()) {
						Ok(Some(value)) => storage_results.push(value),
						Ok(None) => continue,
						Err(error) => {
							send_error::<Block>(&sender, operation.operation_id(), error);
							return
						},
					}
				},
				StorageQueryType::Hash =>
					match self.client.query_hash(hash, &item.key, child_key.as_ref()) {
						Ok(Some(value)) => storage_results.push(value),
						Ok(None) => continue,
						Err(error) => {
							send_error::<Block>(&sender, operation.operation_id(), error);
							return
						},
					},
				StorageQueryType::ClosestDescendantMerkleValue =>
					match self.client.query_merkle_value(hash, &item.key, child_key.as_ref()) {
						Ok(Some(value)) => storage_results.push(value),
						Ok(None) => continue,
						Err(error) => {
							send_error::<Block>(&sender, operation.operation_id(), error);
							return
						},
					},
				StorageQueryType::DescendantsValues => self.iter_operations.push_back(QueryIter {
					query_key: item.key,
					ty: IterQueryType::Value,
					pagination_start_key: None,
				}),
				StorageQueryType::DescendantsHashes => self.iter_operations.push_back(QueryIter {
					query_key: item.key,
					ty: IterQueryType::Hash,
					pagination_start_key: None,
				}),
			};
		}

		if !storage_results.is_empty() {
			let _ = sender.unbounded_send(FollowEvent::<Block::Hash>::OperationStorageItems(
				OperationStorageItems {
					operation_id: operation.operation_id(),
					items: storage_results,
				},
			));
		}

		self.generate_storage_iter_events(block_guard, hash, child_key).await
	}
}

/// Build and send the opaque error back to the `chainHead_follow` method.
fn send_error<Block: BlockT>(
	sender: &TracingUnboundedSender<FollowEvent<Block::Hash>>,
	operation_id: String,
	error: String,
) {
	let _ = sender.unbounded_send(FollowEvent::<Block::Hash>::OperationError(OperationError {
		operation_id,
		error,
	}));
}
