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
///
/// Unpaired nodes are promoted without a sibling, at any level. In the trees below, `(n)` is
/// the proven leaf, `[s]` a sibling in the proof and `|` a promotion.
///
/// No promotion (width 4, position 1, siblings=2):
///
/// ```text
///        root
///       /    \
///      .     [s]
///     / \    / \
///   [0] (1) 2   3
/// ```
///
/// ```
/// assert_eq!(cumulus_aura_authorizer_utils::expected_siblings(4, 1), 2);
/// ```
///
/// Promoted at level 0 (width 7, position 6, siblings 2):
///
/// ```text
///             root
///         /          \
///       [s]            .
///      /   \         /   \
///     .     .      [s]    |
///    / \   / \     / \    |
///   0   1 2   3   4   5  (6)
/// ```
///
/// ```
/// assert_eq!(cumulus_aura_authorizer_utils::expected_siblings(7, 6), 2);
/// ```
///
/// Promoted twice at levels 0 and 1 (width 5, position 4, siblings 1):
///
/// ```text
///           root
///         /      \
///       [s]       |
///      /   \      |
///     .     .     |
///    / \   / \    |
///   0   1 2   3  (4)
/// ```
///
/// ```
/// assert_eq!(cumulus_aura_authorizer_utils::expected_siblings(5, 4), 1);
/// ```
pub fn expected_siblings(mut width: u32, mut position: u32) -> usize {
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
	use alloc::vec::Vec;

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

	/// Every proof the SDK tree builds has exactly the expected number of siblings.
	#[test]
	fn expected_siblings_matches_built_proofs_works() {
		for width in 1u32..=130 {
			let members: Vec<_> = (0..width).map(|i| [i as u8; 32]).collect();

			for position in 0..width {
				let (_, proof) = super::build_collator_proof(&members, position).unwrap();

				assert_eq!(
					super::expected_siblings(width, position),
					proof.len(),
					"width {width}, position {position}"
				);
			}
		}
	}

	#[test]
	fn expected_siblings_works() {
		for (width, position, siblings) in [
			(1, 0, 0), // Single leaf is the root.
			(2, 0, 1),
			(2, 1, 1),
			(6, 4, 2), // Even width, promoted at level 1.
			(1 << 31, 0, 31),
			((1 << 31) + 1, 1 << 31, 1), // Promoted at every level but the top.
			(u32::MAX, 0, 32),           // `width - 1` and `div_ceil` must not overflow.
			(u32::MAX, u32::MAX - 1, 31),
		] {
			assert_eq!(
				super::expected_siblings(width, position),
				siblings,
				"width {width}, position {position}"
			);
		}
	}
}
