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
use sp_mmr_primitives::mmr_lib;

use crate::{INNER_TAG, PEAK_TAG};

/// The canonical hasher for Speculative Messaging —
/// [`BlakeTwo256`](sp_runtime::traits::BlakeTwo256).
///
/// Swap this alias to change the hash function across the entire protocol in one edit.
pub type SpecHasher = sp_runtime::traits::BlakeTwo256;

/// [`mmr_lib::Merge`] adapter that wires domain-tagged hashing into the MMR library.
///
/// `H` must implement [`sp_runtime::traits::Hash`] with `Output = Hash`. Pass this as the `M`
/// type parameter when constructing an `mmr_lib::MMR<Hash, SpecMerge<H>, S>` accumulator.
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

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::vec::Vec;
	use mmr_lib::{
		util::{MemMMR, MemStore},
		Merge,
	};

	type TestMerge = SpecMerge<SpecHasher>;

	fn new_mmr(store: &MemStore<Hash>) -> MemMMR<'_, Hash, TestMerge> {
		MemMMR::<Hash, TestMerge>::new(0, store)
	}

	fn merge(l: Hash, r: Hash) -> Hash {
		TestMerge::merge(&l, &r).expect("merge is infallible; qed")
	}

	fn merge_peaks(l: Hash, r: Hash) -> Hash {
		TestMerge::merge_peaks(&l, &r).expect("merge_peaks is infallible; qed")
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
