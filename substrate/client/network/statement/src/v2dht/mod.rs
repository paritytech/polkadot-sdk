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
mod peers_index;
pub mod peers_topology;

use crate::{affinity::AffinityFilter, LOG_TARGET};
use explicit_affinity::{AffinitySource, ExplicitAffinity};
use peers_topology::{PeersTopology, PeersTopologyConfig};
use sc_network_types::PeerId;
use sp_statement_store::{CategoryMask, Statement, SubmitResult, Topic};
use std::collections::HashSet;

/// Coordinates the v2 DHT-affinity statement gossip path.
#[allow(dead_code)]
pub(crate) struct V2DhtOrchestrator {
	/// Local view of statement-store peers known and connected through network topology events.
	peers_topology: PeersTopology,
	/// Tracks the local node's topic affinity and the filters peers advertise.
	explicit_affinity: ExplicitAffinity,
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

	// === Advertise own filter ===

	/// The [`AffinityFilter`] this node advertises, built from its current topics.
	pub(crate) fn local_filter(&self) -> AffinityFilter {
		self.explicit_affinity.local_filter()
	}

	/// The advertised filter if the local topic set changed since the last read, clearing the flag.
	pub(crate) fn take_local_filter_if_changed(&mut self) -> Option<AffinityFilter> {
		self.explicit_affinity.take_local_filter_if_changed()
	}

	// === Peer-set events ===

	pub(crate) fn on_peer_connected(&mut self, peer: PeerId) {
		// TODO: we may need it for the topology, remove if not
		log::trace!(target: LOG_TARGET, "v2dht: on_peer_connected {peer} (stub)");
	}

	pub(crate) fn on_peer_disconnected(&mut self, peer: PeerId) {
		self.explicit_affinity.on_peer_disconnected(peer);
	}

	// === Notification-substream events ===

	pub(crate) fn on_validate_inbound_substream(&mut self, peer: PeerId) {
		// TODO: we may need it for the peer steering, remove if not
		log::trace!(target: LOG_TARGET, "v2dht: on_validate_inbound_substream {peer} (stub)");
	}

	pub(crate) fn on_substream_opened(&mut self, peer: PeerId) {
		self.peers_topology.on_substream_opened(peer);
		log::trace!(target: LOG_TARGET, "v2dht: on_substream_opened {peer}");
	}

	pub(crate) fn on_substream_closed(&mut self, peer: PeerId) {
		self.peers_topology.on_substream_closed(peer);
		log::trace!(target: LOG_TARGET, "v2dht: on_substream_closed {peer}");
	}

	pub(crate) fn on_peer_filter_update(&mut self, peer: PeerId, filter: AffinityFilter) {
		self.explicit_affinity.on_peer_filter_update(peer, filter);
	}

	// === Forward decision ===

	/// Whether the peer's advertised filter accepts the statement.
	pub(crate) fn peer_has_explicit_affinity(&self, peer: PeerId, stmt: &Statement) -> bool {
		self.explicit_affinity.peer_has_explicit_affinity(peer, stmt)
	}

	// === Receive decision ===

	/// The reasons this node should store the statement.
	pub(crate) fn category_for(&self, stmt: &Statement) -> CategoryMask {
		let mut mask = CategoryMask::TRANSIENT;
		// TODO: can we pass the statement instead of topics to the `is_dht_affine`?
		if stmt.topics().iter().any(|topic| self.peers_topology.is_dht_affine(*topic)) {
			mask.insert(CategoryMask::DHT_AFFINITY);
		}
		if self.explicit_affinity.local_has_explicit_affinity(stmt) {
			mask.insert(CategoryMask::EXPLICIT_AFFINITY);
		}
		mask
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

	pub(crate) fn on_pending_affinities(&mut self) {
		// TODO: We need to know what to propagate
		log::trace!(target: LOG_TARGET, "v2dht: on_pending_affinities (stub)");
	}

	pub(crate) fn on_major_sync_end(&mut self) {
		// TODO: The major sync processing may be different
		log::trace!(target: LOG_TARGET, "v2dht: on_major_sync_end (stub)");
	}
}
#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_helpers::{filter_over, statement_on, topic};
	use std::num::NonZeroUsize;

