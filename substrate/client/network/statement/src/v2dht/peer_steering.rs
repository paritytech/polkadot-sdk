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

//! Peer steering for the v2 DHT gossip path.
//!
//! Implements [#11935](https://github.com/paritytech/polkadot-sdk/issues/11935): the module that
//! keeps the node connected to the peers it needs to cover its subscriptions.
//!
//! It tracks the connected peers and the peers needed for coverage.
//! [`PeerSteering::peers_to_connect`] and [`PeerSteering::peers_to_disconnect`] decide how to align
//! the former with the latter — the desired peers that are not connected and the connected peers no
//! longer desired — closing at most [`MAX_DISCONNECT_PERCENT`] of the connected peers per refresh
//! so the set converges over several refreshes rather than churning at once.
//! [`PeerSteering::refresh_connections`] applies that decision by editing the statement protocol's
//! reserved set: adding a peer dials it and keeps the notification substream open, removing one
//! drops it. The orchestrator supplies the coverage set via
//! [`PeerSteering::update_peers_needing_connections`].

use crate::LOG_TARGET;
use sc_network::{multiaddr, types::ProtocolName, NetworkPeers};
use sc_network_types::PeerId;
use std::collections::HashSet;

/// Upper bound, as a percent of the connected peers, on how many connections a single refresh
/// closes. Capping the churn lets the connected set converge toward the desired one step by step
/// instead of dropping every undesired connection at once.
const MAX_DISCONNECT_PERCENT: usize = 20;

/// Keeps the connected peer set aligned with the peers needed to cover the node's subscriptions.
///
/// The connected set is fed by statement notification substream events; the desired set is supplied
/// by the orchestrator. [`Self::refresh_connections`] drives the two together.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub(crate) struct PeerSteering {
	/// Peers with an open statement notification substream.
	connected: HashSet<PeerId>,
	/// Peers needed to cover the node's subscriptions.
	desired: HashSet<PeerId>,
}

#[allow(dead_code)]
impl PeerSteering {
	pub(crate) fn new() -> Self {
		Self::default()
	}

	// === Connection events ===

	/// Record that the statement notification substream to `peer` opened.
	pub(crate) fn on_substream_opened(&mut self, peer: PeerId) {
		self.connected.insert(peer);
	}

	/// Record that the statement notification substream to `peer` closed.
	pub(crate) fn on_substream_closed(&mut self, peer: PeerId) {
		self.connected.remove(&peer);
	}

	// === Coverage target ===

	/// Replace the set of peers needed to cover the node's subscriptions.
	pub(crate) fn update_peers_needing_connections(
		&mut self,
		peers: impl IntoIterator<Item = PeerId>,
	) {
		self.desired = peers.into_iter().collect();
	}

	// === Reconciliation ===

	/// Desired peers without an open connection.
	pub(crate) fn peers_to_connect(&self) -> Vec<PeerId> {
		self.desired.difference(&self.connected).copied().collect()
	}

	/// Connected peers no longer desired, capped at [`MAX_DISCONNECT_PERCENT`] of the connected
	/// peers (at least one while any remain) so the set converges step by step.
	pub(crate) fn peers_to_disconnect(&self) -> Vec<PeerId> {
		let limit = (self.connected.len() * MAX_DISCONNECT_PERCENT / 100).max(1);
		let mut peers: Vec<PeerId> = self.connected.difference(&self.desired).copied().collect();
		// No score yet to rank by, so drop the lowest peer ids; the order only needs to be stable
		// and the cap honored.
		peers.sort();
		peers.truncate(limit);
		peers
	}

