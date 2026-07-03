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

use super::peers_index::{Key, PeersIndex};
use sc_network_types::PeerId;
use sp_statement_store::Statement;
pub use sp_statement_store::Topic;
use std::{
	cmp::Reverse,
	collections::{HashMap, HashSet},
	num::NonZeroUsize,
	time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct PeersTopologyConfig {
	/// Number of statement-protocol peers responsible for storing a topic.
	///
	/// The DHT-affinity decision uses this to decide whether the local node belongs to the
	/// K-closest peers for a topic according to the locally learned topology.
	pub replication_factor: NonZeroUsize,
	/// Maximum number of connected nodes that we gossip to.
	///
	/// This caps `routing_targets`, i.e. the forwarding candidates selected from
	/// currently connected peers for a topic.
	pub gossip_target: NonZeroUsize,
}

#[derive(Debug, Clone)]
struct PeerInfo {
	supports_protocol: bool,
	/// The statement notification substream to the peer is open.
	connected: bool,
	/// Cached `peer_key`; the peer id never changes and hashing is costly.
	key: Key,
	/// Monotonic timestamp of the most recent event observing this peer; the eviction key for both
	/// staleness and the capacity backstop.
	last_seen: Instant,
}

/// Evict a disconnected peer unseen by any event for this long.
const PEER_STALENESS_TTL: Duration = Duration::from_secs(2 * 60 * 60);

/// Hard cap on `discovered`; bounds memory when discovery outruns staleness eviction.
const MAX_KNOWN_PEERS: usize = 8192;

/// Event-fed local view of statement-store peers that ages out peers unseen for
/// `PEER_STALENESS_TTL`.
///
/// The topology is built from peers learned through routing-table updates, identify metadata and
/// statement notification connections. It computes XOR distances locally over that learned peer
/// set; it does not issue topic-specific Kademlia lookups.
///
/// Topic queries walk the sorted key indexes in increasing XOR distance, without sorting the full
/// peer set.
#[derive(Debug, Clone)]
pub struct PeersTopology {
	local_peer: PeerId,
	local_key: Key,
	config: PeersTopologyConfig,
	/// Known remote peers, evicted by [`PeersTopology::evict`] once stale or over
	/// `MAX_KNOWN_PEERS`.
	discovered: HashMap<PeerId, PeerInfo>,
	/// XOR-ordered index of the statement-protocol peers: the candidates for DHT storage,
	/// affinity and forwarding decisions.
	discovered_index: PeersIndex,
	/// XOR-ordered index of the connected peers, i.e. those with an open statement notification
	/// substream.
	connected: PeersIndex,
}

impl PeersTopology {
	pub fn new(local_peer: PeerId, config: PeersTopologyConfig) -> Self {
		Self {
			local_peer,
			local_key: peer_key(&local_peer),
			config,
			discovered: HashMap::new(),
			discovered_index: PeersIndex::default(),
			connected: PeersIndex::default(),
		}
	}

	/// Record that a litep2p routing-table update saw `peers`.
	///
	/// `RoutingTableUpdate` is used as the discovery source because litep2p's internal
	/// Kademlia table is bucket-limited and may discard many discovered peers.
	pub fn on_peers_discovered(&mut self, peers: impl IntoIterator<Item = PeerId>) {
		for peer in peers {
			self.get_or_insert_peer(peer);
		}
	}

	/// Record statement-protocol support from identify protocol metadata.
	///
	/// Peers that do not support the statement protocol remain known but are excluded from DHT
	/// storage and forwarding decisions.
	pub fn on_peer_identified(&mut self, peer: PeerId, supports_statement_protocol: bool) {
		let info = self.get_or_insert_peer(peer);
		let key = info.key;
		info.supports_protocol = supports_statement_protocol;
		if supports_statement_protocol {
			self.discovered_index.insert(key, peer);
		} else {
			self.discovered_index.remove(key, &peer);
		}
	}

	/// Record that the statement notification substream opened.
	///
	/// An open substream implies statement-protocol support.
	pub fn on_substream_opened(&mut self, peer: PeerId) {
		let info = self.get_or_insert_peer(peer);
		let key = info.key;
		info.supports_protocol = true;
		info.connected = true;
		self.discovered_index.insert(key, peer);
		self.connected.insert(key, peer);
	}

	/// Record that the statement notification substream closed.
	pub fn on_substream_closed(&mut self, peer: PeerId) {
		let Some(info) = self.discovered.get_mut(&peer) else { return };
		let key = info.key;
		info.connected = false;
		info.last_seen = Instant::now();
		self.connected.remove(key, &peer);
	}

	#[allow(dead_code)]
	fn is_connected(&self, peer: &PeerId) -> bool {
		self.discovered.get(peer).is_some_and(|info| info.connected)
	}

	/// The connected statement-protocol peers, those with an open notification substream.
	pub fn connected_peers(&self) -> impl Iterator<Item = PeerId> + '_ {
		self.connected.peers()
	}

	/// Number of known remote peers, including peers without confirmed statement-protocol support.
	pub fn known_peers_count(&self) -> usize {
		self.discovered.len()
	}

	/// Number of known statement-protocol peers, the DHT storage, affinity and forwarding
	/// candidates.
	pub fn dht_eligible_peers_count(&self) -> usize {
		self.discovered_index.peers().count()
	}

	/// Closest known statement-protocol peers for `topic`.
	///
	/// "Closest" is computed over the locally learned statement-protocol peers, not by querying
	/// the network for the true global closest peers.
	#[allow(dead_code)]
	pub fn closest_known(&self, topic: Topic, limit: usize) -> Vec<PeerId> {
		self.closest_known_keyed(topic, limit)
			.into_iter()
			.map(|(peer, _)| peer)
			.collect()
	}

	/// A standalone DHT-affinity oracle, answering "is the local node a storage replica for a
	/// topic" off the handler thread.
	///
	/// Clones only the data the decision needs — the statement-protocol peer index and the local
	/// identity — not the full topology.
	///
	/// TODO: deep-copies the index on every peer-churn event that republishes the oracle.
	pub fn dht_affinity(&self) -> DhtAffinity {
		DhtAffinity {
			index: self.discovered_index.clone(),
			local_peer: self.local_peer,
			local_key: self.local_key,
			replication_factor: self.config.replication_factor,
		}
	}

	/// Connected peers to forward a statement for `topic` to, capped at `gossip_target`: those
	/// closer to the topic than the local node (routing the statement onward) and those that are
	/// themselves DHT replicas for it (co-replicas that must store it).
	pub fn routing_targets(&self, topic: Topic) -> Vec<PeerId> {
		// TODO: benchmark this per-statement path on large connected sets (see
		// benches/peers_topology.rs).
		let local = (self.local_key, self.local_peer);
		let local_distance = xor_distance(*topic, self.local_key);
		let k = self.config.replication_factor.get();
		self.connected
			.closest(*topic)
			.take_while(|(peer, key)| {
				xor_distance(*topic, *key) < local_distance ||
					is_peer_topic_affine(&self.discovered_index, local, (*key, *peer), k, topic)
			})
			.take(self.config.gossip_target.get())
			.map(|(peer, _)| peer)
			.collect()
	}

	/// Local-only explicit-affinity connection candidates for `topics`.
	///
	/// This uses only the locally learned topology, avoiding network lookups that would reveal
	/// explicit-affinity topics, and selects a minimal set of peers covering every topic,
	/// independent of connection state.
	pub fn peers_for_topics(&self, topics: &[Topic]) -> Vec<PeerId> {
		if topics.is_empty() {
			return Vec::new();
		}

		let pool_size = self.config.replication_factor.get();

		let closest_pools = topics
			.iter()
			.map(|topic| self.closest_known_keyed(*topic, pool_size))
			.collect::<Vec<_>>();

		let mut uncovered = (0..topics.len()).collect::<HashSet<_>>();

		let mut selected = Vec::new();
		let limit = topics.len();

		while !uncovered.is_empty() && selected.len() < limit {
			let Some(best_peer) = best_candidate(topics, &closest_pools, &uncovered, &selected)
			else {
				break;
			};

			selected.push(best_peer);
			uncovered.retain(|topic_idx| !pool_contains(&closest_pools[*topic_idx], &best_peer));
		}

		selected
	}

	/// Insert `peer` if absent, refresh its `last_seen`, and return its record.
	fn get_or_insert_peer(&mut self, peer: PeerId) -> &mut PeerInfo {
		let now = Instant::now();
		let info = self.discovered.entry(peer).or_insert_with(|| PeerInfo {
			supports_protocol: false,
			connected: false,
			key: peer_key(&peer),
			last_seen: now,
		});
		info.last_seen = now;
		info
	}

	/// Evict disconnected peers unseen for `PEER_STALENESS_TTL` as of `now`, plus any excess over
	/// `MAX_KNOWN_PEERS`.
	pub fn evict(&mut self, now: Instant) {
		loop {
			let over_cap = self.discovered.len() > MAX_KNOWN_PEERS;
			let Some((victim, last_seen)) = self
				.discovered
				.iter()
				.filter(|(_, info)| !info.connected)
				.min_by_key(|(_, info)| info.last_seen)
				.map(|(peer, info)| (*peer, info.last_seen))
			else {
				return;
			};
			if !over_cap && now.saturating_duration_since(last_seen) < PEER_STALENESS_TTL {
				return;
			}
			if let Some(info) = self.discovered.remove(&victim) {
				self.discovered_index.remove(info.key, &victim);
			}
		}
	}

	/// `closest_known` paired with each peer's key, so callers that compute further distances
	/// reuse the key instead of looking it up again.
	fn closest_known_keyed(&self, topic: Topic, limit: usize) -> Vec<(PeerId, Key)> {
		self.discovered_index.closest(*topic).take(limit).collect()
	}
}

