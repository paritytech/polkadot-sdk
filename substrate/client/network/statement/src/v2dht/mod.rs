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
mod metrics;
mod peer_steering;
mod peers_index;
pub mod peers_topology;

pub(crate) use metrics::V2DhtMetrics;

use crate::{affinity::AffinityFilter, LOG_TARGET};
use explicit_affinity::{AffinitySource, ExplicitAffinity, TopicAffinity};
use peer_steering::PeerSteering;
use peers_topology::{DhtAffinity, PeersTopology, PeersTopologyConfig};
use sc_network::{types::ProtocolName, NetworkPeers};
use sc_network_types::PeerId;
use sp_statement_store::{Hash, Statement, SubmitResult, Topic};
use std::{
	collections::{HashMap, HashSet},
	num::NonZeroUsize,
	sync::{Arc, RwLock},
	time::Instant,
};

/// The reasons a received statement is retained, as a bitmask of independent flags.
///
/// Each set bit records one reason the local node keeps the statement (DHT affinity, explicit
/// affinity). A non-empty mask persists the statement under the normal retention rules. An empty
/// mask marks it transient: held in memory until the next propagation, forwarded once, then dropped
/// without ever reaching the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RetentionReasonMask(u8);

impl RetentionReasonMask {
	/// No reason to persist: the store keeps the statement only until the next propagation.
	pub const TRANSIENT: RetentionReasonMask = RetentionReasonMask(0b00);
	/// The local node is one of the closest DHT replicas for one of the statement's topics.
	pub const DHT_AFFINITY: RetentionReasonMask = RetentionReasonMask(0b01);
	/// The local node has explicit affinity for one of the topics.
	pub const EXPLICIT_AFFINITY: RetentionReasonMask = RetentionReasonMask(0b10);

	/// A mask with every reason set.
	pub fn persistent() -> Self {
		RetentionReasonMask(u8::MAX)
	}

	/// Add `reason` to the mask.
	pub fn insert(&mut self, reason: RetentionReasonMask) {
		self.0 |= reason.0;
	}

	/// Whether `reason` is set.
	pub fn contains(&self, reason: RetentionReasonMask) -> bool {
		self.0 & reason.0 == reason.0 && reason.0 != 0
	}

	/// Whether the statement should be persisted.
	pub fn is_persistent(&self) -> bool {
		self.0 != 0
	}
}

/// Shared affinity view to derive a statement's retention mask.
#[derive(Clone)]
pub(crate) struct RetentionHandle {
	dht_affinity: Arc<RwLock<DhtAffinity>>,
	/// Topics the node has explicit affinity for.
	topic_affinity: Arc<RwLock<TopicAffinity>>,
}

impl RetentionHandle {
	/// A handle seeded with an empty topology, so its resolver persists every statement carrying a
	/// topic until the orchestrator publishes the learned affinity.
	pub(crate) fn new(local_peer: PeerId, replication_factor: NonZeroUsize) -> Self {
		Self {
			dht_affinity: Arc::new(RwLock::new(DhtAffinity::empty(local_peer, replication_factor))),
			topic_affinity: Arc::new(RwLock::new(TopicAffinity::default())),
		}
	}

	/// The resolver the store calls to derive a statement's retention mask.
	pub(crate) fn resolver(&self) -> Box<dyn Fn(&Statement) -> RetentionReasonMask + Send + Sync> {
		let dht_affinity = self.dht_affinity.clone();
		let topic_affinity = self.topic_affinity.clone();
		Box::new(move |stmt| {
			let (Ok(dht), Ok(topics)) = (dht_affinity.read(), topic_affinity.read()) else {
				log::error!(
					target: LOG_TARGET,
					"v2dht: retention affinity lock poisoned; persisting statement defensively",
				);
				return RetentionReasonMask::persistent();
			};
			let mut mask = RetentionReasonMask::TRANSIENT;
			if dht.is_affine(stmt) {
				mask.insert(RetentionReasonMask::DHT_AFFINITY);
			}
			if topics.is_affine(stmt) {
				mask.insert(RetentionReasonMask::EXPLICIT_AFFINITY);
			}
			mask
		})
	}

