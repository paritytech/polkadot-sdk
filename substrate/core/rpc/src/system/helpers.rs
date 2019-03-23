// Copyright 2017-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Substrate system API helpers.

use std::fmt;
use serde_derive::{Serialize};
use serde_json::{Value, map::Map};

/// Node properties
pub type Properties = Map<String, Value>;

/// Running node's static details.
#[derive(Clone, Debug)]
pub struct SystemInfo {
	/// Implementation name.
	pub impl_name: String,
	/// Implementation version.
	pub impl_version: String,
	/// Chain name.
	pub chain_name: String,
	/// A custom set of properties defined in the chain spec.
	pub properties: Properties,
}

/// Health struct returned by the RPC
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
	/// Number of connected peers
	pub peers: usize,
	/// Is the node syncing
	pub is_syncing: bool,
	/// Should this node have any peers
	///
	/// Might be false for local chains or when running without discovery.
	pub should_have_peers: bool,
}

/// Network Peer information
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerInfo<Hash, Number> {
	/// Peer ID
	pub peer_id: String,
	/// Roles
	pub roles: String,
	/// Protocol version
	pub protocol_version: u32,
	/// Peer best block hash
	pub best_hash: Hash,
	/// Peer best block number
	pub best_number: Number,
}

impl fmt::Display for Health {
	fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
		write!(fmt, "{} peers ({})", self.peers, if self.is_syncing {
			"syncing"
		} else { "idle" })
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn should_serialize_health() {
		assert_eq!(
			::serde_json::to_string(&Health {
				peers: 1,
				is_syncing: false,
				should_have_peers: true,
			}).unwrap(),
			r#"{"peers":1,"isSyncing":false,"shouldHavePeers":true}"#,
		);
	}

	#[test]
	fn should_serialize_peer_info() {
		assert_eq!(
			::serde_json::to_string(&PeerInfo {
				peer_id: "2".into(),
				roles: "a".into(),
				protocol_version: 2,
				best_hash: 5u32,
				best_number: 6u32,
			}).unwrap(),
			r#"{"peerId":"2","roles":"a","protocolVersion":2,"bestHash":5,"bestNumber":6}"#,
		);
	}
}
