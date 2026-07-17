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

//! Message-stream MMR machinery: domain-tagged hashing, the append-only
//! frontier, and inclusion / extension proofs.
//!
//! No hand-rolled accumulator: everything is built over
//! `polkadot-ckb-merkle-mountain-range` (`mmr_lib`), an established `no_std`
//! workspace dependency. What this module contributes is the domain-tagged
//! [`SpecMerge`] adapter, the minimal stored state ([`MmrFrontier`]) and
//! proof wrappers whose `verify` *yields* roots instead of taking them on
//! faith.

use alloc::{vec, vec::Vec};
use mmr_lib::{
	helper::{get_peak_map, get_peaks, is_valid_mmr_size},
	leaf_index_to_mmr_size,
};
use polkadot_core_primitives::Hash;
use sp_core::ConstU32;
use sp_runtime::BoundedVec;

use crate::{EMPTY_TAG, INNER_TAG, LEAF_TAG, PEAK_TAG};

/// The canonical hasher for Speculative Messaging —
/// [`BlakeTwo256`](sp_runtime::traits::BlakeTwo256).
///
/// Swap this alias to change the hash function across the entire protocol in
/// one edit.
pub type SpecHasher = sp_runtime::traits::BlakeTwo256;

/// [`mmr_lib::Merge`] adapter that wires domain-tagged hashing into the MMR
/// library.
///
/// `H` must implement [`sp_runtime::traits::Hash`] with `Output = Hash`. Pass
/// this as the `M` type parameter when constructing an
/// `mmr_lib::MMR<Hash, SpecMerge<H>, S>` accumulator.
pub struct SpecMerge<H>(core::marker::PhantomData<H>);
impl<H: sp_runtime::traits::Hash<Output = Hash>> mmr_lib::Merge for SpecMerge<H> {
	type Item = Hash;

	fn merge(left: &Self::Item, right: &Self::Item) -> mmr_lib::Result<Self::Item> {
		let len = <H as sp_core::Hasher>::LENGTH;
		let mut preimage = Vec::with_capacity(1 + len + len);
		preimage.push(INNER_TAG);
		preimage.extend_from_slice(left.as_bytes());
		preimage.extend_from_slice(right.as_bytes());
		Ok(<H as sp_runtime::traits::Hash>::hash(&preimage))
	}

	fn merge_peaks(peak1: &Self::Item, peak2: &Self::Item) -> mmr_lib::Result<Self::Item> {
		let len = <H as sp_core::Hasher>::LENGTH;
		let mut preimage = Vec::with_capacity(1 + len + len);
		preimage.push(PEAK_TAG);
		preimage.extend_from_slice(peak1.as_bytes());
		preimage.extend_from_slice(peak2.as_bytes());
		Ok(<H as sp_runtime::traits::Hash>::hash(&preimage))
	}
}

/// The concrete merge used everywhere in this crate's own verification
/// paths.
type Merge = SpecMerge<SpecHasher>;

/// Hashes a message payload into its MMR leaf.
///
/// The preimage is `LEAF_TAG ++ version ++ payload` and is **transient**:
/// assembled, hashed, discarded — never stored, never sent. `version` is the
/// leaf *preimage layout* version ([`crate::LEAF_VERSION`] today), a
/// trust-free hint on the wire: versions are hash-disjoint domains, so only
/// the correct one can reproduce a committed root.
///
/// Deliberately NOT in the preimage: source, destination, position, length
/// (see the design's Leaf Hashing section — stream context is bound
/// structurally by the commitment tree, order/multiplicity by the MMR
/// itself, and the encoding is already injective).
pub fn hash_leaf<H: sp_runtime::traits::Hash<Output = Hash>>(version: u8, payload: &[u8]) -> Hash {
	let mut preimage = Vec::with_capacity(2 + payload.len());
	preimage.push(LEAF_TAG);
	preimage.push(version);
	preimage.extend_from_slice(payload);
	<H as sp_runtime::traits::Hash>::hash(&preimage)
}

