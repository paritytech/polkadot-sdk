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
//! The trie is keyed by [`StreamId::to_bytes`], so a proof is a walk driven by the key:
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
use codec::{Decode, DecodeAll, DecodeWithMemTracking, Encode, MaxEncodedLen};
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
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
	scale_info::TypeInfo,
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
	Clone,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
	scale_info::TypeInfo,
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

/// First bit index (from the MSB, 0..64) at which two keys differ; returns 64 if identical.
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
/// The trie splits on bytes, so sort by them rather than rely on `Ord`. Unsorted input would leave
/// one side of `split` empty and make `prove` recurse on its own slice.
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
/// For incremental updates in `O(log S)` without rebuilding from scratch in `O(S)`, see
/// [`update_tree_leaf`] and [`StreamsTree`].
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

// Commitment-tree storage types below (`NodeKey`, `InnerNode`, `TreeChild`, `TreeRootEntry`)
// provide an incremental node cache for the sender pallet. This storage layout is not
// normative: foreign chains and protocol verifiers depend only on `StreamsRoot`, `StreamProof`,
// and the canonical `hash_leaf`/`hash_inner` hashing rules.

/// Key identifying an inner node in the commitment trie storage.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	scale_info::TypeInfo,
)]
pub struct NodeKey {
	/// Prefix bytes with bits at index >= prefix_len cleared to zero.
	pub prefix: [u8; STREAM_ID_LEN],
	/// Number of prefix bits shared by all keys under this node (0..64).
	pub prefix_len: u8,
}

impl NodeKey {
	/// Create a new `NodeKey` with prefix bits `0..prefix_len` from `key`.
	pub fn new(key: &[u8; STREAM_ID_LEN], prefix_len: u8) -> Self {
		let mut prefix = *key;
		for i in (prefix_len as usize)..KEY_BITS {
			prefix[i >> 3] &= !(1 << (7 - (i & 7)));
		}
		Self { prefix, prefix_len }
	}
}

/// A child pointer in an inner node or root entry.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
	scale_info::TypeInfo,
)]
pub enum TreeChild {
	/// Child is a leaf storing `(stream_id_key, stream_root)`.
	Leaf { key: [u8; STREAM_ID_LEN], value: Hash },
	/// Child is an inner node stored under `key`.
	Node { key: NodeKey },
}

/// An inner node in the commitment trie.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
	scale_info::TypeInfo,
)]
pub struct InnerNode {
	/// The bit index this node branches on (0..64).
	pub split_bit: u8,
	pub left_child: TreeChild,
	pub left_hash: Hash,
	pub right_child: TreeChild,
	pub right_hash: Hash,
}

impl InnerNode {
	pub fn hash(&self) -> Hash {
		hash_inner(self.split_bit, &self.left_hash, &self.right_hash)
	}
}

/// The root entry of the commitment trie.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
	scale_info::TypeInfo,
)]
pub enum TreeRootEntry {
	/// Tree has a single stream.
	Leaf { key: [u8; STREAM_ID_LEN], value: Hash },
	/// Tree has >= 2 streams, rooted at the inner node with `key`.
	Node { key: NodeKey, root_hash: Hash },
}

impl TreeRootEntry {
	/// The `StreamsRoot` committed by this root entry.
	pub fn streams_root(&self) -> StreamsRoot {
		match self {
			Self::Leaf { key, value } => StreamsRoot(hash_leaf(key, value)),
			Self::Node { root_hash, .. } => StreamsRoot(*root_hash),
		}
	}
}

/// Read-only accessor for inner nodes in the commitment trie.
pub trait NodeLookup {
	fn get_node(&self, key: &NodeKey) -> Option<InnerNode>;
}

/// Storage accessor for inner nodes in the commitment trie.
pub trait NodeStore: NodeLookup {
	fn set_node(&mut self, key: NodeKey, node: InnerNode);
}

