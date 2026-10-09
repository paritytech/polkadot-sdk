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
pub(crate) fn proof_len(collator_set_size: u32) -> usize {
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

/// Invalid collator set supplied to the proof builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollatorProofError {
	EmptySet,
	TooManyCollators,
}

fn padded_leaf_count(size: usize) -> Result<u32, CollatorProofError> {
	if size == 0 {
		return Err(CollatorProofError::EmptySet);
	}
	u32::try_from(size)
		.ok()
		.and_then(u32::checked_next_power_of_two)
		.ok_or(CollatorProofError::TooManyCollators)
}

/// Build a zero-padded Merkle tree and return its root and one proof per key, in input order.
///
/// See [`crate::token::AuthToken::check_proof`] for the proof format.
///
/// Returns an error if `keys` is empty or the padded tree exceeds the SDK's `u32` leaf count.
pub fn build_collator_proofs(
	keys: &[CollatorKey],
) -> Result<(H256, Vec<Vec<H256>>), CollatorProofError> {
	let padded_size = padded_leaf_count(keys.len())?;

	let mut leaves: Vec<H256> = keys.iter().map(|key| H256::from(blake2_256(key))).collect();
	leaves.resize(padded_size as usize, H256::zero());

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
	Ok((root, proofs))
}

#[cfg(test)]
mod tests {
	use super::proof_len;

	#[test]
	fn build_collator_proofs_empty_set_errors() {
		assert_eq!(super::build_collator_proofs(&[]), Err(super::CollatorProofError::EmptySet));
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

	#[test]
	fn proof_len_empty_and_singleton_works() {
		// Empty sets are rejected by the caller; the length helper still returns zero.
		assert_eq!(proof_len(0), 0);
		assert_eq!(proof_len(1), 0);
	}

	#[test]
	fn proof_len_powers_of_two_works() {
		for exponent in 0..32 {
			let size = 1u32 << exponent;
			assert_eq!(proof_len(size), exponent as usize, "set size {size}");
		}
	}

	#[test]
	fn proof_len_power_of_two_neighbors_works() {
		assert_eq!(proof_len(2 - 1), 0);
		assert_eq!(proof_len(2 + 1), 2);
		for exponent in 2..32 {
			let size = 1u32 << exponent;
			assert_eq!(proof_len(size - 1), exponent as usize, "set size {}", size - 1);
			assert_eq!(proof_len(size + 1), exponent as usize + 1, "set size {}", size + 1);
		}
	}

	#[test]
	fn proof_len_maximum_size_works() {
		assert_eq!(proof_len(u32::MAX), 32);
	}
}