/// The peer covering the most uncovered topics, breaking ties by the smallest distance to a
/// covered topic, then by the smallest peer id.
fn best_candidate(
	topics: &[Topic],
	pools: &[Vec<(PeerId, Key)>],
	uncovered: &HashSet<usize>,
	selected: &[PeerId],
) -> Option<PeerId> {
	uncovered
		.iter()
		.flat_map(|topic_idx| pools[*topic_idx].iter().copied())
		.filter(|(peer, _)| !selected.contains(peer))
		.collect::<HashSet<_>>()
		.into_iter()
		.map(|(peer, key)| {
			let (covered_count, best_distance) = uncovered
				.iter()
				.filter(|topic_idx| pool_contains(&pools[**topic_idx], &peer))
				.map(|topic_idx| xor_distance(*topics[*topic_idx], key))
				.fold((0usize, [u8::MAX; 32]), |(count, best), distance| {
					(count + 1, best.min(distance))
				});
			(peer, covered_count, best_distance)
		})
		.min_by_key(|&(peer, covered_count, best_distance)| {
			(Reverse(covered_count), best_distance, peer)
		})
		.map(|(peer, ..)| peer)
}

fn pool_contains(pool: &[(PeerId, Key)], peer: &PeerId) -> bool {
	pool.iter().any(|(candidate, _)| candidate == peer)
}

