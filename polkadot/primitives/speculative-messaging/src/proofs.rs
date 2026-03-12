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

//! Late block proof types for speculative messaging.
//!
//! This module defines proof types that allow a receiving chain to prove that
//! messages it processed under an older `provides` root are still valid under
//! the current `provides` root. The key types are:
//!
//! - [`MmrExtensionProof`]: proves that a newer MMR root extends an older one (the old MMR is a
//!   prefix of the new).
//! - [`LateBlockProof`]: the complete proof a receiver includes in its PoV when its `requires`
//!   references an older root.

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use scale_info::TypeInfo;
use sp_core::H256;

use crate::{
	commitments::RequiresCommitment,
	error::SpeculativeMessagingError,
	merkle_tree::{DestinationMerkleTree, MerkleProof},
};

/// Bags MMR peaks into a single root hash using the standard MMR peak bagging
/// algorithm.
///
/// Folds right-to-left: starts with the last peak, then for each preceding peak
/// computes `blake2_256(peak ++ acc)`.
fn bag_peaks(peaks: &[H256]) -> H256 {
	if peaks.is_empty() {
		return H256::zero();
	}
	if peaks.len() == 1 {
		return peaks[0];
	}
	let mut iter = peaks.iter().rev();
	// Safe because we checked len >= 2 above; qed
	let mut acc = *iter.next().expect("peaks has at least 2 elements; qed");
	for peak in iter {
		let mut combined = [0u8; 64];
		combined[..32].copy_from_slice(peak.as_bytes());
		combined[32..].copy_from_slice(acc.as_bytes());
		acc = H256::from(sp_core::hashing::blake2_256(&combined));
	}
	acc
}

/// Proves that a newer MMR root extends an older one (the old MMR is a prefix
/// of the new).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MmrExtensionProof {
	/// Peaks of the old MMR.
	pub old_peaks: Vec<H256>,
	/// Peaks of the new MMR.
	pub new_peaks: Vec<H256>,
	/// Nodes proving old peaks are a prefix of the new structure.
	pub connecting_nodes: Vec<H256>,
}

impl MmrExtensionProof {
	/// Verifies that the new MMR root extends the old MMR root.
	///
	/// 1. Computes `old_root` from `old_peaks` via [`bag_peaks`].
	/// 2. Computes `new_root` from `new_peaks` via [`bag_peaks`].
	/// 3. Checks computed roots match the expected values.
	/// 4. Verifies the prefix relationship using `connecting_nodes`.
	pub fn verify(&self, old_root: H256, new_root: H256) -> Result<(), SpeculativeMessagingError> {
		let computed_old = bag_peaks(&self.old_peaks);
		let computed_new = bag_peaks(&self.new_peaks);

		if computed_old != old_root {
			return Err(SpeculativeMessagingError::RootMismatch);
		}
		if computed_new != new_root {
			return Err(SpeculativeMessagingError::RootMismatch);
		}

		// Verify that the old peaks are a prefix of the new structure using
		// the connecting nodes. Each old peak must appear in the new MMR or
		// be recoverable from it via the connecting nodes.
		let mut conn_idx = 0;
		for old_peak in &self.old_peaks {
			if self.new_peaks.contains(old_peak) {
				continue;
			}
			// The old peak was merged in the new MMR. Walk through the
			// connecting nodes to verify the merge path leads to a new peak.
			let mut current = *old_peak;
			let mut found = false;
			while conn_idx < self.connecting_nodes.len() {
				let sibling = self.connecting_nodes[conn_idx];
				conn_idx += 1;
				let mut combined = [0u8; 64];
				combined[..32].copy_from_slice(current.as_bytes());
				combined[32..].copy_from_slice(sibling.as_bytes());
				current = H256::from(sp_core::hashing::blake2_256(&combined));
				if self.new_peaks.contains(&current) {
					found = true;
					break;
				}
			}
			if !found {
				return Err(SpeculativeMessagingError::InvalidMmrExtensionProof);
			}
		}

		Ok(())
	}
}

/// The complete proof a receiver includes in its PoV when its `requires`
/// references an older root.
///
/// This proof demonstrates that the receiver's subtree root in the old provides
/// tree is consistent with the current provides tree, allowing the relay chain
/// to accept the receiver's state transition.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LateBlockProof {
	/// Source chain this proof is for.
	pub source: ParaId,
	/// Receiver's subtree root in the old provides.
	pub old_subtree_root: H256,
	/// Proof that `old_subtree_root` was in the old provides root.
	pub old_subtree_proof: MerkleProof,
	/// The current provides root we are updating to.
	pub new_provides_root: H256,
	/// Receiver's subtree root in the new provides.
	pub new_subtree_root: H256,
	/// Proof that `new_subtree_root` is in the new provides root.
	pub new_subtree_proof: MerkleProof,
	/// If the subtree's MMR grew, proves the correct extension.
	pub subtree_extension: Option<MmrExtensionProof>,
}

