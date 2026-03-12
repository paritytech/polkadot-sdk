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

//! Lightweight per-destination MMR that stores only peaks.
//!
//! Each destination parachain gets its own independent MMR. Only the peaks
//! are stored on-chain, which is sufficient to compute the MMR root and to
//! accept new leaves. Full node storage of all leaves and inner nodes is
//! left to off-chain indexing (future work).
//!
//! The MMR root is computed by "bagging" the peaks right-to-left using the
//! same `0x02` domain-prefixed hashing as the speculative messaging
//! primitives.

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;
use sp_core::H256;

/// Maximum number of peaks in an MMR.
///
/// An MMR with `n` leaves has at most `log2(n) + 1` peaks. With 64-bit leaf
/// indices, the theoretical maximum is 64 peaks. We use 64 as the bound.
pub const MAX_PEAKS: u32 = 64;

/// State of a single per-destination MMR, storing only the peaks.
#[derive(
	Debug, Clone, PartialEq, Eq, Default, Encode, Decode, DecodeWithMemTracking, TypeInfo,
)]
pub struct MmrState {
	/// Number of leaves in this MMR.
	pub leaf_count: u64,
	/// The peak hashes, ordered left-to-right (largest subtree first).
	pub peaks: Vec<H256>,
}

impl MmrState {
	/// Create a new empty MMR state.
	pub fn new() -> Self {
		Self::default()
	}

	/// Compute the MMR root by bagging the peaks right-to-left.
	///
	/// Returns `H256::zero()` for an empty MMR.
	pub fn root(&self) -> H256 {
		bag_peaks(&self.peaks)
	}

	/// Push a new leaf hash into the MMR and return the updated root.
	///
	/// This implements the standard MMR push algorithm:
	/// 1. Start with the leaf hash as `current`.
	/// 2. For each trailing 1-bit in the current leaf index, pop the last
	///    peak and merge it with `current` (peak on the left, current on
	///    the right).
	/// 3. Push `current` as a new peak.
	/// 4. Increment `leaf_count`.
	pub fn push(&mut self, leaf_hash: H256) -> H256 {
		let mut current = leaf_hash;
		// The leaf index before inserting is `self.leaf_count`.
		// Each trailing 1-bit in the binary representation of leaf_count
		// means we need to merge with the peak to the left.
		let mut pos = self.leaf_count;
		while pos & 1 == 1 {
			// Pop the rightmost peak (which is the sibling at this height).
			// Safety: peaks is non-empty when pos has a trailing 1-bit because
			// each previous push added a peak for each leading 0-bit; qed
			let left = self.peaks.pop().expect(
				"peaks must be non-empty when merging; \
				 each trailing 1-bit in leaf_count corresponds to an existing peak; qed",
			);
			current = merge_peaks(left, current);
			pos >>= 1;
		}

		self.peaks.push(current);
		self.leaf_count += 1;

		self.root()
	}
}

/// Merge two MMR nodes using the `0x02` domain prefix, consistent with
/// [`polkadot_primitives_speculative_messaging::proofs::bag_peaks`].
fn merge_peaks(left: H256, right: H256) -> H256 {
	let mut buf = [0u8; 65];
	buf[0] = 0x02;
	buf[1..33].copy_from_slice(left.as_bytes());
	buf[33..65].copy_from_slice(right.as_bytes());
	H256::from(sp_core::hashing::blake2_256(&buf))
}

/// Bag MMR peaks into a single root hash.
///
/// Folds right-to-left: starts with the last peak, then for each preceding
/// peak computes `merge_peaks(peak, acc)`.
pub fn bag_peaks(peaks: &[H256]) -> H256 {
	if peaks.is_empty() {
		return H256::zero();
	}
	if peaks.len() == 1 {
		return peaks[0];
	}
	let mut iter = peaks.iter().rev();
	// Safe because we checked len >= 2 above; qed
	let mut acc = *iter.next().expect("peaks has at least 2 elements; qed");
	for peak in iter {
		acc = merge_peaks(*peak, acc);
	}
	acc
}

#[cfg(test)]
mod tests {
	use super::*;

	fn leaf(seed: u8) -> H256 {
		H256::from(sp_core::hashing::blake2_256(&[seed]))
	}

	#[test]
	fn empty_mmr() {
		let state = MmrState::new();
		assert_eq!(state.leaf_count, 0);
		assert_eq!(state.root(), H256::zero());
	}

	#[test]
	fn single_leaf() {
		let mut state = MmrState::new();
		let h = leaf(1);
		let root = state.push(h);
		assert_eq!(state.leaf_count, 1);
		assert_eq!(state.peaks.len(), 1);
		assert_eq!(state.peaks[0], h);
		assert_eq!(root, h);
	}

	#[test]
	fn two_leaves_merge() {
		let mut state = MmrState::new();
		let h0 = leaf(0);
		let h1 = leaf(1);

		state.push(h0);
		let root = state.push(h1);

		// After 2 leaves: one peak = merge(h0, h1)
		assert_eq!(state.leaf_count, 2);
		assert_eq!(state.peaks.len(), 1);
		assert_eq!(state.peaks[0], merge_peaks(h0, h1));
		assert_eq!(root, merge_peaks(h0, h1));
	}

	#[test]
	fn three_leaves() {
		let mut state = MmrState::new();
		let h0 = leaf(0);
		let h1 = leaf(1);
		let h2 = leaf(2);

		state.push(h0);
		state.push(h1);
		let root = state.push(h2);

		// After 3 leaves: two peaks = [merge(h0, h1), h2]
		assert_eq!(state.leaf_count, 3);
		assert_eq!(state.peaks.len(), 2);

		let p0 = merge_peaks(h0, h1);
		assert_eq!(state.peaks[0], p0);
		assert_eq!(state.peaks[1], h2);
		assert_eq!(root, bag_peaks(&[p0, h2]));
	}

	#[test]
	fn four_leaves_full_merge() {
		let mut state = MmrState::new();
		let leaves: Vec<H256> = (0..4u8).map(leaf).collect();
		for h in &leaves {
			state.push(*h);
		}

		// 4 = 0b100: single peak
		assert_eq!(state.leaf_count, 4);
		assert_eq!(state.peaks.len(), 1);

		let p01 = merge_peaks(leaves[0], leaves[1]);
		let p23 = merge_peaks(leaves[2], leaves[3]);
		let expected = merge_peaks(p01, p23);
		assert_eq!(state.peaks[0], expected);
		assert_eq!(state.root(), expected);
	}

	#[test]
	fn peak_count_matches_popcount() {
		// The number of peaks in an MMR with n leaves equals the number of
		// 1-bits in the binary representation of n.
		let mut state = MmrState::new();
		for i in 1u64..=100 {
			state.push(leaf(i as u8));
			let expected_peaks = i.count_ones() as usize;
			assert_eq!(
				state.peaks.len(),
				expected_peaks,
				"mismatch at leaf_count={}",
				i,
			);
		}
	}

	#[test]
	fn root_deterministic() {
		let mut s1 = MmrState::new();
		let mut s2 = MmrState::new();
		for i in 0..10u8 {
			s1.push(leaf(i));
			s2.push(leaf(i));
		}
		assert_eq!(s1.root(), s2.root());
		assert_eq!(s1, s2);
	}

	#[test]
	fn encode_decode_roundtrip() {
		let mut state = MmrState::new();
		for i in 0..7u8 {
			state.push(leaf(i));
		}
		let encoded = state.encode();
		let decoded = MmrState::decode(&mut &encoded[..]).expect("should decode");
		assert_eq!(state, decoded);
		assert_eq!(state.root(), decoded.root());
	}
}
