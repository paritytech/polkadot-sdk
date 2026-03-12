// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Binary Merkle tree for mapping sorted `(ParaId, H256)` pairs to a
//! single top-level root hash.
//!
//! This is **not** an MMR. It is a simple balanced binary Merkle tree
//! rebuilt from scratch whenever the set of destinations changes.

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use scale_info::TypeInfo;
use sp_core::H256;

use crate::error::SpeculativeMessagingError;

/// Proof of inclusion for a single leaf in the destination Merkle tree.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MerkleProof {
	/// Position of the leaf in the sorted leaf array.
	pub leaf_index: u32,
	/// Total number of leaves in the tree.
	pub leaf_count: u32,
	/// Sibling hashes from the leaf up to the root.
	pub siblings: Vec<H256>,
}

/// A builder / verifier for the top-level destination Merkle tree.
///
/// There is no stored state: every method is a pure function.
pub struct DestinationMerkleTree;

impl DestinationMerkleTree {
	/// Compute the Merkle root for a set of `(ParaId, mmr_root)` pairs.
	///
	/// The entries are sorted by `ParaId` before hashing. An empty input
	/// yields `H256::zero()`.
	pub fn compute_root(destinations: &[(ParaId, H256)]) -> H256 {
		if destinations.is_empty() {
			return H256::zero();
		}

		let mut sorted: Vec<(ParaId, H256)> = destinations.to_vec();
		sorted.sort_by_key(|(id, _)| *id);

		let mut level: Vec<H256> = sorted.iter().map(|(id, root)| hash_leaf(*id, *root)).collect();

		while level.len() > 1 {
			let mut next_level = Vec::with_capacity(level.len().div_ceil(2));
			let mut i = 0;
			while i < level.len() {
				if i + 1 < level.len() {
					next_level.push(hash_pair(level[i], level[i + 1]));
					i += 2;
				} else {
					// Odd node out: hash with itself.
					next_level.push(hash_pair(level[i], level[i]));
					i += 1;
				}
			}
			level = next_level;
		}

		level[0]
	}

	/// Generate a Merkle proof for a given `target` ParaId.
	///
	/// Returns the tree root together with a [`MerkleProof`].
	/// Fails with [`SpeculativeMessagingError::DestinationNotFound`]
	/// when `target` is not present among `destinations`.
	pub fn generate_proof(
		destinations: &[(ParaId, H256)],
		target: ParaId,
	) -> Result<(H256, MerkleProof), SpeculativeMessagingError> {
		let mut sorted: Vec<(ParaId, H256)> = destinations.to_vec();
		sorted.sort_by_key(|(id, _)| *id);

		let leaf_index = sorted
			.iter()
			.position(|(id, _)| *id == target)
			.ok_or(SpeculativeMessagingError::DestinationNotFound)? as u32;

		let leaf_count = sorted.len() as u32;

		let mut level: Vec<H256> = sorted.iter().map(|(id, root)| hash_leaf(*id, *root)).collect();

		let mut siblings = Vec::new();
		let mut idx = leaf_index as usize;

		while level.len() > 1 {
			let mut next_level = Vec::with_capacity(level.len().div_ceil(2));
			let mut i = 0;
			let mut next_idx = 0;

			while i < level.len() {
				if i + 1 < level.len() {
					let parent = hash_pair(level[i], level[i + 1]);
					// Record sibling if this pair contains our node.
					if i == idx {
						siblings.push(level[i + 1]);
					} else if i + 1 == idx {
						siblings.push(level[i]);
					}
					// Track index in next level.
					if i == idx || i + 1 == idx {
						next_idx = next_level.len();
					}
					next_level.push(parent);
					i += 2;
				} else {
					// Odd node out: hash with itself.
					let parent = hash_pair(level[i], level[i]);
					if i == idx {
						siblings.push(level[i]);
						next_idx = next_level.len();
					}
					next_level.push(parent);
					i += 1;
				}
			}

			idx = next_idx;
			level = next_level;
		}

		let root = level[0];
		Ok((root, MerkleProof { leaf_index, leaf_count, siblings }))
	}

	/// Verify that `(para_id, mmr_root)` is included in the tree with
	/// the given `root`.
	pub fn verify_proof(
		root: H256,
		para_id: ParaId,
		mmr_root: H256,
		proof: &MerkleProof,
	) -> Result<(), SpeculativeMessagingError> {
		let mut current = hash_leaf(para_id, mmr_root);
		let mut idx = proof.leaf_index as usize;

		// Work out the number of nodes at each level so we know when the
		// last node at a level was unpaired (odd-node promotion).
		let mut level_size = proof.leaf_count as usize;
		let mut sibling_iter = proof.siblings.iter();

		while level_size > 1 {
			let sibling =
				sibling_iter.next().ok_or(SpeculativeMessagingError::InvalidMerkleProof)?;

			if idx % 2 == 0 {
				current = hash_pair(current, *sibling);
			} else {
				current = hash_pair(*sibling, current);
			}

			idx /= 2;
			level_size = level_size.div_ceil(2);
		}

		if current == root {
			Ok(())
		} else {
			Err(SpeculativeMessagingError::RootMismatch)
		}
	}
}