	fn orchestrator() -> V2DhtOrchestrator {
		V2DhtOrchestrator::new(&[], PeerId::random(), PeersTopologyConfig::default())
	}

	fn config(replication_factor: usize) -> PeersTopologyConfig {
		PeersTopologyConfig {
			replication_factor: NonZeroUsize::new(replication_factor).expect("non-zero"),
			gossip_target: NonZeroUsize::new(1).expect("non-zero"),
		}
	}

	#[test]
	fn category_for_marks_explicit_affinity() {
		let orchestrator = V2DhtOrchestrator::new(&[topic(1)], PeerId::random(), config(20));

		let mask = orchestrator.category_for(&statement_on(topic(1)));

		assert!(mask.contains(CategoryMask::EXPLICIT_AFFINITY));
		assert!(mask.is_persistent());
	}

	#[test]
	fn category_for_marks_dht_affinity_when_local_is_a_replica() {
		// An empty topology makes the local node a replica for every topic.
		let orchestrator = V2DhtOrchestrator::new(&[], PeerId::random(), config(20));

		let mask = orchestrator.category_for(&statement_on(topic(9)));

		assert!(mask.contains(CategoryMask::DHT_AFFINITY));
		assert!(!mask.contains(CategoryMask::EXPLICIT_AFFINITY));
	}

	#[test]
	fn category_for_is_transient_without_any_affinity() {
		// One replica per topic, so a single closer peer displaces the local node.
		let mut orchestrator = V2DhtOrchestrator::new(&[], PeerId::random(), config(1));
		let peers = (0..32).map(|_| PeerId::random()).collect::<Vec<_>>();
		orchestrator.on_peers_discovered(peers.clone());
		for peer in &peers {
			orchestrator.on_peer_identified(*peer, true);
		}

		// A topic the local node is not the closest replica for, so DHT affinity does not hold.
		let topic = (0u8..=255)
			.map(topic)
			.find(|topic| !orchestrator.peers_topology.is_dht_affine(*topic))
			.expect("32 peers leave some topic without local DHT affinity");

		let mask = orchestrator.category_for(&statement_on(topic));

		assert!(!mask.is_persistent());
	}

	#[test]
	fn on_peer_filter_update_stores_the_filter() {
		let mut orchestrator = orchestrator();
		let peer = PeerId::random();

		orchestrator.on_peer_filter_update(peer, filter_over(&[topic(1)]));

		assert!(orchestrator.peer_has_explicit_affinity(peer, &statement_on(topic(1))));
		assert!(!orchestrator.peer_has_explicit_affinity(peer, &statement_on(topic(2))));
	}

	#[test]
	fn on_peer_disconnected_drops_the_filter() {
		let mut orchestrator = orchestrator();
		let peer = PeerId::random();
		orchestrator.on_peer_filter_update(peer, filter_over(&[topic(1)]));

		orchestrator.on_peer_disconnected(peer);

		assert!(!orchestrator.peer_has_explicit_affinity(peer, &statement_on(topic(1))));
	}

	#[test]
	fn take_local_filter_if_changed_reflects_subscription_topics() {
		let mut orchestrator = orchestrator();
		assert!(orchestrator.take_local_filter_if_changed().is_none());

		orchestrator.set_rpc_subscription_topics(&HashSet::from([topic(1)]));
		let filter = orchestrator
			.take_local_filter_if_changed()
			.expect("a subscription topic marks the filter changed");
		assert!(filter.contains(&topic(1)));
		assert!(orchestrator.take_local_filter_if_changed().is_none());
	}
}
