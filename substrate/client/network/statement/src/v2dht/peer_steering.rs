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
//!
//! Each connected peer carries a score based on its actions. Undesired peers are
//! dropped lowest-score first.

use crate::LOG_TARGET;
use sc_network::{multiaddr, types::ProtocolName, NetworkPeers};
use sc_network_types::PeerId;
use std::{
	collections::{HashMap, HashSet},
	iter,
};

/// Per-connection score changes
pub(crate) mod score {
	/// A peer delivered a valid statement.
	pub(crate) const GOOD_ACTION: i32 = 1;
	/// A peer sent an invalid statement, a duplicate, or an undecodable message.
	pub(crate) const BAD_ACTION: i32 = -2;
}

/// Upper bound, as a percent of the connected peers, on how many connections a single refresh
/// closes. Capping the churn lets the connected set converge toward the desired one step by step
/// instead of dropping every undesired connection at once.
const MAX_DISCONNECT_PERCENT: usize = 20;

/// Keeps the connected peer set aligned with the peers needed to cover the node's subscriptions.
///
/// The connected set is fed by statement notification substream events; the desired set is supplied
/// by the orchestrator. [`Self::refresh_connections`] drives the two together.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct PeerSteering {
	/// Statement protocol whose reserved set the connections are steered through.
	protocol: ProtocolName,
	/// Peers with an open statement notification substream.
	connected: HashMap<PeerId, ConnectedPeer>,
	/// Monotonic counter handing each new connection its sequence.
	next_connection_seq: u64,
	/// Peers needed to cover the node's subscriptions.
	desired: HashSet<PeerId>,
}

/// The state of one connected peer.
#[derive(Debug)]
struct ConnectedPeer {
	/// Sequence at which the peer connected.
	seq: u64,
	/// Reputation over this connection.
	score: i32,
}

#[allow(dead_code)]
impl PeerSteering {
	pub(crate) fn new(protocol: ProtocolName) -> Self {
		Self {
			protocol,
			connected: HashMap::new(),
			next_connection_seq: 0,
			desired: HashSet::new(),
		}
	}

	// === Connection events ===

	/// Record that the statement notification substream to `peer` opened.
	pub(crate) fn on_substream_opened(&mut self, peer: PeerId) {
		if !self.connected.contains_key(&peer) {
			self.connected
				.insert(peer, ConnectedPeer { seq: self.next_connection_seq, score: 0 });
			self.next_connection_seq += 1;
		}
	}

	/// Record that the statement notification substream to `peer` closed.
	pub(crate) fn on_substream_closed(&mut self, peer: PeerId) {
		self.connected.remove(&peer);
	}

	/// Update peer's connection score.
	pub(crate) fn update_score(&mut self, peer: PeerId, change: i32) {
		if let Some(entry) = self.connected.get_mut(&peer) {
			entry.score = entry.score.saturating_add(change);
		}
	}

	/// Peer's current connection score, or `None` if it has no open substream.
	#[cfg(test)]
	pub(crate) fn score_of(&self, peer: &PeerId) -> Option<i32> {
		self.connected.get(peer).map(|entry| entry.score)
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
		self.desired
			.iter()
			.filter(|peer| !self.connected.contains_key(peer))
			.copied()
			.collect()
	}

	/// Connected peers no longer desired, capped at [`MAX_DISCONNECT_PERCENT`] of the connected
	/// peers (at least one while any remain) so the set converges step by step.
	pub(crate) fn peers_to_disconnect(&self) -> Vec<PeerId> {
		let limit = (self.connected.len() * MAX_DISCONNECT_PERCENT / 100).max(1);
		// Drop the lowest-scored peers first. Among peers of equal score, the longest-connected one
		// goes first, spreading disconnects across peers.
		let mut peers: Vec<(i32, u64, PeerId)> = self
			.connected
			.iter()
			.filter(|(peer, _)| !self.desired.contains(peer))
			.map(|(peer, entry)| (entry.score, entry.seq, *peer))
			.collect();
		peers.sort();
		peers.truncate(limit);
		peers.into_iter().map(|(_, _, peer)| peer).collect()
	}

