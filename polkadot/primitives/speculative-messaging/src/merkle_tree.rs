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
		sorted.dedup_by_key(|(id, _)| *id);

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
		if sorted.windows(2).any(|w| w[0].0 == w[1].0) {
			return Err(SpeculativeMessagingError::DuplicateDestination);
		}

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
		// C3: Validate proof fields from untrusted input.
		if proof.leaf_count == 0 || proof.leaf_index >= proof.leaf_count {
			return Err(SpeculativeMessagingError::InvalidMerkleProof);
		}

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

		// H2: Reject proofs with unconsumed trailing siblings.
		if sibling_iter.next().is_some() {
			return Err(SpeculativeMessagingError::UnconsumedProofData);
		}

		if current == root {
			Ok(())
		} else {
			Err(SpeculativeMessagingError::RootMismatch)
		}
	}
}

/// Hash a single `(ParaId, H256)` leaf with a `0x00` domain prefix to prevent
/// second-preimage attacks between leaves and internal nodes.
fn hash_leaf(para_id: ParaId, mmr_root: H256) -> H256 {
	let mut buf = alloc::vec![0x00u8];
	codec::Encode::encode_to(&(para_id, mmr_root), &mut buf);
	H256::from(sp_core::hashing::blake2_256(&buf))
}

/// Hash two child nodes together to form a parent node, using a `0x01` domain
/// prefix to prevent second-preimage attacks between internal nodes and leaves.
fn hash_pair(left: H256, right: H256) -> H256 {
	let mut buf = [0u8; 65];
	buf[0] = 0x01;
	buf[1..33].copy_from_slice(left.as_bytes());
	buf[33..65].copy_from_slice(right.as_bytes());
	H256::from(sp_core::hashing::blake2_256(&buf))
}

/// A Merkle tree that stores its internal nodes for O(log D) incremental
/// updates when a destination's MMR root changes.
///
/// # Performance
///
/// | Operation | Complexity |
/// |-----------|------------|
/// | `from_destinations` | O(D) |
/// | `root` | O(1) |
/// | `update` (existing dest) | O(log D) |
/// | `upsert` (new dest) | O(D) rebuild |
/// | `remove` | O(D) rebuild |
/// | `generate_proof` | O(log D) read |
///
/// The common hot-path — updating an existing destination's MMR root after
/// sending new messages — touches only the single leaf-to-root path.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct StoredMerkleTree {
	/// Sorted leaves: `(ParaId, MMR root)`.
	leaves: Vec<(ParaId, H256)>,
	/// Hashed nodes level-by-level. `levels[0]` = leaf hashes,
	/// `levels[last]` = `[root]`. Empty when the tree has no leaves.
	levels: Vec<Vec<H256>>,
}

impl Default for StoredMerkleTree {
	fn default() -> Self {
		Self { leaves: Vec::new(), levels: Vec::new() }
	}
}

impl StoredMerkleTree {
	/// Build a tree from a set of `(ParaId, mmr_root)` pairs. **O(D)**.
	pub fn from_destinations(destinations: &[(ParaId, H256)]) -> Self {
		let mut leaves: Vec<(ParaId, H256)> = destinations.to_vec();
		leaves.sort_by_key(|(id, _)| *id);
		leaves.dedup_by_key(|(id, _)| *id);

		let levels = Self::build_levels(&leaves);
		Self { leaves, levels }
	}

	/// The current Merkle root. **O(1)**.
	pub fn root(&self) -> H256 {
		match self.levels.last() {
			Some(top) => top.first().copied().unwrap_or(H256::zero()),
			None => H256::zero(),
		}
	}

	/// Number of leaves (destinations) in the tree.
	pub fn len(&self) -> usize {
		self.leaves.len()
	}

	/// Whether the tree is empty.
	pub fn is_empty(&self) -> bool {
		self.leaves.is_empty()
	}

	/// Update an **existing** destination's MMR root. **O(log D)**.
	///
	/// Only the leaf-to-root path is rehashed.
	pub fn update(
		&mut self,
		dest: ParaId,
		new_mmr_root: H256,
	) -> Result<(), SpeculativeMessagingError> {
		let idx = self
			.leaves
			.binary_search_by_key(&dest, |(id, _)| *id)
			.map_err(|_| SpeculativeMessagingError::DestinationNotFound)?;

		self.leaves[idx].1 = new_mmr_root;
		self.levels[0][idx] = hash_leaf(dest, new_mmr_root);
		self.rehash_path(idx);
		Ok(())
	}

