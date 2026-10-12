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

//! MMR primitives for the Speculative Messaging protocol.
//!
//! Hashing is [`SpecHasher`] with domain tags: leaves by [`crate::message::leaf_hash`]
//! (`LEAF_TAG`), inner nodes by [`SpecMerge::merge`] (`INNER_TAG`), peak bagging by
//! [`SpecMerge::merge_peaks`] (`PEAK_TAG`). `mmr_lib` calls `merge` for tree nodes and
//! `merge_peaks` when bagging, so no two roles collide.
//!
//! [`SpecMerge`] is the `mmr_lib::Merge` for inclusion proofs (`gen_proof` / `MerkleProof::verify`)
//! and ancestry proofs (`gen_ancestry_proof` / `verify_incremental`). [`MmrFrontier`] is the
//! peaks-only state: the sender's on-chain accumulator, and what proofs extend from.

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, Error as CodecError, Input, MaxEncodedLen};
use mmr_lib::{ancestry_proof::bagging_peaks_hashes, Error as MmrError, Merge};
use polkadot_core_primitives::Hash;
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_runtime::{traits::Hash as HashT, BoundedVec};

use crate::{SpecHasher, EMPTY_TAG, INNER_TAG, PEAK_TAG};

/// Upper bound on any leaf count node positions are derived from. `mmr_lib`'s node count, `2 *
/// leaf_count - leaf_count.count_ones()`, overflows `u64` above `2^63`. Peer-supplied counts are
/// checked against this first. `2^48` is far beyond any real stream.
pub const MAX_MMR_LEAF_COUNT: u64 = 1 << 48;

/// A leaf position within a stream's MMR. Defined here because both `lift` and `message` use it.
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
	Default,
	TypeInfo,
)]
pub struct MessagePosition(pub u64);

/// Domain-tagged `mmr_lib::Merge` for the speculative-messaging MMR.
pub struct SpecMerge;

impl Merge for SpecMerge {
	type Item = Hash;

	/// Inner-node merge: `H(INNER_TAG ++ left ++ right)`.
	fn merge(left: &Hash, right: &Hash) -> Result<Hash, MmrError> {
		Ok(tagged_node(INNER_TAG, left, right))
	}

	/// Peak-bagging merge: `H(PEAK_TAG ++ left ++ right)`. A bagged value can never be read as an
	/// inner node.
	fn merge_peaks(left: &Hash, right: &Hash) -> Result<Hash, MmrError> {
		Ok(tagged_node(PEAK_TAG, left, right))
	}
}

/// `H(tag ++ left ++ right)`.
fn tagged_node(tag: u8, left: &Hash, right: &Hash) -> Hash {
	let mut preimage = [0u8; 1 + 32 + 32];
	preimage[0] = tag;
	preimage[1..33].copy_from_slice(left.as_bytes());
	preimage[33..65].copy_from_slice(right.as_bytes());
	<SpecHasher as HashT>::hash(&preimage)
}

/// The root of an empty MMR: `H(EMPTY_TAG)` (encoding spec §3.4). `mmr_lib` has none; the protocol
/// needs a comparable value, since a stream's first consumption starts from the empty frontier.
/// Empty streams are never committed to the `StreamsRoot` tree.
pub fn empty_root() -> Hash {
	<SpecHasher as HashT>::hash(&[EMPTY_TAG])
}

/// Bag the peaks (highest to lowest) into a root with `mmr_lib`'s own bagging, so the result equals
/// `MMR::get_root`. `None` for no peaks; the empty root is the constant [`empty_root`], which
/// `MmrFrontier::root` substitutes.
pub fn root_from_peaks(peaks: &[Hash]) -> Option<Hash> {
	// `SpecMerge` is infallible, so `Err` means no peaks.
	bagging_peaks_hashes::<Hash, SpecMerge>(peaks.to_vec()).ok()
}

/// A bagged MMR root: a stream's committed root at some point in its history. A newtype so it
/// cannot be confused with a `StreamsRoot`, the commitment-tree root.
#[derive(Clone, Copy, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub struct MmrRoot(pub Hash);

