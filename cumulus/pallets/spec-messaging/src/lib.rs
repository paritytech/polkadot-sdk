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

//! # Speculative Messaging pallet — sender side
//!
//! Accumulates this parachain's outbound message streams (design v0.5):
//! every sent payload becomes a leaf in its stream's append-only MMR, and a
//! block that touches at least one stream commits to *all* streams with a
//! single hash — the [`StreamsRoot`], root of the stream commitment tree (a
//! binary compact trie keyed by the canonical [`StreamId`] encoding, leaves
//! = the streams' MMR roots). Payloads themselves travel off-chain between
//! collators; the chain only ever commits to hashes.
//!
//! ## Sender lifecycle
//!
//! - [`Pallet::append_to_stream`] pushes a payload onto [`OutboundMessages`], the per-stream vec of
//!   THIS block's sends (host-side append, O(1) per send). The stored frontier is never touched
//!   mid-block, so `position = frontier.leaf_count + index` holds unchanged for the whole block,
//!   including finalization.
//! - At block end (`on_finalize`) the touched streams' new MMR roots are computed *transiently*
//!   (stored frontier + this block's leaves, in memory) and folded into the stored commitment tree
//!   — one tree-path write per touched stream. The resulting [`StreamsRoot`] is memoized for the
//!   `Provides` UMP signal emission and deposited as the `DigestItem::Consensus(SPMS_ENGINE_ID,
//!   root)` header digest, at most one per header. An idle block folds nothing, emits nothing and
//!   deposits nothing.
//! - At the NEXT block's `on_initialize` the pending messages are hashed (`hash_leaf`, current
//!   `LEAF_VERSION`) into the frontiers and [`OutboundMessages`] is cleared — one atomic step.
//!   Block N's messages therefore stay readable in block N's state, where node-side extraction (the
//!   runtime API) reads them — never storage directly.
//!
//! The commitment tree's node storage ([`TreeNodes`], [`TreeRoot`]) is a
//! rebuildable cache — the frontiers determine the whole tree — kept because
//! incremental path updates need the sibling hashes (O(k·log S) per block
//! for k touched streams among S).
//!
//! Messages must be appended before this pallet's `on_finalize` runs: a
//! payload appended by a later `on_finalize` hook would miss the fold and
//! silently shift every subsequent position. Order such pallets before this
//! one in `construct_runtime`.
//!
//! Channel state and the credit-gated `send` live one layer up (channels &
//! flow control); this pallet provides the internal append primitive they
//! call, and its per-block caps ([`Config::MaxMessagesPerBlock`],
//! [`Config::MaxMsgLen`]) are the hard, consensus-side backpressure
//! underneath the advisory window grants.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use cumulus_primitives_spec_messaging::{
	hash_leaf,
	tree::{bit_at, first_diff_bit, tree_inner_hash, tree_leaf_hash, KEY_BITS},
	MessagePosition, MmrFrontier, SpecHasher, StreamId, StreamsRoot, LEAF_VERSION, SPMS_ENGINE_ID,
	STREAM_ID_LEN,
};
use polkadot_core_primitives::Hash;
use scale_info::TypeInfo;
use sp_runtime::generic::DigestItem;

pub use pallet::*;

/// Storage key of one commitment-tree node: the key prefix it governs.
///
/// An inner node branching at bit `b` governs all stream keys sharing bits
/// `[0, b)` — its key is `len = b` plus exactly those bits. A leaf's key is
/// the stream's full canonical encoding with `len = KEY_BITS`. The mapping
/// is injective over the canonical trie, and stable: inserting a new branch
/// *above* a node never re-keys the node or its subtree.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Debug,
	Eq,
	PartialEq,
)]
pub struct NodeKey {
	/// Number of significant prefix bits, MSB-first; [`KEY_BITS`] = leaf.
	pub len: u8,
	/// The prefix bits; bits at index `>= len` are zero.
	pub prefix: [u8; STREAM_ID_LEN],
}

impl NodeKey {
	/// The leaf node key of a full stream key.
	fn leaf(key: [u8; STREAM_ID_LEN]) -> Self {
		Self { len: KEY_BITS, prefix: key }
	}

