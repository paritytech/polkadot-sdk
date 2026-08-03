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

//! The stream commitment tree: a binary compact (Patricia) trie keyed by
//! the canonical [`StreamId`] encoding, leaves = the streams' MMR roots.
//!
//! A sender's block commits to *all* its streams with one hash — the
//! [`StreamsRoot`], this tree's root. The node format defined here is
//! CONSENSUS-CRITICAL and protocol-fixed: every implementation must
//! reproduce byte-identical roots and every foreign node must verify the
//! inclusion proofs.
//!
//! # Node format
//!
//! The trie branches exactly at the *distinguishing* bits among the keys
//! present (everything between branch points is one compressed edge), so
//! the tree has one branch per distinguishing bit and proof length tracks
//! the number of streams, not the key width. Bits are numbered MSB-first
//! over the 8-byte key (bit 0 = most significant bit of the kind byte).
//!
//! - leaf node: `H(TREE_LEAF_TAG ++ key ++ mmr_root)` — the FULL 8-byte key is bound in the leaf,
//!   so a leaf can never be detached from its stream.
//! - inner node: `H(TREE_INNER_TAG ++ branch_bit ++ left ++ right)` — the branch bit index (1 byte)
//!   makes the shape explicit and the encoding injective; children with bit = 0 are left.
//!
//! Domain tags follow the same discipline (and prevent the same
//! ambiguous-parse attack) as the message MMR — see the design's Leaf
//! Hashing section. All preimages here are fixed-arity with fixed-length
//! fields, so the encoding is injective by construction.
//!
//! This module provides the reference builder/prover over an in-memory
//! entry set (verification, tests, node-side proof generation from fetched
//! material). The sender pallet layers incremental O(k·log S) maintenance
//! over stored nodes on top of the same node format.

use alloc::{collections::BTreeMap, vec::Vec};
use polkadot_core_primitives::Hash;
use polkadot_primitives::StreamsRoot;
use sp_runtime::traits::Hash as _;

use crate::{
	mmr::{MmrRoot, SpecHasher},
	stream_id::{StreamId, STREAM_ID_LEN},
	TREE_INNER_TAG, TREE_LEAF_TAG,
};

/// Total number of key bits; bit indices are `0..KEY_BITS`, MSB-first.
pub const KEY_BITS: u8 = (STREAM_ID_LEN * 8) as u8;

/// Errors verifying a [`TreeInclusionProof`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeError {
	/// Branch bit indices are not strictly decreasing leaf-to-root, or out
	/// of range — the proof is not a valid path.
	InvalidPath,
}

/// Hashes one tree leaf: the stream's MMR root at its id.
///
/// Both id and root are bound in the preimage — a leaf never exists
/// detached from its stream.
pub fn tree_leaf_hash(id: &StreamId, root: &MmrRoot) -> Hash {
	let mut preimage = [0u8; 1 + STREAM_ID_LEN + 32];
	preimage[0] = TREE_LEAF_TAG;
	preimage[1..1 + STREAM_ID_LEN].copy_from_slice(&id.to_bytes());
	preimage[1 + STREAM_ID_LEN..].copy_from_slice(root.0.as_bytes());
	SpecHasher::hash(&preimage)
}

/// Hashes one inner node branching at `bit`.
///
/// Public alongside [`tree_leaf_hash`] so the sender pallet's incremental
/// tree store reproduces the exact node format — no second implementation
/// of a consensus-critical preimage.
pub fn tree_inner_hash(bit: u8, left: &Hash, right: &Hash) -> Hash {
	let mut preimage = [0u8; 2 + 32 + 32];
	preimage[0] = TREE_INNER_TAG;
	preimage[1] = bit;
	preimage[2..34].copy_from_slice(left.as_bytes());
	preimage[34..].copy_from_slice(right.as_bytes());
	SpecHasher::hash(&preimage)
}

/// Bit `bit` (MSB-first) of an 8-byte key.
pub fn bit_at(key: &[u8; STREAM_ID_LEN], bit: u8) -> u8 {
	(key[(bit / 8) as usize] >> (7 - bit % 8)) & 1
}

