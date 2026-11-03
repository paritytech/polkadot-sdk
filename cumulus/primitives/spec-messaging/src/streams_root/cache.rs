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

//! The sender's node cache for the commitment trie: `O(log S)` updates and proofs without a
//! rebuild. Not normative; verifiers depend only on the parent module's roots and proofs.

use alloc::{collections::BTreeMap, vec::Vec};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use polkadot_core_primitives::Hash;
use sp_runtime::BoundedVec;

use super::{
	bit, first_diverging_bit, hash_inner, hash_leaf, StreamProof, StreamsRoot, TreeStep, KEY_BITS,
};
use crate::{stream::StreamId, STREAM_ID_LEN};

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
	/// Child is an inner node stored under `key`, with `height` inner nodes on its longest path.
	Node { key: NodeKey, height: u8 },
}

impl TreeChild {
	/// Inner nodes on the longest path from this child down to a leaf; 0 for a leaf.
	pub fn height(&self) -> u8 {
		match self {
			Self::Leaf { .. } => 0,
			Self::Node { height, .. } => *height,
		}
	}
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

	/// Inner nodes on the longest path from this node down to a leaf, itself included.
	pub fn height(&self) -> u8 {
		1 + self.left_child.height().max(self.right_child.height())
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
	/// Tree has >= 2 streams, rooted at the inner node with `key` and `height`.
	Node { key: NodeKey, root_hash: Hash, height: u8 },
}

impl TreeRootEntry {
	/// The `StreamsRoot` committed by this root entry.
	pub fn streams_root(&self) -> StreamsRoot {
		match self {
			Self::Leaf { key, value } => StreamsRoot(hash_leaf(key, value)),
			Self::Node { root_hash, .. } => StreamsRoot(*root_hash),
		}
	}

	/// The step count of the longest `StreamProof` in the tree.
	pub fn height(&self) -> u8 {
		match self {
			Self::Leaf { .. } => 0,
			Self::Node { height, .. } => *height,
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

/// Error encountered during commitment trie updates.
#[derive(Clone, Copy, Encode, Decode, PartialEq, Eq, Debug, scale_info::TypeInfo)]
pub enum TreeError {
	/// An expected inner node was missing from the node store.
	MissingNode(NodeKey),
	/// A stored inner node does not branch at its key's prefix length, below its parent.
	CorruptNode(NodeKey),
}

/// Load the inner node at `key`. Requiring each node to branch at its key's prefix length, strictly
/// below `parent`'s split bit and within the key, bounds every walk at [`KEY_BITS`] steps, even
/// over a corrupt store.
fn load_node(
	store: &impl NodeLookup,
	key: NodeKey,
	parent: Option<u8>,
) -> Result<InnerNode, TreeError> {
	let node = store.get_node(&key).ok_or(TreeError::MissingNode(key))?;
	let below_parent = parent.map_or(true, |p| node.split_bit > p);
	if node.split_bit != key.prefix_len || (node.split_bit as usize) >= KEY_BITS || !below_parent {
		return Err(TreeError::CorruptNode(key));
	}
	Ok(node)
}

/// An inner node at `split_bit` with `ours` on the side `ours_right` names and `other` opposite.
fn branch(
	split_bit: u8,
	ours_right: bool,
	ours: (TreeChild, Hash),
	other: (TreeChild, Hash),
) -> InnerNode {
	let ((left_child, left_hash), (right_child, right_hash)) =
		if ours_right { (other, ours) } else { (ours, other) };
	InnerNode { split_bit, left_child, left_hash, right_child, right_hash }
}

fn leaf(key: &[u8; STREAM_ID_LEN], value: Hash) -> (TreeChild, Hash) {
	(TreeChild::Leaf { key: *key, value }, hash_leaf(key, &value))
}

/// Split an existing leaf `(old_key, old_val)` with a new leaf `(new_key, new_val)`. Callers
/// guarantee `old_key != new_key`.
fn split_leaf(
	old_key: &[u8; STREAM_ID_LEN],
	old_val: Hash,
	new_key: &[u8; STREAM_ID_LEN],
	new_val: Hash,
) -> (NodeKey, InnerNode) {
	let split = first_diverging_bit(new_key, old_key);
	let node =
		branch(split as u8, bit(new_key, split), leaf(new_key, new_val), leaf(old_key, old_val));
	(NodeKey::new(new_key, split as u8), node)
}

/// Recursively insert or update `(key, stream_root)` into an inner node subtree.
fn insert_inner_node(
	curr_key: NodeKey,
	parent: Option<u8>,
	key: &[u8; STREAM_ID_LEN],
	stream_root: Hash,
	store: &mut impl NodeStore,
) -> Result<(NodeKey, Hash, u8), TreeError> {
	let mut inner = load_node(store, curr_key, parent)?;
	let d = first_diverging_bit(key, &curr_key.prefix);

	// `key` leaves this node's prefix above its split bit: a new inner node goes above it.
	if d < inner.split_bit as usize {
		let above = (TreeChild::Node { key: curr_key, height: inner.height() }, inner.hash());
		let new_node = branch(d as u8, bit(key, d), leaf(key, stream_root), above);
		let (new_key, hash, height) =
			(NodeKey::new(key, d as u8), new_node.hash(), new_node.height());
		store.set_node(new_key, new_node);
		return Ok((new_key, hash, height));
	}

	let (child, child_hash) = if bit(key, inner.split_bit as usize) {
		(&mut inner.right_child, &mut inner.right_hash)
	} else {
		(&mut inner.left_child, &mut inner.left_hash)
	};
	(*child, *child_hash) = match *child {
		TreeChild::Leaf { key: leaf_key, .. } if &leaf_key == key => leaf(key, stream_root),
		TreeChild::Leaf { key: leaf_key, value } => {
			let (node_key, node) = split_leaf(&leaf_key, value, key, stream_root);
			let hash = node.hash();
			store.set_node(node_key, node);
			(TreeChild::Node { key: node_key, height: node.height() }, hash)
		},
		TreeChild::Node { key: child_key, .. } => {
			let (child_key, hash, height) =
				insert_inner_node(child_key, Some(inner.split_bit), key, stream_root, store)?;
			(TreeChild::Node { key: child_key, height }, hash)
		},
	};

	let (hash, height) = (inner.hash(), inner.height());
	store.set_node(curr_key, inner);
	Ok((curr_key, hash, height))
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
				*root_entry =
					Some(TreeRootEntry::Node { key: node_key, root_hash, height: node.height() });
				Ok(StreamsRoot(root_hash))
			}
		},
		Some(TreeRootEntry::Node { key: root_key, .. }) => {
			let root_key = *root_key;
			let (new_root_key, new_root_hash, height) =
				insert_inner_node(root_key, None, &key, stream_root, store)?;
			*root_entry =
				Some(TreeRootEntry::Node { key: new_root_key, root_hash: new_root_hash, height });
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
		TreeRootEntry::Node { key: root_key, root_hash, .. } => {
			let mut curr_key = *root_key;
			let mut parent = None;
			let mut steps = Vec::new();
			loop {
				let inner = load_node(lookup, curr_key, parent).ok()?;
				parent = Some(inner.split_bit);
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
					TreeChild::Node { key: next_key, .. } => curr_key = next_key,
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

	/// The step count of the longest proof in the tree; 0 if empty or single-stream.
	pub fn height(&self) -> u8 {
		self.root.as_ref().map_or(0, |r| r.height())
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