impl NodeLookup for BTreeMap<NodeKey, InnerNode> {
	fn get_node(&self, key: &NodeKey) -> Option<InnerNode> {
		self.get(key).copied()
	}
}

impl NodeStore for BTreeMap<NodeKey, InnerNode> {
	fn set_node(&mut self, key: NodeKey, node: InnerNode) {
		self.insert(key, node);
	}
}

/// A node store backed by getter and setter closures.
pub struct FnNodeStore<G, S> {
	pub get: G,
	pub set: S,
}

impl<G, S> NodeLookup for FnNodeStore<G, S>
where
	G: Fn(&NodeKey) -> Option<InnerNode>,
{
	fn get_node(&self, key: &NodeKey) -> Option<InnerNode> {
		(self.get)(key)
	}
}

impl<G, S> NodeStore for FnNodeStore<G, S>
where
	G: Fn(&NodeKey) -> Option<InnerNode>,
	S: FnMut(NodeKey, InnerNode),
{
	fn set_node(&mut self, key: NodeKey, node: InnerNode) {
		(self.set)(key, node);
	}
}

/// Error encountered during commitment trie updates.
#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, Debug, scale_info::TypeInfo)]
pub enum TreeError {
	/// An expected inner node was missing from the node store.
	MissingNode(NodeKey),
}

/// Split an existing leaf `(old_key, old_val)` with a new leaf `(new_key, new_val)`.
/// Returns the new `(NodeKey, InnerNode)`. Callers guarantee `old_key != new_key`.
fn split_leaf(
	old_key: &[u8; STREAM_ID_LEN],
	old_val: Hash,
	new_key: &[u8; STREAM_ID_LEN],
	new_val: Hash,
) -> (NodeKey, InnerNode) {
	let split = first_diverging_bit(new_key, old_key) as u8;
	let node_key = NodeKey::new(new_key, split);
	let new_leaf_hash = hash_leaf(new_key, &new_val);
	let old_leaf_hash = hash_leaf(old_key, &old_val);
	let (left_child, left_hash, right_child, right_hash) = if bit(new_key, split as usize) {
		(
			TreeChild::Leaf { key: *old_key, value: old_val },
			old_leaf_hash,
			TreeChild::Leaf { key: *new_key, value: new_val },
			new_leaf_hash,
		)
	} else {
		(
			TreeChild::Leaf { key: *new_key, value: new_val },
			new_leaf_hash,
			TreeChild::Leaf { key: *old_key, value: old_val },
			old_leaf_hash,
		)
	};
	let node = InnerNode { split_bit: split, left_child, left_hash, right_child, right_hash };
	(node_key, node)
}