/// A peaks-only MMR frontier: the O(log n) state a stream continues from, and the accumulator that
/// continues it. `leaf_count` fixes where an extension proof's connecting nodes go.
///
/// Peak hashes equal `mmr_lib`'s, so [`root`](Self::root) equals `mmr_lib::MMR::get_root` and
/// proofs from a full `mmr_lib::MMR` over the same leaves verify against it.
///
/// Fields are private. [`new`](Self::new), [`from_parts`](Self::from_parts) and `Decode` are the
/// only constructors, and each enforces one peak per set bit of `leaf_count` and `leaf_count <=
/// MAX_MMR_LEAF_COUNT`.
///
/// Encodes as `leaf_count ‖ peaks`. A decoder reads the count first, and the `BoundedVec` rejects
/// an over-long length prefix before any peak is read.
#[derive(Clone, Encode, PartialEq, Eq, Debug, TypeInfo, Default)]
pub struct MmrFrontier {
	/// Number of leaves these peaks summarize; also the position of the next message to append.
	leaf_count: u64,
	/// MMR peaks, highest to lowest. At most 64 for a `u64` leaf count.
	peaks: BoundedVec<Hash, ConstU32<64>>,
}

impl MmrFrontier {
	/// The empty frontier.
	pub fn new() -> Self {
		Self::default()
	}

	/// A frontier from `(peaks, leaf_count)`. `None` unless there is one peak per set bit of
	/// `leaf_count` and `leaf_count <= MAX_MMR_LEAF_COUNT`.
	pub fn from_parts(peaks: Vec<Hash>, leaf_count: u64) -> Option<Self> {
		if leaf_count > MAX_MMR_LEAF_COUNT || peaks.len() != leaf_count.count_ones() as usize {
			return None;
		}
		// ≤ 64 is implied by the set-bit count of a `u64`, so this cannot fail.
		BoundedVec::try_from(peaks).ok().map(|peaks| Self { leaf_count, peaks })
	}

	/// The peaks, highest to lowest.
	pub fn peaks(&self) -> &[Hash] {
		&self.peaks
	}

	/// The number of leaves these peaks summarize.
	pub fn leaf_count(&self) -> u64 {
		self.leaf_count
	}

	/// Append a leaf, merging equal-height peaks via [`SpecMerge`] (matching `mmr_lib`'s
	/// internal node construction). Preserves both constructor invariants.
	pub fn append(&mut self, leaf: Hash) {
		let mut node = leaf;
		// Going from `leaf_count` to `leaf_count + 1` merges exactly
		// `trailing_zeros(leaf_count + 1)` pairs of peaks (binary-counter carry).
		let merges = (self.leaf_count + 1).trailing_zeros();
		for _ in 0..merges {
			let left = self
				.peaks
				.pop()
				.expect("the constructors enforce one peak per set bit of `leaf_count`; qed");
			node = <SpecMerge as Merge>::merge(&left, &node).expect("SpecMerge is infallible; qed");
		}
		self.peaks
			.try_push(node)
			.expect("a `u64` leaf count never has more than 64 set bits; qed");
		self.leaf_count += 1;
	}

	/// The bagged root. The empty frontier has the defined root `H(EMPTY_TAG)` ([`empty_root`],
	/// encoding spec §3.4), so a frontier that has consumed nothing compares and extends like any
	/// other.
	pub fn root(&self) -> MmrRoot {
		MmrRoot(root_from_peaks(&self.peaks).unwrap_or_else(empty_root))
	}

	/// The `mmr_lib` node count (size) of this frontier's MMR. Total under the leaf-count
	/// invariant.
	pub(crate) fn mmr_size(&self) -> u64 {
		if self.leaf_count == 0 {
			0
		} else {
			mmr_lib::leaf_index_to_mmr_size(self.leaf_count - 1)
		}
	}
}