/// First bit at which two keys differ (`None` if equal).
pub fn first_diff_bit(a: &[u8; STREAM_ID_LEN], b: &[u8; STREAM_ID_LEN]) -> Option<u8> {
	for i in 0..STREAM_ID_LEN {
		let x = a[i] ^ b[i];
		if x != 0 {
			return Some((i * 8) as u8 + x.leading_zeros() as u8);
		}
	}
	None
}

/// Proof that one stream's MMR root is the entry at its [`StreamId`] under
/// a given [`StreamsRoot`]: the sibling hash at each branch on the id's
/// path (~log₂(S) of them) plus the branch bit indices — the
/// path-compression metadata the compact trie structure requires.
///
/// `verify` reconstructs the path from `(StreamId, MmrRoot)` upward and
/// RETURNS the implied [`StreamsRoot`] — id and root are both bound by the
/// path, neither is taken on faith; the caller compares the result against
/// a committed root.
#[derive(
	Clone,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	Debug,
	Default,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct TreeInclusionProof {
	/// `(branch bit, sibling hash)` per branch on the path, ordered
	/// leaf-to-root; bit indices strictly decreasing (branches nearer the
	/// root distinguish earlier bits).
	pub steps: Vec<(u8, Hash)>,
}

impl TreeInclusionProof {
	/// Walks the path from `(id, root)` upward and returns the implied
	/// [`StreamsRoot`].
	pub fn verify(&self, id: &StreamId, root: &MmrRoot) -> Result<StreamsRoot, TreeError> {
		let key = id.to_bytes();
		let mut hash = tree_leaf_hash(id, root);
		let mut previous_bit = KEY_BITS; // exclusive upper sentinel
		for (bit, sibling) in &self.steps {
			if *bit >= previous_bit {
				return Err(TreeError::InvalidPath);
			}
			previous_bit = *bit;
			hash = if bit_at(&key, *bit) == 0 {
				tree_inner_hash(*bit, &hash, sibling)
			} else {
				tree_inner_hash(*bit, sibling, &hash)
			};
		}
		Ok(StreamsRoot(hash))
	}
}

/// One entry, keyed for the recursion.
type Entry = ([u8; STREAM_ID_LEN], Hash);

/// Recursively hashes the canonical trie over a non-empty, sorted,
/// duplicate-free slice of (key, leaf hash) entries.
fn subtree_hash(entries: &[Entry]) -> Hash {
	match entries {
		[(_, leaf)] => *leaf,
		_ => {
			let first = &entries[0].0;
			let last = &entries[entries.len() - 1].0;
			let bit = first_diff_bit(first, last)
				.expect("entries are sorted and duplicate-free, so first != last; qed");
			let split = entries.partition_point(|(key, _)| bit_at(key, bit) == 0);
			tree_inner_hash(bit, &subtree_hash(&entries[..split]), &subtree_hash(&entries[split..]))
		},
	}
}

fn sorted_entries(entries: &BTreeMap<StreamId, MmrRoot>) -> Vec<Entry> {
	// BTreeMap iterates in StreamId's Ord, which equals encoding order —
	// but the trie's shape is defined over key *bytes*, so sort by them
	// explicitly rather than relying on that equivalence here.
	let mut out: Vec<Entry> = entries
		.iter()
		.map(|(id, root)| (id.to_bytes(), tree_leaf_hash(id, root)))
		.collect();
	out.sort_unstable_by_key(|(key, _)| *key);
	out
}

/// Computes the [`StreamsRoot`] over a full entry set (reference builder).
/// `None` for an empty set — a block with no streams commits nothing.
pub fn compute_streams_root(entries: &BTreeMap<StreamId, MmrRoot>) -> Option<StreamsRoot> {
	let entries = sorted_entries(entries);
	(!entries.is_empty()).then(|| StreamsRoot(subtree_hash(&entries)))
}

