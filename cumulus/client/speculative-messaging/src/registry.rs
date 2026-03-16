// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <http://www.gnu.org/licenses/>.

//! Peer discovery registry for speculative messaging.
//!
//! The [`PeerRegistry`] trait abstracts how we look up which relay chain
//! peer handles messages for a given parachain. The MVP implementation
//! ([`HardcodedPeerRegistry`]) stores this mapping in memory, populated
//! from the `RelayPeers` pallet storage via runtime calls.
//!
//! Future implementations may use relay chain authority discovery or
//! DHT-based peer lookup.

use std::collections::HashMap;

use parking_lot::RwLock;
use polkadot_parachain_primitives::primitives::Id as ParaId;

/// Opaque peer identity bytes.
///
/// Represents a serialized `PeerId` from the relay chain network.
/// The conversion to/from concrete `libp2p::PeerId` or
/// `sc_network_types::PeerId` happens at the integration layer when
/// wiring the service into the node.
pub type OpaquePeerId = Vec<u8>;

/// Trait for looking up relay chain peers by parachain ID.
///
/// Implementations must be thread-safe since the registry is shared
/// between the service worker and the node's main loop.
pub trait PeerRegistry: Send + Sync {
	/// Get the relay chain peer identity for a given parachain.
	fn get_peer(&self, para_id: ParaId) -> Option<OpaquePeerId>;

	/// Register a relay chain peer for a given parachain.
	fn set_peer(&self, para_id: ParaId, peer_id: OpaquePeerId);

	/// Remove the relay chain peer registration for a given parachain.
	fn remove_peer(&self, para_id: ParaId);

	/// Return all registered (ParaId, PeerId) pairs.
	fn all_peers(&self) -> Vec<(ParaId, OpaquePeerId)>;
}

/// In-memory peer registry with hardcoded entries.
///
/// This is the MVP implementation where peers are configured manually,
/// for example from CLI flags, a config file, or by reading the
/// `RelayPeers` storage from `pallet-speculative-messaging`.
pub struct HardcodedPeerRegistry {
	peers: RwLock<HashMap<ParaId, OpaquePeerId>>,
}

impl HardcodedPeerRegistry {
	/// Create an empty registry.
	pub fn new() -> Self {
		Self { peers: RwLock::new(HashMap::new()) }
	}

	/// Create a registry pre-populated with the given entries.
	pub fn with_peers(peers: impl IntoIterator<Item = (ParaId, OpaquePeerId)>) -> Self {
		Self { peers: RwLock::new(peers.into_iter().collect()) }
	}
}

impl Default for HardcodedPeerRegistry {
	fn default() -> Self {
		Self::new()
	}
}

impl PeerRegistry for HardcodedPeerRegistry {
	fn get_peer(&self, para_id: ParaId) -> Option<OpaquePeerId> {
		self.peers.read().get(&para_id).cloned()
	}

	fn set_peer(&self, para_id: ParaId, peer_id: OpaquePeerId) {
		self.peers.write().insert(para_id, peer_id);
	}

	fn remove_peer(&self, para_id: ParaId) {
		self.peers.write().remove(&para_id);
	}

	fn all_peers(&self) -> Vec<(ParaId, OpaquePeerId)> {
		self.peers.read().iter().map(|(k, v)| (*k, v.clone())).collect()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn empty_registry_returns_none() {
		let registry = HardcodedPeerRegistry::new();
		assert!(registry.get_peer(ParaId::from(100)).is_none());
	}

	#[test]
	fn set_and_get_peer() {
		let registry = HardcodedPeerRegistry::new();
		let para = ParaId::from(100);
		let peer = vec![1, 2, 3, 4];

		registry.set_peer(para, peer.clone());
		assert_eq!(registry.get_peer(para), Some(peer));
	}

	#[test]
	fn set_overwrites_existing() {
		let registry = HardcodedPeerRegistry::new();
		let para = ParaId::from(100);

		registry.set_peer(para, vec![1]);
		registry.set_peer(para, vec![2]);
		assert_eq!(registry.get_peer(para), Some(vec![2]));
	}

	#[test]
	fn remove_peer() {
		let registry = HardcodedPeerRegistry::new();
		let para = ParaId::from(100);

		registry.set_peer(para, vec![1, 2, 3]);
		registry.remove_peer(para);
		assert!(registry.get_peer(para).is_none());
	}

	#[test]
	fn remove_nonexistent_is_noop() {
		let registry = HardcodedPeerRegistry::new();
		registry.remove_peer(ParaId::from(999)); // should not panic
	}

	#[test]
	fn all_peers() {
		let registry = HardcodedPeerRegistry::new();
		registry.set_peer(ParaId::from(100), vec![1]);
		registry.set_peer(ParaId::from(200), vec![2]);
		registry.set_peer(ParaId::from(300), vec![3]);

		let mut all = registry.all_peers();
		all.sort_by_key(|(id, _)| *id);
		assert_eq!(all.len(), 3);
		assert_eq!(all[0], (ParaId::from(100), vec![1]));
		assert_eq!(all[1], (ParaId::from(200), vec![2]));
		assert_eq!(all[2], (ParaId::from(300), vec![3]));
	}

	#[test]
	fn with_peers_constructor() {
		let registry = HardcodedPeerRegistry::with_peers([
			(ParaId::from(100), vec![1]),
			(ParaId::from(200), vec![2]),
		]);

		assert_eq!(registry.get_peer(ParaId::from(100)), Some(vec![1]));
		assert_eq!(registry.get_peer(ParaId::from(200)), Some(vec![2]));
		assert!(registry.get_peer(ParaId::from(300)).is_none());
	}

	#[test]
	fn default_is_empty() {
		let registry = HardcodedPeerRegistry::default();
		assert!(registry.all_peers().is_empty());
	}
}
