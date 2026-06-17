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
use std::{collections::BTreeMap, ops::Bound};

/// A point in the 32-byte key space shared by topics and hashed peer ids.
pub(crate) type Key = [u8; 32];
type KeyRange = (Bound<Key>, Bound<Key>);

/// Peers keyed by their position in the 32-byte key space, queryable in increasing XOR distance to
/// a target without sorting the full set.
#[derive(Debug, Clone, Default)]
pub(crate) struct PeersIndex {
	/// Peers grouped by key; keys are ordered, and peers sharing a key are ordered by peer id.
	peers_by_key: BTreeMap<Key, Vec<PeerId>>,
}

impl PeersIndex {
	pub(crate) fn insert(&mut self, key: Key, peer: PeerId) {
		let peers = self.peers_by_key.entry(key).or_default();
		if let Err(position) = peers.binary_search(&peer) {
			peers.insert(position, peer);
		}
	}

	pub(crate) fn remove(&mut self, key: Key, peer: &PeerId) {
		if let Some(peers) = self.peers_by_key.get_mut(&key) {
			if let Ok(position) = peers.binary_search(peer) {
				peers.remove(position);
			}
			if peers.is_empty() {
				self.peers_by_key.remove(&key);
			}
		}
	}

	pub(crate) fn closest(&self, target: Key) -> Closest<'_> {
		closest(&self.peers_by_key, target)
	}

	/// All peers in the index, in `(key, peer)` order.
	pub(crate) fn peers(&self) -> impl Iterator<Item = PeerId> + '_ {
		self.peers_by_key.values().flatten().copied()
	}
}

/// Peers of `index` in increasing `(xor_distance(target, key), peer)` order, without sorting the
/// full set.
///
/// The keys are already sorted in the `BTreeMap`. Each pending key range is split at the most
/// significant bit where its lowest and highest keys differ; the half whose bit matches the target
/// holds the closer keys and is visited first. Peers sharing a key have equal distance, so the
/// peer-id order within a key completes the `(distance, peer)` order.
fn closest(index: &BTreeMap<Key, Vec<PeerId>>, target: Key) -> Closest<'_> {
	Closest { index, target, stack: vec![(Bound::Unbounded, Bound::Unbounded)], current: None }
}

pub(crate) struct Closest<'a> {
	index: &'a BTreeMap<Key, Vec<PeerId>>,
	target: Key,
	/// Pending key ranges, the range nearest to `target` on top.
	stack: Vec<KeyRange>,
	/// Key and remaining peers of the entry currently being yielded.
	current: Option<(Key, std::slice::Iter<'a, PeerId>)>,
}

impl<'a> Iterator for Closest<'a> {
	type Item = (PeerId, Key);

	fn next(&mut self) -> Option<Self::Item> {
		loop {
			if let Some((key, mut peers)) = self.current.take() {
				if let Some(peer) = peers.next() {
					self.current = Some((key, peers));
					return Some((*peer, key));
				}
			}
			let range = self.stack.pop()?;
			match self.range_keys(range) {
				RangeKeys::Empty => continue,
				RangeKeys::Single { key, peers } => self.current = Some((key, peers)),
				RangeKeys::Multiple { first, last } => {
					let (lo, hi) = split_range(&first, &last, &self.target);
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

/// `key` with bit `bit` set and every following bit cleared: the lower bound of the upper half of a
/// key range splitting at `bit`.
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_helpers::peer;

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
	fn index_mutations_keep_peers_ordered_and_pruned() {
		let mut index = PeersIndex::default();
		let key = [7; 32];

		index.insert(key, peer(3));
		index.insert(key, peer(2));
		index.insert(key, peer(2));
		assert_eq!(index.peers_by_key[&key], vec![peer(2), peer(3)]);

		index.remove(key, &peer(2));
		index.remove(key, &peer(2));
		assert_eq!(index.peers_by_key[&key], vec![peer(3)]);

		index.remove(key, &peer(3));
		assert!(index.peers_by_key.is_empty());
	}
}
