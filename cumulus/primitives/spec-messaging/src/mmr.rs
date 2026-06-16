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

use alloc::vec::Vec;
use polkadot_core_primitives::Hash;
use sp_core::Get;
use sp_io::hashing::blake2_256;
use sp_runtime::BoundedVec;

use crate::{EMPTY_TAG, INNER_TAG, PEAK_TAG};

/// Generic interface for accumulating MMR leaves and producing a root commitment.
/// Abstracted as a trait so the underlying scheme (MMR, or in the future MMB)
/// can change without breaking callers.
pub trait MmrAccumulator {
	/// Append a new leaf hash to the accumulator.
	fn append(&mut self, leaf: Hash);

	/// The current root commitment over all appended leaves.
	fn root(&self) -> Hash;

	/// The number of leaves appended so far.
	fn size(&self) -> u64;

	/// Post-MVP: Produce a proof that the structure was validly extended.
	fn extension_proof(&self) -> Result<(), &'static str> {
		Err("Not implemented yet")
	}
}

#[derive(Default)]
pub struct Mmr<MaxPeaks: Get<u32>> {
	peaks: BoundedVec<Hash, MaxPeaks>,
	size: u64,
}

impl<MaxPeaks: Get<u32>> MmrAccumulator for Mmr<MaxPeaks> {
	/// Appends a new leaf hash, updating the peak list and leaf count.
	///
	/// Appending a leaf is structurally identical to incrementing `size` by one in
	/// binary: each "carry" merges two equal-height peaks via [`merge_inner`] into a
	/// single peak of the next height up, until no two adjacent peaks share a height.
	/// After this call, `self.peaks.len() == self.size.count_ones()`.
	fn append(&mut self, leaf: Hash) {
		let mut node = leaf;

		// Going from `size` to `size + 1` leaves merges exactly
		// `trailing_zeros(size + 1)` pairs of peaks (binary-counter carry).
		let merges = (self.size + 1).trailing_zeros();
		for _ in 0..merges {
			let left = self.peaks.pop().expect("a peak exists for each merge; qed");
			node = merge_inner(left, node);
		}

		self.peaks
			.try_push(node)
			.expect("number of peaks is boubded by log2(size), which never exceeds MaxPeaks; qed");
		self.size += 1;
	}

	/// Get the root hash of the MMR.
	fn root(&self) -> Hash {
		if self.size == 0 {
			empty_root()
		} else {
			merge_peaks(&self.peaks)
		}
	}

	/// Get the size of the MMR.
	fn size(&self) -> u64 {
		self.size
	}
}


/// Merges two sibling nodes into a parent by hashing `[INNER_TAG, left, right]`.
pub fn merge_inner(left: Hash, right: Hash) -> Hash {
	let mut preimage = Vec::with_capacity(1 + 32 + 32);
	preimage.push(INNER_TAG);
	preimage.extend_from_slice(left.as_bytes());
	preimage.extend_from_slice(right.as_bytes());
	blake2_256(&preimage).into()
}

/// Bags all peaks into a single root by hashing `[PEAK_TAG, peak_0, peak_1, ...]`.
pub fn merge_peaks(peaks: &[Hash]) -> Hash {
	let mut preimage = Vec::with_capacity(1 + 32 * peaks.len());
	preimage.push(PEAK_TAG);
	for peak in peaks {
		preimage.extend_from_slice(peak.as_bytes());
	}

	blake2_256(&preimage).into()
}

/// Returns the canonical root of an MMR with no leaves, hashed as `[EMPTY_TAG]`.
pub fn empty_root() -> Hash {
	let preimage = [EMPTY_TAG];
	blake2_256(&preimage).into()
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::ConstU32;

	type TestMmr = Mmr<ConstU32<64>>;

	#[test]
	fn empty_mmr_has_zero_size_and_empty_root() {
		let mmr = TestMmr::default();

		assert_eq!(mmr.size(), 0);
		assert_eq!(mmr.root(), empty_root());
	}

	#[test]
	fn peak_count_matches_leaf_count_population() {
		let mut mmr = TestMmr::default();

		for expected_size in 1..=16u64 {
			mmr.append(Hash::repeat_byte(expected_size as u8));

			assert_eq!(mmr.size(), expected_size);
			assert_eq!(mmr.peaks.len() as u32, mmr.size().count_ones());
		}
	}

	#[test]
	fn single_leaf_root_is_its_peak_bagged_alone() {
		let mut mmr = TestMmr::default();
		let leaf = Hash::repeat_byte(7);

		mmr.append(leaf);

		assert_eq!(mmr.root(), merge_peaks(&[leaf]));
	}

	#[test]
	fn root_is_deterministic_for_the_same_sequence() {
		let mut a = TestMmr::default();
		let mut b = TestMmr::default();

		for i in 0..5u8 {
			a.append(Hash::repeat_byte(i));
			b.append(Hash::repeat_byte(i));
		}

		assert_eq!(a.root(), b.root());
	}

	#[test]
	fn root_changes_after_each_append() {
		let mut mmr = TestMmr::default();
		let mut roots = Vec::new();

		for i in 0..8u8 {
			mmr.append(Hash::repeat_byte(i));
			roots.push(mmr.root());
		}

		for (i, a) in roots.iter().enumerate() {
			for (j, b) in roots.iter().enumerate() {
				if i != j {
					assert_ne!(a, b, "roots at {i} and {j} should differ");
				}
			}
		}
	}

	#[test]
	fn merge_inner_is_order_sensitive() {
		let a = Hash::repeat_byte(1);
		let b = Hash::repeat_byte(2);

		assert_ne!(merge_inner(a, b), merge_inner(b, a));
	}

	#[test]
	fn merge_peaks_is_order_sensitive() {
		let a = Hash::repeat_byte(1);
		let b = Hash::repeat_byte(2);

		assert_ne!(merge_peaks(&[a, b]), merge_peaks(&[b, a]));
	}

	#[test]
	fn domain_tags_isolate_inner_peak_and_empty_hashes() {
		let a = Hash::repeat_byte(1);
		let b = Hash::repeat_byte(2);

		let inner = merge_inner(a, b);
		let peaks = merge_peaks(&[a, b]);
		let empty = empty_root();

		assert_ne!(inner, peaks);
		assert_ne!(inner, empty);
		assert_ne!(peaks, empty);
	}

	#[test]
	fn extension_proof_is_unsupported_by_default() {
		let mmr = TestMmr::default();

		assert!(mmr.extension_proof().is_err());
	}
}
