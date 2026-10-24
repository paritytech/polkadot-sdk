// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! `StreamsRoot`: the sender's stream commitment.
//!
//! Each block that sends commits one hash, the root of a keyed binary Patricia trie over
//! `(StreamId, current stream root)` for every active stream. The relay records it as the sender's
//! `Provides`, and a receiver proves the streams it consumed against it, without a sender-state
//! proof.
//!
//! # Why a keyed trie
//!
//! The trie is keyed by the `StreamId`'s 8-byte encoding, so a proof is a walk driven by the key:
//! verification recomputes the leaf from the caller's `StreamId` and takes each branch direction
//! from that key's bits. A proof built for stream A cannot verify as stream B. Requires-lift
//! synthesis depends on this, since lifts are matched to the consumption record's streams by
//! position. A positional Merkle tree does not give it. Patricia compression also keeps proofs
//! `O(log S)` in the number of streams rather than `O(64)` in the key width.
//!
//! Roles: the sender builds the root ([`streams_root`]) and its node serves proofs
//! ([`gen_stream_proof`]), checking its own header digest ([`read_streams_root`]); a receiver's
//! lift walks a proof up to the committed root ([`streams_root_from_proof`]).
//!
//! A leaf hash binds the full key, an inner hash its split bit.

use alloc::{collections::BTreeMap, vec::Vec};
use codec::{Decode, DecodeAll, DecodeWithMemTracking, Encode};
use polkadot_core_primitives::Hash;
use sp_core::ConstU32;
use sp_runtime::{traits::Hash as HashT, BoundedVec, Digest, DigestItem};

use crate::{
	stream::StreamId, SpecHasher, SPMS_ENGINE_ID, STREAM_ID_LEN, TREE_INNER_TAG, TREE_LEAF_TAG,
};

/// Bit width of a `StreamId` key (the Patricia trie's maximum depth).
const KEY_BITS: usize = STREAM_ID_LEN * 8;

/// The sender's per-block stream commitment root. Defined relay-side and re-exported here.
pub use polkadot_primitives::StreamsRoot;

/// One step of a trie walk. The branch direction is not stored; verification takes it from the
/// proven key's bit at `split_bit`.
#[derive(
	Clone, Copy, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, scale_info::TypeInfo,
)]
pub struct TreeStep {
	/// Key-bit index this node branches on, `0..KEY_BITS` with `0` the MSB. Part of the inner-node
	/// hash; the proven key's bit here gives the direction.
	pub split_bit: u8,
	pub sibling: Hash,
}

/// A membership proof that `(stream, stream_root)` is committed under a `StreamsRoot`. Served by
/// the sender, verified by the receiver. Carries no leaf index; the key drives the walk.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, scale_info::TypeInfo,
)]
pub struct StreamProof {
	/// Branch steps leaf to root, split bits strictly decreasing. Empty for a single-stream tree.
	/// Bounded at the trie depth, so an over-long proof is rejected at decode.
	pub steps: BoundedVec<TreeStep, ConstU32<{ KEY_BITS as u32 }>>,
}

/// Bit `i` of an 8-byte key, `0` the MSB of byte 0.
fn bit(key: &[u8; STREAM_ID_LEN], i: usize) -> bool {
	(key[i >> 3] >> (7 - (i & 7))) & 1 == 1
}

/// First bit index (from the MSB) at which two distinct keys differ. Callers guarantee `a != b`.
fn first_diverging_bit(a: &[u8; STREAM_ID_LEN], b: &[u8; STREAM_ID_LEN]) -> usize {
	(u64::from_be_bytes(*a) ^ u64::from_be_bytes(*b)).leading_zeros() as usize
}

fn hash_leaf(key: &[u8; STREAM_ID_LEN], stream_root: &Hash) -> Hash {
	let mut pre = [0u8; 1 + STREAM_ID_LEN + 32];
	pre[0] = TREE_LEAF_TAG;
	pre[1..1 + STREAM_ID_LEN].copy_from_slice(key);
	pre[1 + STREAM_ID_LEN..].copy_from_slice(stream_root.as_bytes());
	<SpecHasher as HashT>::hash(&pre)
}

fn hash_inner(split_bit: u8, left: &Hash, right: &Hash) -> Hash {
	let mut pre = [0u8; 1 + 1 + 32 + 32];
	pre[0] = TREE_INNER_TAG;
	pre[1] = split_bit;
	pre[2..34].copy_from_slice(left.as_bytes());
	pre[34..].copy_from_slice(right.as_bytes());
	<SpecHasher as HashT>::hash(&pre)
}

