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

use sc_network_types::PeerId;
use schnellru::{ByLength, LruMap};

/// The maximum number of disconnected peers to keep track of.
///
/// When a peer disconnects, we must keep track if it was in the middle of a request.
/// The peer may disconnect because it cannot keep up with the number of requests
/// (ie not having enough resources available to handle the requests); or because it is malicious.
const MAX_DISCONNECTED_PEERS_STATE: u32 = 512;

/// The time we are going to backoff a peer that has disconnected with an inflight request.
///
/// The backoff time is calculated as `num_disconnects * DISCONNECTED_PEER_BACKOFF_SECONDS`.
/// This is to prevent submitting a request to a peer that has disconnected because it could not
/// keep up with the number of requests.
///
/// The peer may disconnect due to the keep-alive timeout, however disconnections without
/// an inflight request are not tracked.
const DISCONNECTED_PEER_BACKOFF_SECONDS: u64 = 20;

/// Maximum number of disconnects with a request in flight before a peer is banned.
const MAX_NUM_DISCONNECTS: u64 = 3;

/// Forget the persistent state after 15 minutes.
const FORGET_PERSISTENT_STATE_SECONDS: u64 = 900;

pub struct DisconnectedPeerState {
	/// The total number of disconnects.
	num_disconnects: u64,
	/// The time at the last disconnect.
	last_disconnect: std::time::Instant,
}

impl DisconnectedPeerState {
	/// Create a new `DisconnectedPeerState`.
	pub fn new() -> Self {
		Self { num_disconnects: 1, last_disconnect: std::time::Instant::now() }
	}

	/// Increment the number of disconnects.
	pub fn increment(&mut self) {
		self.num_disconnects = self.num_disconnects.saturating_add(1);
		self.last_disconnect = std::time::Instant::now();
	}

	/// Get the number of disconnects.
	pub fn num_disconnects(&self) -> u64 {
		self.num_disconnects
	}

	/// Get the time of the last disconnect.
	pub fn last_disconnect(&self) -> std::time::Instant {
		self.last_disconnect
	}
}

pub struct PersistentPeersState {
	/// The state of disconnected peers.
	disconnected_peers: LruMap<PeerId, DisconnectedPeerState>,
}

impl PersistentPeersState {
	/// Create a new `PersistentPeersState`.
	pub fn new() -> Self {
		Self { disconnected_peers: LruMap::new(ByLength::new(MAX_DISCONNECTED_PEERS_STATE)) }
	}

	/// Insert a new peer to the persistent state if not seen before, or update the state if seen.
	///
	/// Returns true if the peer should be disconnected.
	pub fn remove_peer(&mut self, peer: PeerId) -> bool {
		if let Some(state) = self.disconnected_peers.get(&peer) {
			state.increment();
			return state.num_disconnects() >= MAX_NUM_DISCONNECTS
		}

		// First time we see this peer.
		self.disconnected_peers.insert(peer, DisconnectedPeerState::new());
		false
	}

	/// Check if a peer is available for queries.
	pub fn is_peer_available(&mut self, peer_id: &PeerId) -> bool {
		let Some(state) = self.disconnected_peers.get(peer_id) else {
			return true;
		};

		let elapsed = state.last_disconnect().elapsed();
		if elapsed.as_secs() > FORGET_PERSISTENT_STATE_SECONDS {
			self.disconnected_peers.remove(peer_id);
			return true;
		}

		elapsed.as_secs() >= DISCONNECTED_PEER_BACKOFF_SECONDS * state.num_disconnects
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_persistent_peer_state() {
		let mut state = PersistentPeersState::new();
		let peer = PeerId::random();

		// Is not part of the disconnected peers yet.
		assert_eq!(state.is_peer_available(&peer), true);

		assert_eq!(state.remove_peer(peer), false);
		assert_eq!(state.remove_peer(peer), false);
		assert_eq!(state.remove_peer(peer), true);

		// The peer is backed off.
		assert_eq!(state.is_peer_available(&peer), false);
	}

	#[test]
	fn ensure_backoff_time() {
		let mut state = PersistentPeersState::new();
		let peer = PeerId::random();

		assert_eq!(state.remove_peer(peer), false);
		assert_eq!(state.is_peer_available(&peer), false);

		// Wait until the backoff time has passed
		std::thread::sleep(Duration::from_secs(DISCONNECTED_PEER_BACKOFF_SECONDS + 1));

		assert_eq!(state.is_peer_available(&peer), true);
	}
}
