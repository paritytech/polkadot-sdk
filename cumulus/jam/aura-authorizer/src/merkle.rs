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

/// Hash of one collator-set leaf.
pub(crate) fn collator_leaf_hash(key: &CollatorKey) -> [u8; 32] {
	blake2_256(key)
}

/// Hash of an ordered pair of nodes.
pub(crate) fn join(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
	let mut input = [0u8; 64];
	input[..32].copy_from_slice(left);
	input[32..].copy_from_slice(right);
	blake2_256(&input)
}

/// Build a zero-padded Merkle tree and return its root and one proof per key, in input order.
///
/// See [`crate::token::AuthToken::check_proof`] for the proof format.
///
/// # Panics
///
/// Panics if `keys` is empty.
pub fn build_collator_tree(keys: &[CollatorKey]) -> (H256, Vec<Vec<H256>>) {
	assert!(!keys.is_empty(), "a collator set must have at least one collator");

	let mut level: Vec<[u8; 32]> = keys.iter().map(collator_leaf_hash).collect();
	level.resize(keys.len().next_power_of_two(), [0u8; 32]);

	// Every level except the root; a proof takes one sibling from each of them.
	let mut levels = Vec::new();
	while level.len() > 1 {
		let parents = level.chunks(2).map(|pair| join(&pair[0], &pair[1])).collect();
		levels.push(level);
		level = parents;
	}

	let proofs = (0..keys.len())
		.map(|leaf| {
			levels
				.iter()
				.enumerate()
				.map(|(depth, nodes)| H256::from(nodes[(leaf >> depth) ^ 1]))
				.collect()
		})
		.collect();

	(H256::from(level[0]), proofs)
}
