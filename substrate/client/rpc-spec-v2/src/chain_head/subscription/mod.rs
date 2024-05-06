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

use jsonrpsee::ConnectionId;
use parking_lot::RwLock;
use sc_client_api::Backend;
use sp_runtime::traits::Block as BlockT;
use std::{sync::Arc, time::Duration};

mod error;
mod inner;

use crate::{
	chain_head::chain_head::LOG_TARGET,
	common::connections::{RegisteredConnection, ReservedConnection, RpcConnections},
};

use self::inner::SubscriptionsInner;

pub use self::inner::OperationState;
pub use error::SubscriptionManagementError;
pub use inner::{BlockGuard, InsertedSubscriptionData};

/// Manage block pinning / unpinning for subscription IDs.
pub struct SubscriptionManagement<Block: BlockT, BE: Backend<Block>> {
	/// Manage subscription by mapping the subscription ID
	/// to a set of block hashes.
	inner: Arc<RwLock<SubscriptionsInner<Block, BE>>>,

	/// Ensures that chainHead methods can be called from a single connection context.
	///
	/// For example, `chainHead_storage` cannot be called with a subscription ID that
	/// was obtained from a different connection.
	rpc_connections: RpcConnections,
}

impl<Block: BlockT, BE: Backend<Block>> Clone for SubscriptionManagement<Block, BE> {
	fn clone(&self) -> Self {
		SubscriptionManagement {
			inner: self.inner.clone(),
			rpc_connections: self.rpc_connections.clone(),
		}
	}
}

impl<Block: BlockT, BE: Backend<Block>> SubscriptionManagement<Block, BE> {
	/// Construct a new [`SubscriptionManagement`].
	pub fn new(
		global_max_pinned_blocks: usize,
		local_max_pin_duration: Duration,
		max_ongoing_operations: usize,
		max_follow_subscriptions_per_connection: usize,
		backend: Arc<BE>,
	) -> Self {
		SubscriptionManagement {
			inner: Arc::new(RwLock::new(SubscriptionsInner::new(
				global_max_pinned_blocks,
				local_max_pin_duration,
				max_ongoing_operations,
				backend,
			))),
			rpc_connections: RpcConnections::new(max_follow_subscriptions_per_connection),
		}
	}

	/// Create a new instance from the inner state.
	///
	/// # Note
	///
	/// Used for testing.
	#[cfg(test)]
	pub(crate) fn _from_inner(
		inner: Arc<RwLock<SubscriptionsInner<Block, BE>>>,
		rpc_connections: RpcConnections,
	) -> Self {
		SubscriptionManagement { inner, rpc_connections }
	}

	/// Reserve space for a subscriptions.
	///
	/// Fails if the connection ID is has reached the maximum number of active subscriptions.
	pub fn reserve_subscription(
		&self,
		connection_id: ConnectionId,
	) -> Option<ReservedSubscription<Block, BE>> {
		let reserved_token = self.rpc_connections.reserve_space(connection_id)?;

		Some(ReservedSubscription {
			state: ConnectionState::Reserved(reserved_token),
			inner: self.inner.clone(),
		})
	}

	/// Check if the given connection contains the given subscription.
	pub fn contains_subscription(
		&self,
		connection_id: ConnectionId,
		subscription_id: &str,
	) -> bool {
		self.rpc_connections.contains_identifier(connection_id, subscription_id)
	}

	/// Remove the subscription ID with associated pinned blocks.
	pub fn remove_subscription(&self, sub_id: &str) {
		let mut inner = self.inner.write();
		inner.remove_subscription(sub_id)
	}

	/// The block is pinned in the backend only once when the block's hash is first encountered.
	///
	/// Each subscription is expected to call this method twice:
	/// - once from the `NewBlock` import
	/// - once from the `Finalized` import
	///
	/// Returns
	/// - Ok(true) if the subscription did not previously contain this block
	/// - Ok(false) if the subscription already contained this this
	/// - Error if the backend failed to pin the block or the subscription ID is invalid
	pub fn pin_block(
		&self,
		sub_id: &str,
		hash: Block::Hash,
	) -> Result<bool, SubscriptionManagementError> {
		let mut inner = self.inner.write();
		inner.pin_block(sub_id, hash)
	}