/// Recursively insert or update `(key, stream_root)` into an inner node subtree.
fn insert_inner_node(
	curr_key: NodeKey,
	key: &[u8; STREAM_ID_LEN],
	stream_root: Hash,
	store: &mut impl NodeStore,
) -> Result<(NodeKey, Hash), TreeError> {
	let mut inner = store.get_node(&curr_key).ok_or(TreeError::MissingNode(curr_key))?;
	let d = first_diverging_bit(key, &curr_key.prefix);

	// Case 1: `key` diverges before this inner node's split bit.
	// A new inner node must be inserted above this one.
	if d < inner.split_bit as usize {
		let new_split = d as u8;
		let new_node_key = NodeKey::new(key, new_split);
		let leaf_hash = hash_leaf(key, &stream_root);
		let old_inner_hash = inner.hash();
		let (left_child, left_hash, right_child, right_hash) = if bit(key, d) {
			(
				TreeChild::Node { key: curr_key },
				old_inner_hash,
				TreeChild::Leaf { key: *key, value: stream_root },
				leaf_hash,
			)
		} else {
			(
				TreeChild::Leaf { key: *key, value: stream_root },
				leaf_hash,
				TreeChild::Node { key: curr_key },
				old_inner_hash,
			)
		};
		let new_node =
			InnerNode { split_bit: new_split, left_child, left_hash, right_child, right_hash };
		let hash = new_node.hash();
		store.set_node(new_node_key, new_node);
		return Ok((new_node_key, hash));
	}

	// Case 2: `key` matches the prefix up to `split_bit`. Descend left or right.
	let branch_right = bit(key, inner.split_bit as usize);
	let child = if branch_right { inner.right_child } else { inner.left_child };

	let (updated_child, updated_hash) = match child {
		TreeChild::Leaf { key: leaf_key, value: leaf_val } => {
			if &leaf_key == key {
				let leaf_hash = hash_leaf(key, &stream_root);
				(TreeChild::Leaf { key: *key, value: stream_root }, leaf_hash)
			} else {
				let (child_node_key, child_node) =
					split_leaf(&leaf_key, leaf_val, key, stream_root);
				let hash = child_node.hash();
				store.set_node(child_node_key, child_node);
				(TreeChild::Node { key: child_node_key }, hash)
			}
		},
		TreeChild::Node { key: child_key } => {
			let (updated_key, hash) = insert_inner_node(child_key, key, stream_root, store)?;
			(TreeChild::Node { key: updated_key }, hash)
		},
	};

	if branch_right {
		inner.right_child = updated_child;
		inner.right_hash = updated_hash;
	} else {
		inner.left_child = updated_child;
		inner.left_hash = updated_hash;
	}

	let hash = inner.hash();
	store.set_node(curr_key, inner);
	Ok((curr_key, hash))
}

/// Incrementally update or insert a stream into the commitment trie storage.
///
/// Updates the path in `O(log S)` time, reading only nodes along the path from root to leaf and
/// updating inner node sibling caches. Untouched subtrees are neither read nor recomputed.
///
/// Returns the new `StreamsRoot`, or `TreeError` if an expected inner node is missing from `store`.
pub fn update_tree_leaf(
	root_entry: &mut Option<TreeRootEntry>,
	stream: StreamId,
	stream_root: Hash,
	store: &mut impl NodeStore,
) -> Result<StreamsRoot, TreeError> {
	let key = stream.to_bytes();
	match root_entry {
		None => {
			*root_entry = Some(TreeRootEntry::Leaf { key, value: stream_root });
			Ok(StreamsRoot(hash_leaf(&key, &stream_root)))
		},
		Some(TreeRootEntry::Leaf { key: leaf_key, value: leaf_val }) => {
			if leaf_key == &key {
				*root_entry = Some(TreeRootEntry::Leaf { key, value: stream_root });
				Ok(StreamsRoot(hash_leaf(&key, &stream_root)))
			} else {
				let (node_key, node) = split_leaf(leaf_key, *leaf_val, &key, stream_root);
				let root_hash = node.hash();
				store.set_node(node_key, node);
				*root_entry = Some(TreeRootEntry::Node { key: node_key, root_hash });
				Ok(StreamsRoot(root_hash))
			}
		},
		Some(TreeRootEntry::Node { key: root_key, .. }) => {
			let root_key = *root_key;
			let (new_root_key, new_root_hash) =
				insert_inner_node(root_key, &key, stream_root, store)?;
			*root_entry = Some(TreeRootEntry::Node { key: new_root_key, root_hash: new_root_hash });
			Ok(StreamsRoot(new_root_hash))
		},
	}
}