/// Split a key-sorted slice at its top-level branch: the bit where its first and last keys diverge,
/// with all `bit == 0` keys before all `bit == 1` keys. Both sides are non-empty. Callers guarantee
/// `entries.len() >= 2` and distinct keys.
fn split<'a>(
	entries: &'a [([u8; STREAM_ID_LEN], Hash)],
) -> (usize, &'a [([u8; STREAM_ID_LEN], Hash)], &'a [([u8; STREAM_ID_LEN], Hash)]) {
	let split_bit = first_diverging_bit(&entries[0].0, &entries[entries.len() - 1].0);
	let p = entries.partition_point(|(k, _)| !bit(k, split_bit));
	let (left, right) = entries.split_at(p);
	(split_bit, left, right)
}

/// Trie root over a key-sorted, non-empty slice.
fn node_hash(entries: &[([u8; STREAM_ID_LEN], Hash)]) -> Hash {
	if let [(key, root)] = entries {
		return hash_leaf(key, root);
	}
	let (split_bit, left, right) = split(entries);
	hash_inner(split_bit as u8, &node_hash(left), &node_hash(right))
}

/// Encoded keys in trie order.
///
/// `BTreeMap` iterates in `StreamId`'s `Ord`, which equals the byte order. The trie splits on
/// bytes, so sort by them here rather than rely on that. Unsorted input would leave one side of
/// `split` empty and make `prove` recurse on its own slice.
fn keyed(entries: &BTreeMap<StreamId, Hash>) -> Vec<([u8; STREAM_ID_LEN], Hash)> {
	let mut keyed: Vec<_> =
		entries.iter().map(|(stream, root)| (stream.to_bytes(), *root)).collect();
	keyed.sort_unstable_by_key(|(key, _)| *key);
	keyed
}

/// The `StreamsRoot` over `(stream, stream_root)` entries; `None` with no active streams. Sender
/// side. Takes a `BTreeMap` so keys are unique; duplicates would reach `first_diverging_bit` with
/// identical keys.
///
/// TODO(spec-msg): update one changed stream in O(log S) instead of rebuilding in O(S); the sender
/// pallet rebuilds this every touched block over all its streams.
pub fn streams_root(entries: &BTreeMap<StreamId, Hash>) -> Option<StreamsRoot> {
	let entries = keyed(entries);
	if entries.is_empty() {
		return None;
	}
	Some(StreamsRoot(node_hash(&entries)))
}

/// Walk key-sorted `entries` toward `key`, pushing each branch step root-first, and return the
/// subtree's root. `key` must be present.
fn prove(
	entries: &[([u8; STREAM_ID_LEN], Hash)],
	key: &[u8; STREAM_ID_LEN],
	steps: &mut Vec<TreeStep>,
) -> Hash {
	if let [(k, root)] = entries {
		return hash_leaf(k, root);
	}
	let (split_bit, left, right) = split(entries);
	let target_right = bit(key, split_bit);
	let (sibling, descend) =
		if target_right { (node_hash(left), right) } else { (node_hash(right), left) };
	steps.push(TreeStep { split_bit: split_bit as u8, sibling });
	let child = prove(descend, key, steps);
	if target_right {
		hash_inner(split_bit as u8, &sibling, &child)
	} else {
		hash_inner(split_bit as u8, &child, &sibling)
	}
}

/// Build the trie over `entries` and return `(StreamsRoot, proof)` for `stream`, or `None` if
/// absent. Sender side.
pub fn gen_stream_proof(
	entries: &BTreeMap<StreamId, Hash>,
	stream: StreamId,
) -> Option<(StreamsRoot, StreamProof)> {
	if !entries.contains_key(&stream) {
		return None;
	}
	let entries = keyed(entries);
	let key = stream.to_bytes();
	let mut steps = Vec::new();
	let root = prove(&entries, &key, &mut steps);
	// `prove` pushes root-first; the wire order is leaf-to-root.
	steps.reverse();
	let steps = BoundedVec::try_from(steps).expect("trie depth <= KEY_BITS; qed");
	Some((StreamsRoot(root), StreamProof { steps }))
}

/// Read the sender's `StreamsRoot` from its header digest. `None` if absent or not exactly what the
/// sender deposits: one `SPMS` item with a 32-byte payload. Foreign readers must agree on
/// validity, so nothing else is accepted.
pub fn read_streams_root(digest: &Digest) -> Option<StreamsRoot> {
	let mut items = digest.logs().iter().filter_map(|item| match item {
		DigestItem::Consensus(id, payload) if *id == SPMS_ENGINE_ID => Some(payload),
		_ => None,
	});
	let payload = items.next()?;
	if items.next().is_some() {
		return None;
	}
	StreamsRoot::decode_all(&mut &payload[..]).ok()
}

