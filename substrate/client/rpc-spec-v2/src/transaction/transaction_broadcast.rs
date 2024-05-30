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

//! API implementation for broadcasting transactions.

use crate::{
	common::connections::RpcConnections, transaction::api::TransactionBroadcastApiServer,
	SubscriptionTaskExecutor,
};
use codec::Decode;
use futures::{FutureExt, Stream, StreamExt};
use futures_util::stream::AbortHandle;
use jsonrpsee::{
	core::{async_trait, RpcResult},
	ConnectionDetails,
};
use parking_lot::RwLock;
use rand::{distributions::Alphanumeric, Rng};
use sc_client_api::BlockchainEvents;
use sc_transaction_pool_api::{
	error::IntoPoolError, TransactionFor, TransactionPool, TransactionSource,
};
use sp_blockchain::HeaderBackend;
use sp_core::Bytes;
use sp_runtime::traits::Block as BlockT;
use std::{collections::HashMap, sync::Arc};

use super::error::ErrorBroadcast;

/// An API for transaction RPC calls.
pub struct TransactionBroadcast<Pool: TransactionPool, Client> {
	/// Substrate client.
	client: Arc<Client>,
	/// Transactions pool.
	pool: Arc<Pool>,
	/// Executor to spawn subscriptions.
	executor: SubscriptionTaskExecutor,
	/// The broadcast operation IDs.
	broadcast_ids: Arc<RwLock<HashMap<String, BroadcastState<Pool>>>>,
	/// Keep track of how many concurrent operations are active for each connection.
	rpc_connections: RpcConnections,
}

/// The state of a broadcast operation.
struct BroadcastState<Pool: TransactionPool> {
	/// Handle to abort the running future that broadcasts the transaction.
	handle: AbortHandle,
	/// Associated tx hash.
	tx_hash: <Pool as TransactionPool>::Hash,
}

impl<Pool: TransactionPool, Client> TransactionBroadcast<Pool, Client> {
	/// Creates a new [`TransactionBroadcast`].
	pub fn new(
		client: Arc<Client>,
		pool: Arc<Pool>,
		executor: SubscriptionTaskExecutor,
		max_transactions_per_connection: usize,
	) -> Self {
		TransactionBroadcast {
			client,
			pool,
			executor,
			broadcast_ids: Default::default(),
			rpc_connections: RpcConnections::new(max_transactions_per_connection),
		}
	}

	/// Generate an unique operation ID for the `transaction_broadcast` RPC method.
	pub fn generate_unique_id(&self) -> String {
		let generate_operation_id = || {
			// The length of the operation ID.
			const OPERATION_ID_LEN: usize = 16;

			rand::thread_rng()
				.sample_iter(Alphanumeric)
				.take(OPERATION_ID_LEN)
				.map(char::from)
				.collect::<String>()
		};

		let mut id = generate_operation_id();

		let broadcast_ids = self.broadcast_ids.read();

		while broadcast_ids.contains_key(&id) {
			id = generate_operation_id();
		}

		id
	}
}

/// Currently we treat all RPC transactions as externals.
///
/// Possibly in the future we could allow opt-in for special treatment
/// of such transactions, so that the block authors can inject
/// some unique transactions via RPC and have them included in the pool.
const TX_SOURCE: TransactionSource = TransactionSource::External;

