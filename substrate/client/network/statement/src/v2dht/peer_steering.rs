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
//! keeps the node connected to the peers it needs: those covering its subscriptions and its
//! routing peers.
//!
//! The module keeps three sets of peers: the peers with an open substream, the peers the node
//! needs connected (the desired set), and the peers it holds in the statement protocol's reserved
//! set. Each refresh moves the reserved set toward the desired one. A desired
//! peer gets reserved, which dials it and keeps its substream open. A reserved peer no longer
//! desired is released, which drops a peer this node dialed and leaves a peer that dialed in as
//! an ordinary inbound peer while an inbound slot is free. A refresh releases at most
//! [`MAX_DISCONNECT_PERCENT`] of the connected peers, so the set converges over several
//! refreshes instead of churning at once.
//! The orchestrator supplies the desired set through
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

/// Per-connection score changes.
///
/// The weights are a first cut, not a tuned reputation model.
pub(crate) mod score {
	/// A peer delivered a valid statement.
	pub(crate) const GOOD_ACTION: i32 = 1;
	/// A peer sent an invalid statement, a duplicate, or an undecodable message.
	pub(crate) const BAD_ACTION: i32 = -50;
	/// A peer resent a banned statement.
	pub(crate) const WASTEFUL_ACTION: i32 = -1;
}

/// Upper bound, as a percent of the connected peers, on how many connections a single refresh
/// closes. Capping the churn lets the connected set converge toward the desired one step by step
/// instead of dropping every undesired connection at once.
const MAX_DISCONNECT_PERCENT: usize = 20;

/// Keeps the statement protocol's reserved set aligned with the peers the node needs connected.
///
/// The desired set comes from the orchestrator and the connected set from the statement substream
/// events. [`Self::refresh_connections`] edits the reserved set to match the desired one.
#[derive(Debug)]
pub(crate) struct PeerSteering {
	/// Statement protocol whose reserved set the connections are steered through.
	protocol: ProtocolName,
	/// Peers with an open statement notification substream.
	connected: HashMap<PeerId, ConnectedPeer>,
	/// Monotonic counter handing each new connection its sequence.
	next_connection_seq: u64,
	/// Peers the node needs connected: those covering its subscriptions and its routing peers.
	desired: HashSet<PeerId>,
	/// Peers held in the statement protocol's reserved set.
	reserved: HashSet<PeerId>,
}

/// The state of one connected peer.
#[derive(Debug)]
struct ConnectedPeer {
	/// Sequence at which the peer connected.
	seq: u64,
	/// Reputation over this connection.
	score: i32,
}

