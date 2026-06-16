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

//! DHT-targeted gossip path for the statement protocol.

mod explicit_affinity;
mod peer_steering;
mod peers_index;
pub mod peers_topology;

use crate::{affinity::AffinityFilter, LOG_TARGET};
use explicit_affinity::{AffinitySource, ExplicitAffinity};
use peer_steering::PeerSteering;
use peers_topology::{PeersTopology, PeersTopologyConfig};
use sc_network::{types::ProtocolName, NetworkPeers};
use sc_network_types::PeerId;
use sp_statement_store::{SubmitResult, Topic};
use std::collections::HashSet;

/// Coordinates the v2 DHT-affinity statement gossip path.
#[allow(dead_code)]
pub(crate) struct V2DhtOrchestrator {
	/// Local view of statement-store peers known and connected through network topology events.
	peers_topology: PeersTopology,
	/// Tracks the local node's topic affinity and the filters peers advertise.
	explicit_affinity: ExplicitAffinity,
	/// Keeps the connected peer set aligned with the peers needed to cover subscriptions.
	peer_steering: PeerSteering,
}

#[allow(dead_code)]
impl V2DhtOrchestrator {
	pub(crate) fn new(
		configured_topics: &[Topic],
		local_peer: PeerId,
		peers_topology_config: PeersTopologyConfig,
	) -> Self {
		Self {
			peers_topology: PeersTopology::new(local_peer, peers_topology_config),
			explicit_affinity: ExplicitAffinity::new(configured_topics),
			peer_steering: PeerSteering::new(),
		}
	}

	pub(crate) fn on_peers_discovered(&mut self, peers: impl IntoIterator<Item = PeerId>) {
		self.peers_topology.on_peers_discovered(peers);
	}

	pub(crate) fn on_peer_identified(&mut self, peer: PeerId, supports_statement_protocol: bool) {
		self.peers_topology.on_peer_identified(peer, supports_statement_protocol);
	}

	// === RPC-subscription source ===

	/// Refresh the topics the node has affinity for through its active RPC subscriptions.
	pub(crate) fn set_rpc_subscription_topics(&mut self, topics: &HashSet<Topic>) {
		self.explicit_affinity
			.replace_source_topics(AffinitySource::RpcSubscription, topics);
	}

	/// The topics this node currently has affinity for.
	#[cfg(test)]
	pub(crate) fn topics(&self) -> Vec<Topic> {
		self.explicit_affinity.topics()
	}

	// === Peer-set events ===

	pub(crate) fn on_peer_connected(&mut self, peer: PeerId) {
		// TODO: we may need it for the topology, remove if not
		log::trace!(target: LOG_TARGET, "v2dht: on_peer_connected {peer} (stub)");
	}

	pub(crate) fn on_peer_disconnected(&mut self, peer: PeerId) {
		// TODO: we may need it for the topology, remove if not
		log::trace!(target: LOG_TARGET, "v2dht: on_peer_disconnected {peer} (stub)");
	}

	// === Notification-substream events ===

	pub(crate) fn on_validate_inbound_substream(&mut self, peer: PeerId) {
		// TODO: we may need it for the peer steering, remove if not
		log::trace!(target: LOG_TARGET, "v2dht: on_validate_inbound_substream {peer} (stub)");
	}

	pub(crate) fn on_substream_opened(&mut self, peer: PeerId) {
		self.peers_topology.on_substream_opened(peer);
		self.peer_steering.on_substream_opened(peer);
		log::trace!(target: LOG_TARGET, "v2dht: on_substream_opened {peer}");
	}

	pub(crate) fn on_substream_closed(&mut self, peer: PeerId) {
		self.peers_topology.on_substream_closed(peer);
		self.peer_steering.on_substream_closed(peer);
		log::trace!(target: LOG_TARGET, "v2dht: on_substream_closed {peer}");
	}

	pub(crate) fn on_peer_filter_update(&mut self, peer: PeerId, _filter: AffinityFilter) {
		// TODO: we may need it for the topology or explicit affinity, remove if not
		log::trace!(target: LOG_TARGET, "v2dht: on_peer_filter_update {peer} (stub)");
	}

	// === Post-submit hook ===

	pub(crate) fn on_statement_imported(&mut self, peer: PeerId, _result: &SubmitResult) {
		// TODO: We may need to reflect the import result in the peer's score, remove if not
		log::trace!(target: LOG_TARGET, "v2dht: on_statement_imported {peer} (stub)");
	}

	// === Periodic ticks & post-iteration hooks ===

	pub(crate) async fn propagate_statements(&mut self) {
		// TODO: We need to know where to propagate
		log::trace!(target: LOG_TARGET, "v2dht: propagate_statements (stub)");
	}

	pub(crate) async fn on_initial_sync(&mut self) {
		// TODO: We need to know what to propagate
		log::trace!(target: LOG_TARGET, "v2dht: on_initial_sync (stub)");
	}

	/// Recompute the peers needed to cover the node's topics and hand them to peer steering.
	// TODO: `peers_for_topics` returns gaps only — it omits topics already served by a connected
	// peer and never lists connected peers. Peer steering reconciles toward this set and
	// disconnects connected peers absent from it, so a peer that starts covering its topic drops
	// out of the next result and gets disconnected, then reconnected — it flaps. A stable,
	// connection-independent coverage target (e.g. the closest known peers per topic) must replace
	// this before steering is enabled.
	pub(crate) fn on_pending_affinities(&mut self) {
		let topics = self.explicit_affinity.topics();
		let desired = self.peers_topology.peers_for_topics(&topics);
		self.peer_steering.update_peers_needing_connections(desired);
	}

	/// Align the connected peers with the peers needed to cover the node's subscriptions, opening
	/// and closing connections through `network`'s reserved set for `protocol`.
	pub(crate) fn refresh_connections<N: NetworkPeers>(
		&self,
		network: &N,
		protocol: &ProtocolName,
	) {
		self.peer_steering.refresh_connections(network, protocol);
	}

	pub(crate) fn on_major_sync_end(&mut self) {
		// TODO: The major sync processing may be different
		log::trace!(target: LOG_TARGET, "v2dht: on_major_sync_end (stub)");
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn topic(n: u8) -> Topic {
		Topic([n; 32])
	}

	#[test]
	fn affinity_tick_marks_coverage_peers_for_connection() {
		let mut orchestrator =
			V2DhtOrchestrator::new(&[topic(1)], PeerId::random(), PeersTopologyConfig::default());

		let peers: Vec<PeerId> = (0..5).map(|_| PeerId::random()).collect();
		orchestrator.on_peers_discovered(peers.clone());
		for peer in &peers {
			orchestrator.on_peer_identified(*peer, true);
		}

		// The tick turns the configured topic into a coverage target.
		orchestrator.on_pending_affinities();
		let desired = orchestrator.peers_topology.peers_for_topics(&[topic(1)]);
		assert!(!desired.is_empty());

		// Every coverage peer is queued for a connection, since none are connected yet.
		let connect = orchestrator.peer_steering.peers_to_connect();
		assert!(desired.iter().all(|peer| connect.contains(peer)));
	}
}
