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

//! API implementation for `chainHead`.

use super::{
	chain_head_storage::ChainHeadStorage,
	event::{MethodResponseStarted, OperationBodyDone, OperationCallDone},
};
use crate::{
	chain_head::{
		api::ChainHeadApiServer,
		chain_head_follow::ChainHeadFollower,
		error::Error as ChainHeadRpcError,
		event::{FollowEvent, MethodResponse, OperationError},
		subscription::{SubscriptionManagement, SubscriptionManagementError},
	},
	common::events::StorageQuery,
	hex_string, SubscriptionTaskExecutor,
};
use codec::Encode;
use futures::{channel::oneshot, future::FutureExt};
use jsonrpsee::{
	core::async_trait, server::ResponsePayload, types::SubscriptionId, ConnectionDetails,
	MethodResponseFuture, PendingSubscriptionSink, SubscriptionSink,
};
use log::debug;
use sc_client_api::{
	Backend, BlockBackend, BlockchainEvents, CallExecutor, ChildInfo, ExecutorProvider, StorageKey,
	StorageProvider,
};
use sc_rpc::utils::to_sub_message;
use sp_api::CallApiAt;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_core::{traits::CallContext, Bytes};
use sp_rpc::list::ListOrValue;
use sp_runtime::traits::Block as BlockT;
use std::{marker::PhantomData, sync::Arc, time::Duration};

pub(crate) const LOG_TARGET: &str = "rpc-spec-v2";

/// The configuration of [`ChainHead`].
pub struct ChainHeadConfig {
	/// The maximum number of pinned blocks across all subscriptions.
	pub global_max_pinned_blocks: usize,
	/// The maximum duration that a block is allowed to be pinned per subscription.
	pub subscription_max_pinned_duration: Duration,
	/// The maximum number of ongoing operations per subscription.
	pub subscription_max_ongoing_operations: usize,
	/// Stop all subscriptions if the distance between the leaves and the current finalized
	/// block is larger than this value.
	pub max_lagging_distance: usize,
	/// The maximum number of items reported by the `chainHead_storage` before
	/// pagination is required.
	pub operation_max_storage_items: usize,
	/// The maximum number of `chainHead_follow` subscriptions per connection.
	pub max_follow_subscriptions_per_connection: usize,
}

/// Maximum pinned blocks across all connections.
/// This number is large enough to consider immediate blocks.
/// Note: This should never exceed the `PINNING_CACHE_SIZE` from client/db.
const MAX_PINNED_BLOCKS: usize = 512;

/// Any block of any subscription should not be pinned more than
/// this constant. When a subscription contains a block older than this,
/// the subscription becomes subject to termination.
/// Note: This should be enough for immediate blocks.
const MAX_PINNED_DURATION: Duration = Duration::from_secs(60);

/// The maximum number of ongoing operations per subscription.
/// Note: The lower limit imposed by the spec is 16.
const MAX_ONGOING_OPERATIONS: usize = 16;

/// The maximum number of items the `chainHead_storage` can return
/// before paginations is required.
const MAX_STORAGE_ITER_ITEMS: usize = 5;

/// Stop all subscriptions if the distance between the leaves and the current finalized
/// block is larger than this value.
const MAX_LAGGING_DISTANCE: usize = 128;

/// The maximum number of `chainHead_follow` subscriptions per connection.
const MAX_FOLLOW_SUBSCRIPTIONS_PER_CONNECTION: usize = 4;

impl Default for ChainHeadConfig {
	fn default() -> Self {
		ChainHeadConfig {
			global_max_pinned_blocks: MAX_PINNED_BLOCKS,
			subscription_max_pinned_duration: MAX_PINNED_DURATION,
			subscription_max_ongoing_operations: MAX_ONGOING_OPERATIONS,
			max_lagging_distance: MAX_LAGGING_DISTANCE,
			operation_max_storage_items: MAX_STORAGE_ITER_ITEMS,
			max_follow_subscriptions_per_connection: MAX_FOLLOW_SUBSCRIPTIONS_PER_CONNECTION,
		}
	}
}

