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
	data: Arc<Mutex<HashMap<ConnectionId, HashSet<String>>>>,
}

impl RpcConnections {
	/// Constructs a new instance of [`RpcConnections`].
	pub fn new(capacity: usize) -> Self {
		RpcConnections { capacity, data: Default::default() }
	}

	/// Register a token for the given connection.
	///
	/// Returns true if the token can be registered, false otherwise.
	pub fn register_token(&self, connection_id: ConnectionId, token: String) -> bool {
		let mut data = self.data.lock();

		let mut entry = data.entry(connection_id).or_insert_with(HashSet::new);
		if entry.len() >= self.capacity {
			return false;
		}

		entry.insert(token)
	}

	/// Unregister a token for the given connection.
	pub fn unregister_token(&self, connection_id: ConnectionId, token: &str) {
		let mut data = self.data.lock();
		if let Some(tokens) = data.get_mut(&connection_id) {
			tokens.remove(token);
			if tokens.is_empty() {
				data.remove(&connection_id);
			}
		}
	}

	/// Check if the given connection contains the given token.
	pub fn contains_token(&self, connection_id: ConnectionId, token: &str) -> bool {
		let data = self.data.lock();
		data.get(&connection_id).map(|tokens| tokens.contains(token)).unwrap_or(false)
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