/// Generates the inclusion proof for `id` over a full entry set. `None` if
/// `id` has no entry.
pub fn prove_stream(
	entries: &BTreeMap<StreamId, MmrRoot>,
	id: &StreamId,
) -> Option<TreeInclusionProof> {
	if !entries.contains_key(id) {
		return None;
	}
	let key = id.to_bytes();
	let entries = sorted_entries(entries);

	let mut steps = Vec::new();
	fn walk(entries: &[Entry], key: &[u8; STREAM_ID_LEN], steps: &mut Vec<(u8, Hash)>) {
		if entries.len() == 1 {
			return;
		}
		let first = &entries[0].0;
		let last = &entries[entries.len() - 1].0;
		let bit = first_diff_bit(first, last)
			.expect("entries are sorted and duplicate-free, so first != last; qed");
		let split = entries.partition_point(|(k, _)| bit_at(k, bit) == 0);
		let (own, sibling) = if bit_at(key, bit) == 0 {
			(&entries[..split], &entries[split..])
		} else {
			(&entries[split..], &entries[..split])
		};
		walk(own, key, steps);
		steps.push((bit, subtree_hash(sibling)));
	}
	walk(&entries, &key, &mut steps);

	Some(TreeInclusionProof { steps })
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mmr::MmrRoot;

	fn channel(recipient: u32) -> StreamId {
		StreamId::Channel { recipient: recipient.into(), domain: 0, num: 0 }
	}

	fn ack(recipient: u32) -> StreamId {
		StreamId::Ack { recipient: recipient.into(), domain: 0, num: 0 }
	}

	fn broadcast() -> StreamId {
		StreamId::Broadcast { domain: 0, subdomain: 0, num: 0 }
	}

	fn root(byte: u8) -> MmrRoot {
		MmrRoot(Hash::repeat_byte(byte))
	}

	/// The design's worked example: channels to B (2001) and C (2002), B's
	/// ack register, one broadcast stream. Kinds 0x00/0x01 differ only in
	/// their last bit (bit 7), 0x02 splits off at bit 6, the two channels
	/// diverge in the recipient's last byte (bit 38).
	fn example() -> BTreeMap<StreamId, MmrRoot> {
		[
			(channel(2001), root(1)),
			(channel(2002), root(2)),
			(ack(2001), root(3)),
			(broadcast(), root(4)),
		]
		.into_iter()
		.collect()
	}

	#[test]
	fn example_tree_shape_and_proofs() {
		let entries = example();
		let streams_root = compute_streams_root(&entries).unwrap();

		// Proof for Channel{B}: exactly the design's three siblings, bottom
		// up: (38, H(Channel{C})), (7, H(Ack{B})), (6, H(Broadcast{0})).
		let proof = prove_stream(&entries, &channel(2001)).unwrap();
		let bits: Vec<u8> = proof.steps.iter().map(|(bit, _)| *bit).collect();
		assert_eq!(bits, alloc::vec![38, 7, 6]);
		assert_eq!(proof.steps[0].1, tree_leaf_hash(&channel(2002), &root(2)));
		assert_eq!(proof.steps[1].1, tree_leaf_hash(&ack(2001), &root(3)));
		assert_eq!(proof.steps[2].1, tree_leaf_hash(&broadcast(), &root(4)));

		// Every entry's proof verifies to the same root.
		for (id, mmr_root) in &entries {
			let proof = prove_stream(&entries, id).unwrap();
			assert_eq!(proof.verify(id, mmr_root).unwrap(), streams_root, "{id:?}");
		}

		// The root is a pure function of the entry set: recomputing from a
		// differently-ordered insertion agrees.
		let reversed: BTreeMap<_, _> = example().into_iter().rev().collect();
		assert_eq!(compute_streams_root(&reversed).unwrap(), streams_root);
	}

	#[test]
	fn streams_root_known_answer() {
		// Consensus-critical: pins the node format (tags, branch-bit byte,
		// field layout) via the worked example. Never "fix" the vector, fix
		// the code.
		let streams_root = compute_streams_root(&example()).unwrap();
		assert_eq!(
			streams_root.0.as_bytes(),
			array_bytes::hex2bytes(
				"95f9edcb6bef60742c36736e65c4b607dcc7af856fa63044e94a439576076ae0"
			)
			.unwrap()
			.as_slice(),
		);
	}

	#[test]
	fn verify_binds_id_and_root() {
		let entries = example();
		let streams_root = compute_streams_root(&entries).unwrap();
		let proof = prove_stream(&entries, &channel(2001)).unwrap();

		// Wrong MMR root: verifies to a different StreamsRoot.
		assert_ne!(proof.verify(&channel(2001), &root(9)).unwrap(), streams_root);
		// Wrong id: same. The same payload can never verify under another
		// stream's entry (cross-stream replay).
		assert_ne!(proof.verify(&channel(2002), &root(1)).unwrap(), streams_root);
		// Truncated path: different root.
		let truncated = TreeInclusionProof { steps: proof.steps[..2].to_vec() };
		assert_ne!(truncated.verify(&channel(2001), &root(1)).unwrap(), streams_root);
	}

	#[test]
	fn verify_rejects_non_decreasing_paths() {
		let entries = example();
		let proof = prove_stream(&entries, &channel(2001)).unwrap();

		let mut reordered = proof.clone();
		reordered.steps.swap(0, 1);
		assert_eq!(reordered.verify(&channel(2001), &root(1)), Err(TreeError::InvalidPath));

		let mut duplicated = proof.clone();
		duplicated.steps.push(proof.steps[2]);
		assert_eq!(duplicated.verify(&channel(2001), &root(1)), Err(TreeError::InvalidPath));

		let mut out_of_range = proof;
		out_of_range.steps[0].0 = 64;
		assert_eq!(out_of_range.verify(&channel(2001), &root(1)), Err(TreeError::InvalidPath));
	}

	#[test]
	fn single_entry_tree() {
		let entries: BTreeMap<_, _> = [(channel(7), root(1))].into_iter().collect();
		let streams_root = compute_streams_root(&entries).unwrap();
		// One leaf IS the root; its proof is empty.
		assert_eq!(streams_root.0, tree_leaf_hash(&channel(7), &root(1)));
		let proof = prove_stream(&entries, &channel(7)).unwrap();
		assert!(proof.steps.is_empty());
		assert_eq!(proof.verify(&channel(7), &root(1)).unwrap(), streams_root);
	}

	#[test]
	fn empty_and_missing() {
		let empty = BTreeMap::new();
		assert_eq!(compute_streams_root(&empty), None);
		assert_eq!(prove_stream(&example(), &channel(9999)), None);
	}

	#[test]
	fn insertion_is_stable() {
		// Adding a stream perturbs only the insert path: an untouched
		// stream's entry remains provable, and its NEW proof differs from
		// the old one only in sibling hashes / one extra step — the leaf
		// itself and unrelated subtree structure stay put.
		let mut entries = example();
		let streams_root_before = compute_streams_root(&entries).unwrap();
		let proof_before = prove_stream(&entries, &channel(2001)).unwrap();

		// Insert a second broadcast stream — disjoint from Channel{B}'s
		// subtree below bit 6.
		entries.insert(StreamId::Broadcast { domain: 0, subdomain: 0, num: 1 }, root(5));
		let streams_root_after = compute_streams_root(&entries).unwrap();
		assert_ne!(streams_root_before, streams_root_after);

		let proof_after = prove_stream(&entries, &channel(2001)).unwrap();
		// Same path shape (the insert branched inside the sibling subtree),
		// same lower siblings, only the top sibling hash changed.
		let bits: Vec<u8> = proof_after.steps.iter().map(|(bit, _)| *bit).collect();
		assert_eq!(bits, alloc::vec![38, 7, 6]);
		assert_eq!(proof_after.steps[..2], proof_before.steps[..2]);
		assert_ne!(proof_after.steps[2].1, proof_before.steps[2].1);
		assert_eq!(proof_after.verify(&channel(2001), &root(1)).unwrap(), streams_root_after);

		// An update to one stream's root: every other entry still verifies
		// under the recomputed root.
		entries.insert(channel(2002), root(0xEE));
		let streams_root_updated = compute_streams_root(&entries).unwrap();
		for (id, mmr_root) in &entries {
			let proof = prove_stream(&entries, id).unwrap();
			assert_eq!(proof.verify(id, mmr_root).unwrap(), streams_root_updated);
		}
	}
}
