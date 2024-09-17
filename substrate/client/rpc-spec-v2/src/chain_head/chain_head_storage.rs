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

use std::{marker::PhantomData, sync::Arc};

use futures::SinkExt;
use sc_client_api::{Backend, ChildInfo, StorageKey, StorageProvider};
use sc_rpc::SubscriptionTaskExecutor;
use sp_runtime::traits::Block as BlockT;
use tokio::sync::mpsc;

use crate::{
	chain_head::{
		chain_head::LOG_TARGET,
		event::{OperationError, OperationId, OperationStorageItems},
		subscription::{BlockGuard, StopHandle},
		FollowEvent, FollowEventSendError, FollowEventSender,
	},
	common::{
		events::{StorageQuery, StorageQueryType},
		storage::{IterQueryType, QueryIter, QueryResult, Storage},
	},
};

/// Generates the events of the `chainHead_storage` method.
pub struct ChainHeadStorage<Client, Block, BE> {
	/// Storage client.
	client: Storage<Client, Block, BE>,
	_phandom: PhantomData<(BE, Block)>,
}

impl<Client, Block, BE> ChainHeadStorage<Client, Block, BE> {
	/// Constructs a new [`ChainHeadStorage`].
	pub fn new(client: Arc<Client>, executor: SubscriptionTaskExecutor) -> Self {
		Self { client: Storage::new(client, executor), _phandom: PhantomData }
	}
}

impl<Client, Block, BE> ChainHeadStorage<Client, Block, BE>
where
	Block: BlockT + Send + 'static,
	BE: Backend<Block> + Send + 'static,
	Client: StorageProvider<Block, BE> + Send + Sync + 'static,
{
	/// Iterate over (key, hash) and (key, value) generating the `WaitingForContinue` event if
	/// necessary.
	async fn generate_storage_iter_events(
		&self,
		query_iter: Vec<QueryIter>,
		mut block_guard: BlockGuard<Block, BE>,
		hash: Block::Hash,
		child_key: Option<ChildInfo>,
	) -> Result<(), FollowEventSendError> {
		let stop_handle = block_guard.operation().stop_handle().clone();

		if stop_handle.is_stopped() {
			return Ok(());
		}

		if !query_iter.is_empty() {
			process_storage_iter_stream(
				self.client.query_iter_pagination(query_iter, hash, child_key),
				block_guard.response_sender(),
				block_guard.operation().operation_id().to_owned(),
				&stop_handle,
			)
			.await?;
		}

		if !stop_handle.is_stopped() {
			block_guard
				.response_sender()
				.send(FollowEvent::OperationStorageDone(OperationId {
					operation_id: block_guard.operation().operation_id().to_owned(),
				}))
				.await?;
		}

		Ok(())
	}

	/// Generate the block events for the `chainHead_storage` method.
	pub async fn generate_events(
		&mut self,
		mut block_guard: BlockGuard<Block, BE>,
		hash: Block::Hash,
		items: Vec<StorageQuery<StorageKey>>,
		child_key: Option<ChildInfo>,
	) -> Result<(), FollowEventSendError> {
		let mut iter_ops = Vec::new();
		let mut sender = block_guard.response_sender();
		let operation = block_guard.operation();

		let mut storage_results = Vec::with_capacity(items.len());
		for item in items {
			match item.query_type {
				StorageQueryType::Value => {
					match self.client.query_value(hash, &item.key, child_key.as_ref()) {
						Ok(Some(value)) => storage_results.push(value),
						Ok(None) => continue,
						Err(error) => {
							return send_error(&mut sender, operation.operation_id(), error).await;
						},
					}
				},
				StorageQueryType::Hash =>
					match self.client.query_hash(hash, &item.key, child_key.as_ref()) {
						Ok(Some(value)) => storage_results.push(value),
						Ok(None) => continue,
						Err(error) => {
							return send_error(&mut sender, operation.operation_id(), error).await;
						},
					},
				StorageQueryType::ClosestDescendantMerkleValue =>
					match self.client.query_merkle_value(hash, &item.key, child_key.as_ref()) {
						Ok(Some(value)) => storage_results.push(value),
						Ok(None) => continue,
						Err(error) => {
							return send_error(&mut sender, operation.operation_id(), error).await;
						},
					},
				StorageQueryType::DescendantsValues => iter_ops.push(QueryIter {
					query_key: item.key,
					ty: IterQueryType::Value,
					pagination_start_key: None,
				}),
				StorageQueryType::DescendantsHashes => iter_ops.push(QueryIter {
					query_key: item.key,
					ty: IterQueryType::Hash,
					pagination_start_key: None,
				}),
			};
		}

		if !storage_results.is_empty() {
			sender
				.send(FollowEvent::<Block::Hash>::OperationStorageItems(OperationStorageItems {
					operation_id: operation.operation_id(),
					items: storage_results,
				}))
				.await?;
		}

		self.generate_storage_iter_events(iter_ops, block_guard, hash, child_key).await
	}
}

/// Build and send the opaque error back to the `chainHead_follow` method.
async fn send_error<Hash>(
	sender: &mut FollowEventSender<Hash>,
	operation_id: String,
	error: String,
) -> Result<(), FollowEventSendError> {
	sender
		.send(FollowEvent::OperationError(OperationError { operation_id, error }))
		.await
}

async fn process_storage_iter_stream<Hash>(
	mut storage_query_stream: mpsc::Receiver<QueryResult>,
	mut sender: FollowEventSender<Hash>,
	operation_id: String,
	stop_handle: &StopHandle,
) -> Result<(), FollowEventSendError> {
	let mut buf = Vec::new();

	loop {
		tokio::select! {
			_ = stop_handle.stopped() => return Ok(()),
			len = storage_query_stream.recv_many(&mut buf, 1024) => {
				if len == 0 {
					break Ok(());
				}

				let mut items = Vec::with_capacity(buf.len());

				for val in buf.drain(..) {
					match val {
						QueryResult::Err(error) =>
							return send_error(&mut sender, operation_id.clone(), error).await,
						QueryResult::Ok(Some(v)) => {
							items.push(v)
						},
						QueryResult::Ok(None) => continue
					}
				}

				// Send back the results of the iteration produced so far.
				sender
					.send(FollowEvent::OperationStorageItems(OperationStorageItems {
						operation_id: operation_id.clone(),
						items,
				})).await?;

			},
		}
	}
}