impl PeerSteering {
	pub(crate) fn new(protocol: ProtocolName) -> Self {
		Self {
			protocol,
			connected: HashMap::new(),
			next_connection_seq: 0,
			desired: HashSet::new(),
			reserved: HashSet::new(),
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

	// === Desired set ===

	/// Replace the set of peers the node needs connected.
	pub(crate) fn update_peers_needing_connections(
		&mut self,
		peers: impl IntoIterator<Item = PeerId>,
	) {
		self.desired = peers.into_iter().collect();
	}

	/// Whether `peer` is held in the statement protocol's reserved set.
	pub(crate) fn is_reserved(&self, peer: &PeerId) -> bool {
		self.reserved.contains(peer)
	}

	#[cfg(test)]
	pub(crate) fn reserve(&mut self, peers: impl IntoIterator<Item = PeerId>) {
		self.reserved.extend(peers);
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

	/// Desired peers outside the reserved set.
	pub(crate) fn peers_to_reserve(&self) -> Vec<PeerId> {
		self.desired
			.iter()
			.filter(|peer| !self.reserved.contains(peer))
			.copied()
			.collect()
	}

	/// Reserved peers no longer desired. A peer without an open substream is released at once. The
	/// connected ones go at most [`MAX_DISCONNECT_PERCENT`] of the connected peers per refresh, and
	/// at least one, so the set converges step by step.
	pub(crate) fn peers_to_release(&self) -> Vec<PeerId> {
		let limit = (self.connected.len() * MAX_DISCONNECT_PERCENT / 100).max(1);
		let mut to_release = Vec::new();
		// Drop the lowest-scored peers first. Among peers of equal score, the longest-connected one
		// goes first, spreading disconnects across peers.
		let mut to_disconnect: Vec<(i32, u64, PeerId)> = Vec::new();
		for peer in self.reserved.iter().filter(|peer| !self.desired.contains(peer)) {
			match self.connected.get(peer) {
				Some(entry) => to_disconnect.push((entry.score, entry.seq, *peer)),
				None => to_release.push(*peer),
			}
		}
		to_disconnect.sort();
		to_disconnect.truncate(limit);
		to_release.extend(to_disconnect.into_iter().map(|(_, _, peer)| peer));
		to_release
	}

	/// Edit the statement protocol's reserved set on `network` so it matches the desired set.
	///
	/// The connected set tracks open substreams, so it updates through the substream events as the
	/// changes take effect, not here.
	pub(crate) fn refresh_connections<N: NetworkPeers>(&mut self, network: &N) {
		let reserve = self.peers_to_reserve();
		let release = self.peers_to_release();

		log::trace!(
			target: LOG_TARGET,
			"peer_steering: refresh_connections reserve {} release {}",
			reserve.len(),
			release.len(),
		);

		// Leaving the reserved set drops a peer this node dialed only while the set has no
		// outbound slots, otherwise the controller moves it to a free slot and an explicit
		// disconnect is needed.
		for peer in release {
			match network.remove_peers_from_reserved_set(self.protocol.clone(), vec![peer]) {
				Ok(()) => {
					self.reserved.remove(&peer);
				},
				Err(err) => {
					log::warn!(target: LOG_TARGET, "peer_steering: release {peer} failed: {err}");
				},
			}
		}
		for peer in reserve {
			let addr =
				iter::once(multiaddr::Protocol::P2p(peer.into())).collect::<multiaddr::Multiaddr>();
			match network
				.add_peers_to_reserved_set(self.protocol.clone(), iter::once(addr).collect())
			{
				Ok(()) => {
					self.reserved.insert(peer);
				},
				Err(err) => {
					log::warn!(target: LOG_TARGET, "peer_steering: reserve {peer} failed: {err}");
				},
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

	/// Opens a substream to each peer and marks it reserved, as a completed refresh leaves it.
	fn connect_reserved(steering: &mut PeerSteering, seeds: impl IntoIterator<Item = u8>) {
		for seed in seeds {
			steering.on_substream_opened(peer(seed));
			steering.reserve([peer(seed)]);
		}
	}

	#[test]
	fn reserves_desired_and_releases_undesired() {
		let mut steering = PeerSteering::new("/statement/test".into());
		// peer(1) is reserved and desired; peer(2) is reserved but undesired; peer(3) is desired
		// but not reserved; peer(4) is connected on its own and undesired.
		connect_reserved(&mut steering, [1, 2]);
		steering.on_substream_opened(peer(4));
		steering.update_peers_needing_connections([peer(1), peer(3)]);

		assert_eq!(steering.peers_to_reserve(), vec![peer(3)]);
		assert_eq!(steering.peers_to_release(), vec![peer(2)]);
	}

	#[test]
	fn desired_peer_connected_on_its_own_is_reserved() {
		let mut steering = PeerSteering::new("/statement/test".into());
		steering.on_substream_opened(peer(1));
		steering.update_peers_needing_connections([peer(1)]);

		assert!(steering.peers_to_connect().is_empty());
		assert_eq!(steering.peers_to_reserve(), vec![peer(1)]);
	}

	#[test]
	fn converged_set_has_no_work() {
		let mut steering = PeerSteering::new("/statement/test".into());
		connect_reserved(&mut steering, [1, 2]);
		steering.update_peers_needing_connections([peer(1), peer(2)]);

		assert!(steering.peers_to_reserve().is_empty());
		assert!(steering.peers_to_release().is_empty());
	}

	#[test]
	fn unconnected_reserved_peers_are_released_at_once() {
		let mut steering = PeerSteering::new("/statement/test".into());
		connect_reserved(&mut steering, [1, 2, 3, 4, 5]);
		steering.reserve([peer(6), peer(7)]);

		// The two idle peers go in full, plus one connected peer under the cap.
		let release = steering.peers_to_release();
		assert_eq!(release.len(), 3);
		assert!(release.contains(&peer(6)));
		assert!(release.contains(&peer(7)));
		assert!(release.contains(&peer(1)));
	}

	#[test]
	fn disconnects_are_capped_at_twenty_percent() {
		let mut steering = PeerSteering::new("/statement/test".into());
		connect_reserved(&mut steering, 1..=10);

		assert!(steering.peers_to_reserve().is_empty());
		// Two of ten connected peers, the longest-connected, with none desired.
		assert_eq!(steering.peers_to_release(), vec![peer(1), peer(2)]);
	}

	#[test]
	fn disconnects_drop_the_longest_connected_not_the_lowest_peer_id() {
		let mut steering = PeerSteering::new("/statement/test".into());
		// Connect the high peer ids first, so connection age and peer id rank disagree.
		connect_reserved(&mut steering, [10u8, 9, 1, 2]);

		// peer(10) connected first, so it is dropped before the lower-id peers.
		assert_eq!(steering.peers_to_release(), vec![peer(10)]);
	}

	#[test]
	fn score_outranks_connection_age_when_disconnecting() {
		let mut steering = PeerSteering::new("/statement/test".into());
		// peer(1) connects first, so connection age alone would drop it before peer(2).
		connect_reserved(&mut steering, [1, 2]);
		// The longest-connected peer behaves well, the younger one misbehaves.
		steering.update_score(peer(1), score::GOOD_ACTION);
		steering.update_score(peer(2), score::BAD_ACTION);

		// Score outranks age: the misbehaving peer is dropped despite being younger.
		assert_eq!(steering.peers_to_release(), vec![peer(2)]);
	}

	#[test]
	fn update_score_ignores_unconnected_peers_and_resets_on_reconnect() {
		let mut steering = PeerSteering::new("/statement/test".into());

		// Reporting a peer without an open substream changes nothing.
		steering.update_score(peer(1), score::BAD_ACTION);
		assert!(steering.peers_to_release().is_empty());

		// A fresh connection starts at 0, unaffected by the earlier report. Only the peer scored
		// down over this connection is dropped first.
		connect_reserved(&mut steering, [1, 2]);
		steering.update_score(peer(2), score::BAD_ACTION);
		assert_eq!(steering.peers_to_release(), vec![peer(2)]);
	}

	#[test]
	fn capped_disconnects_drain_over_successive_refreshes() {
		let mut steering = PeerSteering::new("/statement/test".into());
		connect_reserved(&mut steering, 1..=10);

		let mut dropped = HashSet::new();
		while !steering.connected.is_empty() {
			let release = steering.peers_to_release();
			assert!(release.len() <= 2, "each refresh drops at most a fifth");
			for peer in release {
				assert!(dropped.insert(peer), "no peer dropped twice");
				steering.reserved.remove(&peer);
				steering.on_substream_closed(peer);
			}
		}

		assert_eq!(dropped.len(), 10);
	}
}
