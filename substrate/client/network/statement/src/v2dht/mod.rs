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
use sp_statement_store::{Hash, Statement, SubmitResult, Topic};
use std::collections::{HashMap, HashSet};

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
		log::trace!(target: LOG_TARGET, "v2dht: on_substream_opened {peer}");
	}

	pub(crate) fn on_substream_closed(&mut self, peer: PeerId) {
		self.peers_topology.on_substream_closed(peer);
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

	/// For each connected peer, the indices of `statements` it should receive.
	///
	/// A peer is a target for a statement when it is a DHT routing target for one of the
	/// statement's topics ([`PeersTopology::routing_targets`]) or its advertised filter matches the
	/// statement ([`ExplicitAffinity::peer_has_explicit_affinity`]).
	pub(crate) fn propagation_plan(
		&self,
		statements: &[(Hash, Statement)],
	) -> Vec<(PeerId, Vec<usize>)> {
		let mut statements_by_peer: HashMap<PeerId, Vec<usize>> = HashMap::new();

		for (index, (_hash, statement)) in statements.iter().enumerate() {
			let mut targets: HashSet<PeerId> = HashSet::new();

			for topic in statement.topics() {
				targets.extend(self.peers_topology.routing_targets(*topic));
			}
			for peer in self.peers_topology.connected_peers() {
				if self.explicit_affinity.peer_has_explicit_affinity(peer, statement) {
					targets.insert(peer);
				}
			}

			for peer_id in targets {
				statements_by_peer.entry(peer_id).or_default().push(index);
			}
		}

		statements_by_peer.into_iter().collect()
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
	use std::num::NonZeroUsize;

	fn peer(seed: u8) -> PeerId {
		let mut bytes = [seed; 34];
		bytes[0] = 0;
		bytes[1] = 32;
		PeerId::from_bytes(&bytes).expect("identity multihash peer id")
	}

	fn config(replication_factor: usize, gossip_target: usize) -> PeersTopologyConfig {
		PeersTopologyConfig {
			replication_factor: NonZeroUsize::new(replication_factor).expect("non-zero"),
			gossip_target: NonZeroUsize::new(gossip_target).expect("non-zero"),
		}
	}

	fn orchestrator(local_seed: u8, config: PeersTopologyConfig) -> V2DhtOrchestrator {
		V2DhtOrchestrator::new(&[], peer(local_seed), config)
	}

	fn connect(orchestrator: &mut V2DhtOrchestrator, peer: PeerId) {
		orchestrator.on_peers_discovered([peer]);
		orchestrator.on_peer_identified(peer, true);
		orchestrator.on_substream_opened(peer);
	}

	fn statement(seed: u8, topics: &[Topic]) -> (Hash, Statement) {
		let mut statement = Statement::new();
		statement.set_plain_data(vec![seed]);
		for (index, topic) in topics.iter().enumerate() {
			statement.set_topic(index, *topic);
		}
		(statement.hash(), statement)
	}

	/// Routing targets `propagation_plan` is expected to reach for `topics`, taken from the
	/// already-tested [`PeersTopology::routing_targets`] rather than recomputed from XOR distances.
	fn routing_targets(orchestrator: &V2DhtOrchestrator, topics: &[Topic]) -> Vec<PeerId> {
		let mut targets: Vec<PeerId> = topics
			.iter()
			.flat_map(|topic| orchestrator.peers_topology.routing_targets(*topic))
			.collect();
		targets.sort();
		targets.dedup();
		targets
	}

	#[test]
	fn routes_a_statement_to_the_union_of_its_topics_routing_targets_once() {
		let mut orchestrator = orchestrator(1, config(20, 3));
		for seed in 2u8..=60 {
			connect(&mut orchestrator, peer(seed));
		}
		let topics = [Topic([7; 32]), Topic([42; 32])];
		let expected = routing_targets(&orchestrator, &topics);
		assert!(!expected.is_empty(), "fixture must yield routing targets");

		let plan = orchestrator.propagation_plan(&[statement(1, &topics)]);

		let mut planned: Vec<PeerId> = plan.iter().map(|(peer, _)| *peer).collect();
		planned.sort();
		assert_eq!(planned, expected);
		assert!(
			plan.iter().all(|(_, indices)| indices == &[0]),
			"a target receives the statement once however many of its topics route there"
		);
	}

	#[test]
	fn batches_statements_bound_for_the_same_peer() {
		let mut orchestrator = orchestrator(1, config(20, 3));
		for seed in 2u8..=60 {
			connect(&mut orchestrator, peer(seed));
		}
		let topic = Topic([7; 32]);
		let target = *routing_targets(&orchestrator, &[topic])
			.first()
			.expect("fixture must yield a routing target");

		let plan =
			orchestrator.propagation_plan(&[statement(1, &[topic]), statement(2, &[topic])]);

		let batch = plan
			.iter()
			.find(|(peer, _)| *peer == target)
			.map(|(_, indices)| indices.clone())
			.expect("the shared target must be planned");
		assert_eq!(batch, vec![0, 1]);
	}
}
