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

//! API implementation for submitting transactions.

use crate::{
	transaction::{
		api::TransactionApiServer,
		error::Error,
		event::{
			TransactionBlock, TransactionBroadcasted, TransactionDropped, TransactionError,
			TransactionEvent,
		},
	},
	SubscriptionTaskExecutor,
};
use jsonrpsee::{
	core::{async_trait, RpcResult},
	types::error::ErrorObject,
	PendingSubscriptionSink,
};
use parking_lot::RwLock;
use rand::{distributions::Alphanumeric, Rng};
use sc_rpc::utils::pipe_from_stream;
use sc_transaction_pool_api::{
	error::IntoPoolError, BlockHash, TransactionFor, TransactionPool, TransactionSource,
	TransactionStatus,
};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::Bytes;
use sp_runtime::traits::Block as BlockT;
use std::{collections::HashSet, sync::Arc};

use codec::Decode;
use futures::{StreamExt, TryFutureExt};

/// An API for transaction RPC calls.
pub struct Transaction<Pool, Client> {
	/// Substrate client.
	client: Arc<Client>,
	/// Transactions pool.
	pool: Arc<Pool>,
	/// Executor to spawn subscriptions.
	executor: SubscriptionTaskExecutor,
	/// The brodcast operation IDs.
	broadcast_ids: Arc<RwLock<HashSet<String>>>,
}

impl<Pool, Client> Transaction<Pool, Client> {
	/// Creates a new [`Transaction`].
	pub fn new(client: Arc<Client>, pool: Arc<Pool>, executor: SubscriptionTaskExecutor) -> Self {
		Transaction { client, pool, executor, broadcast_ids: Default::default() }
	}

	/// Generate and track an unique operation ID for the `transaction_broadcast` RPC method.
	pub fn insert_unique_id(&self) -> String {
		let generate_operation_id = || {
			// The lenght of the operation ID.
			const OPERATION_ID_LEN: usize = 16;

			let mut rng = rand::thread_rng();
			(&mut rng)
				.sample_iter(Alphanumeric)
				.take(OPERATION_ID_LEN)
				.map(char::from)
				.collect::<String>()
		};

		let mut id = generate_operation_id();

		let mut broadcast_ids = self.broadcast_ids.write();

		while broadcast_ids.contains(&id) {
			id = generate_operation_id();
		}

		broadcast_ids.insert(id.clone());

		id
	}
}

/// Currently we treat all RPC transactions as externals.
///
/// Possibly in the future we could allow opt-in for special treatment
/// of such transactions, so that the block authors can inject
/// some unique transactions via RPC and have them included in the pool.
const TX_SOURCE: TransactionSource = TransactionSource::External;

/// Extrinsic has an invalid format.
///
/// # Note
///
/// This is similar to the old `author` API error code.
const BAD_FORMAT: i32 = 1001;

