// This file is part of Cumulus.

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

/// Invalid collator set or index supplied to the proof builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollatorProofError {
	EmptySet,
	TooManyCollators,
	InvalidCollatorIndex,
}

/// Build an unpadded Merkle tree and return its root and proof for `collator_index`.
///
/// Keys are hashed as leaves; unpaired nodes are promoted to the next level.
/// Construction takes O(n) hashing work for n keys.
///
/// Returns an error for an empty set, more than `u32::MAX` keys, or an invalid index.
pub fn build_collator_proof(
	keys: &[CollatorKey],
	collator_index: u32,
) -> Result<(H256, Vec<H256>), CollatorProofError> {
	if keys.is_empty() {
		return Err(CollatorProofError::EmptySet);
	}
	u32::try_from(keys.len()).map_err(|_| CollatorProofError::TooManyCollators)?;
	if collator_index as usize >= keys.len() {
		return Err(CollatorProofError::InvalidCollatorIndex);
	}

	let proof = binary_merkle_tree::merkle_proof::<CollatorHasher, _, _>(keys, collator_index);
	Ok((proof.root, proof.proof))
}

/// Verify collator membership, including the leaf index and proof length.
pub(crate) fn verify_collator_proof(
	root: &H256,
	proof: &[H256],
	size: u32,
	collator_index: u32,
	key: &CollatorKey,
) -> bool {
	if collator_index >= size || proof.len() != expected_siblings(size, collator_index) {
		return false;
	}

	binary_merkle_tree::verify_proof::<CollatorHasher, _, _>(
		root,
		proof.iter().copied(),
		size,
		collator_index,
		binary_merkle_tree::Leaf::Value(key),
	)
}

/// Count sibling hashes needed to prove membership of the value at the position.
///
/// This is used to ensure that the right number of hashes was provided by the proof.
pub(crate) fn expected_siblings(mut width: u32, mut position: u32) -> usize {
	let mut siblings = 0;

	while width > 1 {
		// Odd positions have a left sibling; even positions need a node to their right.
		if position % 2 == 1 || position < width - 1 {
			siblings += 1;
		}
		position /= 2;
		width = width.div_ceil(2);
	}

	siblings
}

#[cfg(test)]
mod tests {
	#[test]
	fn build_collator_proof_empty_set_errors() {
		for index in [0, 1, u32::MAX] {
			assert_eq!(
				super::build_collator_proof(&[], index),
				Err(super::CollatorProofError::EmptySet)
			);
		}
	}

	#[test]
	fn build_collator_proof_invalid_index_errors() {
		for index in [3, 4, u32::MAX] {
			assert_eq!(
				super::build_collator_proof(&[[1; 32], [2; 32], [3; 32]], index),
				Err(super::CollatorProofError::InvalidCollatorIndex)
			);
		}
	}
}