	/// Insert or update a destination. **O(log D)** when updating,
	/// **O(D)** when inserting a new destination (full rebuild).
	pub fn upsert(&mut self, dest: ParaId, mmr_root: H256) {
		match self.leaves.binary_search_by_key(&dest, |(id, _)| *id) {
			Ok(idx) => {
				self.leaves[idx].1 = mmr_root;
				self.levels[0][idx] = hash_leaf(dest, mmr_root);
				self.rehash_path(idx);
			},
			Err(_) => {
				// New destination — positions shift, full rebuild required.
				self.leaves.push((dest, mmr_root));
				self.leaves.sort_by_key(|(id, _)| *id);
				self.levels = Self::build_levels(&self.leaves);
			},
		}
	}

	/// Remove a destination. **O(D)** (full rebuild).
	pub fn remove(&mut self, dest: ParaId) -> Result<(), SpeculativeMessagingError> {
		let idx = self
			.leaves
			.binary_search_by_key(&dest, |(id, _)| *id)
			.map_err(|_| SpeculativeMessagingError::DestinationNotFound)?;

		self.leaves.remove(idx);
		self.levels = Self::build_levels(&self.leaves);
		Ok(())
	}

	/// Generate a Merkle proof by reading stored nodes. **O(log D)**.
	pub fn generate_proof(
		&self,
		target: ParaId,
	) -> Result<(H256, MerkleProof), SpeculativeMessagingError> {
		let leaf_index = self
			.leaves
			.binary_search_by_key(&target, |(id, _)| *id)
			.map_err(|_| SpeculativeMessagingError::DestinationNotFound)? as u32;

		let leaf_count = self.leaves.len() as u32;
		let mut siblings = Vec::new();
		let mut idx = leaf_index as usize;

		for level_idx in 0..self.levels.len().saturating_sub(1) {
			let level = &self.levels[level_idx];
			let sibling_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };

			if sibling_idx < level.len() {
				siblings.push(level[sibling_idx]);
			} else {
				// Odd promotion: paired with itself.
				siblings.push(level[idx]);
			}

			idx /= 2;
		}

