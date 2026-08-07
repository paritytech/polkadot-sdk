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
//! The hash function is a type parameter `H` (a `sp_runtime::traits::Hash` with a
//! 32-byte output), so the protocol can switch hashers (e.g. blake2 ↔ keccak)
//! without changing this code. Consumers pick a concrete hasher; the POC uses
//! `polkadot_primitives::v9::SpecHasher` (blake2_256).
//!
//! Domain separation: leaves are hashed by [`crate::message::leaf_hash`] (`LEAF_TAG`), inner
//! nodes by [`SpecMerge::merge`] (`INNER_TAG`), and peak-bagging by
//! [`SpecMerge::merge_peaks`] (`PEAK_TAG`). `mmr_lib` calls `merge` for tree nodes
//! and the overridable `merge_peaks` when bagging peaks, so no two roles collide on
//! the same hash.
//!
//! [`SpecMerge`] is the `mmr_lib::Merge` used for inclusion proofs (`gen_proof`/
//! `MerkleProof::verify`) and append-only ancestry proofs (`gen_ancestry_proof`/
//! `verify_incremental`). [`Mmr`] is a peaks-only [`MmrAccumulator`] for the
//! sender's on-chain state.

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use mmr_lib::{Error as MmrError, Merge};
use polkadot_core_primitives::Hash;
use scale_info::TypeInfo;
use sp_runtime::traits::Hash as HashT;

use crate::{EMPTY_TAG, INNER_TAG, PEAK_TAG};

/// A leaf position within a stream's MMR (its leaf count). Newtype, per the design's
/// `MessagePosition` — a position is not an arbitrary `u64`. Lives here (a leaf module both `lift`
/// and `message` depend on) so neither has to depend on the other for it.
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

/// Domain-tagged merge for the speculative-messaging MMR, generic over the hash
/// function `H`. Used as the `mmr_lib::Merge` implementation for the subtree.
pub struct SpecMerge<H>(PhantomData<H>);

impl<H: HashT<Output = Hash>> Merge for SpecMerge<H> {
	type Item = Hash;

	/// Inner-node merge: `H(INNER_TAG ++ left ++ right)`.
	fn merge(left: &Hash, right: &Hash) -> Result<Hash, MmrError> {
		Ok(tagged_node::<H>(INNER_TAG, left, right))
	}

	/// Peak-bagging merge: `H(PEAK_TAG ++ left ++ right)`. Domain-separated from
	/// `merge` so a bagged value can never be reinterpreted as an inner node.
	fn merge_peaks(left: &Hash, right: &Hash) -> Result<Hash, MmrError> {
		Ok(tagged_node::<H>(PEAK_TAG, left, right))
	}
}

/// `H(tag ++ left ++ right)`.
fn tagged_node<H: HashT<Output = Hash>>(tag: u8, left: &Hash, right: &Hash) -> Hash {
	let mut preimage = [0u8; 1 + 32 + 32];
	preimage[0] = tag;
	preimage[1..33].copy_from_slice(left.as_bytes());
	preimage[33..65].copy_from_slice(right.as_bytes());
	<H as HashT>::hash(&preimage)
}

/// The defined root of an *empty* MMR frontier: `H(EMPTY_TAG)`. `mmr_lib` errors on
/// empty MMRs, but the protocol needs a comparable value — the `Interval.start` of a
/// stream's first-ever consumption is exactly this root (encoding spec §3.4). Empty
/// streams are still never committed to a `StreamsRoot` tree entry.
pub fn empty_root<H: HashT<Output = Hash>>() -> Hash {
	<H as HashT>::hash(&[EMPTY_TAG])
}

/// Compute the MMR root from its non-empty peaks (highest to lowest), matching
/// `mmr_lib`'s bagging (`merge_peaks(right, left)` folded right-to-left).
///
/// This lets the on-chain outbox keep only the O(log n) peaks and still derive the
/// same stream root that `mmr_lib`'s `MMR::get_root` and `MerkleProof::verify`
/// produce.
///
/// # Panics
///
/// Panics on an empty `peaks` slice — the empty root is a *distinct constant*, not a
/// bag of zero peaks: use [`empty_root`] (via `MmrFrontier::root`, which branches).
pub fn root_from_peaks<H: HashT<Output = Hash>>(peaks: &[Hash]) -> Hash {
	let mut iter = peaks.iter().rev();
	let mut acc = *iter.next().expect(
		"root_from_peaks called on empty peaks; the empty root is `empty_root`, not a bag; qed",
	);
	for left in iter {
		// mmr_lib bags as merge_peaks(right, left); `acc` carries the right side.
		acc =
			<SpecMerge<H> as Merge>::merge_peaks(&acc, left).expect("SpecMerge is infallible; qed");
	}
	acc
}

/// Generic interface for accumulating MMR leaves and producing a root commitment.
/// Abstracted as a trait so the underlying scheme (MMR, or in the future MMB) can
/// change without breaking callers.
pub trait MmrAccumulator {
	/// Append a new leaf hash to the accumulator.
	fn append(&mut self, leaf: Hash);

	/// The current root commitment over all appended leaves.
	fn root(&self) -> Hash;

	/// The number of leaves appended so far.
	fn size(&self) -> u64;
}