/// Root of a single message stream's MMR (bagged peaks).
///
/// Newtype over [`Hash`]: roots flow through every layer (tree leaves, wire
/// responses, extension proofs) alongside block hashes, leaf hashes, peaks
/// and `StreamsRoot`s — confusing them must not typecheck. Leaf and
/// inner-node hashes stay bare [`Hash`]: they are internal to the
/// accumulator code and already domain-separated cryptographically by the
/// hash tags.
#[derive(
	Clone,
	Copy,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	codec::MaxEncodedLen,
	Debug,
	Default,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	scale_info::TypeInfo,
)]
pub struct MmrRoot(pub Hash);

/// Index of a message (= leaf) in a stream's MMR, starting at 0.
///
/// Positions are never stored per message: they are always derived as
/// `frontier leaf count + index into the current block's message vec`, and
/// they are not part of the leaf preimage either. They materialize only as
/// addressing in the off-chain fetch protocol and as local bookkeeping. The
/// newtype exists so the places doing this arithmetic can't confuse
/// positions with other counters. Note this is the *leaf index*, not
/// `mmr_lib`'s internal node position (which also counts inner nodes).
#[derive(
	Clone,
	Copy,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	codec::MaxEncodedLen,
	Debug,
	Default,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	scale_info::TypeInfo,
)]
pub struct MessagePosition(pub u64);

/// Errors from frontier / proof operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MmrError {
	/// A frontier's peak count does not match its leaf count.
	InconsistentFrontier,
	/// An extension proof does not extend forward from the given state.
	NotForward,
	/// A proof is structurally invalid for its claimed placement.
	InvalidProof,
	/// A position lies outside the proof's MMR.
	PositionOutOfRange,
}

/// The complete, minimal append-state of an MMR: peaks + leaf count. The
/// peaks are the roots of the O(log n) perfect subtrees; *which* peaks exist
/// is fully determined by `leaf_count` (its binary representation), so
/// nothing else is needed. Appending is O(1) amortized (a new leaf merges
/// with equal-height peaks); the root is computed by bagging the peaks.
///
/// Peaks are ordered left to right (highest subtree first), matching
/// `mmr_lib`'s canonical peak order.
#[derive(
	Clone,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	codec::MaxEncodedLen,
	Debug,
	Default,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct MmrFrontier {
	/// Number of leaves; also the position of the next message to append.
	pub leaf_count: u64,
	/// The peak hashes, left to right. ≤ 64 peaks for u64 leaf counts.
	pub peaks: BoundedVec<Hash, ConstU32<64>>,
}

impl MmrFrontier {
	/// `true` iff the peak count matches the leaf count's binary
	/// representation. Codec-decoded frontiers must be checked before use in
	/// verification paths.
	pub fn is_consistent(&self) -> bool {
		self.peaks.len() == self.leaf_count.count_ones() as usize
	}

	/// Appends one leaf hash, merging equal-height peaks (`INNER_TAG`
	/// merges, exactly as the full MMR would).
	pub fn append_leaf(&mut self, leaf: Hash) {
		let mut new_peak = leaf;
		let mut count = self.leaf_count;
		while count & 1 == 1 {
			let left = self.peaks.pop().expect(
				"a trailing one-bit in leaf_count corresponds to an existing peak on a \
				 consistent frontier; qed",
			);
			new_peak = <Merge as mmr_lib::Merge>::merge(&left, &new_peak)
				.expect("SpecMerge::merge is infallible; qed");
			count >>= 1;
		}
		self.peaks
			.try_push(new_peak)
			.expect("peak count is bounded by 64 for u64 leaf counts; qed");
		self.leaf_count += 1;
	}

	/// The stream root: the bagged peaks, matching `mmr_lib`'s `get_root`
	/// bit-for-bit (single peak = the peak itself; multiple peaks bagged
	/// right to left with `PEAK_TAG`). The empty frontier has the defined
	/// root `H(EMPTY_TAG)` (`mmr_lib` errors on empty; the protocol needs a
	/// value) — known-answer-tested below.
	pub fn root(&self) -> MmrRoot {
		MmrRoot(bag_peaks(&self.peaks).unwrap_or_else(empty_root))
	}
}

/// The defined root of an empty stream.
fn empty_root() -> Hash {
	<SpecHasher as sp_runtime::traits::Hash>::hash(&[EMPTY_TAG])
}