	/// Replace the DHT-affinity oracle
	fn set_dht_affinity(&self, dht_affinity: DhtAffinity) {
		if let Ok(mut cell) = self.dht_affinity.write() {
			*cell = dht_affinity;
		}
	}

	/// Replace the explicit topic-affinity oracle
	fn set_topic_affinity(&self, topic_affinity: TopicAffinity) {
		if let Ok(mut cell) = self.topic_affinity.write() {
			*cell = topic_affinity;
		}
	}
}

/// Coordinates the v2 DHT-affinity statement gossip path.
#[allow(dead_code)]
pub(crate) struct V2DhtOrchestrator {
	/// Local view of statement-store peers known and connected through network topology events.
	peers_topology: PeersTopology,
	/// Tracks the local node's topic affinity and the filters peers advertise.
	explicit_affinity: ExplicitAffinity,
	/// Keeps the connected peer set aligned with the peers needed to cover subscriptions.
	peer_steering: PeerSteering,
	/// Shared affinity view the store reads to decide statement retention.
	retention: Option<RetentionHandle>,
	/// Prometheus metrics.
	metrics: Option<V2DhtMetrics>,
}

#[allow(dead_code)]
impl V2DhtOrchestrator {
	pub(crate) fn new(
		configured_topics: &[Topic],
		local_peer: PeerId,
		peers_topology_config: PeersTopologyConfig,
		protocol: ProtocolName,
		metrics: Option<V2DhtMetrics>,
	) -> Self {
		Self {
			peers_topology: PeersTopology::new(local_peer, peers_topology_config),
			explicit_affinity: ExplicitAffinity::new(configured_topics),
			peer_steering: PeerSteering::new(protocol),
			retention: None,
			metrics,
		}
	}

	/// Install the handle the store reads to decide statement retention.
	pub(crate) fn set_retention_handle(&mut self, handle: RetentionHandle) {
		self.retention = Some(handle);
		// Seed both oracles from the current state so configured topics and the learned topology
		// drive retention before the first peer or subscription event publishes them.
		self.publish_dht_affinity();
		self.publish_topic_affinity();
	}

	/// The resolver the store calls to derive a statement's retention mask, if retention is wired.
	pub(crate) fn retention_resolver(
		&self,
	) -> Option<Box<dyn Fn(&Statement) -> RetentionReasonMask + Send + Sync>> {
		self.retention.as_ref().map(RetentionHandle::resolver)
	}

	/// Refresh the store's DHT-affinity oracle
	fn publish_dht_affinity(&self) {
		if let Some(handle) = &self.retention {
			handle.set_dht_affinity(self.peers_topology.dht_affinity());
		}
	}

	/// Refresh the store's explicit topic-affinity oracle
	fn publish_topic_affinity(&self) {
		if let Some(handle) = &self.retention {
			handle.set_topic_affinity(self.explicit_affinity.topic_affinity());
		}
	}

	fn report_topology_size(&self) {
		if let Some(metrics) = &self.metrics {
			metrics.set_topology_size(
				self.peers_topology.known_peers_count(),
				self.peers_topology.connected_peers().count(),
				self.peers_topology.dht_eligible_peers_count(),
			);
		}
	}

	pub(crate) fn on_peers_discovered(&mut self, peers: impl IntoIterator<Item = PeerId>) {
		self.peers_topology.on_peers_discovered(peers);
		self.report_topology_size();
	}

	pub(crate) fn on_peer_identified(&mut self, peer: PeerId, supports_statement_protocol: bool) {
		self.peers_topology.on_peer_identified(peer, supports_statement_protocol);
		// Changes the DHT peer index, hence affinity.
		self.publish_dht_affinity();
		self.report_topology_size();
	}

	// === RPC-subscription source ===

	/// Refresh the topics the node has affinity for through its active RPC subscriptions.
	pub(crate) fn set_rpc_subscription_topics(&mut self, topics: &HashSet<Topic>) {
		if self
			.explicit_affinity
			.replace_source_topics(AffinitySource::RpcSubscription, topics)
		{
			self.publish_topic_affinity();
		}
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
		self.peer_steering.on_substream_opened(peer);
		// Adds the peer to the DHT index, hence affinity.
		self.publish_dht_affinity();
		self.report_topology_size();
		log::trace!(target: LOG_TARGET, "v2dht: on_substream_opened {peer}");
	}