/// Generate a `StreamProof` for `stream` using the node storage cache.
///
/// Walks down the tree in `O(log S)` time, retrieving sibling hashes cached in `InnerNode`
/// without reading untouched subtrees from storage.
pub fn gen_stream_proof_from_nodes(
	root_entry: &Option<TreeRootEntry>,
	stream: StreamId,
	lookup: &impl NodeLookup,
) -> Option<(StreamsRoot, StreamProof)> {
	let key = stream.to_bytes();
	match root_entry.as_ref()? {
		TreeRootEntry::Leaf { key: leaf_key, value: leaf_val } => {
			if leaf_key == &key {
				Some((
					StreamsRoot(hash_leaf(&key, leaf_val)),
					StreamProof { steps: BoundedVec::new() },
				))
			} else {
				None
			}
		},
		TreeRootEntry::Node { key: root_key, root_hash } => {
			let mut curr_key = *root_key;
			let mut steps = Vec::new();
			loop {
				let inner = lookup.get_node(&curr_key)?;
				let d = first_diverging_bit(&key, &curr_key.prefix);
				if d < inner.split_bit as usize {
					return None;
				}
				let (sibling, child) = if bit(&key, inner.split_bit as usize) {
					(inner.left_hash, inner.right_child)
				} else {
					(inner.right_hash, inner.left_child)
				};
				steps.push(TreeStep { split_bit: inner.split_bit, sibling });
				match child {
					TreeChild::Leaf { key: leaf_key, .. } if leaf_key == key => break,
					TreeChild::Leaf { .. } => return None,
					TreeChild::Node { key: next_key } => curr_key = next_key,
				}
			}
			steps.reverse();
			let steps = BoundedVec::try_from(steps).ok()?;
			Some((StreamsRoot(*root_hash), StreamProof { steps }))
		},
	}
}

/// An in-memory commitment tree caching inner nodes for `O(log S)` updates and proofs.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct StreamsTree {
	root: Option<TreeRootEntry>,
	nodes: BTreeMap<NodeKey, InnerNode>,
}

impl StreamsTree {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn is_empty(&self) -> bool {
		self.root.is_none()
	}

	/// The current `StreamsRoot`, or `None` if empty.
	pub fn root(&self) -> Option<StreamsRoot> {
		self.root.as_ref().map(|r| r.streams_root())
	}

	/// Update or insert a stream root in `O(log S)` time.
	pub fn insert(
		&mut self,
		stream: StreamId,
		stream_root: Hash,
	) -> Result<StreamsRoot, TreeError> {
		update_tree_leaf(&mut self.root, stream, stream_root, &mut self.nodes)
	}

	/// Build a proof for `stream` in `O(log S)` time.
	pub fn gen_stream_proof(&self, stream: StreamId) -> Option<(StreamsRoot, StreamProof)> {
		gen_stream_proof_from_nodes(&self.root, stream, &self.nodes)
	}

	/// Construct a tree from an existing map of entries.
	pub fn from_entries(entries: &BTreeMap<StreamId, Hash>) -> Self {
		let mut tree = Self::new();
		for (stream, root) in entries {
			tree.insert(*stream, *root).expect("in-memory node store never fails; qed");
		}
		tree
	}
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

	#[test]
	fn streams_tree_matches_from_scratch_streams_root() {
		let map = entries();
		let tree = StreamsTree::from_entries(&map);
		assert_eq!(tree.root(), streams_root(&map));

		// Check proof generation from tree
		for (stream, sr) in &map {
			let (r_tree, proof_tree) = tree.gen_stream_proof(*stream).unwrap();
			let (r_scratch, proof_scratch) = gen_stream_proof(&map, *stream).unwrap();
			assert_eq!(r_tree, r_scratch);
			assert_eq!(proof_tree, proof_scratch);
			assert!(verify_stream_membership(r_tree, *stream, *sr, &proof_tree));
		}
	}

	#[test]
	fn streams_tree_incremental_updates() {
		let mut map = BTreeMap::new();
		let mut tree = StreamsTree::new();
		assert!(tree.is_empty());
		assert_eq!(tree.root(), None);

		// Insert 100 streams incrementally
		for i in 0..100u32 {
			let stream = ch(1000 + i);
			let r = root(i as u8);
			map.insert(stream, r);
			let tree_root = tree.insert(stream, r).unwrap();
			assert_eq!(Some(tree_root), streams_root(&map));
			assert_eq!(tree.root(), Some(tree_root));
		}

		// Update 50 existing streams with new roots
		for i in (0..100u32).step_by(2) {
			let stream = ch(1000 + i);
			let new_r = root((i + 17) as u8);
			map.insert(stream, new_r);
			let tree_root = tree.insert(stream, new_r).unwrap();
			assert_eq!(Some(tree_root), streams_root(&map));
			assert_eq!(tree.root(), Some(tree_root));
		}

		// Verify proofs for all 100 streams
		for (stream, sr) in &map {
			let (r_tree, proof_tree) = tree.gen_stream_proof(*stream).unwrap();
			let (r_scratch, proof_scratch) = gen_stream_proof(&map, *stream).unwrap();
			assert_eq!(r_tree, r_scratch);
			assert_eq!(proof_tree, proof_scratch);
			assert!(verify_stream_membership(r_tree, *stream, *sr, &proof_tree));
		}
	}