/// Bags peaks right to left via `merge_peaks(right, left)`, replicating
/// `mmr_lib::bagging_peaks_hashes`. `None` for no peaks.
fn bag_peaks(peaks: &[Hash]) -> Option<Hash> {
	let (last, rest) = peaks.split_last()?;
	let root = rest.iter().rev().fold(*last, |acc, left| {
		<Merge as mmr_lib::Merge>::merge_peaks(&acc, left)
			.expect("SpecMerge::merge_peaks is infallible; qed")
	});
	Some(root)
}

/// MMR inclusion proof for a single leaf (`mmr_lib`'s [`MerkleProof`]
/// items): the sibling path plus the other peaks for root bagging.
///
/// Verification *yields* the stream root implied by (position, leaf, proof)
/// — derived, never declared — for the caller to compare against an entry
/// proven under a committed `StreamsRoot`. Used by lossy consumers
/// (ack-register and event reads); channel consumption verifies by
/// recomputation and needs no proofs.
///
/// [`MerkleProof`]: mmr_lib::MerkleProof
#[derive(
	Clone,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	Debug,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct MmrInclusionProof {
	/// `mmr_lib` size (node count) of the MMR the proof was generated
	/// against. Determines the leaf count — how a head read is pinned.
	pub mmr_size: u64,
	/// The proof items (sibling path + peaks), in `mmr_lib` order.
	pub items: Vec<Hash>,
}

impl MmrInclusionProof {
	/// Number of leaves in the proven MMR (the peak map of a valid MMR size
	/// *is* the leaf count).
	pub fn leaf_count(&self) -> u64 {
		get_peak_map(self.mmr_size)
	}

	/// Verifies `leaf` (already hashed via [`hash_leaf`]) as the **head** —
	/// the last leaf — of the proven MMR and yields the read's full pinned
	/// placement: the head position and the frontier (peaks + leaf count)
	/// of exactly that MMR.
	///
	/// This is the register/event *head read* verification: lossy consumers
	/// need the peaks, not just the root, because the read context enters
	/// the consumption record as an interval whose `end` frontier the next
	/// gap check — or the lift's extension proof — extends from. As
	/// everywhere, the root (`frontier.root()`) is *derived*: a tampered
	/// item yields a frontier no lift can bind; only structural defects
	/// fail here.
	///
	/// The expected proof shape is what [`gen_proof`] for the last leaf
	/// produces: the other peaks left to right, then the head's sibling
	/// path bottom-up. The head leaf is the rightmost leaf of the last
	/// (smallest) peak's subtree, so the path is exactly
	/// `leaf_count.trailing_zeros()` LEFT siblings — the item count is
	/// checked exactly; the proof bytes have one valid form.
	///
	/// [`gen_proof`]: mmr_lib::MMR::gen_proof
	pub fn verify_head(&self, leaf: Hash) -> Result<(MessagePosition, MmrFrontier), MmrError> {
		if !is_valid_mmr_size(self.mmr_size) {
			return Err(MmrError::InvalidProof);
		}
		let leaf_count = self.leaf_count();
		if leaf_count == 0 {
			return Err(MmrError::PositionOutOfRange);
		}
		let path_len = leaf_count.trailing_zeros() as usize;
		let other_peaks = leaf_count.count_ones() as usize - 1;
		if self.items.len() != other_peaks + path_len {
			return Err(MmrError::InvalidProof);
		}

		let (peak_items, path) = self.items.split_at(other_peaks);
		let mut last_peak = leaf;
		for sibling in path {
			last_peak = <Merge as mmr_lib::Merge>::merge(sibling, &last_peak)
				.expect("SpecMerge::merge is infallible; qed");
		}
		let peaks = peak_items
			.iter()
			.copied()
			.chain(core::iter::once(last_peak))
			.collect::<Vec<_>>()
			.try_into()
			.expect("a u64 leaf count has at most 64 peaks; qed");

		Ok((MessagePosition(leaf_count - 1), MmrFrontier { leaf_count, peaks }))
	}

	/// Verifies `leaf` (already hashed via [`hash_leaf`]) at `position` and
	/// returns the implied stream root.
	pub fn verify_leaf(&self, position: MessagePosition, leaf: Hash) -> Result<MmrRoot, MmrError> {
		if !is_valid_mmr_size(self.mmr_size) {
			return Err(MmrError::InvalidProof);
		}
		if position.0 >= self.leaf_count() {
			return Err(MmrError::PositionOutOfRange);
		}
		let pos = mmr_lib::leaf_index_to_pos(position.0);
		mmr_lib::MerkleProof::<Hash, Merge>::new(self.mmr_size, self.items.clone())
			.calculate_root(vec![(pos, leaf)])
			.map(MmrRoot)
			.map_err(|_| MmrError::InvalidProof)
	}
}

