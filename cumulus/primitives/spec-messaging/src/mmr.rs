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
use sp_io::hashing::blake2_256;
use sp_mmr_primitives::mmr_lib;

use crate::{INNER_TAG, PEAK_TAG};

/// [`mmr_lib::Merge`] adapter that wires domain-tagged blake2_256 hashing into the MMR library.
///
/// Pass this as the `M` type parameter when constructing an
/// `mmr_lib::MMR<Hash, SpecMerge, S>` accumulator.
pub struct SpecMerge;
impl mmr_lib::Merge for SpecMerge {
	type Item = Hash;

	fn merge(left: &Self::Item, right: &Self::Item) -> mmr_lib::Result<Self::Item> {
		let mut preimage = Vec::with_capacity(1 + 32 + 32);
		preimage.push(INNER_TAG);
		preimage.extend_from_slice(left.as_bytes());
		preimage.extend_from_slice(right.as_bytes());
		Ok(blake2_256(&preimage).into())
	}

	fn merge_peaks(peak1: &Self::Item, peak2: &Self::Item) -> mmr_lib::Result<Self::Item> {
		let mut preimage = Vec::with_capacity(1 + 32 + 32);
		preimage.push(PEAK_TAG);
		preimage.extend_from_slice(peak1.as_bytes());
		preimage.extend_from_slice(peak2.as_bytes());

		Ok(blake2_256(&preimage).into())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::vec::Vec;
	use mmr_lib::util::{MemMMR, MemStore};

	fn new_mmr(store: &MemStore<Hash>) -> MemMMR<'_, Hash, SpecMerge> {
		MemMMR::<Hash, SpecMerge>::new(0, store)
	}

	fn merge(l: Hash, r: Hash) -> Hash {
		SpecMerge::merge(&l, &r).expect("merge is infallible; qed")
	}

	fn merge_peaks(l: Hash, r: Hash) -> Hash {
		SpecMerge::merge_peaks(&l, &r).expect("merge_peaks is infallible; qed")
	}

	#[test]
	fn empty_mmr_is_empty() {
		let store = MemStore::default();
		let mmr = new_mmr(&store);

		assert!(mmr.is_empty());
		assert!(mmr.get_root().is_err());
	}

	#[test]
	fn root_is_deterministic_for_the_same_sequence() {
		let store_a = MemStore::default();
		let store_b = MemStore::default();
		let mut a = new_mmr(&store_a);
		let mut b = new_mmr(&store_b);

		for i in 0..5u8 {
			a.push(Hash::repeat_byte(i)).expect("push is infallible; qed");
			b.push(Hash::repeat_byte(i)).expect("push is infallible; qed");
		}

		assert_eq!(a.get_root().unwrap(), b.get_root().unwrap());
	}

	#[test]
	fn root_changes_after_each_append() {
		let store = MemStore::default();
		let mut mmr = new_mmr(&store);
		let mut roots = Vec::new();

		for i in 0..8u8 {
			mmr.push(Hash::repeat_byte(i)).expect("push is infallible; qed");
			roots.push(mmr.get_root().expect("non-empty after push; qed"));
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
	fn merge_is_order_sensitive() {
		let a = Hash::repeat_byte(1);
		let b = Hash::repeat_byte(2);

		assert_ne!(merge(a, b), merge(b, a));
	}

	#[test]
	fn merge_peaks_is_order_sensitive() {
		let a = Hash::repeat_byte(1);
		let b = Hash::repeat_byte(2);

		assert_ne!(merge_peaks(a, b), merge_peaks(b, a));
	}

	#[test]
	fn domain_tags_isolate_inner_and_peak_hashes() {
		let a = Hash::repeat_byte(1);
		let b = Hash::repeat_byte(2);

		assert_ne!(merge(a, b), merge_peaks(a, b));
	}
}