/// Map a peer id into the 32-byte topic key space.
///
/// Blake2b-256 spreads peer ids uniformly over the key space. The keys only need to be
/// consistent across statement-store nodes, which all derive them here; they need not match
/// litep2p's SHA-256 Kademlia keys because the topology never queries Kademlia by topic.
fn peer_key(peer: &PeerId) -> Key {
	sp_crypto_hashing::blake2_256(&peer.to_bytes())
}

fn xor_distance(a: Key, b: Key) -> Key {
	let mut distance = [0; 32];
	for ((distance, a), b) in distance.iter_mut().zip(a).zip(b) {
		*distance = a ^ b;
	}
	distance
}

/// Whether `candidate` is among the `k` closest to `topic`, ranked against the peers in `index`
/// plus the local node — i.e. topic-affine, a DHT storage replica for the topic.
fn is_peer_topic_affine(
	index: &PeersIndex,
	local: (Key, PeerId),
	candidate: (Key, PeerId),
	k: usize,
	topic: Topic,
) -> bool {
	let candidate_distance = (xor_distance(*topic, candidate.0), candidate.1);
	let local_node_is_closer = (xor_distance(*topic, local.0), local.1) < candidate_distance;
	let mut closer = index
		.closest(*topic)
		.take_while(|(peer, key)| (xor_distance(*topic, *key), *peer) < candidate_distance)
		.take(k)
		.count();
	// The local node is not in `index`, so count its slot here when it outranks the candidate.
	// For the oracle's self query (`candidate == local`) this never fires: a node never outranks
	// itself.
	if local_node_is_closer {
		closer += 1;
	}
	closer < k
}

/// Standalone DHT-affinity oracle: the minimal snapshot needed to answer "is the local node a
/// storage replica for a topic", shared with the store so it decides retention without the full
/// [`PeersTopology`].
#[derive(Clone)]
pub struct DhtAffinity {
	index: PeersIndex,
	local_peer: PeerId,
	local_key: Key,
	replication_factor: NonZeroUsize,
}