/// Fold a membership proof for `(stream, stream_root)` up to the `StreamsRoot` it implies, or
/// `None` on a malformed step. Callers compare the root themselves (the requires-lift records it,
/// the fetch checks it against `under`). Each branch direction is `stream`'s bit at `split_bit`, so
/// a proof built for another stream folds the siblings on the wrong sides and yields a different
/// root.
pub fn streams_root_from_proof(
	stream: StreamId,
	stream_root: Hash,
	membership: &StreamProof,
) -> Option<StreamsRoot> {
	let key = stream.to_bytes();
	let mut node = hash_leaf(&key, &stream_root);
	// Steps are leaf-to-root with strictly decreasing split bits; anything else is rejected.
	// Uniqueness rests on the key-in-leaf and bit-in-inner commitments, not on this rule. The
	// length bound is in the type.
	let mut prev: Option<u8> = None;
	for step in membership.steps.iter() {
		let split_bit = step.split_bit as usize;
		// An out-of-range branch would index the key out of bounds.
		if split_bit >= KEY_BITS {
			return None;
		}
		if prev.is_some_and(|p| step.split_bit >= p) {
			return None;
		}
		prev = Some(step.split_bit);
		// The branch direction is the proven key's bit at `split_bit`. This is the key binding.
		node = if bit(&key, split_bit) {
			hash_inner(step.split_bit, &step.sibling, &node)
		} else {
			hash_inner(step.split_bit, &node, &step.sibling)
		};
	}
	Some(StreamsRoot(node))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_utils::ch;
	use sp_core::H256;

	fn root(b: u8) -> Hash {
		H256::repeat_byte(b)
	}

	/// Whether `stream_root` is committed for `stream` under `streams_root`.
	fn verify_stream_membership(
		streams_root: StreamsRoot,
		stream: StreamId,
		stream_root: Hash,
		membership: &StreamProof,
	) -> bool {
		streams_root_from_proof(stream, stream_root, membership) == Some(streams_root)
	}
	fn entries() -> BTreeMap<StreamId, Hash> {
		// inserted out of order; the map canonicalizes
		BTreeMap::from([
			(ch(4000), root(0xB)),
			(ch(2000), root(0xA)),
			(ch(3000), root(0xC)),
			(StreamId::Ack { recipient: 2000.into(), domain: 0, num: 0 }, root(0xD)),
			(StreamId::Broadcast { domain: 1, subdomain: 0, num: 7 }, root(0xE)),
		])
	}

	#[test]
	fn streams_root_frozen_vector() {
		// Frozen vector. Any change to the trie encoding (tags, `split_bit`, ordering, key binding)
		// is a consensus break.
		assert_eq!(
			streams_root(&entries()).unwrap(),
			StreamsRoot(H256(hex_literal::hex!(
				"12de235fb6d96933d6ba770efe16f891c87fb8a88a75e37a384b88ba11dee0dc"
			))),
		);
	}

	#[test]
	fn round_trip_membership_verifies() {
		for (stream, sr) in entries() {
			let (r, proof) = gen_stream_proof(&entries(), stream).unwrap();
			assert_eq!(Some(r), streams_root(&entries()));
			assert!(verify_stream_membership(r, stream, sr, &proof), "{stream:?} must verify");
		}
	}

	#[test]
	fn wrong_root_or_stream_or_value_fails() {
		let (r, proof) = gen_stream_proof(&entries(), ch(3000)).unwrap();
		// wrong stream_root for the (correct) stream
		assert!(!verify_stream_membership(r, ch(3000), root(0xFF), &proof));
		// right value/proof but claimed for a different stream (key binding must reject)
		assert!(!verify_stream_membership(r, ch(2000), root(0xC), &proof));
		// wrong streams root
		assert!(!verify_stream_membership(StreamsRoot(root(0x00)), ch(3000), root(0xC), &proof));
	}

	#[test]
	fn proof_is_not_transferable_between_streams() {
		// A proof for one stream must not verify for another, even with that stream's own value.
		let (r, proof) = gen_stream_proof(&entries(), ch(4000)).unwrap();
		for (other, other_root) in entries() {
			if other == ch(4000) {
				continue;
			}
			assert!(
				!verify_stream_membership(r, other, other_root, &proof),
				"proof for ch(4000) must not verify as {other:?}"
			);
		}
	}

	#[test]
	fn order_independent_root() {
		// Insertion order cannot affect the root.
		let reversed: BTreeMap<_, _> = entries().into_iter().rev().collect();
		assert_eq!(streams_root(&entries()), streams_root(&reversed));
	}

	#[test]
	fn single_stream_tree_has_empty_proof() {
		let one = BTreeMap::from([(ch(2000), root(0xA))]);
		let (r, proof) = gen_stream_proof(&one, ch(2000)).unwrap();
		assert!(proof.steps.is_empty());
		assert_eq!(Some(r), streams_root(&one));
		assert!(verify_stream_membership(r, ch(2000), root(0xA), &proof));
	}

	#[test]
	fn proof_is_logarithmic() {
		// 64 clustered channels: proof length is O(log S), not the 64-bit key width.
		let many: BTreeMap<_, _> = (0..64u32).map(|i| (ch(2000 + i), root(i as u8))).collect();
		let (_r, proof) = gen_stream_proof(&many, ch(2000 + 33)).unwrap();
		assert!(proof.steps.len() <= 7, "log2(64) = 6-ish, got {}", proof.steps.len());
	}

	#[test]
	fn absent_stream_has_no_proof() {
		assert!(gen_stream_proof(&entries(), ch(9999)).is_none());
	}

	#[test]
	fn out_of_range_split_bit_is_rejected() {
		let (r, mut proof) = gen_stream_proof(&entries(), ch(3000)).unwrap();
		// Splice the bogus step in first (largest bit) so the range check, not the order check,
		// rejects it.
		let mut steps = vec![TreeStep { split_bit: 64, sibling: root(0) }];
		steps.extend(proof.steps.iter().copied());
		proof.steps = steps.try_into().unwrap();
		assert!(!verify_stream_membership(r, ch(3000), root(0xC), &proof));
	}

	#[test]
	fn over_long_proof_is_rejected_at_decode() {
		// A valid proof never exceeds the trie depth, so longer input fails at decode.
		let raw = vec![TreeStep { split_bit: 0, sibling: root(0) }; KEY_BITS + 1].encode();
		assert!(StreamProof::decode(&mut &raw[..]).is_err());
	}

	#[test]
	fn non_decreasing_step_order_is_rejected() {
		// A reversed sequence is rejected even though every hash in it is genuine.
		let (r, proof) = gen_stream_proof(&entries(), ch(3000)).unwrap();
		assert!(proof.steps.len() >= 2, "need a multi-step proof");
		let reversed: Vec<TreeStep> = proof.steps.iter().rev().copied().collect();
		let bad = StreamProof { steps: reversed.try_into().unwrap() };
		assert!(!verify_stream_membership(r, ch(3000), root(0xC), &bad));
	}

	#[test]
	fn read_streams_root_extracts_digest() {
		let r = streams_root(&entries()).unwrap();
		let digest = Digest {
			logs: vec![
				DigestItem::Other(vec![1, 2, 3]),
				DigestItem::Consensus(SPMS_ENGINE_ID, r.encode()),
			],
		};
		assert_eq!(read_streams_root(&digest), Some(r));
		assert_eq!(read_streams_root(&Digest { logs: vec![] }), None);
	}

	#[test]
	fn read_streams_root_accepts_only_the_canonical_digest() {
		let r = streams_root(&entries()).unwrap();
		let spms = |payload: Vec<u8>| DigestItem::Consensus(SPMS_ENGINE_ID, payload);
		let digest = |logs: Vec<DigestItem>| Digest { logs };

		// Exactly 32 bytes.
		let mut trailing = r.encode();
		trailing.push(0);
		assert_eq!(read_streams_root(&digest(vec![spms(trailing)])), None);
		assert_eq!(read_streams_root(&digest(vec![spms(r.encode()[..31].to_vec())])), None);

		// Exactly one item. A malformed first item is not skipped for a later one.
		let other = StreamsRoot(Hash::repeat_byte(0xAB));
		assert_eq!(read_streams_root(&digest(vec![spms(r.encode()), spms(other.encode())])), None);
		assert_eq!(read_streams_root(&digest(vec![spms(vec![0; 3]), spms(r.encode())])), None);

		// Other engines' items are not counted against it.
		let foreign = DigestItem::Consensus(*b"AURA", vec![0; 32]);
		assert_eq!(read_streams_root(&digest(vec![foreign, spms(r.encode())])), Some(r));
	}
}
