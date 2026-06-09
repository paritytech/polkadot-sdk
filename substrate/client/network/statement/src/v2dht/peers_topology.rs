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
pub use sp_statement_store::Topic;
use std::{
	cmp::Ordering,
	collections::{HashMap, HashSet},
	num::NonZeroUsize,
};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PeersTopologyConfig {
	/// Number of statement-protocol peers responsible for storing a topic.
	pub replication_factor: NonZeroUsize,
	/// Maximum number of connected nodes that we gossip to.
	pub gossip_target: NonZeroUsize,
	/// Widens each per-topic candidate pool in `peers_for_topics` by this factor, so the selector
	/// keeps spare peers when the closest are already connected, stale, or unreachable.
	pub candidate_multiplier: NonZeroUsize,
}

impl Default for PeersTopologyConfig {
	fn default() -> Self {
		Self {
			replication_factor: NonZeroUsize::new(20).expect("20 is non-zero"),
			gossip_target: NonZeroUsize::new(3).expect("3 is non-zero"),
			candidate_multiplier: NonZeroUsize::new(3).expect("3 is non-zero"),
		}
	}
}

#[derive(Debug, Clone, Default)]
struct PeerInfo {
	supports: bool,
}

/// Pure, event-fed local view of statement-store peers.
///
/// The topology is built from peers learned through routing-table updates, identify metadata and
/// statement notification connections. It computes XOR distances locally over that learned peer
/// set; it does not issue topic-specific Kademlia lookups.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PeersTopology {
	local_peer: PeerId,
	config: PeersTopologyConfig,
	discovered: HashMap<PeerId, PeerInfo>,
	connected: HashSet<PeerId>,
}

#[allow(dead_code)]
impl PeersTopology {
	pub fn new(local_peer: PeerId, config: PeersTopologyConfig) -> Self {
		Self { local_peer, config, discovered: HashMap::new(), connected: HashSet::new() }
	}

	/// Record that a routing-table update saw `peer`.
	pub fn note_seen(&mut self, peer: PeerId) {
		if peer != self.local_peer {
			self.peer_info_mut(peer);
		}
	}

	/// Record statement-protocol support from identify protocol metadata.
	pub fn note_identified(&mut self, peer: PeerId, supports_statement_protocol: bool) {
		if peer == self.local_peer {
			return;
		}

		self.peer_info_mut(peer).supports = supports_statement_protocol;
	}

	/// Record that the statement notification substream opened.
	///
	/// An open substream implies statement-protocol support.
	pub fn on_substream_opened(&mut self, peer: PeerId) {
		if peer == self.local_peer {
			return;
		}

		self.peer_info_mut(peer).supports = true;
		self.connected.insert(peer);
	}

	/// Record that the statement notification substream closed.
	pub fn on_substream_closed(&mut self, peer: PeerId) {
		self.connected.remove(&peer);
	}

	fn is_connected(&self, peer: &PeerId) -> bool {
		self.connected.contains(peer)
	}

	/// Number of known remote peers, including peers without confirmed statement-protocol support.
	pub fn known_peers_count(&self) -> usize {
		self.discovered.len()
	}

	/// Closest known statement-protocol peers for `topic`.
	///
	/// "Closest" is computed over the locally learned statement-protocol peers, not by querying
	/// the network for the true global closest peers.
	pub fn closest_known(&self, topic: Topic, n: usize) -> Vec<PeerId> {
		let mut peers = self.dht_candidates().collect::<Vec<_>>();
		peers.sort_by(|a, b| cmp_distance_then_peer(topic, a, b));
		peers.truncate(n);
		peers
	}

	/// Returns whether the local node is one of the closest DHT storage replicas for `topic`.
	pub fn is_dht_affine(&self, topic: Topic) -> bool {
		let mut candidates = self.dht_candidates().collect::<Vec<_>>();
		candidates.push(self.local_peer);
		candidates.sort_by(|a, b| cmp_distance_then_peer(topic, a, b));
		candidates
			.into_iter()
			.take(self.config.replication_factor.get())
			.any(|peer| peer == self.local_peer)
	}

	/// Closest connected statement-protocol peers for `topic`.
	pub fn routing_targets(&self, topic: Topic) -> Vec<PeerId> {
		let mut peers = self
			.connected
			.iter()
			.copied()
			.filter(|peer| self.is_dht_candidate(peer))
			.collect::<Vec<_>>();

		peers.sort_by(|a, b| cmp_distance_then_peer(topic, a, b));
		peers.truncate(self.config.gossip_target.get());
		peers
	}

	/// Local-only explicit-affinity connection candidates for `topics`.
	pub fn peers_for_topics(&self, topics: &[Topic]) -> Vec<PeerId> {
		if topics.is_empty() {
			return Vec::new();
		}

		let pool_size =
			self.config.replication_factor.get() * self.config.candidate_multiplier.get();

		let closest_pools = topics
			.iter()
			.map(|topic| self.closest_known(*topic, pool_size))
			.collect::<Vec<_>>();

		let mut uncovered = closest_pools
			.iter()
			.enumerate()
			.filter_map(|(topic_idx, pool)| {
				(!pool.iter().any(|peer| self.is_connected(peer))).then_some(topic_idx)
			})
			.collect::<HashSet<_>>();

		let pools = closest_pools
			.into_iter()
			.map(|pool| {
				pool.into_iter()
					.filter(|peer| *peer != self.local_peer && !self.is_connected(peer))
					.collect::<Vec<_>>()
			})
			.collect::<Vec<_>>();

		let mut selected = Vec::new();
		let limit = topics.len() * self.config.candidate_multiplier.get();

		while !uncovered.is_empty() && selected.len() < limit {
			let Some(best_peer) = self.best_candidate(topics, &pools, &uncovered, &selected) else {
				break;
			};

			selected.push(best_peer);
			uncovered.retain(|topic_idx| !pools[*topic_idx].contains(&best_peer));
		}

		selected
	}