#[async_trait]
impl<Pool, Client> TransactionBroadcastApiServer for TransactionBroadcast<Pool, Client>
where
	Pool: TransactionPool + Sync + Send + 'static,
	Pool::Error: IntoPoolError,
	<Pool::Block as BlockT>::Hash: Unpin,
	Client: HeaderBackend<Pool::Block> + BlockchainEvents<Pool::Block> + Send + Sync + 'static,
{
	async fn broadcast(
		&self,
		connection_details: ConnectionDetails,
		bytes: Bytes,
	) -> RpcResult<Option<String>> {
		let pool = self.pool.clone();

		// The unique ID of this operation.
		let id = self.generate_unique_id();

		// Ensure that the connection has not reached the maximum number of active operations.
		let Some(reserved_connection) = self.rpc_connections.reserve_space(connection_details.id())
		else {
			return Ok(None)
		};
		let Some(reserved_identifier) = reserved_connection.register(id.clone()) else {
			// This can only happen if the generated operation ID is not unique.
			return Ok(None)
		};

		// The JSON-RPC server might check whether the transaction is valid before broadcasting it.
		// If it does so and if the transaction is invalid, the server should silently do nothing
		// and the JSON-RPC client is not informed of the problem. Invalid transactions should still
		// count towards the limit to the number of simultaneously broadcasted transactions.
		let Ok(decoded_extrinsic) = TransactionFor::<Pool>::decode(&mut &bytes[..]) else {
			return Ok(Some(id));
		};
		// Save the tx hash to remove it later.
		let tx_hash = pool.hash_of(&decoded_extrinsic);

		// The compiler can no longer deduce the type of the stream and complains
		// about `one type is more general than the other`.
		let mut best_block_import_stream: std::pin::Pin<
			Box<dyn Stream<Item = <Pool::Block as BlockT>::Hash> + Send>,
		> =
			Box::pin(self.client.import_notification_stream().filter_map(
				|notification| async move { notification.is_new_best.then_some(notification.hash) },
			));

		let broadcast_transaction_fut = async move {
			// Flag to determine if the we should broadcast the transaction again.
			let mut is_done = false;

			while !is_done {
				// Wait for the last block to become available.
				let Some(best_block_hash) =
					last_stream_element(&mut best_block_import_stream).await
				else {
					return;
				};

				let mut stream = match pool
					.submit_and_watch(best_block_hash, TX_SOURCE, decoded_extrinsic.clone())
					.await
				{
					Ok(stream) => stream,
					// The transaction was not included to the pool.
					Err(e) => {
						let Ok(pool_err) = e.into_pool_error() else { return };

						if pool_err.is_retriable() {
							// Try to resubmit the transaction at a later block for
							// recoverable errors.
							continue
						} else {
							return;
						}
					},
				};

				while let Some(event) = stream.next().await {
					// Check if the transaction could be submitted again
					// at a later time.
					if event.is_retriable() {
						break;
					}

					// Stop if this is the final event of the transaction stream
					// and the event is not retriable.
					if event.is_final() {
						is_done = true;
						break;
					}
				}
			}
		};

		// Convert the future into an abortable future, for easily terminating it from the
		// `transaction_stop` method.
		let (fut, handle) = futures::future::abortable(broadcast_transaction_fut);
		let broadcast_ids = self.broadcast_ids.clone();
		let drop_id = id.clone();
		let pool = self.pool.clone();
		// The future expected by the executor must be `Future<Output = ()>` instead of
		// `Future<Output = Result<(), Aborted>>`.
		let fut = fut.map(move |result| {
			// Connection space is cleaned when this object is dropped.
			drop(reserved_identifier);

			// Remove the entry from the broadcast IDs map.
			let Some(broadcast_state) = broadcast_ids.write().remove(&drop_id) else { return };

			// The broadcast was not stopped.
			if result.is_ok() {
				return
			}

			// Best effort pool removal (tx can already be finalized).
			pool.remove_invalid(&[broadcast_state.tx_hash]);
		});

		// Keep track of this entry and the abortable handle.
		{
			let mut broadcast_ids = self.broadcast_ids.write();
			broadcast_ids.insert(id.clone(), BroadcastState { handle, tx_hash });
		}

		sc_rpc::utils::spawn_subscription_task(&self.executor, fut);

		Ok(Some(id))
	}

	async fn stop_broadcast(
		&self,
		connection_details: ConnectionDetails,
		operation_id: String,
	) -> Result<(), ErrorBroadcast> {
		// The operation ID must correlate to the same connection ID.
		if !self.rpc_connections.contains_identifier(connection_details.id(), &operation_id) {
			return Err(ErrorBroadcast::InvalidOperationID)
		}

		let mut broadcast_ids = self.broadcast_ids.write();

		let Some(broadcast_state) = broadcast_ids.remove(&operation_id) else {
			return Err(ErrorBroadcast::InvalidOperationID)
		};

		broadcast_state.handle.abort();

		Ok(())
	}
}

/// Returns the last element of the provided stream, or `None` if the stream is closed.
async fn last_stream_element<S>(stream: &mut S) -> Option<S::Item>
where
	S: Stream + Unpin,
{
	let Some(mut element) = stream.next().await else { return None };

	// We are effectively polling the stream for the last available item at this time.
	// The `now_or_never` returns `None` if the stream is `Pending`.
	//
	// If the stream contains `Hash0x1 Hash0x2 Hash0x3 Hash0x4`, we want only `Hash0x4`.
	while let Some(next) = stream.next().now_or_never() {
		let Some(next) = next else {
			// Nothing to do if the stream terminated.
			return None
		};
		element = next;
	}

	Some(element)
}

#[cfg(test)]
mod tests {
	use super::*;
	use tokio_stream::wrappers::ReceiverStream;

	#[tokio::test]
	async fn check_last_stream_element() {
		let (tx, rx) = tokio::sync::mpsc::channel(16);

		let mut stream = ReceiverStream::new(rx);
		// Check the stream with one element queued.
		tx.send(1).await.unwrap();
		assert_eq!(last_stream_element(&mut stream).await, Some(1));

		// Check the stream with multiple elements.
		tx.send(1).await.unwrap();
		tx.send(2).await.unwrap();
		tx.send(3).await.unwrap();
		assert_eq!(last_stream_element(&mut stream).await, Some(3));

		// Drop the stream with some elements
		tx.send(1).await.unwrap();
		tx.send(2).await.unwrap();
		drop(tx);
		assert_eq!(last_stream_element(&mut stream).await, None);
	}
}