	/// The inner-node key branching at `bit` on `key`'s path.
	fn inner(key: &[u8; STREAM_ID_LEN], bit: u8) -> Self {
		Self { len: bit, prefix: mask_prefix(key, bit) }
	}

	/// `true` iff this key identifies a leaf.
	fn is_leaf(&self) -> bool {
		self.len == KEY_BITS
	}

	/// First bit in `[0, len)` at which `key` diverges from this node's
	/// prefix; `None` if `key` extends the prefix.
	fn divergence(&self, key: &[u8; STREAM_ID_LEN]) -> Option<u8> {
		first_diff_bit(key, &self.prefix).filter(|bit| *bit < self.len)
	}
}

/// Zeroes all bits of `key` at index `>= len`.
fn mask_prefix(key: &[u8; STREAM_ID_LEN], len: u8) -> [u8; STREAM_ID_LEN] {
	let mut prefix = [0u8; STREAM_ID_LEN];
	let full = (len / 8) as usize;
	prefix[..full].copy_from_slice(&key[..full]);
	let partial = len % 8;
	if partial != 0 {
		prefix[full] = key[full] & (0xFF << (8 - partial));
	}
	prefix
}

/// Reference to a commitment-tree node together with its subtree hash.
///
/// Child hashes are stored inline in the parent (and the root's in
/// [`TreeRoot`]): walking down a stream's path collects every sibling hash
/// the bottom-up recomputation needs, without visiting sibling subtrees.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Debug,
	Eq,
	PartialEq,
)]
pub struct TreeChild {
	/// The child node's storage key ([`NodeKey::is_leaf`] = leaf).
	pub key: NodeKey,
	/// The child's subtree hash — `tree_leaf_hash` for leaves,
	/// `tree_inner_hash` for inner nodes.
	pub hash: Hash,
}

/// A stored inner node of the commitment tree. Its branch bit is its own
/// storage key's `len`; children with bit = 0 are left. Leaves have no
/// stored node — a leaf is fully described by its parent's [`TreeChild`]
/// (or by [`TreeRoot`] in a single-stream tree).
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Debug,
	Eq,
	PartialEq,
)]
pub struct InnerNode {
	/// The subtree of keys with the branch bit = 0.
	pub left: TreeChild,
	/// The subtree of keys with the branch bit = 1.
	pub right: TreeChild,
}

impl InnerNode {
	fn child(&self, side: u8) -> &TreeChild {
		if side == 0 {
			&self.left
		} else {
			&self.right
		}
	}

	fn child_mut(&mut self, side: u8) -> &mut TreeChild {
		if side == 0 {
			&mut self.left
		} else {
			&mut self.right
		}
	}

	/// This node's hash; `bit` is its branch bit (== its node key's `len`).
	fn node_hash(&self, bit: u8) -> Hash {
		tree_inner_hash(bit, &self.left.hash, &self.right.hash)
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Hard per-message payload size bound, in bytes — a consensus
		/// constant of this chain's streams (the advisory
		/// `WindowGrant::max_message_size` rides on top of it).
		#[pallet::constant]
		type MaxMsgLen: Get<u32>;

		/// Per-stream, per-block cap on appended messages — with
		/// [`Config::MaxMsgLen`] the hard, consensus-side backpressure of
		/// the transport.
		#[pallet::constant]
		type MaxMessagesPerBlock: Get<u32>;
	}

	/// Per-stream MMR frontiers (peaks + leaf count) — the only long-lived
	/// sender accumulator state. Throughout a block this reflects state as
	/// of the PREVIOUS block: this block's sends are appended at the next
	/// block's `on_initialize`. Roots are computed on demand by bagging;
	/// no stream root is ever stored.
	#[pallet::storage]
	pub type OutboundFrontier<T: Config> =
		StorageMap<_, Twox64Concat, StreamId, MmrFrontier, ValueQuery>;

	/// Messages sent in THIS block only, per stream, kept for node-side
	/// extraction via the runtime API (which reads them in this block's
	/// state). Message `i` sits at position `OutboundFrontier[stream]
	/// .leaf_count + i`; appends go through the `append` host function, so
	/// a send is O(1) and gaps are unrepresentable.
	#[pallet::storage]
	pub type OutboundMessages<T: Config> = StorageMap<
		_,
		Twox64Concat,
		StreamId,
		BoundedVec<BoundedVec<u8, T::MaxMsgLen>, T::MaxMessagesPerBlock>,
		ValueQuery,
	>;

