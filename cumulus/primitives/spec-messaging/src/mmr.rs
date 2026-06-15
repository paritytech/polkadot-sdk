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

	fn root(&self) -> Hash {
		if self.size == 0 {
			empty_root()
		} else {
			merge_peaks(&self.peaks)
		}
	}

	fn size(&self) -> u64 {
		self.size
	}
}

pub fn merge_inner(left: Hash, right: Hash) -> Hash {
	let mut preimage = Vec::with_capacity(1 + 32 + 32);
	preimage.push(INNER_TAG);
	preimage.extend_from_slice(left.as_bytes());
	preimage.extend_from_slice(right.as_bytes());
	blake2_256(&preimage).into()
}

pub fn merge_peaks(peaks: &[Hash]) -> Hash {
	let mut preimage = Vec::with_capacity(1 + 32 * peaks.len());
	preimage.push(PEAK_TAG);
	for peak in peaks {
		preimage.extend_from_slice(peak.as_bytes());
	}

	blake2_256(&preimage).into()
}

pub fn empty_root() -> Hash {
	let preimage = [EMPTY_TAG];
	blake2_256(&preimage).into()
}