#[async_trait]
impl<Pool, Client> TransactionApiServer<BlockHash<Pool>> for Transaction<Pool, Client>
where
	Pool: TransactionPool + Sync + Send + 'static,
	Pool::Hash: Unpin,
	<Pool::Block as BlockT>::Hash: Unpin,
	Client: HeaderBackend<Pool::Block> + ProvideRuntimeApi<Pool::Block> + Send + Sync + 'static,
{
	fn submit_and_watch(&self, pending: PendingSubscriptionSink, xt: Bytes) {
		let client = self.client.clone();
		let pool = self.pool.clone();

		let fut = async move {
			// This is the only place where the RPC server can return an error for this
			// subscription. Other defects must be signaled as events to the sink.
			let decoded_extrinsic = match TransactionFor::<Pool>::decode(&mut &xt[..]) {
				Ok(decoded_extrinsic) => decoded_extrinsic,
				Err(e) => {
					let err = ErrorObject::owned(
						BAD_FORMAT,
						format!("Extrinsic has invalid format: {}", e),
						None::<()>,
					);
					let _ = pending.reject(err).await;
					return
				},
			};

			let best_block_hash = client.info().best_hash;

			let submit = pool
				.submit_and_watch(best_block_hash, TX_SOURCE, decoded_extrinsic)
				.map_err(|e| {
					e.into_pool_error()
						.map(Error::from)
						.unwrap_or_else(|e| Error::Verification(Box::new(e)))
				});

			match submit.await {
				Ok(stream) => {
					let mut state = TransactionState::new();
					let stream =
						stream.filter_map(move |event| async move { state.handle_event(event) });
					pipe_from_stream(pending, stream.boxed()).await;
				},
				Err(err) => {
					// We have not created an `Watcher` for the tx. Make sure the
					// error is still propagated as an event.
					let event: TransactionEvent<<Pool::Block as BlockT>::Hash> = err.into();
					pipe_from_stream(pending, futures::stream::once(async { event }).boxed()).await;
				},
			};
		};

		sc_rpc::utils::spawn_subscription_task(&self.executor, fut);
	}

	fn broadcast(&self, bytes: Bytes) -> RpcResult<Option<String>> {
		let client = self.client.clone();
		let pool = self.pool.clone();

		// The ID is unique and has been inserted to the broadcast ID set.
		let id = self.insert_unique_id();

		let fut = async move {
			// There is nothing we could do with an extrinsic of invalid format.
			let Ok(decoded_extrinsic) = TransactionFor::<Pool>::decode(&mut &bytes[..]) else {
				return
			};

			// Flag to determine if the we should broadcast the transaction again.
			let mut is_done = false;

			while !is_done {
				let best_block_hash = client.info().best_hash;
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

						// Dropped transaction may renter the pool at a later time, when other
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

		sc_rpc::utils::spawn_subscription_task(&self.executor, fut);

		Ok(Some(id))
	}

	fn stop_broadcast(&self, _operation_id: String) -> RpcResult<()> {
		Ok(())
	}
}

/// The transaction's state that needs to be preserved between
/// multiple events generated by the transaction-pool.
///
/// # Note
///
/// In the future, the RPC server can submit only the last event when multiple
/// identical events happen in a row.
#[derive(Clone, Copy)]
struct TransactionState {
	/// True if the transaction was previously broadcasted.
	broadcasted: bool,
}

impl TransactionState {
	/// Construct a new [`TransactionState`].
	pub fn new() -> Self {
		TransactionState { broadcasted: false }
	}

	/// Handle events generated by the transaction-pool and convert them
	/// to the new API expected state.
	#[inline]
	pub fn handle_event<Hash: Clone, BlockHash: Clone>(
		&mut self,
		event: TransactionStatus<Hash, BlockHash>,
	) -> Option<TransactionEvent<BlockHash>> {
		match event {
			TransactionStatus::Ready | TransactionStatus::Future =>
				Some(TransactionEvent::<BlockHash>::Validated),
			TransactionStatus::Broadcast(peers) => {
				// Set the broadcasted flag once if we submitted the transaction to
				// at least one peer.
				self.broadcasted = self.broadcasted || !peers.is_empty();

				Some(TransactionEvent::Broadcasted(TransactionBroadcasted {
					num_peers: peers.len(),
				}))
			},
			TransactionStatus::InBlock((hash, index)) =>
				Some(TransactionEvent::BestChainBlockIncluded(Some(TransactionBlock {
					hash,
					index,
				}))),
			TransactionStatus::Retracted(_) => Some(TransactionEvent::BestChainBlockIncluded(None)),
			TransactionStatus::FinalityTimeout(_) =>
				Some(TransactionEvent::Dropped(TransactionDropped {
					broadcasted: self.broadcasted,
					error: "Maximum number of finality watchers has been reached".into(),
				})),
			TransactionStatus::Finalized((hash, index)) =>
				Some(TransactionEvent::Finalized(TransactionBlock { hash, index })),
			TransactionStatus::Usurped(_) => Some(TransactionEvent::Invalid(TransactionError {
				error: "Extrinsic was rendered invalid by another extrinsic".into(),
			})),
			TransactionStatus::Dropped => Some(TransactionEvent::Invalid(TransactionError {
				error: "Extrinsic dropped from the pool due to exceeding limits".into(),
			})),
			TransactionStatus::Invalid => Some(TransactionEvent::Invalid(TransactionError {
				error: "Extrinsic marked as invalid".into(),
			})),
		}
	}
}