/// Extends the verifier's own MMR state (frontier, or peaks reconstructed
/// from an in-block inclusion proof) to a newer root: O(log n) connecting
/// nodes summarizing the appended range.
///
/// `verify` RETURNS the new root — derived from the proof, never declared
/// alongside it (design Appendix B). Generated node-side from the stream's
/// leaf hashes (`mmr_lib::MMR::gen_ancestry_proof`); payloads are never
/// needed.
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
pub struct MMRExtensionProof {
	/// Leaf count of the extended (newer) MMR. `0` together with empty
	/// `connecting_nodes` is the canonical *empty* proof: the verifier's
	/// state already is the target (identity extension).
	pub leaf_count: u64,
	/// Connecting nodes as `(mmr position, hash)`, in `mmr_lib`'s
	/// prev-peaks-proof order; their placement is checked against the old
	/// frontier's leaf count.
	pub connecting_nodes: Vec<(u64, Hash)>,
}

impl MMRExtensionProof {
	/// The canonical empty proof (identity extension).
	pub fn empty() -> Self {
		Self::default()
	}

	/// `true` for the canonical empty proof.
	pub fn is_empty(&self) -> bool {
		self.leaf_count == 0 && self.connecting_nodes.is_empty()
	}

	/// Builds the proof from `mmr_lib`'s [`AncestryProof`] over the same
	/// merge (generation side; typically node-side lift assembly).
	///
	/// [`AncestryProof`]: mmr_lib::AncestryProof
	pub fn from_ancestry_proof<M: mmr_lib::Merge<Item = Hash>>(
		new_leaf_count: u64,
		proof: &mmr_lib::AncestryProof<Hash, M>,
	) -> Self {
		Self {
			leaf_count: new_leaf_count,
			connecting_nodes: proof.prev_peaks_proof.proof_items().to_vec(),
		}
	}