	/// Unpin the blocks from the subscription.
	///
	/// Blocks are reference counted and when the last subscription unpins a given block, the block
	/// is also unpinned from the backend.
	///
	/// This method is called only once per subscription.
	///
	/// Returns an error if the subscription ID is invalid, or any of the blocks are not pinned
	/// for the subscriptions. When an error is returned, it is guaranteed that no blocks have
	/// been unpinned.
	pub fn unpin_blocks(
		&self,
		sub_id: &str,
		hashes: impl IntoIterator<Item = Block::Hash> + Clone,
	) -> Result<(), SubscriptionManagementError> {
		let mut inner = self.inner.write();
		inner.unpin_blocks(sub_id, hashes)
	}

	/// Ensure the block remains pinned until the return object is dropped.
	///
	/// Returns a [`BlockGuard`] that pins and unpins the block hash in RAII manner
	/// and reserves capacity for ogoing operations.
	///
	/// Returns an error if the block hash is not pinned for the subscription,
	/// the subscription ID is invalid or the limit of ongoing operations was exceeded.
	pub fn lock_block(
		&self,
		sub_id: &str,
		hash: Block::Hash,
		to_reserve: usize,
	) -> Result<BlockGuard<Block, BE>, SubscriptionManagementError> {
		let mut inner = self.inner.write();
		inner.lock_block(sub_id, hash, to_reserve)
	}

	/// Get the operation state.
	pub fn get_operation(&self, sub_id: &str, operation_id: &str) -> Option<OperationState> {
		let mut inner = self.inner.write();
		inner.get_operation(sub_id, operation_id)
	}
}

/// The state of the connection.
///
/// The state starts in a [`ConnectionState::Reserved`] state and then transitions to
/// [`ConnectionState::Registered`] when the subscription is inserted.
enum ConnectionState {
	Reserved(ReservedConnection),
	Registered { _unregister_on_drop: RegisteredConnection, sub_id: String },
	Empty,
}

/// RAII wrapper that removes the subscription from internal mappings and
/// gives back the reserved space for the connection.
pub struct ReservedSubscription<Block: BlockT, BE: Backend<Block>> {
	state: ConnectionState,
	inner: Arc<RwLock<SubscriptionsInner<Block, BE>>>,
}

impl<Block: BlockT, BE: Backend<Block>> ReservedSubscription<Block, BE> {
	/// Insert a new subscription ID.
	///
	/// If the subscription was not previously inserted, returns the receiver that is
	/// triggered upon the "Stop" event. Otherwise, if the subscription ID was already
	/// inserted returns none.
	///
	/// # Note
	///
	/// This method should be called only once.
	pub fn insert_subscription(
		&mut self,
		sub_id: String,
		runtime_updates: bool,
	) -> Option<InsertedSubscriptionData<Block>> {
		match std::mem::replace(&mut self.state, ConnectionState::Empty) {
			ConnectionState::Reserved(reserved) => {
				let registered_token = reserved.register(sub_id.clone())?;
				self.state = ConnectionState::Registered {
					_unregister_on_drop: registered_token,
					sub_id: sub_id.clone(),
				};

				let mut inner = self.inner.write();
				inner.insert_subscription(sub_id, runtime_updates)
			},
			// Cannot insert multiple subscriptions into one single reserved space.
			ConnectionState::Registered { .. } | ConnectionState::Empty => {
				log::error!(target: LOG_TARGET, "Called insert_subscription on a connection that is not reserved");
				None
			},
		}
	}

	/// Stop all active subscriptions.
	///
	/// For all active subscriptions, the internal data is discarded, blocks are unpinned and the
	/// `Stop` event will be generated.
	pub fn stop_all_subscriptions(&self) {
		let mut inner = self.inner.write();
		inner.stop_all_subscriptions()
	}
}

impl<Block: BlockT, BE: Backend<Block>> Drop for ReservedSubscription<Block, BE> {
	fn drop(&mut self) {
		if let ConnectionState::Registered { sub_id, .. } = &self.state {
			self.inner.write().remove_subscription(sub_id);
		}
	}
}