impl DhtAffinity {
	/// An oracle that knows no peers yet, so the local node is the sole replica for every topic.
	pub fn empty(local_peer: PeerId, replication_factor: NonZeroUsize) -> Self {
		Self {
			index: PeersIndex::default(),
			local_peer,
			local_key: peer_key(&local_peer),
			replication_factor,
		}
	}

	/// Whether the local node is one of the closest DHT storage replicas for any of the statement's
	/// topics, over the locally learned statement-store peers.
	pub fn is_affine(&self, stmt: &Statement) -> bool {
		stmt.topics().iter().any(|topic| self.is_topic_affine(*topic))
	}

	/// Whether the local node is one of the closest DHT storage replicas for `topic`.
	fn is_topic_affine(&self, topic: Topic) -> bool {
		let local = (self.local_key, self.local_peer);
		is_peer_topic_affine(&self.index, local, local, self.replication_factor.get(), topic)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_helpers::{peer, statement_on, topic, topology_config};
	use std::cmp::Ordering;

	fn distance_to(topic: Topic, peer: &PeerId) -> [u8; 32] {
		xor_distance(*topic, peer_key(peer))
	}

	fn cmp_distance_then_peer(topic: Topic, a: &PeerId, b: &PeerId) -> Ordering {
		distance_to(topic, a).cmp(&distance_to(topic, b)).then_with(|| a.cmp(b))
	}

	fn topology(local_seed: u8) -> PeersTopology {
		PeersTopology::new(peer(local_seed), topology_config(2, 2))
	}

	fn dht_peer(topology: &mut PeersTopology, peer: PeerId) {
		topology.on_peers_discovered([peer]);
		topology.on_peer_identified(peer, true);
	}

	fn known_dht_peers(topology: &PeersTopology, topic: Topic) -> Vec<PeerId> {
		topology.closest_known(topic, topology.known_peers_count())
	}

	#[test]
	fn new_topology_starts_empty() {
		let topology = topology(1);

		assert_eq!(topology.known_peers_count(), 0);
		assert!(topology.routing_targets(topic(1)).is_empty());
		assert!(!topology.is_connected(&peer(2)));
	}

	#[test]
	fn lifecycle_mutators_are_idempotent_and_filter_protocol_support() {
		let mut topology = topology(1);
		let supported = peer(2);
		let unsupported = peer(3);

		dht_peer(&mut topology, supported);
		dht_peer(&mut topology, supported);
		topology.on_peers_discovered([unsupported]);
		topology.on_peer_identified(unsupported, false);

		assert_eq!(topology.known_peers_count(), 2);
		assert_eq!(known_dht_peers(&topology, topic(9)), vec![supported]);

		topology.on_substream_opened(supported);
		assert!(topology.is_connected(&supported));

		topology.on_peer_identified(supported, false);
		assert!(known_dht_peers(&topology, topic(9)).is_empty());

		topology.on_substream_closed(supported);
		assert!(!topology.is_connected(&supported));
		assert_eq!(topology.known_peers_count(), 2);
	}

	#[test]
	fn substream_lifecycle_drives_routing_targets() {
		let mut topology = topology(1);
		let topic = topic(1);
		let self_distance = distance_to(topic, &peer(1));
		let peer = (2..=80)
			.map(peer)
			.find(|candidate| distance_to(topic, candidate) < self_distance)
			.expect("test peer fixture must include a peer closer than self");

		topology.on_peers_discovered([peer]);
		topology.on_peer_identified(peer, true);

		assert_eq!(topology.known_peers_count(), 1);
		assert_eq!(topology.closest_known(topic, 1), vec![peer]);
		assert!(topology.routing_targets(topic).is_empty());

		topology.on_substream_opened(peer);
		assert_eq!(topology.routing_targets(topic), vec![peer]);
		assert_eq!(topology.closest_known(topic, 1), vec![peer]);

		topology.on_substream_closed(peer);
		assert!(topology.routing_targets(topic).is_empty());
	}

	#[test]
	fn storage_affinity_uses_known_dht_peers_and_self() {
		let mut topology = topology(10);
		let peers = (11..=20).map(peer).collect::<Vec<_>>();

		for peer in &peers {
			dht_peer(&mut topology, *peer);
		}

		let topic = topic(42);
		let mut responsible = known_dht_peers(&topology, topic);
		responsible.push(peer(10));
		responsible.sort_by(|a, b| cmp_distance_then_peer(topic, a, b));
		let expected = responsible.into_iter().take(2).any(|p| p == peer(10));

		assert_eq!(topology.dht_affinity().is_affine(&statement_on(topic)), expected);
	}

	#[test]
	fn closest_known_returns_k_closest_peers_for_topic() {
		let mut topology = topology(1);
		let peers = (2..=20).map(peer).collect::<Vec<_>>();
		let topic = topic(42);

		for peer in &peers {
			dht_peer(&mut topology, *peer);
		}

		let mut expected = peers.clone();
		expected.sort_by(|a, b| cmp_distance_then_peer(topic, a, b));
		expected.truncate(5);

		assert_eq!(topology.closest_known(topic, 5), expected);
	}

	#[test]
	fn routing_targets_are_closest_connected_peers_closer_to_topic_than_self() {
		let mut topology = topology(1);
		let topic = topic(7);
		let self_distance = distance_to(topic, &peer(1));
		let mut closer = Vec::new();
		let mut farther = Vec::new();

		for seed in 2..=80 {
			let candidate = peer(seed);
			let distance = distance_to(topic, &candidate);
			if distance < self_distance && closer.len() < 3 {
				closer.push(candidate);
			} else if distance > self_distance && farther.len() < 3 {
				farther.push(candidate);
			}
		}
		assert_eq!(closer.len(), 3, "test peer fixture must include peers closer than self");
		assert_eq!(farther.len(), 3, "test peer fixture must include peers farther than self");

		for peer in closer.iter().chain(&farther) {
			dht_peer(&mut topology, *peer);
			topology.on_substream_opened(*peer);
		}

		let expected = {
			let mut peers = closer;
			peers.sort_by(|a, b| cmp_distance_then_peer(topic, a, b));
			peers.truncate(2);
			peers
		};

		assert_eq!(topology.routing_targets(topic), expected);
	}

	#[test]
	fn replica_forwards_to_a_farther_co_replica_but_not_to_non_replicas() {
		let mut topology = topology(1); // replication_factor = 2, gossip_target = 2
		let topic = topic(7);
		let self_distance = distance_to(topic, &peer(1));

		// Connected peers all farther from the topic than the local node, so the local node is the
		// closest and, with k=2, only the single nearest of them is its co-replica.
		let mut farther: Vec<PeerId> = (2..=80)
			.map(peer)
			.filter(|candidate| distance_to(topic, candidate) > self_distance)
			.take(3)
			.collect();
		assert_eq!(farther.len(), 3, "test peer fixture must include peers farther than self");
		farther.sort_by(|a, b| cmp_distance_then_peer(topic, a, b));

		for peer in &farther {
			dht_peer(&mut topology, *peer);
			topology.on_substream_opened(*peer);
		}

		// The co-replica (nearest farther peer) is forwarded to even though it is farther than
		// self; the farther non-replicas are not, so a replica does not spray every connected
		// peer.
		assert_eq!(topology.routing_targets(topic), vec![farther[0]]);
	}

	#[test]
	fn peers_for_topics_keeps_connected_coverage_peer() {
		let mut topology = topology(1);
		let topic = topic(9);

		for peer in (2..=10).map(peer) {
			dht_peer(&mut topology, peer);
		}

		let connected = topology.closest_known(topic, 1)[0];
		let before = topology.peers_for_topics(&[topic]);
		topology.on_substream_opened(connected);

		assert_eq!(topology.peers_for_topics(&[topic]), before);
		assert!(topology.peers_for_topics(&[topic]).contains(&connected));
	}

	#[test]
	fn peers_for_topics_is_independent_of_discovery_order() {
		let peers = (2..=30).map(peer).collect::<Vec<_>>();
		let topics = [topic(1), topic(9), topic(17)];

		let mut forward = topology(1);
		for peer in &peers {
			dht_peer(&mut forward, *peer);
		}
		let mut reverse = topology(1);
		for peer in peers.iter().rev() {
			dht_peer(&mut reverse, *peer);
		}

		let selected = forward.peers_for_topics(&topics);
		assert_eq!(selected, reverse.peers_for_topics(&topics));
		assert!(!selected.is_empty());
		assert!(selected.len() <= topics.len());

		// The cover is complete: every topic's candidate pool contains a selected peer.
		let pool_size = forward.config.replication_factor.get();
		for topic in topics {
			let pool = forward.closest_known(topic, pool_size);
			assert!(pool.iter().any(|peer| selected.contains(peer)));
		}

		// Peer ids and their blake2 keys are fixed, so the greedy cover must pick the same
		// peers in every process.
		let selected_seeds = selected
			.iter()
			.map(|selected| {
				(2..=30)
					.find(|seed| peer(*seed) == *selected)
					.expect("selected peer is a fixture peer")
			})
			.collect::<Vec<u8>>();
		assert_eq!(selected_seeds, vec![13]);
	}

	#[test]
	fn queries_match_naive_recomputation() {
		let local = peer(1);
		let mut topology = PeersTopology::new(local, topology_config(5, 3));
		let mut records = Vec::new();

		for seed in 2..=220 {
			let peer = peer(seed);
			let supports = seed % 3 != 0;
			let connected = seed % 4 == 0;
			topology.on_peers_discovered([peer]);
			if connected {
				topology.on_substream_opened(peer);
			}
			// Identify after the substream opens: a connected peer may withdraw protocol
			// support yet must remain a routing candidate.
			topology.on_peer_identified(peer, supports);
			records.push((peer, supports, connected));
		}
		let dht = topology.dht_affinity();

		for topic_seed in 0..32 {
			let topic = topic(topic_seed);

			let mut candidates = records
				.iter()
				.filter(|(_, supports, _)| *supports)
				.map(|(peer, ..)| *peer)
				.collect::<Vec<_>>();
			candidates.sort_by(|a, b| cmp_distance_then_peer(topic, a, b));
			assert_eq!(
				topology.closest_known(topic, 7),
				candidates.iter().copied().take(7).collect::<Vec<_>>()
			);

			let mut with_local = candidates.clone();
			with_local.push(local);
			with_local.sort_by(|a, b| cmp_distance_then_peer(topic, a, b));
			let expected_affine = with_local.iter().take(5).any(|peer| *peer == local);
			assert_eq!(dht.is_affine(&statement_on(topic)), expected_affine);

			let local_distance = distance_to(topic, &local);
			let mut connected = records
				.iter()
				.filter(|(_, _, connected)| *connected)
				.map(|(peer, ..)| *peer)
				.filter(|peer| distance_to(topic, peer) < local_distance)
				.collect::<Vec<_>>();
			connected.sort_by(|a, b| cmp_distance_then_peer(topic, a, b));
			connected.truncate(3);
			assert_eq!(topology.routing_targets(topic), connected);
		}
	}

	#[test]
	fn eviction_bounds_known_peers_at_capacity() {
		let mut topology = topology(1);

		for _ in 0..(MAX_KNOWN_PEERS + 50) {
			dht_peer(&mut topology, PeerId::random());
		}

		// Insertion no longer evicts; the periodic sweep bounds the set.
		topology.evict(Instant::now());

		assert_eq!(topology.known_peers_count(), MAX_KNOWN_PEERS);
	}

	#[test]
	fn eviction_removes_least_recently_seen_first() {
		let mut topology = topology(1);
		let oldest = PeerId::random();
		let next_oldest = PeerId::random();
		dht_peer(&mut topology, oldest);
		dht_peer(&mut topology, next_oldest);
		for _ in 2..MAX_KNOWN_PEERS {
			dht_peer(&mut topology, PeerId::random());
		}

		// Re-seeing the oldest peer makes the next-oldest the least-recently-seen instead.
		topology.on_peer_identified(oldest, true);

		// One new peer puts the set one over capacity; the sweep drops a single peer.
		dht_peer(&mut topology, PeerId::random());
		topology.evict(Instant::now());

		assert_eq!(topology.known_peers_count(), MAX_KNOWN_PEERS);
		let known = known_dht_peers(&topology, topic(7));
		assert!(known.contains(&oldest));
		assert!(!known.contains(&next_oldest));
	}

	#[test]
	fn staleness_keeps_peers_seen_within_the_ttl() {
		let mut topology = topology(1);
		let peer = peer(2);
		dht_peer(&mut topology, peer);

		let last_seen = topology.discovered[&peer].last_seen;
		topology.evict(last_seen + PEER_STALENESS_TTL - Duration::from_secs(1));

		assert_eq!(topology.known_peers_count(), 1);
	}

	#[test]
	fn staleness_never_drops_connected_peers() {
		let mut topology = topology(1);
		let live = peer(2);
		let idle = peer(3);
		dht_peer(&mut topology, live);
		topology.on_substream_opened(live);
		dht_peer(&mut topology, idle);

		topology.evict(Instant::now() + PEER_STALENESS_TTL + Duration::from_secs(1));

		assert!(topology.is_connected(&live));
		assert_eq!(topology.known_peers_count(), 1);
		assert!(!known_dht_peers(&topology, topic(7)).contains(&idle));
	}
}
