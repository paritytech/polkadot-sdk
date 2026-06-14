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

use bitvec::{order::Msb0, view::BitView};
use sc_network_types::PeerId;
pub use sp_statement_store::Topic;
use std::{
	cmp::Reverse,
	collections::{BTreeMap, HashMap, HashSet},
	num::NonZeroUsize,
	ops::Bound,
};

/// A point in the 32-byte key space shared by topics and hashed peer ids.
type Key = [u8; 32];
type KeyRange = (Bound<Key>, Bound<Key>);

#[derive(Debug, Clone)]
pub struct PeersTopologyConfig {
	/// Number of statement-protocol peers responsible for storing a topic.
	///
	/// `is_dht_affine` uses this to decide whether the local node belongs to the
	/// K-closest peers for a topic according to the locally learned topology.
	pub replication_factor: NonZeroUsize,
	/// Maximum number of connected nodes that we gossip to.
	///
	/// This caps `routing_targets`, i.e. the forwarding candidates selected from
	/// currently connected peers for a topic.
	pub gossip_target: NonZeroUsize,
}

impl Default for PeersTopologyConfig {
	fn default() -> Self {
		Self {
			replication_factor: NonZeroUsize::new(20).expect("20 is non-zero"),
			gossip_target: NonZeroUsize::new(3).expect("3 is non-zero"),
		}
	}
}

#[derive(Debug, Clone)]
struct PeerInfo {
	supports_protocol: bool,
	/// The statement notification substream to the peer is open.
	connected: bool,
	/// Cached `peer_key`; the peer id never changes and hashing is costly.
	key: Key,
}

/// Pure, event-fed local view of statement-store peers.
///
/// The topology is built from peers learned through routing-table updates, identify metadata and
/// statement notification connections. It computes XOR distances locally over that learned peer
/// set; it does not issue topic-specific Kademlia lookups.
///
/// Topic queries traverse the sorted key indexes as an implicit binary trie (see [`closest`]),
/// so peers arrive in increasing XOR distance without sorting the full peer set.
#[derive(Debug, Clone)]
pub struct PeersTopology {
	local_peer: PeerId,
	local_key: Key,
	config: PeersTopologyConfig,
	// TODO: add an eviction mechanism; this map grows unbounded as peers are discovered.
	// A follow-up should evict peers that leave the network entirely or become unresponsive.
	discovered: HashMap<PeerId, PeerInfo>,
	/// XOR-ordered index of the statement-protocol peers: the candidates for DHT storage,
	/// affinity and forwarding decisions. Buckets are ordered by peer id.
	candidates: PeersIndex,
	/// XOR-ordered index of the connected peers, i.e. those with an open statement notification
	/// substream.
	///
	/// Kept separately from `candidates` rather than as a flag: the implicit-trie descent prunes
	/// by range emptiness and cannot see per-entry flags. Also not a subset of `candidates`: a
	/// connected peer stays a forwarding candidate even after identify metadata withdraws
	/// statement-protocol support.
	connected: PeersIndex,
}