/// Field order matches the derived `Encode`. The decoded pair goes through
/// [`from_parts`](MmrFrontier::from_parts), so a frontier off the wire holds the same invariants as
/// one built here. The bound rejects a bad length prefix before any peak is read.
impl Decode for MmrFrontier {
	fn decode<I: Input>(input: &mut I) -> Result<Self, CodecError> {
		let leaf_count = u64::decode(input)?;
		let peaks = BoundedVec::<Hash, ConstU32<64>>::decode(input)?;
		Self::from_parts(peaks.into_inner(), leaf_count)
			.ok_or_else(|| "MmrFrontier: inconsistent peaks / leaf count".into())
	}
}

impl DecodeWithMemTracking for MmrFrontier {}

#[cfg(test)]
mod tests {
	use super::*;
	use mmr_lib::{
		leaf_index_to_pos,
		util::{MemMMR, MemStore},
		MerkleProof,
	};
	fn h(byte: u8) -> Hash {
		Hash::repeat_byte(byte)
	}

	#[test]
	fn merge_and_merge_peaks_are_domain_separated() {
		let a = h(1);
		let b = h(2);
		// Inner-node merge and peak-bagging of the same inputs must differ (domain tags).
		assert_ne!(
			<SpecMerge as Merge>::merge(&a, &b).unwrap(),
			<SpecMerge as Merge>::merge_peaks(&a, &b).unwrap()
		);
	}

	#[test]
	fn merge_is_order_sensitive() {
		let a = h(1);
		let b = h(2);
		assert_ne!(
			<SpecMerge as Merge>::merge(&a, &b).unwrap(),
			<SpecMerge as Merge>::merge(&b, &a).unwrap()
		);
	}

	#[test]
	fn from_parts_enforces_append_invariants() {
		// Consistent pairs round-trip, including the empty accumulator.
		assert!(MmrFrontier::from_parts(Vec::new(), 0).is_some());
		assert!(MmrFrontier::from_parts(vec![h(1)], 1).is_some());
		assert!(MmrFrontier::from_parts(vec![h(1), h(2)], 3).is_some());
		// One peak per set bit, or `append` would pop from an empty vec.
		assert!(MmrFrontier::from_parts(Vec::new(), 1).is_none());
		assert!(MmrFrontier::from_parts(vec![h(1)], 3).is_none());
		assert!(MmrFrontier::from_parts(vec![h(1), h(2)], 1).is_none());
		// And within the ceiling, or the node-position arithmetic downstream is not total.
		assert!(MmrFrontier::from_parts(vec![h(1)], MAX_MMR_LEAF_COUNT).is_some());
		assert!(MmrFrontier::from_parts(vec![h(1)], 1 << 63).is_none());
		assert!(MmrFrontier::from_parts(vec![h(1); 64], u64::MAX).is_none());
	}

	#[test]
	fn accumulator_root_matches_mmr_lib() {
		// Build via the peaks-only accumulator and via a full mmr_lib MMR; the
		// roots and peak-count invariant must agree.
		let mut acc = MmrFrontier::new();
		let store = MemStore::<Hash>::default();
		let mut reference = MemMMR::<Hash, SpecMerge>::new(0, &store);
		for i in 1..=5u8 {
			acc.append(h(i));
			reference.push(h(i)).unwrap();
			assert_eq!(acc.peaks().len() as u32, acc.leaf_count().count_ones());
		}
		assert_eq!(acc.root().0, reference.get_root().unwrap());
	}

	#[test]
	fn empty_accumulator_root_is_empty_root() {
		// A fresh accumulator is valid, and its root is the defined constant, not a panic in
		// `root_from_peaks`.
		assert_eq!(root_from_peaks(&[]), None);
		assert_eq!(MmrFrontier::new().root().0, empty_root());
		assert_eq!(MmrFrontier::from_parts(Vec::new(), 0).unwrap().root().0, empty_root());
	}

