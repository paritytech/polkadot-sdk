// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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
use primitive_types::H256;
use sp_crypto_hashing::blake2_256;

use crate::token::CollatorKey;

/// Number of sibling hashes in a collator membership proof.
///
/// This is ⌈log₂(collator_set_size)⌉, the depth of the zero-padded power-of-two tree.
pub(crate) fn proof_depth(collator_set_size: u32) -> usize {
	(u32::BITS - collator_set_size.saturating_sub(1).leading_zeros()) as usize
}

/// Blake2b-256 adapter for the SDK's binary Merkle tree.
pub(crate) struct CollatorHasher;

impl hash_db::Hasher for CollatorHasher {
	type Out = H256;
	type StdHasher = hash256_std_hasher::Hash256StdHasher;
	const LENGTH: usize = 32;

	fn hash(input: &[u8]) -> H256 {
		H256::from(blake2_256(input))
	}
}

/// Build a zero-padded Merkle tree and return its root and one proof per key, in input order.
///
/// See [`crate::token::AuthToken::check_proof`] for the proof format.
///
/// # Panics
///
/// Panics if `keys` is empty or the padded tree exceeds the SDK's `u32` leaf count.
pub fn build_collator_tree(keys: &[CollatorKey]) -> (H256, Vec<Vec<H256>>) {
	assert!(!keys.is_empty(), "a collator set must have at least one collator");

	let mut leaves: Vec<H256> = keys.iter().map(|key| H256::from(blake2_256(key))).collect();
	leaves.resize(keys.len().next_power_of_two(), H256::zero());
	assert!(u32::try_from(leaves.len()).is_ok(), "collator tree exceeds the SDK leaf limit");

	let mut root = H256::zero();
	let proofs = (0..keys.len())
		.map(|index| {
			let proof = binary_merkle_tree::merkle_proof_raw::<CollatorHasher, _>(
				leaves.iter().copied(),
				index as u32,
			);
			root = proof.root;
			proof.proof
		})
		.collect();
	(root, proofs)
}