impl LateBlockProof {
	/// Verifies this late block proof against the old provides root.
	///
	/// # Arguments
	///
	/// * `old_provides_root` - The old provides root that the receiver built its state transition
	///   against.
	/// * `receiver_para_id` - The receiver's `ParaId` (used as the leaf key in the sender's
	///   destination Merkle tree).
	///
	/// # Verification steps
	///
	/// 1. Verify that `(receiver_para_id, old_subtree_root)` is in `old_provides_root`.
	/// 2. Verify that `(receiver_para_id, new_subtree_root)` is in `new_provides_root`.
	/// 3. If the subtree root changed, require and verify the extension proof.
	/// 4. If the subtree root did not change but an extension proof is present, return an error.
	/// 5. Return a [`RequiresCommitment`] with the updated provides root.
	pub fn verify(
		&self,
		old_provides_root: H256,
		receiver_para_id: ParaId,
	) -> Result<RequiresCommitment, SpeculativeMessagingError> {
		// Step 1: Verify old subtree proof.
		DestinationMerkleTree::verify_proof(
			old_provides_root,
			receiver_para_id,
			self.old_subtree_root,
			&self.old_subtree_proof,
		)?;

		// Step 2: Verify new subtree proof.
		DestinationMerkleTree::verify_proof(
			self.new_provides_root,
			receiver_para_id,
			self.new_subtree_root,
			&self.new_subtree_proof,
		)?;

		// Step 3 & 4: Handle subtree extension.
		if self.old_subtree_root != self.new_subtree_root {
			let extension = self
				.subtree_extension
				.as_ref()
				.ok_or(SpeculativeMessagingError::MissingSubtreeExtension)?;
			extension.verify(self.old_subtree_root, self.new_subtree_root)?;
		} else if self.subtree_extension.is_some() {
			return Err(SpeculativeMessagingError::InvalidMmrExtensionProof);
		}

		// Step 5: Return updated commitment.
		Ok(RequiresCommitment { source: self.source, expected_root: self.new_provides_root })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::vec;
	use codec::{Decode, Encode};
	use sp_core::hashing::blake2_256;

	fn make_hash(byte: u8) -> H256 {
		H256::from([byte; 32])
	}

	fn hash_pair(a: H256, b: H256) -> H256 {
		let mut combined = [0u8; 64];
		combined[..32].copy_from_slice(a.as_bytes());
		combined[32..].copy_from_slice(b.as_bytes());
		H256::from(blake2_256(&combined))
	}

	/// Helper: build a Merkle tree from a set of `(ParaId, mmr_root)`
	/// pairs and return both the root and a proof for a specific
	/// destination.
	fn build_tree_and_proof(
		destinations: &[(ParaId, H256)],
		prove_for: ParaId,
	) -> (H256, MerkleProof) {
		let (root, proof) = DestinationMerkleTree::generate_proof(
			destinations,
			prove_for,
		)
		.expect("destination should exist in the tree");
		(root, proof)
	}

	// ---- bag_peaks tests ----

	#[test]
	fn bag_peaks_empty() {
		assert_eq!(bag_peaks(&[]), H256::zero());
	}

	#[test]
	fn bag_peaks_single() {
		let peak = make_hash(42);
		assert_eq!(bag_peaks(&[peak]), peak);
	}

	#[test]
	fn bag_peaks_two() {
		let a = make_hash(1);
		let b = make_hash(2);
		// Fold right: start with B, then H(A ++ B)
		let expected = hash_pair(a, b);
		assert_eq!(bag_peaks(&[a, b]), expected);
	}

	#[test]
	fn bag_peaks_three() {
		let a = make_hash(1);
		let b = make_hash(2);
		let c = make_hash(3);
		// Fold right: start with C, then H(B ++ C), then H(A ++ H(B ++ C))
		let bc = hash_pair(b, c);
		let expected = hash_pair(a, bc);
		assert_eq!(bag_peaks(&[a, b, c]), expected);
	}

	// ---- MmrExtensionProof tests ----

	#[test]
	fn mmr_extension_proof_same_root() {
		let peak = make_hash(10);
		let root = bag_peaks(&[peak]);
		let proof = MmrExtensionProof {
			old_peaks: vec![peak],
			new_peaks: vec![peak],
			connecting_nodes: vec![],
		};
		assert_eq!(proof.verify(root, root), Ok(()));
	}

	#[test]
	fn mmr_extension_proof_root_mismatch() {
		let peak = make_hash(10);
		let root = bag_peaks(&[peak]);
		let wrong_root = make_hash(99);
		let proof = MmrExtensionProof {
			old_peaks: vec![peak],
			new_peaks: vec![peak],
			connecting_nodes: vec![],
		};
		assert_eq!(proof.verify(wrong_root, root), Err(SpeculativeMessagingError::RootMismatch));
		assert_eq!(proof.verify(root, wrong_root), Err(SpeculativeMessagingError::RootMismatch));
	}

	// ---- LateBlockProof tests ----

	#[test]
	fn late_block_proof_same_subtree() {
		let source = ParaId::from(1000);
		let receiver = ParaId::from(2000);
		let subtree_root = make_hash(5);

		// Build old and new trees with the same subtree root.
		let (old_root, old_proof) = build_tree_and_proof(&[(receiver, subtree_root)], receiver);
		let (new_root, new_proof) = build_tree_and_proof(&[(receiver, subtree_root)], receiver);

		let proof = LateBlockProof {
			source,
			old_subtree_root: subtree_root,
			old_subtree_proof: old_proof,
			new_provides_root: new_root,
			new_subtree_root: subtree_root,
			new_subtree_proof: new_proof,
			subtree_extension: None,
		};

		let result = proof.verify(old_root, receiver);
		assert!(result.is_ok());
		let commitment = result.unwrap();
		assert_eq!(commitment.source, source);
		assert_eq!(commitment.expected_root, new_root);
	}

	#[test]
	fn late_block_proof_extended_subtree() {
		let source = ParaId::from(1000);
		let receiver = ParaId::from(2000);
		let old_subtree = make_hash(5);
		// Simulate the subtree growing: the new root is the result of
		// merging the old peak with a new sibling.
		let new_sibling = make_hash(6);
		let new_subtree = hash_pair(old_subtree, new_sibling);

		let (old_root, old_proof) = build_tree_and_proof(&[(receiver, old_subtree)], receiver);
		let (new_root, new_proof) = build_tree_and_proof(&[(receiver, new_subtree)], receiver);

		let extension = MmrExtensionProof {
			old_peaks: vec![old_subtree],
			new_peaks: vec![new_subtree],
			connecting_nodes: vec![new_sibling],
		};

		let proof = LateBlockProof {
			source,
			old_subtree_root: old_subtree,
			old_subtree_proof: old_proof,
			new_provides_root: new_root,
			new_subtree_root: new_subtree,
			new_subtree_proof: new_proof,
			subtree_extension: Some(extension),
		};

		let result = proof.verify(old_root, receiver);
		assert!(result.is_ok());
		let commitment = result.unwrap();
		assert_eq!(commitment.source, source);
		assert_eq!(commitment.expected_root, new_root);
	}

	#[test]
	fn late_block_proof_missing_extension() {
		let source = ParaId::from(1000);
		let receiver = ParaId::from(2000);
		let old_subtree = make_hash(5);
		let new_subtree = make_hash(6);

		let (old_root, old_proof) = build_tree_and_proof(&[(receiver, old_subtree)], receiver);
		let (new_root, new_proof) = build_tree_and_proof(&[(receiver, new_subtree)], receiver);

		let proof = LateBlockProof {
			source,
			old_subtree_root: old_subtree,
			old_subtree_proof: old_proof,
			new_provides_root: new_root,
			new_subtree_root: new_subtree,
			new_subtree_proof: new_proof,
			subtree_extension: None,
		};

		assert_eq!(
			proof.verify(old_root, receiver),
			Err(SpeculativeMessagingError::MissingSubtreeExtension)
		);
	}

	#[test]
	fn late_block_proof_unnecessary_extension() {
		let source = ParaId::from(1000);
		let receiver = ParaId::from(2000);
		let subtree_root = make_hash(5);

		let (old_root, old_proof) = build_tree_and_proof(&[(receiver, subtree_root)], receiver);
		let (new_root, new_proof) = build_tree_and_proof(&[(receiver, subtree_root)], receiver);

		// Subtree didn't change but we provide an extension anyway.
		let extension = MmrExtensionProof {
			old_peaks: vec![subtree_root],
			new_peaks: vec![subtree_root],
			connecting_nodes: vec![],
		};

		let proof = LateBlockProof {
			source,
			old_subtree_root: subtree_root,
			old_subtree_proof: old_proof,
			new_provides_root: new_root,
			new_subtree_root: subtree_root,
			new_subtree_proof: new_proof,
			subtree_extension: Some(extension),
		};

		assert_eq!(
			proof.verify(old_root, receiver),
			Err(SpeculativeMessagingError::InvalidMmrExtensionProof)
		);
	}

	#[test]
	fn late_block_proof_returns_updated_commitment() {
		let source = ParaId::from(3000);
		let receiver = ParaId::from(4000);
		let subtree_root = make_hash(11);

		let (old_root, old_proof) = build_tree_and_proof(&[(receiver, subtree_root)], receiver);
		let (new_root, new_proof) = build_tree_and_proof(&[(receiver, subtree_root)], receiver);

		let proof = LateBlockProof {
			source,
			old_subtree_root: subtree_root,
			old_subtree_proof: old_proof,
			new_provides_root: new_root,
			new_subtree_root: subtree_root,
			new_subtree_proof: new_proof,
			subtree_extension: None,
		};

		let commitment = proof.verify(old_root, receiver).unwrap();
		assert_eq!(commitment, RequiresCommitment { source, expected_root: new_root });
	}

	#[test]
	fn late_block_proof_invalid_old_merkle_proof() {
		let source = ParaId::from(1000);
		let receiver = ParaId::from(2000);
		let other = ParaId::from(3000);
		let subtree_root = make_hash(5);
		let other_root = make_hash(6);

		// Build old tree with two destinations: receiver + other.
		let old_dests = [(receiver, subtree_root), (other, other_root)];
		let (old_root, _) = build_tree_and_proof(&old_dests, receiver);

		// Build a DIFFERENT tree (different set) and get a proof from it.
		// This proof has siblings that won't match old_root.
		let wrong_dests = [(receiver, subtree_root), (ParaId::from(5555), make_hash(99))];
		let (_, wrong_proof) = build_tree_and_proof(&wrong_dests, receiver);

		// Build valid new tree.
		let new_dests = [(receiver, subtree_root), (other, other_root)];
		let (new_root, new_proof) = build_tree_and_proof(&new_dests, receiver);

		let proof = LateBlockProof {
			source,
			old_subtree_root: subtree_root,
			old_subtree_proof: wrong_proof,
			new_provides_root: new_root,
			new_subtree_root: subtree_root,
			new_subtree_proof: new_proof,
			subtree_extension: None,
		};

		assert!(proof.verify(old_root, receiver).is_err());
	}

	#[test]
	fn late_block_proof_invalid_new_merkle_proof() {
		let source = ParaId::from(1000);
		let receiver = ParaId::from(2000);
		let other = ParaId::from(3000);
		let subtree_root = make_hash(5);
		let other_root = make_hash(6);

		// Build valid old tree.
		let old_dests = [(receiver, subtree_root), (other, other_root)];
		let (old_root, old_proof) = build_tree_and_proof(&old_dests, receiver);

		// Build valid new tree.
		let new_dests = [(receiver, subtree_root), (other, other_root)];
		let (new_root, _) = build_tree_and_proof(&new_dests, receiver);

		// Build a DIFFERENT tree and get a proof from it for receiver.
		let wrong_dests = [(receiver, subtree_root), (ParaId::from(7777), make_hash(88))];
		let (_, wrong_proof) = build_tree_and_proof(&wrong_dests, receiver);

		let proof = LateBlockProof {
			source,
			old_subtree_root: subtree_root,
			old_subtree_proof: old_proof,
			new_provides_root: new_root,
			new_subtree_root: subtree_root,
			new_subtree_proof: wrong_proof,
			subtree_extension: None,
		};

		assert!(proof.verify(old_root, receiver).is_err());
	}

	#[test]
	fn encode_decode_roundtrip() {
		// MmrExtensionProof
		let ext_proof = MmrExtensionProof {
			old_peaks: vec![make_hash(1), make_hash(2)],
			new_peaks: vec![make_hash(3)],
			connecting_nodes: vec![make_hash(4)],
		};
		let encoded = ext_proof.encode();
		let decoded =
			MmrExtensionProof::decode(&mut &encoded[..]).expect("MmrExtensionProof should decode");
		assert_eq!(ext_proof, decoded);

		// LateBlockProof
		let receiver = ParaId::from(2000);
		let subtree_root = make_hash(5);
		let (_old_root, old_proof) = build_tree_and_proof(&[(receiver, subtree_root)], receiver);
		let (new_root, new_proof) = build_tree_and_proof(&[(receiver, subtree_root)], receiver);

		let late_proof = LateBlockProof {
			source: ParaId::from(1000),
			old_subtree_root: subtree_root,
			old_subtree_proof: old_proof,
			new_provides_root: new_root,
			new_subtree_root: subtree_root,
			new_subtree_proof: new_proof,
			subtree_extension: Some(MmrExtensionProof {
				old_peaks: vec![subtree_root],
				new_peaks: vec![subtree_root],
				connecting_nodes: vec![],
			}),
		};
		let encoded = late_proof.encode();
		let decoded =
			LateBlockProof::decode(&mut &encoded[..]).expect("LateBlockProof should decode");
		assert_eq!(late_proof, decoded);
	}
}