	#[test]
	fn decode_enforces_the_same_invariants_as_from_parts() {
		// A frontier off the wire goes through `from_parts`, so it holds the invariants `append`
		// and the position arithmetic rely on. Consistent bytes round-trip;
		let good = MmrFrontier::from_parts(vec![h(1), h(2)], 3).unwrap();
		assert_eq!(MmrFrontier::decode(&mut &good.encode()[..]).unwrap(), good);
		// … inconsistent ones are rejected at decode, not later in a pop or a multiply.
		let bad_shape = (3u64, vec![h(1)]).encode();
		assert!(MmrFrontier::decode(&mut &bad_shape[..]).is_err());
		let too_large = (1u64 << 63, vec![h(1)]).encode();
		assert!(MmrFrontier::decode(&mut &too_large[..]).is_err());
		// The bound is checked at the length prefix: 65 peaks are refused before being read.
		let too_many = (u64::MAX, vec![h(1); 65]).encode();
		assert!(MmrFrontier::decode(&mut &too_many[..]).is_err());
		// And the wire order is the design's: `leaf_count` first.
		assert_eq!(good.encode(), (3u64, vec![h(1), h(2)]).encode());
	}

	#[test]
	fn empty_root_frozen_vector() {
		// FROZEN consensus vector: blake2b-256 of the single byte 0x04 (EMPTY_TAG),
		// the defined root of an empty frontier (encoding spec §3.4).
		assert_eq!(
			empty_root(),
			Hash::from(hex_literal::hex!(
				"642206314f534b29ad297d82440a5f9f210e30ca5ced805a587ca402de927342"
			))
		);
	}

	#[test]
	fn mmr_root_frozen_vector() {
		// FROZEN consensus vector: appending these fixed leaves must always bag to this exact root.
		// Any change to `SpecMerge`'s inner/peak domain tags or the peak-bagging order is a
		// consensus break and breaks this test. (Round-trip tests can't catch a silent
		// byte-format change.) Pinned for `LEAF/INNER/PEAK = 0x1/0x2/0x3`.
		let mut acc = MmrFrontier::new();
		for i in 1..=5u8 {
			acc.append(h(i));
		}
		assert_eq!(
			acc.root().0,
			Hash::from(hex_literal::hex!(
				"2cfe88aae5315b66e2252889957efd68c4749d1cfbcba31d0ca00b902117e862"
			))
		);
	}

	#[test]
	fn mmr_root_frozen_vectors_at_64_and_65_leaves() {
		// FROZEN consensus vectors (encoding spec §12.2): 64 leaves collapse to a single peak
		// through the full six-level merge chain; 65 adds a second peak and exercises bagging over
		// it. Values computed independently of this crate (a Python model of §3.2's tag layout).
		let mut acc = MmrFrontier::new();
		for i in 1..=64u8 {
			acc.append(h(i));
		}
		assert_eq!(acc.peaks().len(), 1);
		assert_eq!(
			acc.root().0,
			Hash::from(hex_literal::hex!(
				"6dc59753f520a0cd93409dd10b81e73601bb54524366c616ef386ddc18e2e951"
			))
		);
		acc.append(h(65));
		assert_eq!(acc.peaks().len(), 2);
		assert_eq!(
			acc.root().0,
			Hash::from(hex_literal::hex!(
				"9f4bf61fd4315662e635c43f237c2f6e74badbd28124d8117e4a5567aaf14edc"
			))
		);
	}

	#[test]
	fn inclusion_proof_round_trips() {
		let store = MemStore::<Hash>::default();
		let mut mmr = MemMMR::<Hash, SpecMerge>::new(0, &store);
		let positions: Vec<u64> = (0..6u8).map(|i| mmr.push(h(i)).unwrap()).collect();
		let root = mmr.get_root().unwrap();

		let proof: MerkleProof<Hash, SpecMerge> =
			mmr.gen_proof(vec![leaf_index_to_pos(1), leaf_index_to_pos(4)]).unwrap();

		assert!(proof.verify(root, vec![(positions[1], h(1)), (positions[4], h(4))]).unwrap());
		assert!(!proof.verify(root, vec![(positions[1], h(99)), (positions[4], h(4))]).unwrap());
	}
}