		Ok((self.root(), MerkleProof { leaf_index, leaf_count, siblings }))
	}

	/// Look up a specific destination's MMR root.
	pub fn get_destination_root(&self, dest: ParaId) -> Option<H256> {
		self.leaves
			.binary_search_by_key(&dest, |(id, _)| *id)
			.ok()
			.map(|idx| self.leaves[idx].1)
	}

	/// Sorted slice of all `(ParaId, MMR root)` leaves.
	pub fn destinations(&self) -> &[(ParaId, H256)] {
		&self.leaves
	}

	/// Produce a [`ProvidesCommitment`] from the current root.
	pub fn provides_commitment(&self) -> crate::commitments::ProvidesCommitment {
		crate::commitments::ProvidesCommitment { root: self.root() }
	}

	// ------------------------------------------------------------------
	// Internal helpers
	// ------------------------------------------------------------------

	/// Build all levels bottom-up from sorted leaves.
	fn build_levels(leaves: &[(ParaId, H256)]) -> Vec<Vec<H256>> {
		if leaves.is_empty() {
			return Vec::new();
		}

		let mut levels = Vec::new();
		let leaf_hashes: Vec<H256> =
			leaves.iter().map(|(id, root)| hash_leaf(*id, *root)).collect();
		levels.push(leaf_hashes);

		while levels.last().map_or(true, |l| l.len() > 1) {
			let prev = levels.last().unwrap();
			let mut next = Vec::with_capacity(prev.len().div_ceil(2));
			let mut i = 0;
			while i < prev.len() {
				if i + 1 < prev.len() {
					next.push(hash_pair(prev[i], prev[i + 1]));
					i += 2;
				} else {
					next.push(hash_pair(prev[i], prev[i]));
					i += 1;
				}
			}
			levels.push(next);
		}

		levels
	}

	/// Rehash from `levels[0][leaf_idx]` up to the root.
	fn rehash_path(&mut self, leaf_idx: usize) {
		let mut idx = leaf_idx;

		for level_idx in 0..self.levels.len().saturating_sub(1) {
			let pair_start = idx & !1; // round down to even
			let left = self.levels[level_idx][pair_start];
			let right = if pair_start + 1 < self.levels[level_idx].len() {
				self.levels[level_idx][pair_start + 1]
			} else {
				left // odd promotion
			};

			let parent_idx = pair_start / 2;
			self.levels[level_idx + 1][parent_idx] = hash_pair(left, right);
			idx = parent_idx;
		}
	}
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

	// ---------------------------------------------------------------
	// StoredMerkleTree — consistency tests
	// ---------------------------------------------------------------

	#[test]
	fn stored_tree_empty() {
		let tree = StoredMerkleTree::from_destinations(&[]);
		assert_eq!(tree.root(), H256::zero());
		assert_eq!(tree.len(), 0);
		assert!(tree.is_empty());
	}

	#[test]
	fn stored_tree_root_matches_stateless() {
		for count in [1, 2, 3, 5, 8, 16, 100] {
			let dests: Vec<(ParaId, H256)> =
				(1..=count).map(|i| (para(i), dummy_root(i as u8))).collect();

			let stored = StoredMerkleTree::from_destinations(&dests);
			let stateless = DestinationMerkleTree::compute_root(&dests);

			assert_eq!(stored.root(), stateless, "root mismatch for {} destinations", count);
			assert_eq!(stored.len(), count as usize);
			assert!(!stored.is_empty());
		}
	}

	#[test]
	fn stored_tree_sorting() {
		let forward =
			[(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB)), (para(3), dummy_root(0xCC))];
		let shuffled =
			[(para(3), dummy_root(0xCC)), (para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB))];

		let t1 = StoredMerkleTree::from_destinations(&forward);
		let t2 = StoredMerkleTree::from_destinations(&shuffled);
		assert_eq!(t1.root(), t2.root());
	}

	// ---------------------------------------------------------------
	// StoredMerkleTree — incremental update tests
	// ---------------------------------------------------------------

	#[test]
	fn stored_tree_update_existing() {
		let dests =
			[(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB)), (para(3), dummy_root(0xCC))];
		let mut tree = StoredMerkleTree::from_destinations(&dests);

		let new_root = dummy_root(0xFF);
		tree.update(para(2), new_root).expect("update should succeed");

		let expected_dests =
			[(para(1), dummy_root(0xAA)), (para(2), new_root), (para(3), dummy_root(0xCC))];
		let fresh = StoredMerkleTree::from_destinations(&expected_dests);
		assert_eq!(tree.root(), fresh.root());
	}

	#[test]
	fn stored_tree_update_nonexistent_fails() {
		let dests = [(para(1), dummy_root(0xAA))];
		let mut tree = StoredMerkleTree::from_destinations(&dests);

		let result = tree.update(para(99), dummy_root(0xFF));
		assert_eq!(result, Err(SpeculativeMessagingError::DestinationNotFound));
	}

	#[test]
	fn stored_tree_update_multiple_sequential() {
		let dests: Vec<(ParaId, H256)> = (1..=10).map(|i| (para(i), dummy_root(i as u8))).collect();
		let mut tree = StoredMerkleTree::from_destinations(&dests);

		// Update destinations 3, 7, 10 sequentially.
		let updates = [(3u32, 0xA0u8), (7, 0xB0), (10, 0xC0)];

		let mut current_dests = dests.clone();
		for (id, seed) in updates {
			let new_val = dummy_root(seed);
			tree.update(para(id), new_val).expect("update should succeed");

			// Apply same change to our reference vector.
			let pos = current_dests.iter().position(|(p, _)| *p == para(id)).unwrap();
			current_dests[pos].1 = new_val;

			let fresh = StoredMerkleTree::from_destinations(&current_dests);
			assert_eq!(tree.root(), fresh.root(), "mismatch after updating para {}", id);
		}
	}

	#[test]
	fn stored_tree_update_same_dest_twice() {
		let dests = [(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB))];
		let mut tree = StoredMerkleTree::from_destinations(&dests);

		tree.update(para(1), dummy_root(0x11)).expect("first update should succeed");
		tree.update(para(1), dummy_root(0x22)).expect("second update should succeed");

		let expected = [(para(1), dummy_root(0x22)), (para(2), dummy_root(0xBB))];
		let fresh = StoredMerkleTree::from_destinations(&expected);
		assert_eq!(tree.root(), fresh.root());
	}

	#[test]
	fn stored_tree_update_all_destinations() {
		let dests: Vec<(ParaId, H256)> = (1..=50).map(|i| (para(i), dummy_root(i as u8))).collect();
		let mut tree = StoredMerkleTree::from_destinations(&dests);

		// Update every destination with a new root.
		let updated: Vec<(ParaId, H256)> =
			(1..=50).map(|i| (para(i), dummy_root((i as u8).wrapping_add(128)))).collect();

		for &(id, new_val) in &updated {
			tree.update(id, new_val).expect("update should succeed");
		}

		let fresh = StoredMerkleTree::from_destinations(&updated);
		assert_eq!(tree.root(), fresh.root());
	}

	// ---------------------------------------------------------------
	// StoredMerkleTree — upsert tests
	// ---------------------------------------------------------------

	#[test]
	fn stored_tree_upsert_existing() {
		let dests = [(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB))];
		let mut tree = StoredMerkleTree::from_destinations(&dests);

		tree.upsert(para(2), dummy_root(0xFF));

		let expected = [(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xFF))];
		let fresh = StoredMerkleTree::from_destinations(&expected);
		assert_eq!(tree.root(), fresh.root());
		assert_eq!(tree.len(), 2);
	}

	#[test]
	fn stored_tree_upsert_new_destination() {
		let dests = [(para(1), dummy_root(0xAA)), (para(3), dummy_root(0xCC))];
		let mut tree = StoredMerkleTree::from_destinations(&dests);

		tree.upsert(para(2), dummy_root(0xBB));

		let expected =
			[(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB)), (para(3), dummy_root(0xCC))];
		let fresh = StoredMerkleTree::from_destinations(&expected);
		assert_eq!(tree.root(), fresh.root());
		assert_eq!(tree.len(), 3);
	}

	#[test]
	fn stored_tree_upsert_multiple_new() {
		let dests = [(para(10), dummy_root(0x10))];
		let mut tree = StoredMerkleTree::from_destinations(&dests);

		let additions = [
			(para(1), dummy_root(0x01)),
			(para(5), dummy_root(0x05)),
			(para(15), dummy_root(0x0F)),
			(para(20), dummy_root(0x14)),
			(para(25), dummy_root(0x19)),
		];

		let mut all_dests = dests.to_vec();
		for (id, root) in additions {
			tree.upsert(id, root);
			all_dests.push((id, root));

			let fresh = StoredMerkleTree::from_destinations(&all_dests);
			assert_eq!(tree.root(), fresh.root(), "mismatch after upserting para {:?}", id);
		}
		assert_eq!(tree.len(), 6);
	}

	// ---------------------------------------------------------------
	// StoredMerkleTree — remove tests
	// ---------------------------------------------------------------

	#[test]
	fn stored_tree_remove_existing() {
		let dests =
			[(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB)), (para(3), dummy_root(0xCC))];
		let mut tree = StoredMerkleTree::from_destinations(&dests);

		tree.remove(para(2)).expect("remove should succeed");

		let remaining = [(para(1), dummy_root(0xAA)), (para(3), dummy_root(0xCC))];
		let fresh = StoredMerkleTree::from_destinations(&remaining);
		assert_eq!(tree.root(), fresh.root());
		assert_eq!(tree.len(), 2);
	}

	#[test]
	fn stored_tree_remove_nonexistent_fails() {
		let dests = [(para(1), dummy_root(0xAA))];
		let mut tree = StoredMerkleTree::from_destinations(&dests);

		let result = tree.remove(para(99));
		assert_eq!(result, Err(SpeculativeMessagingError::DestinationNotFound));
	}

	#[test]
	fn stored_tree_remove_all() {
		let dests: Vec<(ParaId, H256)> = (1..=5).map(|i| (para(i), dummy_root(i as u8))).collect();
		let mut tree = StoredMerkleTree::from_destinations(&dests);

		for i in 1..=5u32 {
			tree.remove(para(i)).expect("remove should succeed");
		}

		assert_eq!(tree.root(), H256::zero());
		assert!(tree.is_empty());
		assert_eq!(tree.len(), 0);
	}

	// ---------------------------------------------------------------
	// StoredMerkleTree — proof tests
	// ---------------------------------------------------------------

	#[test]
	fn stored_tree_proof_matches_stateless() {
		let dests: Vec<(ParaId, H256)> = (1..=10).map(|i| (para(i), dummy_root(i as u8))).collect();
		let tree = StoredMerkleTree::from_destinations(&dests);

		for &(id, mmr) in &dests {
			let (root, proof) =
				tree.generate_proof(id).expect("stored proof generation should succeed");

			// Verify using the stateless verifier.
			DestinationMerkleTree::verify_proof(root, id, mmr, &proof)
				.expect("stateless verification should succeed");

			// Also cross-check root matches.
			assert_eq!(root, tree.root());
		}
	}

	#[test]
	fn stored_tree_proof_after_update() {
		let dests = [
			(para(1), dummy_root(0xAA)),
			(para(2), dummy_root(0xBB)),
			(para(3), dummy_root(0xCC)),
			(para(4), dummy_root(0xDD)),
		];
		let mut tree = StoredMerkleTree::from_destinations(&dests);

		let new_root = dummy_root(0xFF);
		tree.update(para(2), new_root).expect("update should succeed");

		// Proof for the updated destination.
		let (root, proof) =
			tree.generate_proof(para(2)).expect("proof for updated dest should succeed");
		DestinationMerkleTree::verify_proof(root, para(2), new_root, &proof)
			.expect("verification of updated dest should succeed");

		// Proof for an unchanged destination.
		let (root2, proof2) =
			tree.generate_proof(para(4)).expect("proof for unchanged dest should succeed");
		DestinationMerkleTree::verify_proof(root2, para(4), dummy_root(0xDD), &proof2)
			.expect("verification of unchanged dest should succeed");

		assert_eq!(root, root2);
	}

	#[test]
	fn stored_tree_proof_all_destinations_100() {
		let dests: Vec<(ParaId, H256)> =
			(1..=100).map(|i| (para(i), dummy_root(i as u8))).collect();
		let tree = StoredMerkleTree::from_destinations(&dests);
		let expected_root = tree.root();

		for &(id, mmr) in &dests {
			let (root, proof) = tree.generate_proof(id).expect("proof should succeed");

			assert_eq!(root, expected_root);
			assert_eq!(proof.leaf_count, 100);

			DestinationMerkleTree::verify_proof(root, id, mmr, &proof)
				.expect("verification should succeed");
		}
	}

	// ---------------------------------------------------------------
	// StoredMerkleTree — state integration tests
	// ---------------------------------------------------------------

	#[test]
	fn stored_tree_provides_commitment() {
		use crate::commitments::ProvidesCommitment;

		let dests = [(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB))];
		let tree = StoredMerkleTree::from_destinations(&dests);

		let commitment = tree.provides_commitment();
		assert_eq!(commitment, ProvidesCommitment { root: tree.root() });
	}

	#[test]
	fn stored_tree_get_destination_root() {
		let dests =
			[(para(1), dummy_root(0xAA)), (para(2), dummy_root(0xBB)), (para(3), dummy_root(0xCC))];
		let tree = StoredMerkleTree::from_destinations(&dests);

		assert_eq!(tree.get_destination_root(para(1)), Some(dummy_root(0xAA)));
		assert_eq!(tree.get_destination_root(para(2)), Some(dummy_root(0xBB)));
		assert_eq!(tree.get_destination_root(para(3)), Some(dummy_root(0xCC)));
		assert_eq!(tree.get_destination_root(para(99)), None);
	}

	#[test]
	fn stored_tree_destinations_sorted() {
		let dests = [
			(para(5), dummy_root(0x55)),
			(para(1), dummy_root(0x11)),
			(para(3), dummy_root(0x33)),
			(para(10), dummy_root(0xAA)),
			(para(2), dummy_root(0x22)),
		];
		let tree = StoredMerkleTree::from_destinations(&dests);

		let sorted = tree.destinations();
		for window in sorted.windows(2) {
			assert!(
				window[0].0 < window[1].0,
				"destinations not sorted: {:?} >= {:?}",
				window[0].0,
				window[1].0
			);
		}

		// Also verify after upsert which triggers rebuild.
		let mut tree2 = tree.clone();
		tree2.upsert(para(4), dummy_root(0x44));
		let sorted2 = tree2.destinations();
		for window in sorted2.windows(2) {
			assert!(
				window[0].0 < window[1].0,
				"destinations not sorted after upsert: {:?} >= {:?}",
				window[0].0,
				window[1].0
			);
		}
	}

	#[test]
	fn stored_tree_encode_decode_roundtrip() {
		let dests: Vec<(ParaId, H256)> = (1..=10).map(|i| (para(i), dummy_root(i as u8))).collect();
		let tree = StoredMerkleTree::from_destinations(&dests);

		let encoded = tree.encode();
		let decoded = StoredMerkleTree::decode(&mut &encoded[..]).expect("decoding should succeed");

		assert_eq!(tree, decoded);
		assert_eq!(tree.root(), decoded.root());
		assert_eq!(tree.len(), decoded.len());
		assert_eq!(tree.destinations(), decoded.destinations());
	}
}