	/// Stream commitment tree inner nodes — a rebuildable cache (the
	/// frontiers determine the whole tree), kept because incremental path
	/// updates need the sibling hashes. Node format per
	/// `cumulus_primitives_spec_messaging::tree`; layout per [`NodeKey`].
	#[pallet::storage]
	pub type TreeNodes<T: Config> = StorageMap<_, Twox64Concat, NodeKey, InnerNode, OptionQuery>;

	/// The commitment tree's root node; `None` until the first stream is
	/// ever touched. Part of the same rebuildable cache as [`TreeNodes`].
	/// Its `hash` is the [`StreamsRoot`] as of the last fold.
	#[pallet::storage]
	pub type TreeRoot<T: Config> = StorageValue<_, TreeChild, OptionQuery>;

	/// The [`StreamsRoot`] committed by the block being executed — the
	/// end-of-block fold's memo, feeding the `Provides` UMP signal.
	/// Transient: set by [`Pallet::commit_streams_root`] iff this block
	/// touched a stream, cleared at the next `on_initialize`.
	#[pallet::storage]
	pub type BlockStreamsRoot<T: Config> = StorageValue<_, StreamsRoot, OptionQuery>;

	#[pallet::error]
	#[derive(PartialEq)]
	pub enum Error<T> {
		/// The payload exceeds [`Config::MaxMsgLen`], the consensus hard
		/// per-message size bound.
		MessageTooBig,
		/// The stream already carries [`Config::MaxMessagesPerBlock`]
		/// messages in this block.
		TooManyMessages,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			// TODO: benchmark; DbWeight-based estimate for now, and the
			// unaccounted `on_finalize` fold (O(k·log S) node writes) needs
			// to be charged here once weights land.
			let mut weight = T::DbWeight::get().reads_writes(1, 1);

			// The previous block's fold memo dies with its block.
			BlockStreamsRoot::<T>::kill();

			// Bump the frontiers with the previous block's sends and clear
			// the per-block vecs — one atomic step. From here on the stored
			// frontiers reflect everything sent up to and including the
			// previous block.
			for (stream, messages) in OutboundMessages::<T>::drain() {
				let mut frontier = OutboundFrontier::<T>::get(stream);
				for payload in &messages {
					frontier.append_leaf(hash_leaf::<SpecHasher>(LEAF_VERSION, payload));
				}
				OutboundFrontier::<T>::insert(stream, frontier);
				weight.saturating_accrue(T::DbWeight::get().reads_writes(2, 2));
			}

			weight
		}