/// An API for chain head RPC calls.
pub struct ChainHead<BE: Backend<Block>, Block: BlockT, Client> {
	/// Substrate client.
	client: Arc<Client>,
	/// Backend of the chain.
	backend: Arc<BE>,
	/// Executor to spawn subscriptions.
	executor: SubscriptionTaskExecutor,
	/// Keep track of the pinned blocks for each subscription.
	subscriptions: SubscriptionManagement<Block, BE>,
	/// The maximum number of items reported by the `chainHead_storage` before
	/// pagination is required.
	operation_max_storage_items: usize,
	/// Stop all subscriptions if the distance between the leaves and the current finalized
	/// block is larger than this value.
	max_lagging_distance: usize,
	/// Phantom member to pin the block type.
	_phantom: PhantomData<Block>,
}

impl<BE: Backend<Block>, Block: BlockT, Client> ChainHead<BE, Block, Client> {
	/// Create a new [`ChainHead`].
	pub fn new(
		client: Arc<Client>,
		backend: Arc<BE>,
		executor: SubscriptionTaskExecutor,
		config: ChainHeadConfig,
	) -> Self {
		Self {
			client,
			backend: backend.clone(),
			executor,
			subscriptions: SubscriptionManagement::new(
				config.global_max_pinned_blocks,
				config.subscription_max_pinned_duration,
				config.subscription_max_ongoing_operations,
				config.max_follow_subscriptions_per_connection,
				backend,
			),
			operation_max_storage_items: config.operation_max_storage_items,
			max_lagging_distance: config.max_lagging_distance,
			_phantom: PhantomData,
		}
	}
}

/// Helper to convert the `subscription ID` to a string.
pub fn read_subscription_id_as_string(sink: &SubscriptionSink) -> String {
	match sink.subscription_id() {
		SubscriptionId::Num(n) => n.to_string(),
		SubscriptionId::Str(s) => s.into_owned().into(),
	}
}

/// Parse hex-encoded string parameter as raw bytes.
///
/// If the parsing fails, returns an error propagated to the RPC method.
fn parse_hex_param(param: String) -> Result<Vec<u8>, ChainHeadRpcError> {
	// Methods can accept empty parameters.
	if param.is_empty() {
		return Ok(Default::default())
	}

	match array_bytes::hex2bytes(&param) {
		Ok(bytes) => Ok(bytes),
		Err(_) => Err(ChainHeadRpcError::InvalidParam(param)),
	}
}