	#[test]
	fn missing_node_returns_error() {
		// Tree with an entry pointing to a missing inner node
		let node_key = NodeKey::new(&ch(1000).to_bytes(), 10);
		let mut root_entry = Some(TreeRootEntry::Node { key: node_key, root_hash: root(0x1) });
		let mut empty_store = BTreeMap::new();
		let before = root_entry;
		let res = update_tree_leaf(&mut root_entry, ch(1001), root(0x2), &mut empty_store);
		assert_eq!(res, Err(TreeError::MissingNode(node_key)));
		assert_eq!(root_entry, before);
		assert!(empty_store.is_empty());
	}

	#[test]
	fn differential_fuzz_random_tree_insertions_and_updates() {
		let mut seed = 0x85EB_CA6B_472C_1A55u64;
		let mut rng = || {
			seed ^= seed << 13;
			seed ^= seed >> 7;
			seed ^= seed << 17;
			seed
		};

		let mut map = BTreeMap::new();
		let mut tree = StreamsTree::new();

		// Narrow field ranges give keys that split inside the kind, recipient and domain bytes.
		let random_stream = |x: u64| match x % 3 {
			0 => StreamId::Channel {
				recipient: (((x >> 8) % 64) as u32).into(),
				domain: (x >> 16) as u8 % 2,
				num: (x >> 24) as u16 % 4,
			},
			1 => StreamId::Ack {
				recipient: (((x >> 8) % 64) as u32).into(),
				domain: (x >> 16) as u8 % 2,
				num: (x >> 24) as u16 % 4,
			},
			_ => StreamId::Broadcast {
				domain: (x >> 8) as u16 % 4,
				subdomain: (x >> 16) as u8 % 2,
				num: (x >> 24) as u32 % 64,
			},
		};

		// Insert 200 streams with pseudo-random keys and values
		for _ in 0..200 {
			let stream = random_stream(rng());
			let r = Hash::from_low_u64_be(rng());
			map.insert(stream, r);
			let tree_root = tree.insert(stream, r).unwrap();
			assert_eq!(Some(tree_root), streams_root(&map));
		}

		// Mutate 100 random streams
		let keys: Vec<_> = map.keys().copied().collect();
		for _ in 0..100 {
			let idx = (rng() as usize) % keys.len();
			let stream = keys[idx];
			let new_r = Hash::from_low_u64_be(rng());
			map.insert(stream, new_r);
			let tree_root = tree.insert(stream, new_r).unwrap();
			assert_eq!(Some(tree_root), streams_root(&map));
		}

		// Verify every proof
		for (stream, sr) in &map {
			let (r_tree, proof_tree) = tree.gen_stream_proof(*stream).unwrap();
			let (r_scratch, proof_scratch) = gen_stream_proof(&map, *stream).unwrap();
			assert_eq!(r_tree, r_scratch);
			assert_eq!(proof_tree, proof_scratch);
			assert!(verify_stream_membership(r_tree, *stream, *sr, &proof_tree));
		}

		// The cached layout does not depend on insertion order.
		let mut reversed = StreamsTree::new();
		for (stream, sr) in map.iter().rev() {
			reversed.insert(*stream, *sr).unwrap();
		}
		assert_eq!(reversed, tree);
	}
}
