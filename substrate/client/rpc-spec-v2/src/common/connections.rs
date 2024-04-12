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

/// Connection state which keeps track whether a connection exist and
/// the number of concurrent operations.
#[derive(Default, Clone)]
pub struct RpcConnections {
	/// The number of identifiers that can be registered for each connection.
	///
	/// # Example
	///
	/// This is used to limit how many `chainHead_follow` subscriptions are active at one time.
	capacity: usize,
	/// Map the connecton ID to a set of identifiers.
	data: Arc<Mutex<HashMap<ConnectionId, ConnectionData>>>,
}

#[derive(Default)]
struct ConnectionData {
	/// The total number of identifiers for the given connection.
	///
	/// An identifier for a connection might be:
	/// - the subscription ID for chainHead_follow
	/// - the operation ID for the transactionBroadcast API
	/// - or simply how many times the transaction API has been called.
	///
	/// # Note
	///
	/// Because a pending subscription sink does not expose the future subscription ID,
	/// we cannot register a subscription ID before the pending subscription is accepted.
	/// This variable ensures that we have enough capacity to register an identifier, after
	/// the subscription is accepted. Otherwise, a jsonrpc error object should be returned.
	num_identifiers: usize,
	/// Active registered identifiers for the given connection.
	///
	/// # Note
	///
	/// For chainHead, this represents the subscription ID.
	/// For transactionBroadcast, this represents the operation ID.
	/// For transaction, this is empty and the number of active calls is tracked by
	/// [`Self::num_identifiers`].
	identifiers: HashSet<String>,
}

impl RpcConnections {
	/// Constructs a new instance of [`RpcConnections`].
	pub fn new(capacity: usize) -> Self {
		RpcConnections { capacity, data: Default::default() }
	}

	/// Reserve space for a new connection identifier.
	///
	/// If the number of active identifiers for the given connection exceeds the capacity,
	/// returns None.
	pub fn reserve_space(&self, connection_id: ConnectionId) -> Option<ReservedConnection> {
		let mut data = self.data.lock();

		let entry = data.entry(connection_id).or_insert_with(ConnectionData::default);
		if entry.num_identifiers >= self.capacity {
			return None;
		}
		entry.num_identifiers = entry.num_identifiers.saturating_add(1);

		Some(ReservedConnection { connection_id, rpc_connections: Some(self.clone()) })
	}

	/// Gives back the reserved space before the connection identifier is registered.
	///
	/// # Note
	///
	/// This may happen if the pending subscription cannot be accepted (unlikely).
	fn unreserve_space(&self, connection_id: ConnectionId) {
		let mut data = self.data.lock();

		let entry = data.entry(connection_id).or_insert_with(ConnectionData::default);
		entry.num_identifiers = entry.num_identifiers.saturating_sub(1);

		if entry.num_identifiers == 0 {
			data.remove(&connection_id);
		}
	}

	/// Register an identifier for the given connection.
	///
	/// Users must call [`Self::reserve_space`] before calling this method to ensure enough
	/// space is available.
	///
	/// Returns true if the identifier was inserted successfully, false if the identifier was
	/// already inserted or reached capacity.
	fn register_identifier(&self, connection_id: ConnectionId, identifier: String) -> bool {
		let mut data = self.data.lock();

		let entry = data.entry(connection_id).or_insert_with(ConnectionData::default);
		// Should be already checked `Self::reserve_space`.
		if entry.identifiers.len() >= self.capacity {
			return false;
		}

		entry.identifiers.insert(identifier)
	}

	/// Unregister an identifier for the given connection.
	fn unregister_identifier(&self, connection_id: ConnectionId, identifier: &str) {
		let mut data = self.data.lock();
		if let Some(connection_data) = data.get_mut(&connection_id) {
			connection_data.identifiers.remove(identifier);
			connection_data.num_identifiers = connection_data.num_identifiers.saturating_sub(1);

			if connection_data.num_identifiers == 0 {
				data.remove(&connection_id);
			}
		}
	}

	/// Check if the given connection contains the given identifier.
	pub fn contains_identifier(&self, connection_id: ConnectionId, identifier: &str) -> bool {
		let data = self.data.lock();
		data.get(&connection_id)
			.map(|connection_data| connection_data.identifiers.contains(identifier))
			.unwrap_or(false)
	}
}