#[async_trait]
impl<BE, Block, Client> ChainHeadApiServer<Block::Hash> for ChainHead<BE, Block, Client>
where
	Block: BlockT + 'static,
	Block::Header: Unpin,
	BE: Backend<Block> + 'static,
	Client: BlockBackend<Block>
		+ ExecutorProvider<Block>
		+ HeaderBackend<Block>
		+ HeaderMetadata<Block, Error = BlockChainError>
		+ BlockchainEvents<Block>
		+ CallApiAt<Block>
		+ StorageProvider<Block, BE>
		+ 'static,
{
	fn chain_head_unstable_follow(&self, pending: PendingSubscriptionSink, with_runtime: bool) {
		let subscriptions = self.subscriptions.clone();
		let backend = self.backend.clone();
		let client = self.client.clone();
		let max_lagging_distance = self.max_lagging_distance;

		let fut = async move {
			// Ensure the current connection ID has enough space to accept a new subscription.
			let connection_id = pending.connection_id();
			// The RAII `reserved_subscription` will clean up resources on drop:
			// - free the reserved subscription for the connection ID.
			// - remove the subscription ID from the subscription management.
			let Some(mut reserved_subscription) = subscriptions.reserve_subscription(connection_id)
			else {
				pending.reject(ChainHeadRpcError::ReachedLimits).await;
				return
			};

			let Ok(sink) = pending.accept().await else { return };

			let sub_id = read_subscription_id_as_string(&sink);
			// Keep track of the subscription.
			let Some(sub_data) =
				reserved_subscription.insert_subscription(sub_id.clone(), with_runtime)
			else {
				// Inserting the subscription can only fail if the JsonRPSee generated a duplicate
				// subscription ID.
				debug!(target: LOG_TARGET, "[follow][id={:?}] Subscription already accepted", sub_id);
				let msg = to_sub_message(&sink, &FollowEvent::<String>::Stop);
				let _ = sink.send(msg).await;
				return
			};
			debug!(target: LOG_TARGET, "[follow][id={:?}] Subscription accepted", sub_id);

			let mut chain_head_follow = ChainHeadFollower::new(
				client,
				backend,
				subscriptions,
				with_runtime,
				sub_id.clone(),
				max_lagging_distance,
			);
			let result = chain_head_follow.generate_events(sink, sub_data).await;
			if let Err(SubscriptionManagementError::BlockDistanceTooLarge) = result {
				debug!(target: LOG_TARGET, "[follow][id={:?}] All subscriptions are stopped", sub_id);
				reserved_subscription.stop_all_subscriptions();
			}

			debug!(target: LOG_TARGET, "[follow][id={:?}] Subscription removed", sub_id);
		};

		self.executor.spawn("substrate-rpc-subscription", Some("rpc"), fut.boxed());
	}

	async fn chain_head_unstable_body(
		&self,
		connection_details: ConnectionDetails,
		follow_subscription: String,
		hash: Block::Hash,
	) -> ResponsePayload<'static, MethodResponse> {
		if !self
			.subscriptions
			.contains_subscription(connection_details.id(), &follow_subscription)
		{
			// The spec says to return `LimitReached` if the follow subscription is invalid or
			// stale.
			return ResponsePayload::success(MethodResponse::LimitReached);
		}

		let client = self.client.clone();
		let subscriptions = self.subscriptions.clone();
		let executor = self.executor.clone();

		let result = spawn_blocking(&self.executor, async move {
			let mut block_guard = match subscriptions.lock_block(&follow_subscription, hash, 1) {
				Ok(block) => block,
				Err(SubscriptionManagementError::SubscriptionAbsent) |
				Err(SubscriptionManagementError::ExceededLimits) =>
					return ResponsePayload::success(MethodResponse::LimitReached),
				Err(SubscriptionManagementError::BlockHashAbsent) => {
					// Block is not part of the subscription.
					return ResponsePayload::error(ChainHeadRpcError::InvalidBlock);
				},
				Err(_) => return ResponsePayload::error(ChainHeadRpcError::InvalidBlock),
			};

			let operation_id = block_guard.operation().operation_id();

			let event = match client.block(hash) {
				Ok(Some(signed_block)) => {
					let extrinsics = signed_block
						.block
						.extrinsics()
						.iter()
						.map(|extrinsic| hex_string(&extrinsic.encode()))
						.collect();
					FollowEvent::<Block::Hash>::OperationBodyDone(OperationBodyDone {
						operation_id: operation_id.clone(),
						value: extrinsics,
					})
				},
				Ok(None) => {
					// The block's body was pruned. This subscription ID has become invalid.
					debug!(
						target: LOG_TARGET,
						"[body][id={:?}] Stopping subscription because hash={:?} was pruned",
						&follow_subscription,
						hash
					);
					subscriptions.remove_subscription(&follow_subscription);
					return ResponsePayload::error(ChainHeadRpcError::InvalidBlock)
				},
				Err(error) => FollowEvent::<Block::Hash>::OperationError(OperationError {
					operation_id: operation_id.clone(),
					error: error.to_string(),
				}),
			};

			let (rp, rp_fut) = method_started_response(operation_id, None);
			let fut = async move {
				// Wait for the server to send out the response and if it produces an error no event
				// should be generated.
				if rp_fut.await.is_err() {
					return;
				}

				let _ = block_guard.response_sender().unbounded_send(event);
			};
			executor.spawn_blocking("substrate-rpc-subscription", Some("rpc"), fut.boxed());

			rp
		});

		result
			.await
			.unwrap_or_else(|_| ResponsePayload::success(MethodResponse::LimitReached))
	}

	async fn chain_head_unstable_header(
		&self,
		connection_details: ConnectionDetails,
		follow_subscription: String,
		hash: Block::Hash,
	) -> Result<Option<String>, ChainHeadRpcError> {
		if !self
			.subscriptions
			.contains_subscription(connection_details.id(), &follow_subscription)
		{
			return Ok(None);
		}

		let block_guard = match self.subscriptions.lock_block(&follow_subscription, hash, 1) {
			Ok(block) => block,
			Err(SubscriptionManagementError::SubscriptionAbsent) |
			Err(SubscriptionManagementError::ExceededLimits) => return Ok(None),
			Err(SubscriptionManagementError::BlockHashAbsent) => {
				// Block is not part of the subscription.
				return Err(ChainHeadRpcError::InvalidBlock.into())
			},
			Err(_) => return Err(ChainHeadRpcError::InvalidBlock.into()),
		};

		let client = self.client.clone();
		let result = spawn_blocking(&self.executor, async move {
			let _block_guard = block_guard;

			client
				.header(hash)
				.map(|opt_header| opt_header.map(|h| hex_string(&h.encode())))
				.map_err(|err| ChainHeadRpcError::InternalError(err.to_string()))
		});
		result.await.unwrap_or_else(|_| Ok(None))
	}

	async fn chain_head_unstable_storage(
		&self,
		connection_details: ConnectionDetails,
		follow_subscription: String,
		hash: Block::Hash,
		items: Vec<StorageQuery<String>>,
		child_trie: Option<String>,
	) -> ResponsePayload<'static, MethodResponse> {
		if !self
			.subscriptions
			.contains_subscription(connection_details.id(), &follow_subscription)
		{
			// The spec says to return `LimitReached` if the follow subscription is invalid or
			// stale.
			return ResponsePayload::success(MethodResponse::LimitReached);
		}

		// Gain control over parameter parsing and returned error.
		let items = match items
			.into_iter()
			.map(|query| {
				let key = StorageKey(parse_hex_param(query.key)?);
				Ok(StorageQuery { key, query_type: query.query_type })
			})
			.collect::<Result<Vec<_>, ChainHeadRpcError>>()
		{
			Ok(items) => items,
			Err(err) => {
				return ResponsePayload::error(err);
			},
		};

		let child_trie = match child_trie.map(|child_trie| parse_hex_param(child_trie)).transpose()
		{
			Ok(c) => c.map(ChildInfo::new_default_from_vec),
			Err(e) => return ResponsePayload::error(e),
		};

		let mut block_guard =
			match self.subscriptions.lock_block(&follow_subscription, hash, items.len()) {
				Ok(block) => block,
				Err(SubscriptionManagementError::SubscriptionAbsent) |
				Err(SubscriptionManagementError::ExceededLimits) => {
					return ResponsePayload::success(MethodResponse::LimitReached);
				},
				Err(SubscriptionManagementError::BlockHashAbsent) => {
					// Block is not part of the subscription.
					return ResponsePayload::error(ChainHeadRpcError::InvalidBlock)
				},
				Err(_) => return ResponsePayload::error(ChainHeadRpcError::InvalidBlock),
			};

		let mut storage_client = ChainHeadStorage::<Client, Block, BE>::new(
			self.client.clone(),
			self.operation_max_storage_items,
		);
		let operation = block_guard.operation();
		let operation_id = operation.operation_id();

		// The number of operations we are allowed to execute.
		let num_operations = operation.num_reserved();
		let discarded = items.len().saturating_sub(num_operations);
		let mut items = items;
		items.truncate(num_operations);

		let (rp, rp_fut) = method_started_response(operation_id, Some(discarded));
		let fut = async move {
			// Wait for the server to send out the response and if it produces an error no event
			// should be generated.
			if rp_fut.await.is_err() {
				return;
			}

			storage_client.generate_events(block_guard, hash, items, child_trie).await;
		};
		self.executor
			.spawn_blocking("substrate-rpc-subscription", Some("rpc"), fut.boxed());

		rp
	}

	async fn chain_head_unstable_call(
		&self,
		connection_details: ConnectionDetails,
		follow_subscription: String,
		hash: Block::Hash,
		function: String,
		call_parameters: String,
	) -> ResponsePayload<'static, MethodResponse> {
		let call_parameters = match parse_hex_param(call_parameters) {
			Ok(hex) => Bytes::from(hex),
			Err(err) => return ResponsePayload::error(err),
		};

		if !self
			.subscriptions
			.contains_subscription(connection_details.id(), &follow_subscription)
		{
			// The spec says to return `LimitReached` if the follow subscription is invalid or
			// stale.
			return ResponsePayload::success(MethodResponse::LimitReached);
		}

		let mut block_guard = match self.subscriptions.lock_block(&follow_subscription, hash, 1) {
			Ok(block) => block,
			Err(SubscriptionManagementError::SubscriptionAbsent) |
			Err(SubscriptionManagementError::ExceededLimits) => {
				// Invalid invalid subscription ID.
				return ResponsePayload::success(MethodResponse::LimitReached)
			},
			Err(SubscriptionManagementError::BlockHashAbsent) => {
				// Block is not part of the subscription.
				return ResponsePayload::error(ChainHeadRpcError::InvalidBlock)
			},
			Err(_) => return ResponsePayload::error(ChainHeadRpcError::InvalidBlock),
		};

		// Reject subscription if with_runtime is false.
		if !block_guard.has_runtime() {
			return ResponsePayload::error(ChainHeadRpcError::InvalidRuntimeCall(
				"The runtime updates flag must be set".to_string(),
			));
		}

		let operation_id = block_guard.operation().operation_id();
		let client = self.client.clone();

		let (rp, rp_fut) = method_started_response(operation_id.clone(), None);
		let fut = async move {
			// Wait for the server to send out the response and if it produces an error no event
			// should be generated.
			if rp_fut.await.is_err() {
				return
			}

			let event = client
				.executor()
				.call(hash, &function, &call_parameters, CallContext::Offchain)
				.map(|result| {
					FollowEvent::<Block::Hash>::OperationCallDone(OperationCallDone {
						operation_id: operation_id.clone(),
						output: hex_string(&result),
					})
				})
				.unwrap_or_else(|error| {
					FollowEvent::<Block::Hash>::OperationError(OperationError {
						operation_id: operation_id.clone(),
						error: error.to_string(),
					})
				});

			let _ = block_guard.response_sender().unbounded_send(event);
		};
		self.executor
			.spawn_blocking("substrate-rpc-subscription", Some("rpc"), fut.boxed());

		rp
	}

	async fn chain_head_unstable_unpin(
		&self,
		connection_details: ConnectionDetails,
		follow_subscription: String,
		hash_or_hashes: ListOrValue<Block::Hash>,
	) -> Result<(), ChainHeadRpcError> {
		if !self
			.subscriptions
			.contains_subscription(connection_details.id(), &follow_subscription)
		{
			return Ok(());
		}

		let result = match hash_or_hashes {
			ListOrValue::Value(hash) =>
				self.subscriptions.unpin_blocks(&follow_subscription, [hash]),
			ListOrValue::List(hashes) =>
				self.subscriptions.unpin_blocks(&follow_subscription, hashes),
		};

		match result {
			Ok(()) => Ok(()),
			Err(SubscriptionManagementError::SubscriptionAbsent) => {
				// Invalid invalid subscription ID.
				Ok(())
			},
			Err(SubscriptionManagementError::BlockHashAbsent) => {
				// Block is not part of the subscription.
				Err(ChainHeadRpcError::InvalidBlock)
			},
			Err(SubscriptionManagementError::DuplicateHashes) =>
				Err(ChainHeadRpcError::InvalidDuplicateHashes),
			Err(_) => Err(ChainHeadRpcError::InvalidBlock),
		}
	}

	async fn chain_head_unstable_continue(
		&self,
		connection_details: ConnectionDetails,
		follow_subscription: String,
		operation_id: String,
	) -> Result<(), ChainHeadRpcError> {
		if !self
			.subscriptions
			.contains_subscription(connection_details.id(), &follow_subscription)
		{
			return Ok(())
		}

		let Some(operation) = self.subscriptions.get_operation(&follow_subscription, &operation_id)
		else {
			return Ok(())
		};

		if !operation.submit_continue() {
			// Continue called without generating a `WaitingForContinue` event.
			Err(ChainHeadRpcError::InvalidContinue.into())
		} else {
			Ok(())
		}
	}

	async fn chain_head_unstable_stop_operation(
		&self,
		connection_details: ConnectionDetails,
		follow_subscription: String,
		operation_id: String,
	) -> Result<(), ChainHeadRpcError> {
		if !self
			.subscriptions
			.contains_subscription(connection_details.id(), &follow_subscription)
		{
			return Ok(())
		}

		let Some(operation) = self.subscriptions.get_operation(&follow_subscription, &operation_id)
		else {
			return Ok(())
		};

		operation.stop_operation();

		Ok(())
	}
}

fn method_started_response(
	operation_id: String,
	discarded_items: Option<usize>,
) -> (ResponsePayload<'static, MethodResponse>, MethodResponseFuture) {
	let rp = MethodResponse::Started(MethodResponseStarted { operation_id, discarded_items });
	ResponsePayload::success(rp).notify_on_completion()
}

/// Spawn a blocking future on the provided executor and return the result on a oneshot channel.
///
/// This is a wrapper to extract the result of a `executor.spawn_blocking` future.
fn spawn_blocking<R>(
	executor: &SubscriptionTaskExecutor,
	fut: impl std::future::Future<Output = R> + Send + 'static,
) -> oneshot::Receiver<R>
where
	R: Send + 'static,
{
	let (tx, rx) = oneshot::channel();

	let blocking_fut = async move {
		let result = fut.await;
		// Send the result back on the channel.
		let _ = tx.send(result);
	};

	executor.spawn_blocking("substrate-rpc-subscription", Some("rpc"), blocking_fut.boxed());

	rx
}
