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
use parking_lot::Mutex;
use std::{
	collections::{HashMap, HashSet},
	sync::Arc,
};

/// Limit the RPC functionality to a single connection.
#[derive(Default, Clone)]
pub struct RpcConnections {
	/// The number of tokens that can be registered for each connection.
	capacity: usize,
	/// Map the connecton ID to a set of tokens.
	data: Arc<Mutex<HashMap<ConnectionId, ConnectionData>>>,
}

#[derive(Default)]
struct ConnectionData {
	/// The total number of tokens.
	///
	/// # Note
	///
	/// Because a pending subscription sink does not expose the future subscription ID,
	/// we cannot register a token before the pending subscription is accepted.
	/// This variable ensures that we have enough capacity to register a token, after
	/// the subscription is accepted. Otherwise, a jsonrpc error object should be returned.
	num_tokens: usize,
	/// Active registered tokens.
	tokens: HashSet<String>,
}

impl RpcConnections {
	/// Constructs a new instance of [`RpcConnections`].
	pub fn new(capacity: usize) -> Self {
		RpcConnections { capacity, data: Default::default() }
	}

	/// Reserve space for a token.
	pub fn reserve_token(&self, connection_id: ConnectionId) -> Option<ReservedConnectionToken> {
		let mut data = self.data.lock();

		let entry = data.entry(connection_id).or_insert_with(ConnectionData::default);
		if entry.num_tokens >= self.capacity {
			return None;
		}
		entry.num_tokens += 1;

		Some(ReservedConnectionToken { connection_id, rpc_connections: Some(self.clone()) })
	}

	/// Gives back the reserved space before the token is registered.
	///
	/// # Note
	///
	/// This may happen if the pending subscription cannot be accepted (unlikely).
	fn unreserve_token(&self, connection_id: ConnectionId) {
		let mut data = self.data.lock();

		let entry = data.entry(connection_id).or_insert_with(ConnectionData::default);
		if entry.num_tokens > 0 {
			entry.num_tokens -= 1;
		}
	}

	/// Register a token for the given connection.
	///
	/// Users should call [`Self::reserve_token`] before calling this method.
	fn register_token(&self, connection_id: ConnectionId, token: String) {
		let mut data = self.data.lock();

		let entry = data.entry(connection_id).or_insert_with(ConnectionData::default);
		entry.tokens.insert(token);
	}

	/// Unregister a token for the given connection.
	fn unregister_token(&self, connection_id: ConnectionId, token: &str) {
		let mut data = self.data.lock();
		if let Some(connection_data) = data.get_mut(&connection_id) {
			connection_data.tokens.remove(token);
			connection_data.num_tokens -= 1;

			if connection_data.num_tokens == 0 {
				data.remove(&connection_id);
			}
		}
	}

	/// Check if the given connection contains the given token.
	pub fn contains_token(&self, connection_id: ConnectionId, token: &str) -> bool {
		let data = self.data.lock();
		data.get(&connection_id)
			.map(|connection_data| connection_data.tokens.contains(token))
			.unwrap_or(false)
	}
}

/// RAII wrapper that ensures the reserved space is given back if the object is
/// dropped before the token is registered.
pub struct ReservedConnectionToken {
	connection_id: ConnectionId,
	rpc_connections: Option<RpcConnections>,
}

impl ReservedConnectionToken {
	/// Register the token for the given connection.
	pub fn register(mut self, token: String) -> RegisteredConnectionToken {
		let rpc_connections = self
			.rpc_connections
			.take()
			.expect("Always constructed with rpc connections; qed");

		rpc_connections.register_token(self.connection_id, token.clone());
		RegisteredConnectionToken { connection_id: self.connection_id, token, rpc_connections }
	}
}

impl Drop for ReservedConnectionToken {
	fn drop(&mut self) {
		if let Some(rpc_connections) = self.rpc_connections.take() {
			rpc_connections.unreserve_token(self.connection_id);
		}
	}
}

/// RAII wrapper that ensures the token is unregistered if the object is dropped.
pub struct RegisteredConnectionToken {
	connection_id: ConnectionId,
	token: String,
	rpc_connections: RpcConnections,
}

impl Drop for RegisteredConnectionToken {
	fn drop(&mut self) {
		self.rpc_connections.unregister_token(self.connection_id, &self.token);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn register_token() {
		let rpc_connections = RpcConnections::new(2);
		assert!(rpc_connections.register_token(1, "token1".to_string()));
		assert!(rpc_connections.register_token(1, "token2".to_string()));
		// Cannot be registered due to exceeding limits.
		assert!(!rpc_connections.register_token(1, "token3".to_string()));
	}

	#[test]
	fn unregister_token() {
		let rpc_connections = RpcConnections::new(2);
		rpc_connections.register_token(1, "token1".to_string());
		rpc_connections.register_token(1, "token2".to_string());

		rpc_connections.unregister_token(1, "token1");
		assert!(!rpc_connections.contains_token(1, "token1"));
		assert!(rpc_connections.contains_token(1, "token2"));
	}
}