	/// Align the connected set with the desired set by editing `network`'s reserved set for
	/// `protocol`.
	///
	/// The connected set tracks open substreams, so it updates through the substream events as the
	/// changes take effect, not here.
	pub(crate) fn refresh_connections<N: NetworkPeers>(
		&self,
		network: &N,
		protocol: &ProtocolName,
	) {
		let connect = self.peers_to_connect();
		let disconnect = self.peers_to_disconnect();

		log::trace!(
			target: LOG_TARGET,
			"peer_steering: refresh_connections connect {} disconnect {}",
			connect.len(),
			disconnect.len(),
		);

		for peer in disconnect {
			if let Err(err) = network.remove_peers_from_reserved_set(protocol.clone(), vec![peer]) {
				log::error!(target: LOG_TARGET, "peer_steering: disconnect {peer} failed: {err}");
			}
		}
		for peer in connect {
			let addr = std::iter::once(multiaddr::Protocol::P2p(peer.into()))
				.collect::<multiaddr::Multiaddr>();
			if let Err(err) =
				network.add_peers_to_reserved_set(protocol.clone(), std::iter::once(addr).collect())
			{
				log::error!(target: LOG_TARGET, "peer_steering: connect {peer} failed: {err}");
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn peer(seed: u8) -> PeerId {
		let mut bytes = [seed; 34];
		bytes[0] = 0;
		bytes[1] = 32;
		PeerId::from_bytes(&bytes).expect("identity multihash peer id")
	}

	fn as_set(peers: Vec<PeerId>) -> HashSet<PeerId> {
		peers.into_iter().collect()
	}

	#[test]
	fn substream_events_maintain_the_connected_set() {
		let mut steering = PeerSteering::new();

		steering.on_substream_opened(peer(1));
		steering.on_substream_opened(peer(2));
		assert_eq!(steering.connected, HashSet::from([peer(1), peer(2)]));

		steering.on_substream_closed(peer(1));
		assert_eq!(steering.connected, HashSet::from([peer(2)]));
	}

	#[test]
	fn connects_desired_and_disconnects_undesired() {
		let mut steering = PeerSteering::new();
		// peer(1) is connected and desired; peer(2) is connected but undesired; peer(3) is desired
		// but not connected.
		steering.on_substream_opened(peer(1));
		steering.on_substream_opened(peer(2));
		steering.update_peers_needing_connections([peer(1), peer(3)]);

		assert_eq!(steering.peers_to_connect(), vec![peer(3)]);
		assert_eq!(steering.peers_to_disconnect(), vec![peer(2)]);
	}

	#[test]
	fn converged_set_has_no_work() {
		let mut steering = PeerSteering::new();
		steering.on_substream_opened(peer(1));
		steering.on_substream_opened(peer(2));
		steering.update_peers_needing_connections([peer(1), peer(2)]);

		assert!(steering.peers_to_connect().is_empty());
		assert!(steering.peers_to_disconnect().is_empty());
	}

	#[test]
	fn without_connected_peers_only_connects() {
		let mut steering = PeerSteering::new();
		steering.update_peers_needing_connections([peer(1), peer(2)]);

		assert_eq!(as_set(steering.peers_to_connect()), HashSet::from([peer(1), peer(2)]));
		assert!(steering.peers_to_disconnect().is_empty());
	}

	#[test]
	fn disconnects_are_capped_at_twenty_percent() {
		let mut steering = PeerSteering::new();
		for seed in 1..=10 {
			steering.on_substream_opened(peer(seed));
		}

		assert!(steering.peers_to_connect().is_empty());
		// Two of ten connected peers, the lowest peer ids, with none desired.
		assert_eq!(steering.peers_to_disconnect(), vec![peer(1), peer(2)]);
	}

	#[test]
	fn capped_disconnects_drain_over_successive_refreshes() {
		let mut steering = PeerSteering::new();
		for seed in 1..=10 {
			steering.on_substream_opened(peer(seed));
		}

		let mut dropped = HashSet::new();
		while !steering.connected.is_empty() {
			let disconnect = steering.peers_to_disconnect();
			assert!(disconnect.len() <= 2, "each refresh drops at most a fifth");
			for peer in disconnect {
				assert!(dropped.insert(peer), "no peer dropped twice");
				steering.on_substream_closed(peer);
			}
		}

		assert_eq!(dropped.len(), 10);
	}

	#[test]
	fn empty_steering_has_no_work() {
		let steering = PeerSteering::new();
		assert!(steering.peers_to_connect().is_empty());
		assert!(steering.peers_to_disconnect().is_empty());
	}
}