/// Peaks-only MMR accumulator (O(log n) state), generic over the hash function.
///
/// Peak hashes are identical to `mmr_lib`'s, so [`root`](MmrAccumulator::root)
/// (via [`root_from_peaks`]) equals `mmr_lib::MMR::get_root`, and proofs generated
/// by a full `mmr_lib::MMR` over the same leaves verify against this root. The
/// sender keeps `(peaks, size)` in storage and round-trips through
/// [`from_parts`](Mmr::from_parts) / [`into_parts`](Mmr::into_parts).
pub struct Mmr<H> {
	peaks: Vec<Hash>,
	size: u64,
	_marker: PhantomData<H>,
}

impl<H> Mmr<H> {
	/// An empty accumulator.
	pub fn new() -> Self {
		Self { peaks: Vec::new(), size: 0, _marker: PhantomData }
	}

	/// Reconstruct an accumulator from previously stored `(peaks, size)`.
	pub fn from_parts(peaks: Vec<Hash>, size: u64) -> Self {
		Self { peaks, size, _marker: PhantomData }
	}

	/// Decompose into `(peaks, size)` for storage.
	pub fn into_parts(self) -> (Vec<Hash>, u64) {
		(self.peaks, self.size)
	}

	/// The current peaks (highest to lowest).
	pub fn peaks(&self) -> &[Hash] {
		&self.peaks
	}
}

impl<H> Default for Mmr<H> {
	fn default() -> Self {
		Self::new()
	}
}

impl<H: HashT<Output = Hash>> MmrAccumulator for Mmr<H> {
	/// Append a leaf, merging equal-height peaks via [`SpecMerge`] (matching
	/// `mmr_lib`'s internal node construction). After this call,
	/// `peaks.len() == size.count_ones()`.
	fn append(&mut self, leaf: Hash) {
		let mut node = leaf;
		// Going from `size` to `size + 1` leaves merges exactly
		// `trailing_zeros(size + 1)` pairs of peaks (binary-counter carry).
		let merges = (self.size + 1).trailing_zeros();
		for _ in 0..merges {
			let left = self.peaks.pop().expect("a peak exists for each merge; qed");
			node =
				<SpecMerge<H> as Merge>::merge(&left, &node).expect("SpecMerge is infallible; qed");
		}
		self.peaks.push(node);
		self.size += 1;
	}

	fn root(&self) -> Hash {
		root_from_peaks::<H>(&self.peaks)
	}

	fn size(&self) -> u64 {
		self.size
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use mmr_lib::{
		leaf_index_to_pos,
		util::{MemMMR, MemStore},
		MerkleProof,
	};
	use sp_runtime::traits::BlakeTwo256;

	type H = BlakeTwo256;

	fn h(byte: u8) -> Hash {
		Hash::repeat_byte(byte)
	}

	#[test]
	fn merge_and_merge_peaks_are_domain_separated() {
		let a = h(1);
		let b = h(2);
		// Inner-node merge and peak-bagging of the same inputs must differ (domain tags).
		assert_ne!(
			<SpecMerge<H> as Merge>::merge(&a, &b).unwrap(),
			<SpecMerge<H> as Merge>::merge_peaks(&a, &b).unwrap()
		);
	}

	#[test]
	fn merge_is_order_sensitive() {
		let a = h(1);
		let b = h(2);
		assert_ne!(
			<SpecMerge<H> as Merge>::merge(&a, &b).unwrap(),
			<SpecMerge<H> as Merge>::merge(&b, &a).unwrap()
		);
	}

	#[test]
	fn accumulator_root_matches_mmr_lib() {
		// Build via the peaks-only accumulator and via a full mmr_lib MMR; the
		// roots and peak-count invariant must agree.
		let mut acc = Mmr::<H>::new();
		let store = MemStore::<Hash>::default();
		let mut reference = MemMMR::<Hash, SpecMerge<H>>::new(0, &store);
		for i in 1..=5u8 {
			acc.append(h(i));
			reference.push(h(i)).unwrap();
			assert_eq!(acc.peaks().len() as u32, acc.size().count_ones());
		}
		assert_eq!(acc.root(), reference.get_root().unwrap());
	}

	#[test]
	fn empty_root_frozen_vector() {
		// FROZEN consensus vector: blake2b-256 of the single byte 0x04 (EMPTY_TAG),
		// the defined root of an empty frontier (encoding spec §3.4).
		assert_eq!(
			empty_root::<H>(),
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
		let mut acc = Mmr::<H>::new();
		for i in 1..=5u8 {
			acc.append(h(i));
		}
		assert_eq!(
			acc.root(),
			Hash::from(hex_literal::hex!(
				"2cfe88aae5315b66e2252889957efd68c4749d1cfbcba31d0ca00b902117e862"
			))
		);
	}

	#[test]
	fn inclusion_proof_round_trips() {
		let store = MemStore::<Hash>::default();
		let mut mmr = MemMMR::<Hash, SpecMerge<H>>::new(0, &store);
		let positions: Vec<u64> = (0..6u8).map(|i| mmr.push(h(i)).unwrap()).collect();
		let root = mmr.get_root().unwrap();

		let proof: MerkleProof<Hash, SpecMerge<H>> =
			mmr.gen_proof(vec![leaf_index_to_pos(1), leaf_index_to_pos(4)]).unwrap();

		assert!(proof.verify(root, vec![(positions[1], h(1)), (positions[4], h(4))]).unwrap());
		assert!(!proof.verify(root, vec![(positions[1], h(99)), (positions[4], h(4))]).unwrap());
	}
}