#[allow(dead_code)]
impl PeersTopology {
	pub fn new(local_peer: PeerId, config: PeersTopologyConfig) -> Self {
		Self {
			local_peer,
			local_key: peer_key(&local_peer),
			config,
			discovered: HashMap::new(),
			candidates: PeersIndex::default(),
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
		let was_supported = info.supports_protocol;
		info.supports_protocol = supports_statement_protocol;
		match (was_supported, supports_statement_protocol) {
			(false, true) => self.candidates.insert(key, peer),
			(true, false) => self.candidates.remove(key, &peer),
			_ => (),
		}
	}

	/// Record that the statement notification substream opened.
	///
	/// An open substream implies statement-protocol support.
	pub fn on_substream_opened(&mut self, peer: PeerId) {
		let info = self.get_or_insert_peer(peer);
		let key = info.key;
		let was_supported = info.supports_protocol;
		let was_connected = info.connected;
		info.supports_protocol = true;
		info.connected = true;
		if !was_supported {
			self.candidates.insert(key, peer);
		}
		if !was_connected {
			self.connected.insert(key, peer);
		}
	}

	/// Record that the statement notification substream closed.
	pub fn on_substream_closed(&mut self, peer: PeerId) {
		let Some(info) = self.discovered.get_mut(&peer) else { return };
		let key = info.key;
		let was_connected = info.connected;
		info.connected = false;
		if was_connected {
			self.connected.remove(key, &peer);
		}
	}

	fn is_connected(&self, peer: &PeerId) -> bool {
		self.discovered.get(peer).is_some_and(|info| info.connected)
	}

	/// Number of known remote peers, including peers without confirmed statement-protocol support.
	pub fn known_peers_count(&self) -> usize {
		self.discovered.len()
	}

	/// Closest known statement-protocol peers for `topic`.
	///
	/// "Closest" is computed over the locally learned statement-protocol peers, not by querying
	/// the network for the true global closest peers.
	pub fn closest_known(&self, topic: Topic, limit: usize) -> Vec<PeerId> {
		self.closest_known_keyed(topic, limit)
			.into_iter()
			.map(|(peer, _)| peer)
			.collect()
	}

	/// Returns whether the local node is one of the closest DHT storage replicas for `topic`.
	///
	/// This answers whether this node should store statements for `topic` according to DHT
	/// affinity over the locally learned statement-store peers.
	pub fn is_dht_affine(&self, topic: Topic) -> bool {
		let local_distance = xor_distance(*topic, self.local_key);
		let replication_factor = self.config.replication_factor.get();
		// The descent yields candidates in `(distance, peer)` order, so the candidates ranked
		// before the local node form a prefix; the local node is a replica when that prefix is
		// shorter than the replication factor.
		let closer_count = self
			.candidates
			.closest(*topic)
			.take_while(|(peer, key)| {
				(xor_distance(*topic, *key), *peer) < (local_distance, self.local_peer)
			})
			.take(replication_factor)
			.count();
		closer_count < replication_factor
	}

	/// Connected peers closer to `topic` than the local node, capped at `gossip_target`.
	///
	/// Use these for forwarding decisions: each hop moves the statement towards the peers
	/// responsible for storing it.
	pub fn routing_targets(&self, topic: Topic) -> Vec<PeerId> {
		let local_distance = xor_distance(*topic, self.local_key);
		self.connected
			.closest(*topic)
			.take_while(|(_, key)| xor_distance(*topic, *key) < local_distance)
			.take(self.config.gossip_target.get())
			.map(|(peer, _)| peer)
			.collect()
	}

	/// Local-only explicit-affinity connection candidates for `topics`.
	///
	/// This uses only the locally learned topology, avoiding network lookups that would reveal
	/// explicit-affinity topics, and tries to minimize new connections by choosing peers that
	/// cover currently uncovered topics.
	pub fn peers_for_topics(&self, topics: &[Topic]) -> Vec<PeerId> {
		if topics.is_empty() {
			return Vec::new();
		}

		let pool_size = self.config.replication_factor.get();

		let closest_pools = topics
			.iter()
			.map(|topic| self.closest_known_keyed(*topic, pool_size))
			.collect::<Vec<_>>();

		let mut uncovered = closest_pools
			.iter()
			.enumerate()
			.filter_map(|(topic_idx, pool)| {
				(!pool.iter().any(|(peer, _)| self.is_connected(peer))).then_some(topic_idx)
			})
			.collect::<HashSet<_>>();

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

	/// Insert `peer` into the discovered set if absent and return its record.
	fn get_or_insert_peer(&mut self, peer: PeerId) -> &mut PeerInfo {
		self.discovered.entry(peer).or_insert_with(|| PeerInfo {
			supports_protocol: false,
			connected: false,
			key: peer_key(&peer),
		})
	}

	/// `closest_known` paired with each peer's key, so callers that compute further distances
	/// reuse the key instead of looking it up again.
	fn closest_known_keyed(&self, topic: Topic, limit: usize) -> Vec<(PeerId, Key)> {
		self.candidates.closest(*topic).take(limit).collect()
	}
}

#[derive(Debug, Clone, Default)]
struct PeersIndex {
	/// Buckets are ordered by key; peers within each bucket are ordered by peer id.
	buckets: BTreeMap<Key, Vec<PeerId>>,
}

impl PeersIndex {
	fn insert(&mut self, key: Key, peer: PeerId) {
		let bucket = self.buckets.entry(key).or_default();
		if let Err(position) = bucket.binary_search(&peer) {
			bucket.insert(position, peer);
		}
	}

	fn remove(&mut self, key: Key, peer: &PeerId) {
		if let Some(bucket) = self.buckets.get_mut(&key) {
			if let Ok(position) = bucket.binary_search(peer) {
				bucket.remove(position);
			}
			if bucket.is_empty() {
				self.buckets.remove(&key);
			}
		}
	}

	fn closest(&self, target: Key) -> Closest<'_> {
		closest(&self.buckets, target)
	}
}

/// Peers of `index` in increasing `(xor_distance(target, key), peer)` order.
///
/// The sorted key set is traversed as an implicit binary trie: a trie node is a contiguous key
/// range sharing a prefix, split at the first bit where the range's outermost keys differ, and
/// the half whose bit matches the target is exhausted first. Equal distance means equal key, so
/// the in-bucket peer-id order completes the `(distance, peer)` order. Visiting a node costs one
/// range lookup; taking `k` peers costs `O((k + depth) · log n)` instead of an `O(n log n)` sort.
fn closest(index: &BTreeMap<Key, Vec<PeerId>>, target: Key) -> Closest<'_> {
	Closest { index, target, stack: vec![(Bound::Unbounded, Bound::Unbounded)], bucket: None }
}

