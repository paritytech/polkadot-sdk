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
use hash_db::Hasher as _;
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

pub(crate) fn padded_leaf_count(size: usize) -> Result<u32, CollatorProofError> {
	if size == 0 {
		return Err(CollatorProofError::EmptySet);
	}

	u32::try_from(size)
		.ok()
		.and_then(u32::checked_next_power_of_two)
		.ok_or(CollatorProofError::TooManyCollators)
}

/// Build a zero-padded Merkle tree and return its root and the proof for `collator_index`.
///
/// Construction takes O(n) hashing work for n keys.
///
/// Returns an error if `keys` is empty, the padded tree exceeds the SDK's `u32` leaf count,
/// or `collator_index` is outside the unpadded collator set.
pub fn build_collator_proof(
	keys: &[CollatorKey],
	collator_index: u32,
) -> Result<(H256, Vec<H256>), CollatorProofError> {
	let padded_size = padded_leaf_count(keys.len())?;
	if collator_index as usize >= keys.len() {
		return Err(CollatorProofError::InvalidCollatorIndex);
	}

	let leaves = (0..padded_size as usize)
		.map(|index| keys.get(index).map_or_else(H256::zero, |key| CollatorHasher::hash(key)));

	let proof = binary_merkle_tree::merkle_proof_raw::<CollatorHasher, _>(leaves, collator_index);
	Ok((proof.root, proof.proof))
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

	#[test]
	fn padded_leaf_count_boundaries_works() {
		for (size, expected) in [(1, 1), (3, 4), (1usize << 31, 1u32 << 31)] {
			assert_eq!(super::padded_leaf_count(size), Ok(expected));
		}
	}

	#[test]
	fn padded_leaf_count_oversized_errors() {
		for size in [(1usize << 31) + 1, u32::MAX as usize, usize::MAX] {
			assert_eq!(
				super::padded_leaf_count(size),
				Err(super::CollatorProofError::TooManyCollators)
			);
		}
	}
}