		fn on_finalize(_n: BlockNumberFor<T>) {
			Self::commit_streams_root();
		}
	}

	impl<T: Config> Pallet<T> {
		/// Appends one payload (already SCALE-encoded `SpecMsgKind`) to this
		/// block's vec for `stream` and returns the message's position in
		/// the stream's MMR.
		///
		/// This is the internal primitive under the channel layer's `send`;
		/// it enforces only the consensus hard caps
		/// ([`Config::MaxMsgLen`], [`Config::MaxMessagesPerBlock`]) —
		/// channel phase and credit gating happen in the caller. On error,
		/// state is untouched.
		pub fn append_to_stream(
			stream: StreamId,
			payload: Vec<u8>,
		) -> Result<MessagePosition, Error<T>> {
			let payload: BoundedVec<u8, T::MaxMsgLen> =
				payload.try_into().map_err(|_| Error::<T>::MessageTooBig)?;

			// The stored frontier holds state as of the previous block all
			// block long, so the position is stable from here on.
			let index = OutboundMessages::<T>::decode_len(stream).unwrap_or(0) as u64;
			OutboundMessages::<T>::try_append(stream, payload)
				.map_err(|()| Error::<T>::TooManyMessages)?;

			Ok(MessagePosition(OutboundFrontier::<T>::get(stream).leaf_count + index))
		}

		/// Runs the end-of-block commitment fold — idempotent, at most once
		/// per block: computes every touched stream's new MMR root
		/// *transiently* (stored frontier + this block's leaves, in
		/// memory), folds the roots into the stored commitment tree (one
		/// path write per touched stream), memoizes the resulting
		/// [`StreamsRoot`] and deposits the header digest.
		///
		/// Returns `None` on idle blocks: nothing folded, nothing
		/// memoized, nothing deposited. This is what the `Provides` UMP
		/// signal emission hook calls — an unchanged root is never
		/// re-emitted.
		pub fn commit_streams_root() -> Option<StreamsRoot> {
			if let Some(root) = BlockStreamsRoot::<T>::get() {
				// The fold already ran in this block.
				return Some(root);
			}

			let mut touched = false;
			for (stream, messages) in OutboundMessages::<T>::iter() {
				let mut frontier = OutboundFrontier::<T>::get(stream);
				for payload in &messages {
					frontier.append_leaf(hash_leaf::<SpecHasher>(LEAF_VERSION, payload));
				}
				Self::upsert_tree_leaf(&stream, tree_leaf_hash(&stream, &frontier.root()));
				touched = true;
			}
			if !touched {
				return None;
			}

			let root = TreeRoot::<T>::get()
				.map(|node| StreamsRoot(node.hash))
				.expect("at least one leaf was just folded, so the tree is non-empty; qed");
			BlockStreamsRoot::<T>::put(root);
			frame_system::Pallet::<T>::deposit_log(DigestItem::Consensus(
				SPMS_ENGINE_ID,
				root.encode(),
			));

			Some(root)
		}

		/// The [`StreamsRoot`] committed by the current block, if the fold
		/// ran and a stream was touched.
		pub fn current_streams_root() -> Option<StreamsRoot> {
			BlockStreamsRoot::<T>::get()
		}

		/// Upserts one stream's leaf hash into the stored commitment tree
		/// and recomputes the hashes along its path — O(log S) node
		/// reads/writes, everything off the path untouched.
		fn upsert_tree_leaf(id: &StreamId, leaf_hash: Hash) {
			let key = id.to_bytes();
			let leaf = TreeChild { key: NodeKey::leaf(key), hash: leaf_hash };

			let Some(root) = TreeRoot::<T>::get() else {
				// First stream ever: the single leaf is the whole tree.
				TreeRoot::<T>::put(leaf);
				return;
			};

			// Walk down to the leaf's slot, remembering the traversed inner
			// nodes and which side the path took.
			let mut path: Vec<(NodeKey, InnerNode, u8)> = Vec::new();
			let mut cursor = root;
			let mut updated = loop {
				if let Some(bit) = cursor.key.divergence(&key) {
					// `key` splits off the cursor's subtree: a fresh inner
					// node branching at the first differing bit adopts the
					// subtree and the new leaf. The subtree keeps its node
					// key — prefixes never change on insertion above.
					let node_key = NodeKey::inner(&key, bit);
					let node = if bit_at(&key, bit) == 0 {
						InnerNode { left: leaf, right: cursor }
					} else {
						InnerNode { left: cursor, right: leaf }
					};
					TreeNodes::<T>::insert(node_key, node);
					break TreeChild { key: node_key, hash: node.node_hash(bit) };
				}
				if cursor.key.is_leaf() {
					// Exact key match: the stream is already in the tree;
					// its entry gets the new hash.
					break leaf;
				}
				let node = TreeNodes::<T>::get(cursor.key).expect(
					"TreeRoot and stored child pointers only reference existing inner nodes; qed",
				);
				let side = bit_at(&key, cursor.key.len);
				let next = *node.child(side);
				path.push((cursor.key, node, side));
				cursor = next;
			};

			// Recompute the path bottom-up: each traversed node gets the
			// updated child (whose pointer may now be a fresh inner node)
			// and a recomputed hash, using the untouched siblings in place.
			while let Some((node_key, mut node, side)) = path.pop() {
				*node.child_mut(side) = updated;
				TreeNodes::<T>::insert(node_key, node);
				updated = TreeChild { key: node_key, hash: node.node_hash(node_key.len) };
			}
			TreeRoot::<T>::put(updated);
		}
	}
}