	pub(crate) fn on_substream_closed(&mut self, peer: PeerId) {
		self.peers_topology.on_substream_closed(peer);
		self.peer_steering.on_substream_closed(peer);
		self.report_topology_size();
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
		statements: &[(u64, Hash, Statement)],
	) -> Vec<(PeerId, Vec<usize>)> {
		let mut statements_by_peer: HashMap<PeerId, Vec<usize>> = HashMap::new();

		for (index, (_seq, _hash, statement)) in statements.iter().enumerate() {
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

	/// Recompute the peers needed to cover the node's topics and hand them to peer steering.
	pub(crate) fn on_pending_affinities(&mut self) {
		let topics = self.explicit_affinity.topics();
		let desired = self.peers_topology.peers_for_topics(&topics);
		self.peer_steering.update_peers_needing_connections(desired);
	}

	/// Align the connected peers with the peers needed to cover the node's subscriptions, opening
	/// and closing connections through the statement protocol's reserved set on `network`.
	pub(crate) fn refresh_connections<N: NetworkPeers>(&self, network: &N) {
		self.peer_steering.refresh_connections(network);
	}

	pub(crate) fn evict_stale_peers(&mut self) {
		self.peers_topology.evict(Instant::now());
		self.report_topology_size();
	}

	pub(crate) fn on_major_sync_end(&mut self) {
		// TODO: The major sync processing may be different
		log::trace!(target: LOG_TARGET, "v2dht: on_major_sync_end (stub)");
	}
}
#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_helpers::{filter_over, nz, peer, statement_on, topic, topology_config};

	fn orchestrator() -> V2DhtOrchestrator {
		V2DhtOrchestrator::new(
			&[],
			PeerId::random(),
			topology_config(20, 3),
			"/statement/test".into(),
			None,
		)
	}

	/// Like [`orchestrator`] but with a deterministic local identity, so the XOR routing
	/// distances the propagation tests assert on stay reproducible across runs.
	fn orchestrator_with(local_seed: u8, config: PeersTopologyConfig) -> V2DhtOrchestrator {
		V2DhtOrchestrator::new(&[], peer(local_seed), config, "/statement/test".into(), None)
	}

	fn statement(seed: u8, topics: &[Topic]) -> (u64, Hash, Statement) {
		let mut statement = Statement::new();
		statement.set_plain_data(vec![seed]);
		for (index, topic) in topics.iter().enumerate() {
			statement.set_topic(index, *topic);
		}
		(seed as u64, statement.hash(), statement)
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
		let mut orchestrator = orchestrator_with(1, topology_config(20, 3));
		for seed in 2u8..=60 {
			let peer = peer(seed);
			orchestrator.on_peers_discovered([peer]);
			orchestrator.on_peer_identified(peer, /* supports_statement_protocol */ true);
			orchestrator.on_substream_opened(peer);
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
		let mut orchestrator = orchestrator_with(1, topology_config(20, 3));
		for seed in 2u8..=60 {
			let peer = peer(seed);
			orchestrator.on_peers_discovered([peer]);
			orchestrator.on_peer_identified(peer, /* supports_statement_protocol */ true);
			orchestrator.on_substream_opened(peer);
		}
		let topic = Topic([7; 32]);
		let target = *routing_targets(&orchestrator, &[topic])
			.first()
			.expect("fixture must yield a routing target");

		let plan = orchestrator.propagation_plan(&[statement(1, &[topic]), statement(2, &[topic])]);

		let batch = plan
			.iter()
			.find(|(peer, _)| *peer == target)
			.map(|(_, indices)| indices.clone())
			.expect("the shared target must be planned");
		assert_eq!(batch, vec![0, 1]);
	}

	/// A DHT-affinity oracle from many peers with one replica per topic, leaving the local node
	/// non-affine for most topics, paired with one such non-affine topic.
	fn dht_without_local_affinity() -> (DhtAffinity, Topic) {
		let mut topology = PeersTopology::new(peer(1), topology_config(1, 1));
		let peers: Vec<_> = (2u8..=200).map(peer).collect();
		topology.on_peers_discovered(peers.clone());
		for peer in &peers {
			topology.on_peer_identified(*peer, /* supports_statement_protocol */ true);
		}
		let dht = topology.dht_affinity();
		let topic = (0u8..=255)
			.map(topic)
			.find(|topic| !dht.is_affine(&statement_on(*topic)))
			.expect("199 peers leave some topic without local DHT affinity");
		(dht, topic)
	}

	#[test]
	fn resolver_persists_with_an_empty_seeded_topology() {
		// Knowing no peers, the local node is the sole replica for every topic, so it persists.
		assert!(RetentionHandle::new(peer(1), nz(1)).resolver()(&statement_on(topic(1)))
			.is_persistent());
	}

	#[test]
	fn resolver_marks_explicit_affinity() {
		let (dht, topic) = dht_without_local_affinity();
		let handle = RetentionHandle::new(peer(1), nz(1));
		handle.set_dht_affinity(dht);
		handle.set_topic_affinity(ExplicitAffinity::new(&[topic]).topic_affinity());

		let mask = handle.resolver()(&statement_on(topic));

		assert!(mask.contains(RetentionReasonMask::EXPLICIT_AFFINITY));
		assert!(!mask.contains(RetentionReasonMask::DHT_AFFINITY));
		assert!(mask.is_persistent());
	}

	#[test]
	fn resolver_marks_dht_affinity_when_local_is_a_replica() {
		// An empty topology makes the local node a replica for every topic.
		let handle = RetentionHandle::new(peer(1), nz(1));
		handle.set_dht_affinity(PeersTopology::new(peer(1), topology_config(20, 1)).dht_affinity());

		let mask = handle.resolver()(&statement_on(topic(9)));

		assert!(mask.contains(RetentionReasonMask::DHT_AFFINITY));
		assert!(!mask.contains(RetentionReasonMask::EXPLICIT_AFFINITY));
	}

	#[test]
	fn resolver_is_transient_without_any_affinity() {
		let (dht, topic) = dht_without_local_affinity();
		let handle = RetentionHandle::new(peer(1), nz(1));
		handle.set_dht_affinity(dht);

		assert_eq!(handle.resolver()(&statement_on(topic)), RetentionReasonMask::TRANSIENT);
	}

	#[test]
	fn publish_dht_affinity_reflects_the_orchestrator_topology() {
		let mut orchestrator = V2DhtOrchestrator::new(
			&[],
			peer(1),
			topology_config(1, 1),
			"/statement/test".into(),
			None,
		);
		let handle = RetentionHandle::new(peer(1), nz(1));
		orchestrator.set_retention_handle(handle.clone());

		// Identify enough peers that the local node loses DHT affinity for some topic; each
		// `on_peer_identified` refreshes the published oracle.
		let peers: Vec<_> = (2u8..=200).map(peer).collect();
		orchestrator.on_peers_discovered(peers.clone());
		for peer in &peers {
			orchestrator.on_peer_identified(*peer, /* supports_statement_protocol */ true);
		}

		let dht = orchestrator.peers_topology.dht_affinity();
		let non_affine = (0u8..=255)
			.map(topic)
			.find(|topic| !dht.is_affine(&statement_on(*topic)))
			.expect("199 peers leave some topic without local DHT affinity");
		assert_eq!(handle.resolver()(&statement_on(non_affine)), RetentionReasonMask::TRANSIENT);
	}

	#[test]
	fn set_retention_handle_publishes_configured_topics() {
		// Configured topics must drive retention from the moment the handle is installed, before
		// any peer or subscription event.
		let mut orchestrator = V2DhtOrchestrator::new(
			&[topic(1)],
			peer(1),
			topology_config(1, 1),
			"/statement/test".into(),
			None,
		);
		let handle = RetentionHandle::new(peer(1), nz(1));
		orchestrator.set_retention_handle(handle.clone());

		assert!(handle.resolver()(&statement_on(topic(1)))
			.contains(RetentionReasonMask::EXPLICIT_AFFINITY));
	}

	#[test]
	fn set_rpc_subscription_topics_publishes_topic_affinity() {
		let mut orchestrator = V2DhtOrchestrator::new(
			&[],
			peer(1),
			topology_config(1, 1),
			"/statement/test".into(),
			None,
		);
		let handle = RetentionHandle::new(peer(1), nz(1));
		orchestrator.set_retention_handle(handle.clone());
		assert!(!handle.resolver()(&statement_on(topic(1)))
			.contains(RetentionReasonMask::EXPLICIT_AFFINITY));

		// A subscription change reaches the store's oracle.
		orchestrator.set_rpc_subscription_topics(&HashSet::from([topic(1)]));

		assert!(handle.resolver()(&statement_on(topic(1)))
			.contains(RetentionReasonMask::EXPLICIT_AFFINITY));
	}

	#[test]
	fn resolver_marks_both_affinities() {
		// An empty topology makes the local node a DHT replica for every topic; configure the same
		// topic so both retention reasons hold at once.
		let handle = RetentionHandle::new(peer(1), nz(1));
		handle.set_dht_affinity(PeersTopology::new(peer(1), topology_config(20, 1)).dht_affinity());
		handle.set_topic_affinity(ExplicitAffinity::new(&[topic(9)]).topic_affinity());

		let mask = handle.resolver()(&statement_on(topic(9)));

		assert!(mask.contains(RetentionReasonMask::DHT_AFFINITY));
		assert!(mask.contains(RetentionReasonMask::EXPLICIT_AFFINITY));
		assert!(mask.is_persistent());
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

	#[test]
	fn affinity_tick_marks_coverage_peers_for_connection() {
		let mut orchestrator = V2DhtOrchestrator::new(
			&[topic(1)],
			PeerId::random(),
			topology_config(20, 3),
			"/statement/test".into(),
			None,
		);

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

	#[test]
	fn connected_coverage_peer_is_never_disconnected() {
		let mut orchestrator = V2DhtOrchestrator::new(
			&[topic(1)],
			peer(1),
			topology_config(20, 3),
			"/statement/test".into(),
			None,
		);

		for seed in 2..=10 {
			orchestrator.on_peers_discovered([peer(seed)]);
			orchestrator.on_peer_identified(peer(seed), true);
		}

		// The first tick selects the coverage peers and queues them for connection.
		orchestrator.on_pending_affinities();
		let desired = orchestrator.peers_topology.peers_for_topics(&[topic(1)]);
		assert!(!desired.is_empty());

		// The coverage peers connect, then the next tick recomputes the target.
		for peer in &desired {
			orchestrator.on_substream_opened(*peer);
		}
		orchestrator.on_pending_affinities();

		// The connected coverage peers stay desired, so peer steering keeps them: nothing left to
		// connect, nothing to disconnect.
		assert!(orchestrator.peer_steering.peers_to_connect().is_empty());
		assert!(orchestrator.peer_steering.peers_to_disconnect().is_empty());
	}

	#[test]
	fn default_mask_is_transient() {
		assert_eq!(RetentionReasonMask::default(), RetentionReasonMask::TRANSIENT);
		assert!(!RetentionReasonMask::TRANSIENT.is_persistent());
		assert!(!RetentionReasonMask::TRANSIENT.contains(RetentionReasonMask::DHT_AFFINITY));
	}

	#[test]
	fn persistent_mask_holds_every_reason() {
		let mask = RetentionReasonMask::persistent();
		assert!(mask.is_persistent());
		assert!(mask.contains(RetentionReasonMask::DHT_AFFINITY));
		assert!(mask.contains(RetentionReasonMask::EXPLICIT_AFFINITY));
	}

	#[test]
	fn insert_sets_one_reason_at_a_time() {
		let mut mask = RetentionReasonMask::default();
		mask.insert(RetentionReasonMask::EXPLICIT_AFFINITY);
		assert!(mask.is_persistent());
		assert!(mask.contains(RetentionReasonMask::EXPLICIT_AFFINITY));
		assert!(!mask.contains(RetentionReasonMask::DHT_AFFINITY));
	}
}