/// RAII wrapper that ensures the reserved space is given back if the object is
/// dropped before the identifier is registered.
pub struct ReservedConnection {
	connection_id: ConnectionId,
	rpc_connections: Option<RpcConnections>,
}

impl ReservedConnection {
	/// Register the identifier for the given connection.
	pub fn register(mut self, identifier: String) -> Option<RegisteredConnection> {
		let rpc_connections = self.rpc_connections.take()?;

		if rpc_connections.register_identifier(self.connection_id, identifier.clone()) {
			Some(RegisteredConnection {
				connection_id: self.connection_id,
				identifier,
				rpc_connections,
			})
		} else {
			None
		}
	}
}

impl Drop for ReservedConnection {
	fn drop(&mut self) {
		if let Some(rpc_connections) = self.rpc_connections.take() {
			rpc_connections.unreserve_space(self.connection_id);
		}
	}
}

/// RAII wrapper that ensures the identifier is unregistered if the object is dropped.
pub struct RegisteredConnection {
	connection_id: ConnectionId,
	identifier: String,
	rpc_connections: RpcConnections,
}

impl Drop for RegisteredConnection {
	fn drop(&mut self) {
		self.rpc_connections.unregister_identifier(self.connection_id, &self.identifier);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn reserve_space() {
		let rpc_connections = RpcConnections::new(2);
		let reserved = rpc_connections.reserve_space(1);
		assert!(reserved.is_some());
		assert_eq!(1, rpc_connections.data.lock().get(&1).unwrap().num_identifiers);
		assert_eq!(rpc_connections.data.lock().len(), 1);

		let reserved = reserved.unwrap();
		let registered = reserved.register("identifier1".to_string()).unwrap();
		assert!(rpc_connections.contains_identifier(1, "identifier1"));
		assert_eq!(1, rpc_connections.data.lock().get(&1).unwrap().num_identifiers);
		drop(registered);

		// Data is dropped.
		assert!(rpc_connections.data.lock().get(&1).is_none());
		assert!(rpc_connections.data.lock().is_empty());
		// Checks can still happen.
		assert!(!rpc_connections.contains_identifier(1, "identifier1"));
	}

	#[test]
	fn reserve_space_capacity_reached() {
		let rpc_connections = RpcConnections::new(2);

		// Reserve identifier for connection 1.
		let reserved = rpc_connections.reserve_space(1);
		assert!(reserved.is_some());
		assert_eq!(1, rpc_connections.data.lock().get(&1).unwrap().num_identifiers);

		// Add identifier for connection 1.
		let reserved = reserved.unwrap();
		let registered = reserved.register("identifier1".to_string()).unwrap();
		assert!(rpc_connections.contains_identifier(1, "identifier1"));
		assert_eq!(1, rpc_connections.data.lock().get(&1).unwrap().num_identifiers);

		// Reserve identifier for connection 1 again.
		let reserved = rpc_connections.reserve_space(1);
		assert!(reserved.is_some());
		assert_eq!(2, rpc_connections.data.lock().get(&1).unwrap().num_identifiers);

		// Add identifier for connection 1 again.
		let reserved = reserved.unwrap();
		let registered_second = reserved.register("identifier2".to_string()).unwrap();
		assert!(rpc_connections.contains_identifier(1, "identifier2"));
		assert_eq!(2, rpc_connections.data.lock().get(&1).unwrap().num_identifiers);

		// Cannot reserve more identifiers.
		let reserved = rpc_connections.reserve_space(1);
		assert!(reserved.is_none());

		// Drop the first identifier.
		drop(registered);
		assert_eq!(1, rpc_connections.data.lock().get(&1).unwrap().num_identifiers);
		assert!(rpc_connections.contains_identifier(1, "identifier2"));
		assert!(!rpc_connections.contains_identifier(1, "identifier1"));

		// Can reserve again after clearing the space.
		let reserved = rpc_connections.reserve_space(1);
		assert!(reserved.is_some());
		assert_eq!(2, rpc_connections.data.lock().get(&1).unwrap().num_identifiers);

		// Ensure data is cleared.
		drop(reserved);
		drop(registered_second);
		assert!(rpc_connections.data.lock().get(&1).is_none());
	}
}