/// See [`closest`].
struct Closest<'a> {
	index: &'a BTreeMap<Key, Vec<PeerId>>,
	target: Key,
	/// Pending key ranges, the range nearest to `target` on top.
	stack: Vec<KeyRange>,
	/// Key and remaining peers of the bucket currently being yielded.
	bucket: Option<(Key, std::slice::Iter<'a, PeerId>)>,
}

impl<'a> Iterator for Closest<'a> {
	type Item = (PeerId, Key);

	fn next(&mut self) -> Option<Self::Item> {
		loop {
			if let Some((key, mut peers)) = self.bucket.take() {
				if let Some(peer) = peers.next() {
					self.bucket = Some((key, peers));
					return Some((*peer, key));
				}
			}
			let range = self.stack.pop()?;
			match self.range_keys(range) {
				RangeKeys::Empty => continue,
				RangeKeys::Single { key, peers } => self.bucket = Some((key, peers)),
				RangeKeys::Multiple { first, last } => {
					let (lo, hi) = split_range(&first, &last, &self.target);
					// Both halves hold a key (`first` and `last` respectively), so every
					// pushed range is non-empty and strictly smaller: the descent terminates.
					self.stack.push(hi);
					self.stack.push(lo);
				},
			}
		}
	}
}

impl<'a> Closest<'a> {
	fn range_keys(&self, range: KeyRange) -> RangeKeys<'a> {
		let mut keys = self.index.range(range);
		let Some((first, peers)) = keys.next() else { return RangeKeys::Empty };

		match keys.next_back() {
			None => RangeKeys::Single { key: *first, peers: peers.iter() },
			Some((last, _)) => RangeKeys::Multiple { first: *first, last: *last },
		}
	}
}

enum RangeKeys<'a> {
	Empty,
	Single { key: Key, peers: std::slice::Iter<'a, PeerId> },
	Multiple { first: Key, last: Key },
}

fn split_range(lo: &Key, hi: &Key, target: &Key) -> (KeyRange, KeyRange) {
	let bit = divergence_bit(lo, hi);
	let split = split_key(lo, bit);
	let lower = (Bound::Included(*lo), Bound::Excluded(split));
	let upper = (Bound::Included(split), Bound::Included(*hi));

	if target.view_bits::<Msb0>()[usize::from(bit)] {
		(upper, lower)
	} else {
		(lower, upper)
	}
}

/// First bit where `a` and `b` differ; the keys must differ.
fn divergence_bit(a: &Key, b: &Key) -> u16 {
	a.iter()
		.zip(b)
		.enumerate()
		.find_map(|(byte, (a, b))| {
			let diff = a ^ b;
			(diff != 0).then(|| byte as u16 * 8 + diff.leading_zeros() as u16)
		})
		.expect("the keys differ; qed")
}