	fn peer_info_mut(&mut self, peer: PeerId) -> &mut PeerInfo {
		self.discovered.entry(peer).or_default()
	}

	fn is_dht_candidate(&self, peer: &PeerId) -> bool {
		self.discovered.get(peer).is_some_and(|peer_info| peer_info.supports)
	}

	fn dht_candidates(&self) -> impl Iterator<Item = PeerId> + '_ {
		self.discovered
			.iter()
			.filter(|(_, peer_info)| peer_info.supports)
			.map(|(peer, _)| *peer)
	}

	fn best_candidate(
		&self,
		topics: &[Topic],
		pools: &[Vec<PeerId>],
		uncovered: &HashSet<usize>,
		selected: &[PeerId],
	) -> Option<PeerId> {
		let mut candidates = pools
			.iter()
			.flat_map(|pool| pool.iter().copied())
			.filter(|peer| !selected.contains(peer))
			.collect::<HashSet<_>>()
			.into_iter()
			.filter_map(|peer| {
				let mut covering_topics = uncovered
					.iter()
					.copied()
					.filter(|topic_idx| pools[*topic_idx].contains(&peer))
					.collect::<Vec<_>>();
				if covering_topics.is_empty() {
					return None;
				}

				covering_topics.sort_by(|a, b| {
					distance_to(topics[*a], &peer).cmp(&distance_to(topics[*b], &peer))
				});
				let best_distance = distance_to(topics[covering_topics[0]], &peer);
				Some((peer, covering_topics.len(), best_distance))
			})
			.collect::<Vec<_>>();

		candidates.sort_by(|(peer_a, score_a, distance_a), (peer_b, score_b, distance_b)| {
			score_b
				.cmp(score_a)
				.then_with(|| distance_a.cmp(distance_b))
				.then_with(|| peer_a.cmp(peer_b))
		});
		candidates.first().map(|(peer, _, _)| *peer)
	}
}

#[allow(dead_code)]
fn cmp_distance_then_peer(topic: Topic, a: &PeerId, b: &PeerId) -> Ordering {
	distance_to(topic, a).cmp(&distance_to(topic, b)).then_with(|| a.cmp(b))
}

#[allow(dead_code)]
fn distance_to(topic: Topic, peer: &PeerId) -> [u8; 32] {
	xor_distance(*topic, peer_key(peer))
}

#[allow(dead_code)]
fn peer_key(peer: &PeerId) -> [u8; 32] {
	sp_crypto_hashing::blake2_256(&peer.to_bytes())
}

#[allow(dead_code)]
fn xor_distance(a: [u8; 32], b: [u8; 32]) -> [u8; 32] {
	let mut distance = [0; 32];
	for ((distance, a), b) in distance.iter_mut().zip(a).zip(b) {
		*distance = a ^ b;
	}
	distance
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::num::NonZeroUsize;

	fn config(replication_factor: usize, gossip_target: usize) -> PeersTopologyConfig {
		PeersTopologyConfig {
			replication_factor: NonZeroUsize::new(replication_factor).expect("non-zero"),
			gossip_target: NonZeroUsize::new(gossip_target).expect("non-zero"),
			candidate_multiplier: NonZeroUsize::new(3).expect("non-zero"),
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
		topology.note_seen(peer);
		topology.note_identified(peer, true);
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
		topology.note_seen(unsupported);
		topology.note_identified(unsupported, false);

		assert_eq!(topology.known_peers_count(), 2);
		assert_eq!(known_dht_peers(&topology, topic(9)), vec![supported]);

		topology.on_substream_opened(supported);
		assert!(topology.is_connected(&supported));

		topology.note_identified(supported, false);
		assert!(known_dht_peers(&topology, topic(9)).is_empty());

		topology.on_substream_closed(supported);
		assert!(!topology.is_connected(&supported));
		assert_eq!(topology.known_peers_count(), 2);
	}

	#[test]
	fn substream_lifecycle_drives_routing_targets() {
		let mut topology = topology(1);
		let peer = peer(2);
		let topic = topic(1);

		topology.note_seen(peer);
		topology.note_identified(peer, true);

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
	fn routing_targets_are_closest_connected_dht_peers_even_when_farther_than_self() {
		let mut topology = topology(1);
		let topic = topic(7);
		let self_distance = distance_to(topic, &peer(1));
		let mut farther = Vec::new();

		for seed in 2..=80 {
			let candidate = peer(seed);
			if distance_to(topic, &candidate) > self_distance {
				farther.push(candidate);
			}
			if farther.len() == 3 {
				break;
			}
		}
		assert!(farther.len() >= 3, "test peer fixture must include peers farther than self");

		for peer in &farther {
			dht_peer(&mut topology, *peer);
			topology.on_substream_opened(*peer);
		}

		let expected = {
			let mut peers = farther.clone();
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
	fn peers_for_topics_is_deterministic_local_set_cover() {
		let mut topology = topology(1);
		let peers = (2..=30).map(peer).collect::<Vec<_>>();
		let topics = [topic(1), topic(9), topic(17)];

		for peer in &peers {
			dht_peer(&mut topology, *peer);
		}

		let first = topology.peers_for_topics(&topics);
		let second = topology.peers_for_topics(&topics);

		assert_eq!(first, second);
		assert!(!first.is_empty());
		assert!(first.len() <= topics.len() * 3);
		assert!(first.iter().all(|peer| peers.contains(peer)));
	}
}