	/// Extends `old` (the verifier's own state, correct by construction —
	/// no validity checks beyond consistency) and returns the new root. By
	/// construction the result is the root of an MMR of which `old`'s
	/// leaves are a strict prefix; ill-placed connecting nodes fail.
	///
	/// The empty proof returns `old.root()` — the identity case ("the
	/// endpoint already is the target root's entry").
	pub fn verify(&self, old: &MmrFrontier) -> Result<MmrRoot, MmrError> {
		if !old.is_consistent() {
			return Err(MmrError::InconsistentFrontier);
		}
		if self.is_empty() {
			return Ok(old.root());
		}
		if self.leaf_count <= old.leaf_count {
			return Err(MmrError::NotForward);
		}
		let new_mmr_size = leaf_index_to_mmr_size(self.leaf_count - 1);

		if old.leaf_count == 0 {
			// Extending from the empty stream: nothing binds (a prefix of
			// nothing is vacuous), so the connecting nodes must simply BE
			// the new MMR's peaks at their canonical positions.
			let expected = get_peaks(new_mmr_size);
			if self.connecting_nodes.len() != expected.len() ||
				self.connecting_nodes.iter().map(|(p, _)| *p).ne(expected.iter().copied())
			{
				return Err(MmrError::InvalidProof);
			}
			let peaks: Vec<Hash> = self.connecting_nodes.iter().map(|(_, h)| *h).collect();
			return bag_peaks(&peaks).map(MmrRoot).ok_or(MmrError::InvalidProof);
		}

		let old_mmr_size = leaf_index_to_mmr_size(old.leaf_count - 1);
		let old_positions = get_peaks(old_mmr_size);
		if old_positions.len() != old.peaks.len() {
			return Err(MmrError::InconsistentFrontier);
		}
		// The old peaks are complete subtrees of the new MMR at unchanged
		// positions; combining them with the connecting nodes yields the
		// new root — a node-level Merkle proof.
		let nodes: Vec<(u64, Hash)> =
			old_positions.into_iter().zip(old.peaks.iter().copied()).collect();
		mmr_lib::NodeMerkleProof::<Hash, Merge>::new(new_mmr_size, self.connecting_nodes.clone())
			.calculate_root(nodes)
			.map(MmrRoot)
			.map_err(|_| MmrError::InvalidProof)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{LEAF_VERSION, TREE_INNER_TAG, TREE_LEAF_TAG};
	use mmr_lib::{
		util::{MemMMR, MemStore},
		MMRStoreReadOps, Merge as MergeT,
	};
	use sp_runtime::traits::Hash as _;

	type TestMerge = SpecMerge<SpecHasher>;

	fn leaves(n: u64) -> Vec<Hash> {
		(0..n)
			.map(|i| hash_leaf::<SpecHasher>(LEAF_VERSION, &i.to_le_bytes()))
			.collect()
	}

	/// Builds a full reference MMR and a frontier over the same leaves.
	fn build(n: u64) -> (MemStore<Hash>, MmrFrontier) {
		let store = MemStore::default();
		let mut frontier = MmrFrontier::default();
		{
			let mut mmr = MemMMR::<Hash, TestMerge>::new(0, &store);
			for leaf in leaves(n) {
				mmr.push(leaf).unwrap();
				frontier.append_leaf(leaf);
			}
			mmr.commit().unwrap();
		}
		(store, frontier)
	}

	fn full_root(store: &MemStore<Hash>, n: u64) -> Hash {
		let mmr = MemMMR::<Hash, TestMerge>::new(leaf_index_to_mmr_size(n - 1), store);
		mmr.get_root().unwrap()
	}

	#[test]
	fn leaf_preimage_known_answer() {
		// Consensus-critical: the exact preimage layout LEAF_TAG ++
		// LEAF_VERSION ++ payload. Never "fix" the vector, fix the code.
		let payload = b"hello";
		let mut preimage = alloc::vec![LEAF_TAG, LEAF_VERSION];
		preimage.extend_from_slice(payload);
		let expected = SpecHasher::hash(&preimage);
		assert_eq!(hash_leaf::<SpecHasher>(LEAF_VERSION, payload), expected);

		// Pinned bytes: blake2b-256 of 0x01 0x00 "hello".
		assert_eq!(
			expected.as_bytes(),
			array_bytes::hex2bytes(
				"cd31917fb8992dae762dbaaf276d8eb65aa89cdfb87daf69e05f8c08b490e78b"
			)
			.unwrap()
			.as_slice(),
		);
	}

	#[test]
	fn empty_frontier_root_known_answer() {
		let root = MmrFrontier::default().root();
		// Pinned bytes: blake2b-256 of the single byte 0x04 (EMPTY_TAG).
		assert_eq!(
			root.0.as_bytes(),
			array_bytes::hex2bytes(
				"642206314f534b29ad297d82440a5f9f210e30ca5ced805a587ca402de927342"
			)
			.unwrap()
			.as_slice(),
		);
	}

	#[test]
	fn frontier_matches_full_mmr_for_all_sizes() {
		// Property: append N leaves via the frontier vs a full mmr_lib MMR;
		// roots must agree for every N. Crosses every peak-merge boundary up
		// to 130.
		let store = MemStore::default();
		let mut mmr = MemMMR::<Hash, TestMerge>::new(0, &store);
		let mut frontier = MmrFrontier::default();

		for (i, leaf) in leaves(130).into_iter().enumerate() {
			mmr.push(leaf).unwrap();
			frontier.append_leaf(leaf);
			assert!(frontier.is_consistent());
			assert_eq!(frontier.leaf_count, i as u64 + 1);
			assert_eq!(frontier.root().0, mmr.get_root().unwrap(), "at {} leaves", i + 1);
		}
	}

	#[test]
	fn extension_proof_yields_new_root() {
		for (old_n, new_n) in [(1u64, 2u64), (3, 4), (7, 8), (10, 25), (25, 26), (1, 100)] {
			let (_, old_frontier) = build(old_n);
			let (store, _) = build(new_n);
			let mmr = MemMMR::<Hash, TestMerge>::new(leaf_index_to_mmr_size(new_n - 1), &store);
			let ancestry = mmr.gen_ancestry_proof(leaf_index_to_mmr_size(old_n - 1)).unwrap();
			let proof = MMRExtensionProof::from_ancestry_proof(new_n, &ancestry);

			assert_eq!(
				proof.verify(&old_frontier).unwrap().0,
				full_root(&store, new_n),
				"extension {old_n} -> {new_n}"
			);
		}
	}

	#[test]
	fn empty_extension_is_identity() {
		let (store, frontier) = build(10);
		assert_eq!(MMRExtensionProof::empty().verify(&frontier).unwrap().0, full_root(&store, 10));
	}

	#[test]
	fn extension_from_empty_frontier() {
		let (store, _) = build(5);
		let new_mmr_size = leaf_index_to_mmr_size(4);
		let mmr = MemMMR::<Hash, TestMerge>::new(new_mmr_size, &store);
		// From empty, the connecting nodes are exactly the new peaks.
		let connecting_nodes: Vec<(u64, Hash)> = get_peaks(new_mmr_size)
			.into_iter()
			.map(|p| (p, mmr.store().get_elem(p).unwrap().unwrap()))
			.collect();
		let proof = MMRExtensionProof { leaf_count: 5, connecting_nodes };

		assert_eq!(proof.verify(&MmrFrontier::default()).unwrap().0, full_root(&store, 5));

		// Wrong positions rejected.
		let mut bad = proof.clone();
		bad.connecting_nodes.swap(0, 1);
		assert_eq!(bad.verify(&MmrFrontier::default()), Err(MmrError::InvalidProof));
	}

	#[test]
	fn extension_proof_rejects_tampering_and_regression() {
		let (_, old_frontier) = build(10);
		let (store, _) = build(25);
		let mmr = MemMMR::<Hash, TestMerge>::new(leaf_index_to_mmr_size(24), &store);
		let ancestry = mmr.gen_ancestry_proof(leaf_index_to_mmr_size(9)).unwrap();
		let proof = MMRExtensionProof::from_ancestry_proof(25, &ancestry);

		// Tampered connecting node: either structurally invalid or yields a
		// different root — never the committed one.
		let mut tampered = proof.clone();
		tampered.connecting_nodes[0].1 = Hash::repeat_byte(0xAA);
		match tampered.verify(&old_frontier) {
			Ok(root) => assert_ne!(root.0, full_root(&store, 25)),
			Err(e) => assert_eq!(e, MmrError::InvalidProof),
		}

		// Backward "extension" is structurally rejected.
		let (_, newer_frontier) = build(30);
		assert_eq!(proof.verify(&newer_frontier), Err(MmrError::NotForward));

		// Inconsistent frontier rejected.
		let mut bad = old_frontier.clone();
		bad.leaf_count += 1;
		assert_eq!(proof.verify(&bad), Err(MmrError::InconsistentFrontier));
	}

	#[test]
	fn inclusion_proof_yields_root_and_pins_position() {
		let n = 13u64;
		let (store, _) = build(n);
		let mmr_size = leaf_index_to_mmr_size(n - 1);
		let mmr = MemMMR::<Hash, TestMerge>::new(mmr_size, &store);
		let all = leaves(n);

		for position in [0u64, 5, n - 1] {
			let pos = mmr_lib::leaf_index_to_pos(position);
			let proof = mmr.gen_proof(alloc::vec![pos]).unwrap();
			let proof = MmrInclusionProof { mmr_size, items: proof.proof_items().to_vec() };

			assert_eq!(proof.leaf_count(), n);
			assert_eq!(
				proof.verify_leaf(MessagePosition(position), all[position as usize]).unwrap().0,
				full_root(&store, n),
				"position {position}"
			);

			// The same leaf at another position does not verify to the root.
			let other = (position + 1) % n;
			match proof.verify_leaf(MessagePosition(other), all[position as usize]) {
				Ok(root) => assert_ne!(root.0, full_root(&store, n)),
				Err(_) => (),
			}
			// Out-of-range positions rejected outright.
			assert_eq!(
				proof.verify_leaf(MessagePosition(n), all[0]),
				Err(MmrError::PositionOutOfRange)
			);
		}
	}

	#[test]
	fn head_proof_yields_position_and_frontier() {
		// Crosses every peak shape: single leaf, perfect trees, the head
		// being its own peak, multi-peak with a deep path.
		for n in [1u64, 2, 3, 4, 7, 8, 13, 64, 130] {
			let (store, frontier) = build(n);
			let mmr_size = leaf_index_to_mmr_size(n - 1);
			let mmr = MemMMR::<Hash, TestMerge>::new(mmr_size, &store);
			let proof = mmr.gen_proof(alloc::vec![mmr_lib::leaf_index_to_pos(n - 1)]).unwrap();
			let proof = MmrInclusionProof { mmr_size, items: proof.proof_items().to_vec() };
			let head_leaf = leaves(n)[n as usize - 1];

			let (position, yielded) = proof.verify_head(head_leaf).unwrap();
			assert_eq!(position, MessagePosition(n - 1), "n={n}");
			// Bit-identical to the frontier built by appending — root AND
			// peaks, so extension proofs can pick up from it.
			assert_eq!(yielded, frontier, "n={n}");
			assert_eq!(yielded.root().0, full_root(&store, n), "n={n}");
			// Agrees with the generic single-leaf verification.
			assert_eq!(proof.verify_leaf(position, head_leaf).unwrap(), yielded.root());
		}
	}

	#[test]
	fn head_proof_structural_defects_rejected_tampering_yields_other_root() {
		let n = 13u64;
		let (store, _) = build(n);
		let mmr_size = leaf_index_to_mmr_size(n - 1);
		let mmr = MemMMR::<Hash, TestMerge>::new(mmr_size, &store);
		let proof = mmr.gen_proof(alloc::vec![mmr_lib::leaf_index_to_pos(n - 1)]).unwrap();
		let proof = MmrInclusionProof { mmr_size, items: proof.proof_items().to_vec() };
		let head_leaf = leaves(n)[n as usize - 1];

		// Wrong item count (truncated or padded) is structurally invalid.
		let mut truncated = proof.clone();
		truncated.items.pop();
		assert_eq!(truncated.verify_head(head_leaf), Err(MmrError::InvalidProof));
		let mut padded = proof.clone();
		padded.items.push(Hash::repeat_byte(0xAA));
		assert_eq!(padded.verify_head(head_leaf), Err(MmrError::InvalidProof));

		// Invalid MMR size / empty MMR rejected outright.
		let mut bad_size = proof.clone();
		bad_size.mmr_size = 2;
		assert_eq!(bad_size.verify_head(head_leaf), Err(MmrError::InvalidProof));
		let empty = MmrInclusionProof { mmr_size: 0, items: alloc::vec![] };
		assert_eq!(empty.verify_head(head_leaf), Err(MmrError::PositionOutOfRange));

		// A tampered item never "fails": it yields a DIFFERENT frontier —
		// binding to a committed root is the lift's job, not this one's.
		let mut tampered = proof.clone();
		tampered.items[0] = Hash::repeat_byte(0xBB);
		let (_, yielded) = tampered.verify_head(head_leaf).unwrap();
		assert_ne!(yielded.root().0, full_root(&store, n));
	}

	#[test]
	fn domain_tags_isolate_all_contexts() {
		// Same child bytes under every tag must hash differently.
		let payload = [0x42u8; 64];
		let hashes: Vec<Hash> =
			[LEAF_TAG, INNER_TAG, PEAK_TAG, EMPTY_TAG, TREE_LEAF_TAG, TREE_INNER_TAG]
				.iter()
				.map(|tag| {
					let mut preimage = alloc::vec![*tag];
					preimage.extend_from_slice(&payload);
					SpecHasher::hash(&preimage)
				})
				.collect();
		for (i, a) in hashes.iter().enumerate() {
			for (j, b) in hashes.iter().enumerate() {
				if i != j {
					assert_ne!(a, b, "tags {i} and {j} collide");
				}
			}
		}
	}

	#[test]
	fn merge_and_peak_bagging_are_order_sensitive_and_isolated() {
		let a = Hash::repeat_byte(1);
		let b = Hash::repeat_byte(2);
		assert_ne!(TestMerge::merge(&a, &b).unwrap(), TestMerge::merge(&b, &a).unwrap());
		assert_ne!(
			TestMerge::merge_peaks(&a, &b).unwrap(),
			TestMerge::merge_peaks(&b, &a).unwrap()
		);
		assert_ne!(TestMerge::merge(&a, &b).unwrap(), TestMerge::merge_peaks(&a, &b).unwrap());
	}
}
