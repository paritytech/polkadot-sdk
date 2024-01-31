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

use crate::{transaction::api::TransactionBroadcastApiServer, SubscriptionTaskExecutor};
use codec::Decode;
use futures::{FutureExt, StreamExt};
use futures_util::stream::AbortHandle;
use jsonrpsee::core::{async_trait, RpcResult};
use parking_lot::RwLock;
use rand::{distributions::Alphanumeric, Rng};
use sc_client_api::BlockchainEvents;
use sc_transaction_pool_api::{
	BlockHash, TransactionFor, TransactionPool, TransactionSource, TransactionStatus,
};
use sp_blockchain::HeaderBackend;
use sp_core::Bytes;
use sp_runtime::traits::Block as BlockT;
use std::{collections::HashMap, sync::Arc};

use super::error::ErrorBroadcast;

/// An API for transaction RPC calls.
pub struct TransactionBroadcast<Pool, Client> {
	/// Substrate client.
	client: Arc<Client>,
	/// Transactions pool.
	pool: Arc<Pool>,
	/// Executor to spawn subscriptions.
	executor: SubscriptionTaskExecutor,
	/// The brodcast operation IDs.
	broadcast_ids: Arc<RwLock<HashMap<String, BroadcastState>>>,
}

/// The state of a broadcast operation.
struct BroadcastState {
	/// Handle to abort the running future that broadcasts the transaction.
	handle: AbortHandle,
}

impl<Pool, Client> TransactionBroadcast<Pool, Client> {
	/// Creates a new [`TransactionBroadcast`].
	pub fn new(client: Arc<Client>, pool: Arc<Pool>, executor: SubscriptionTaskExecutor) -> Self {
		TransactionBroadcast { client, pool, executor, broadcast_ids: Default::default() }
	}

	/// Generate an unique operation ID for the `transaction_broadcast` RPC method.
	pub fn generate_unique_id(&self) -> String {
		let generate_operation_id = || {
			// The lenght of the operation ID.
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
impl<Pool, Client> TransactionBroadcastApiServer<BlockHash<Pool>>
	for TransactionBroadcast<Pool, Client>
where
	Pool: TransactionPool + Sync + Send + 'static,
	Pool::Hash: Unpin,
	<Pool::Block as BlockT>::Hash: Unpin,
	Client: HeaderBackend<Pool::Block> + BlockchainEvents<Pool::Block> + Send + Sync + 'static,
{
	fn broadcast(&self, bytes: Bytes) -> RpcResult<Option<String>> {
		let pool = self.pool.clone();

		// The unique ID of this operation.
		let id = self.generate_unique_id();

		let mut best_block_import_stream =
			Box::pin(self.client.import_notification_stream().filter_map(
				|notification| async move { notification.is_new_best.then_some(notification.hash) },
			));

		let broadcast_transaction_fut = async move {
			// There is nothing we could do with an extrinsic of invalid format.
			let Ok(decoded_extrinsic) = TransactionFor::<Pool>::decode(&mut &bytes[..]) else {
				return
			};

			// Flag to determine if the we should broadcast the transaction again.
			let mut is_done = false;

			while !is_done {
				// Wait for the next block to become available.
				let Some(mut best_block_hash) = best_block_import_stream.next().await else {
					return
				};
				// We are effectively polling the stream for the last available item at this time.
				// The `now_or_never` returns `None` if the stream is `Pending`.
				//
				// If the stream contains `Hash0x1 Hash0x2 Hash0x3 Hash0x4`, we want only `Hash0x4`.
				while let Some(next) = best_block_import_stream.next().now_or_never() {
					let Some(next) = next else {
						// Nothing to do if the best block stream terminated.
						return
					};
					best_block_hash = next;
				}

				let submit =
					pool.submit_and_watch(best_block_hash, TX_SOURCE, decoded_extrinsic.clone());

				// The transaction was not included to the pool, because it is invalid.
				// However an invalid transaction can become valid at a later time.
				let Ok(mut stream) = submit.await else { return };

				while let Some(event) = stream.next().await {
					match event {
						// The transaction propagation stops when:
						// - The transaction was included in a finalized block via
						//   `TransactionStatus::Finalized`.
						TransactionStatus::Finalized(_) |
						// - The block in which the transaction was included could not be finalized for
						//   more than 256 blocks via `TransactionStatus::FinalityTimeout`. This could
						//   happen when:
						//   - the finality gadget is lagging behing
						//   - the finality gadget is not available for the chain
						TransactionStatus::FinalityTimeout(_) |
						// - The transaction has been replaced by another transaction with identical tags
						// (same sender and same account nonce).
						TransactionStatus::Usurped(_) => {
							is_done = true;
							break;
						},

						// Dropped transaction may enter the pool at a later time, when other
						// transactions have been finalized and remove from the pool.
						TransactionStatus::Dropped |
						// An invalid transaction may become valid at a later time.
						TransactionStatus::Invalid => {
							break;
						},

						// The transaction is still in the pool, the ready or future queue.
						TransactionStatus::Ready | TransactionStatus::Future |
						// Transaction has been broadcasted as intended.
						TransactionStatus::Broadcast(_) |
						// Transaction has been included in a block, but the block is not finalized yet.
						TransactionStatus::InBlock(_) |
						// Transaction has been retracted, but it may be included in a block at a later time.
						TransactionStatus::Retracted(_) => (),
					}
				}
			}
		};

		// Convert the future into an abortable future, for easily terminating it from the
		// `transaction_stop` method.
		let (fut, handle) = futures::future::abortable(broadcast_transaction_fut);
		// The future expected by the executor must be `Future<Output = ()>` instead of
		// `Future<Output = Result<(), Aborted>>`.
		let fut = fut.map(drop);

		// Keep track of this entry and the abortable handle.
		{
			let mut broadcast_ids = self.broadcast_ids.write();
			broadcast_ids.insert(id.clone(), BroadcastState { handle });
		}

		sc_rpc::utils::spawn_subscription_task(&self.executor, fut);

		Ok(Some(id))
	}

	fn stop_broadcast(&self, operation_id: String) -> Result<(), ErrorBroadcast> {
		let mut broadcast_ids = self.broadcast_ids.write();

		let Some(broadcast_state) = broadcast_ids.remove(&operation_id) else {
			return Err(ErrorBroadcast::InvalidOperationID)
		};

		broadcast_state.handle.abort();

		Ok(())
	}
}