/// Hash a single `(ParaId, H256)` leaf.
fn hash_leaf(para_id: ParaId, mmr_root: H256) -> H256 {
	let encoded = (para_id, mmr_root).encode();
	H256::from(sp_core::hashing::blake2_256(&encoded))
}

/// Hash two child nodes together to form a parent node.
fn hash_pair(left: H256, right: H256) -> H256 {
	let mut combined = Vec::with_capacity(64);
	combined.extend_from_slice(left.as_bytes());
	combined.extend_from_slice(right.as_bytes());
	H256::from(sp_core::hashing::blake2_256(&combined))
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::{Decode, Encode};

	fn para(id: u32) -> ParaId {
		ParaId::from(id)
	}

	fn dummy_root(seed: u8) -> H256 {
		H256::from([seed; 32])
	}

	// ---------------------------------------------------------------
	// Root computation
	// ---------------------------------------------------------------

	#[test]
	fn empty_tree_root() {
		let root = DestinationMerkleTree::compute_root(&[]);
		assert_eq!(root, H256::zero());
	}

	#[test]
	fn single_leaf_root() {
		let destinations = [(para(1), dummy_root(0xAA))];
		let root = DestinationMerkleTree::compute_root(&destinations);

		let expected = hash_leaf(para(1), dummy_root(0xAA));
		assert_eq!(root, expected);
	}

	#[test]
	fn two_leaf_root() {
		let destinations = [(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB))];
		let root = DestinationMerkleTree::compute_root(&destinations);

		let l0 = hash_leaf(para(1), dummy_root(0xAA));
		let l1 = hash_leaf(para(2), dummy_root(0xBB));
		let expected = hash_pair(l0, l1);
		assert_eq!(root, expected);
	}

	#[test]
	fn three_leaf_root() {
		let destinations =
			[(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB)), (para(3), dummy_root(0xCC))];
		let root = DestinationMerkleTree::compute_root(&destinations);

		let l0 = hash_leaf(para(1), dummy_root(0xAA));
		let l1 = hash_leaf(para(2), dummy_root(0xBB));
		let l2 = hash_leaf(para(3), dummy_root(0xCC));
		let n01 = hash_pair(l0, l1);
		// Third leaf hashed with itself (odd promotion).
		let n22 = hash_pair(l2, l2);
		let expected = hash_pair(n01, n22);
		assert_eq!(root, expected);
	}

	#[test]
	fn four_leaf_root() {
		let destinations = [
			(para(1), dummy_root(0xAA)),
			(para(2), dummy_root(0xBB)),
			(para(3), dummy_root(0xCC)),
			(para(4), dummy_root(0xDD)),
		];
		let root = DestinationMerkleTree::compute_root(&destinations);

		let l0 = hash_leaf(para(1), dummy_root(0xAA));
		let l1 = hash_leaf(para(2), dummy_root(0xBB));
		let l2 = hash_leaf(para(3), dummy_root(0xCC));
		let l3 = hash_leaf(para(4), dummy_root(0xDD));
		let n01 = hash_pair(l0, l1);
		let n23 = hash_pair(l2, l3);
		let expected = hash_pair(n01, n23);
		assert_eq!(root, expected);
	}

	#[test]
	fn sorting_is_applied() {
		let forward =
			[(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB)), (para(3), dummy_root(0xCC))];
		let backward =
			[(para(3), dummy_root(0xCC)), (para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB))];
		assert_eq!(
			DestinationMerkleTree::compute_root(&forward),
			DestinationMerkleTree::compute_root(&backward),
		);
	}

	// ---------------------------------------------------------------
	// Proof generation & verification
	// ---------------------------------------------------------------

	#[test]
	fn proof_single_leaf() {
		let destinations = [(para(1), dummy_root(0xAA))];
		let (root, proof) = DestinationMerkleTree::generate_proof(&destinations, para(1))
			.expect("proof generation should succeed");

		assert_eq!(proof.leaf_index, 0);
		assert_eq!(proof.leaf_count, 1);
		assert!(proof.siblings.is_empty());

		DestinationMerkleTree::verify_proof(root, para(1), dummy_root(0xAA), &proof)
			.expect("verification should succeed");
	}

	#[test]
	fn proof_two_leaves_left() {
		let destinations = [(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB))];
		let (root, proof) = DestinationMerkleTree::generate_proof(&destinations, para(1))
			.expect("proof generation should succeed");

		assert_eq!(proof.leaf_index, 0);
		assert_eq!(proof.leaf_count, 2);
		assert_eq!(proof.siblings.len(), 1);

		// The sibling should be the hash of the right leaf.
		let expected_sibling = hash_leaf(para(2), dummy_root(0xBB));
		assert_eq!(proof.siblings[0], expected_sibling);

		DestinationMerkleTree::verify_proof(root, para(1), dummy_root(0xAA), &proof)
			.expect("verification should succeed");
	}

	#[test]
	fn proof_two_leaves_right() {
		let destinations = [(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB))];
		let (root, proof) = DestinationMerkleTree::generate_proof(&destinations, para(2))
			.expect("proof generation should succeed");

		assert_eq!(proof.leaf_index, 1);
		assert_eq!(proof.leaf_count, 2);
		assert_eq!(proof.siblings.len(), 1);

		let expected_sibling = hash_leaf(para(1), dummy_root(0xAA));
		assert_eq!(proof.siblings[0], expected_sibling);

		DestinationMerkleTree::verify_proof(root, para(2), dummy_root(0xBB), &proof)
			.expect("verification should succeed");
	}

	#[test]
	fn proof_four_leaves() {
		let destinations = [
			(para(1), dummy_root(0xAA)),
			(para(2), dummy_root(0xBB)),
			(para(3), dummy_root(0xCC)),
			(para(4), dummy_root(0xDD)),
		];

		for &(id, mmr) in &destinations {
			let (root, proof) = DestinationMerkleTree::generate_proof(&destinations, id)
				.expect("proof generation should succeed");

			assert_eq!(proof.leaf_count, 4);

			DestinationMerkleTree::verify_proof(root, id, mmr, &proof)
				.expect("verification should succeed");
		}
	}

	#[test]
	fn proof_seven_leaves() {
		let destinations: Vec<(ParaId, H256)> =
			(1..=7u32).map(|i| (para(i), dummy_root(i as u8))).collect();

		for &(id, mmr) in &destinations {
			let (root, proof) = DestinationMerkleTree::generate_proof(&destinations, id)
				.expect("proof generation should succeed");

			assert_eq!(proof.leaf_count, 7);

			DestinationMerkleTree::verify_proof(root, id, mmr, &proof)
				.expect("verification should succeed");
		}
	}

	// ---------------------------------------------------------------
	// Error paths
	// ---------------------------------------------------------------

	#[test]
	fn proof_invalid_root_fails() {
		let destinations = [(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB))];
		let (_root, proof) = DestinationMerkleTree::generate_proof(&destinations, para(1))
			.expect("proof generation should succeed");

		let bad_root = H256::from([0xFF; 32]);
		let result =
			DestinationMerkleTree::verify_proof(bad_root, para(1), dummy_root(0xAA), &proof);
		assert_eq!(result, Err(SpeculativeMessagingError::RootMismatch));
	}

	#[test]
	fn proof_wrong_mmr_root_fails() {
		let destinations = [(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB))];
		let (root, proof) = DestinationMerkleTree::generate_proof(&destinations, para(1))
			.expect("proof generation should succeed");

		let wrong_mmr = H256::from([0xFF; 32]);
		let result = DestinationMerkleTree::verify_proof(root, para(1), wrong_mmr, &proof);
		assert_eq!(result, Err(SpeculativeMessagingError::RootMismatch));
	}

	#[test]
	fn proof_destination_not_found() {
		let destinations = [(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB))];
		let result = DestinationMerkleTree::generate_proof(&destinations, para(99));
		assert_eq!(result, Err(SpeculativeMessagingError::DestinationNotFound));
	}

	// ---------------------------------------------------------------
	// Codec round-trip
	// ---------------------------------------------------------------

	#[test]
	fn proof_encode_decode_roundtrip() {
		let destinations =
			[(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB)), (para(3), dummy_root(0xCC))];
		let (_root, proof) = DestinationMerkleTree::generate_proof(&destinations, para(2))
			.expect("proof generation should succeed");

		let encoded = proof.encode();
		let decoded = MerkleProof::decode(&mut &encoded[..]).expect("decoding should succeed");
		assert_eq!(proof, decoded);
	}

	// ---------------------------------------------------------------
	// Stress / large tree
	// ---------------------------------------------------------------

	#[test]
	fn large_tree_all_proofs_valid() {
		let destinations: Vec<(ParaId, H256)> =
			(1..=100u32).map(|i| (para(i), dummy_root(i as u8))).collect();

		let expected_root = DestinationMerkleTree::compute_root(&destinations);

		for &(id, mmr) in &destinations {
			let (root, proof) = DestinationMerkleTree::generate_proof(&destinations, id)
				.expect("proof generation should succeed");

			assert_eq!(root, expected_root);
			assert_eq!(proof.leaf_count, 100);

			DestinationMerkleTree::verify_proof(root, id, mmr, &proof)
				.expect("verification should succeed");
		}
	}
}