	/// Align the connected set with the desired set by editing the statement protocol's reserved
	/// set on `network`.
	///
	/// The connected set tracks open substreams, so it updates through the substream events as the
	/// changes take effect, not here.
	pub(crate) fn refresh_connections<N: NetworkPeers>(&self, network: &N) {
		let connect = self.peers_to_connect();
		let disconnect = self.peers_to_disconnect();

		log::trace!(
			target: LOG_TARGET,
			"peer_steering: refresh_connections connect {} disconnect {}",
			connect.len(),
			disconnect.len(),
		);

		for peer in disconnect {
			if let Err(err) =
				network.remove_peers_from_reserved_set(self.protocol.clone(), vec![peer])
			{
				log::warn!(target: LOG_TARGET, "peer_steering: disconnect {peer} failed: {err}");
			}
		}
		for peer in connect {
			let addr =
				iter::once(multiaddr::Protocol::P2p(peer.into())).collect::<multiaddr::Multiaddr>();
			if let Err(err) =
				network.add_peers_to_reserved_set(self.protocol.clone(), iter::once(addr).collect())
			{
				log::warn!(target: LOG_TARGET, "peer_steering: connect {peer} failed: {err}");
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_helpers::peer;

	fn connected_set(steering: &PeerSteering) -> HashSet<PeerId> {
		steering.connected.keys().copied().collect()
	}

	#[test]
	fn substream_events_maintain_the_connected_set() {
		let mut steering = PeerSteering::new("/statement/test".into());

		steering.on_substream_opened(peer(1));
		steering.on_substream_opened(peer(2));
		assert_eq!(connected_set(&steering), HashSet::from([peer(1), peer(2)]));

		steering.on_substream_closed(peer(1));
		assert_eq!(connected_set(&steering), HashSet::from([peer(2)]));
	}

	#[test]
	fn connects_desired_and_disconnects_undesired() {
		let mut steering = PeerSteering::new("/statement/test".into());
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
		let mut steering = PeerSteering::new("/statement/test".into());
		steering.on_substream_opened(peer(1));
		steering.on_substream_opened(peer(2));
		steering.update_peers_needing_connections([peer(1), peer(2)]);

		assert!(steering.peers_to_connect().is_empty());
		assert!(steering.peers_to_disconnect().is_empty());
	}

	#[test]
	fn disconnects_are_capped_at_twenty_percent() {
		let mut steering = PeerSteering::new("/statement/test".into());
		for seed in 1..=10 {
			steering.on_substream_opened(peer(seed));
		}

		assert!(steering.peers_to_connect().is_empty());
		// Two of ten connected peers, the longest-connected, with none desired.
		assert_eq!(steering.peers_to_disconnect(), vec![peer(1), peer(2)]);
	}

	#[test]
	fn disconnects_drop_the_longest_connected_not_the_lowest_peer_id() {
		let mut steering = PeerSteering::new("/statement/test".into());
		// Connect the high peer ids first, so connection age and peer id rank disagree.
		for seed in [10u8, 9, 1, 2] {
			steering.on_substream_opened(peer(seed));
		}

		// peer(10) connected first, so it is dropped before the lower-id peers.
		assert_eq!(steering.peers_to_disconnect(), vec![peer(10)]);
	}

	#[test]
	fn score_outranks_connection_age_when_disconnecting() {
		let mut steering = PeerSteering::new("/statement/test".into());
		// peer(1) connects first, so connection age alone would drop it before peer(2).
		steering.on_substream_opened(peer(1));
		steering.on_substream_opened(peer(2));
		// The longest-connected peer behaves well, the younger one misbehaves.
		steering.update_score(peer(1), score::GOOD_ACTION);
		steering.update_score(peer(2), score::BAD_ACTION);

		// Score outranks age: the misbehaving peer is dropped despite being younger.
		assert_eq!(steering.peers_to_disconnect(), vec![peer(2)]);
	}

	#[test]
	fn update_score_ignores_unconnected_peers_and_resets_on_reconnect() {
		let mut steering = PeerSteering::new("/statement/test".into());

		// Reporting a peer without an open substream changes nothing.
		steering.update_score(peer(1), score::BAD_ACTION);
		assert!(steering.peers_to_disconnect().is_empty());

		// A fresh connection starts at 0, unaffected by the earlier report. Only the peer scored
		// down over this connection is dropped first.
		steering.on_substream_opened(peer(1));
		steering.on_substream_opened(peer(2));
		steering.update_score(peer(2), score::BAD_ACTION);
		assert_eq!(steering.peers_to_disconnect(), vec![peer(2)]);
	}

	#[test]
	fn capped_disconnects_drain_over_successive_refreshes() {
		let mut steering = PeerSteering::new("/statement/test".into());
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
}