/// `key` with bit `bit` set and every following bit cleared: the lower bound of the upper half
/// of a key range splitting at `bit`.
fn split_key(key: &Key, bit: u16) -> Key {
	let bit = usize::from(bit);
	let mut split = [0; 32];

	let source = key.view_bits::<Msb0>();
	let target = split.view_bits_mut::<Msb0>();
	for prefix_bit in 0..bit {
		target.set(prefix_bit, source[prefix_bit]);
	}
	target.set(bit, true);

	split
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

#[cfg(test)]
mod tests {
	use super::*;
	use std::{cmp::Ordering, num::NonZeroUsize};

	fn distance_to(topic: Topic, peer: &PeerId) -> [u8; 32] {
		xor_distance(*topic, peer_key(peer))
	}

	fn cmp_distance_then_peer(topic: Topic, a: &PeerId, b: &PeerId) -> Ordering {
		distance_to(topic, a).cmp(&distance_to(topic, b)).then_with(|| a.cmp(b))
	}

	fn config(replication_factor: usize, gossip_target: usize) -> PeersTopologyConfig {
		PeersTopologyConfig {
			replication_factor: NonZeroUsize::new(replication_factor).expect("non-zero"),
			gossip_target: NonZeroUsize::new(gossip_target).expect("non-zero"),
		}
	}

	fn peer(seed: u8) -> PeerId {
		let mut bytes = [seed; 34];
		bytes[0] = 0;
		bytes[1] = 32;
		PeerId::from_bytes(&bytes).expect("identity multihash peer id")
	}

	fn topology(local_seed: u8) -> PeersTopology {
		PeersTopology::new(peer(local_seed), config(2, 2))
	}

	fn topic(seed: u8) -> Topic {
		Topic([seed; 32])
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

		assert_eq!(topology.is_dht_affine(topic), expected);
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
	fn peers_for_topics_does_not_select_for_topics_covered_by_connected_peers() {
		let mut topology = topology(1);
		let topic = topic(9);

		for peer in (2..=10).map(peer) {
			dht_peer(&mut topology, peer);
		}

		let connected = topology.closest_known(topic, 1)[0];
		topology.on_substream_opened(connected);

		assert!(topology.peers_for_topics(&[topic]).is_empty());
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
	fn closest_yields_keys_in_xor_order_with_peer_tiebreak() {
		let mut index = PeersIndex::default();
		let mut records = Vec::new();

		// Uniformly hashed keys.
		for seed in 2..=120 {
			let peer = peer(seed);
			let key = peer_key(&peer);
			index.insert(key, peer);
			records.push((peer, key));
		}
		// Adversarial keys sharing a 248-bit prefix.
		for seed in 0..8 {
			let mut key = [0xAB; 32];
			key[31] = seed;
			let peer = peer(130 + seed);
			index.insert(key, peer);
			records.push((peer, key));
		}
		// One key shared by several peers, as after a hash collision.
		let shared = [0xCD; 32];
		for seed in [202, 200, 201] {
			let peer = peer(seed);
			index.insert(shared, peer);
			records.push((peer, shared));
		}

		for target in [[0; 32], [0xAB; 32], [0xCD; 32], [0xFF; 32], peer_key(&peer(7))] {
			let mut expected = records.clone();
			expected.sort_by_key(|(peer, key)| (xor_distance(target, *key), *peer));
			assert_eq!(index.closest(target).collect::<Vec<_>>(), expected);
		}
	}

	#[test]
	fn index_mutations_keep_buckets_ordered_and_pruned() {
		let mut index = PeersIndex::default();
		let key = [7; 32];

		index.insert(key, peer(3));
		index.insert(key, peer(2));
		index.insert(key, peer(2));
		assert_eq!(index.buckets[&key], vec![peer(2), peer(3)]);

		index.remove(key, &peer(2));
		index.remove(key, &peer(2));
		assert_eq!(index.buckets[&key], vec![peer(3)]);

		index.remove(key, &peer(3));
		assert!(index.buckets.is_empty());
	}

	#[test]
	fn queries_match_naive_recomputation() {
		let local = peer(1);
		let mut topology = PeersTopology::new(local, config(5, 3));
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
			assert_eq!(topology.is_dht_affine(topic), expected_affine);

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
}
