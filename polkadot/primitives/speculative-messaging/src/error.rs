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

//! Error types for speculative messaging primitives.

use codec::{Decode, DecodeWithMemTracking, Encode};
use core::fmt;
use scale_info::TypeInfo;

/// Errors that can occur during speculative messaging operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub enum SpeculativeMessagingError {
	/// Top-level Merkle proof verification failed.
	InvalidMerkleProof,
	/// MMR extension proof is invalid.
	InvalidMmrExtensionProof,
	/// Computed root doesn't match expected root.
	RootMismatch,
	/// Subtree root doesn't match.
	SubtreeRootMismatch,
	/// Subtree changed but no extension proof was provided.
	MissingSubtreeExtension,
	/// Operation on an empty tree.
	EmptyTree,
	/// Destination ParaId not found in Merkle tree.
	DestinationNotFound,
	/// Message position is out of bounds or non-sequential.
	InvalidMessagePosition,
	/// Message batch contains no messages.
	EmptyBatch,
	/// Duplicate ParaId found in destination set.
	DuplicateDestination,
	/// Proof contains unconsumed data.
	UnconsumedProofData,
	/// Source ParaId doesn't match expected source.
	SourceMismatch,
	/// Too many destinations to fit in a u32-indexed Merkle tree.
	TooManyDestinations,
}

#[cfg(feature = "std")]
impl std::error::Error for SpeculativeMessagingError {}

impl fmt::Display for SpeculativeMessagingError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::InvalidMerkleProof => write!(f, "Top-level Merkle proof verification failed"),
			Self::InvalidMmrExtensionProof => write!(f, "MMR extension proof is invalid"),
			Self::RootMismatch => write!(f, "Computed root doesn't match expected root"),
			Self::SubtreeRootMismatch => write!(f, "Subtree root doesn't match"),
			Self::MissingSubtreeExtension => {
				write!(f, "Subtree changed but no extension proof was provided")
			},
			Self::EmptyTree => write!(f, "Operation on an empty tree"),
			Self::DestinationNotFound => write!(f, "Destination ParaId not found in Merkle tree"),
			Self::InvalidMessagePosition => {
				write!(f, "Message position is out of bounds or non-sequential")
			},
			Self::EmptyBatch => write!(f, "Message batch contains no messages"),
			Self::DuplicateDestination => {
				write!(f, "Duplicate ParaId found in destination set")
			},
			Self::UnconsumedProofData => write!(f, "Proof contains unconsumed data"),
			Self::SourceMismatch => {
				write!(f, "Source ParaId doesn't match expected source")
			},
			Self::TooManyDestinations => {
				write!(f, "Too many destinations to fit in a u32-indexed Merkle tree")
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::{format, string::String};

	#[test]
	fn display_invalid_merkle_proof() {
		let err = SpeculativeMessagingError::InvalidMerkleProof;
		assert_eq!(format!("{}", err), "Top-level Merkle proof verification failed");
	}

	#[test]
	fn display_invalid_mmr_extension_proof() {
		let err = SpeculativeMessagingError::InvalidMmrExtensionProof;
		assert_eq!(format!("{}", err), "MMR extension proof is invalid");
	}

	#[test]
	fn display_root_mismatch() {
		let err = SpeculativeMessagingError::RootMismatch;
		assert_eq!(format!("{}", err), "Computed root doesn't match expected root");
	}

	#[test]
	fn display_subtree_root_mismatch() {
		let err = SpeculativeMessagingError::SubtreeRootMismatch;
		assert_eq!(format!("{}", err), "Subtree root doesn't match");
	}

	#[test]
	fn display_missing_subtree_extension() {
		let err = SpeculativeMessagingError::MissingSubtreeExtension;
		assert_eq!(format!("{}", err), "Subtree changed but no extension proof was provided");
	}

	#[test]
	fn display_empty_tree() {
		let err = SpeculativeMessagingError::EmptyTree;
		assert_eq!(format!("{}", err), "Operation on an empty tree");
	}

	#[test]
	fn display_destination_not_found() {
		let err = SpeculativeMessagingError::DestinationNotFound;
		assert_eq!(format!("{}", err), "Destination ParaId not found in Merkle tree");
	}

	#[test]
	fn display_invalid_message_position() {
		let err = SpeculativeMessagingError::InvalidMessagePosition;
		assert_eq!(format!("{}", err), "Message position is out of bounds or non-sequential");
	}

	#[test]
	fn display_empty_batch() {
		let err = SpeculativeMessagingError::EmptyBatch;
		assert_eq!(format!("{}", err), "Message batch contains no messages");
	}

	#[test]
	fn encode_decode_roundtrip() {
		let variants = [
			SpeculativeMessagingError::InvalidMerkleProof,
			SpeculativeMessagingError::InvalidMmrExtensionProof,
			SpeculativeMessagingError::RootMismatch,
			SpeculativeMessagingError::SubtreeRootMismatch,
			SpeculativeMessagingError::MissingSubtreeExtension,
			SpeculativeMessagingError::EmptyTree,
			SpeculativeMessagingError::DestinationNotFound,
			SpeculativeMessagingError::InvalidMessagePosition,
			SpeculativeMessagingError::EmptyBatch,
			SpeculativeMessagingError::DuplicateDestination,
			SpeculativeMessagingError::UnconsumedProofData,
			SpeculativeMessagingError::SourceMismatch,
			SpeculativeMessagingError::TooManyDestinations,
		];

		for variant in &variants {
			let encoded = variant.encode();
			let decoded = SpeculativeMessagingError::decode(&mut &encoded[..])
				.expect("decoding should succeed");
			assert_eq!(*variant, decoded);
		}
	}

	#[test]
	fn encoded_variant_indices_are_sequential() {
		let variants = [
			SpeculativeMessagingError::InvalidMerkleProof,
			SpeculativeMessagingError::InvalidMmrExtensionProof,
			SpeculativeMessagingError::RootMismatch,
			SpeculativeMessagingError::SubtreeRootMismatch,
			SpeculativeMessagingError::MissingSubtreeExtension,
			SpeculativeMessagingError::EmptyTree,
			SpeculativeMessagingError::DestinationNotFound,
			SpeculativeMessagingError::InvalidMessagePosition,
			SpeculativeMessagingError::EmptyBatch,
			SpeculativeMessagingError::DuplicateDestination,
			SpeculativeMessagingError::UnconsumedProofData,
			SpeculativeMessagingError::SourceMismatch,
			SpeculativeMessagingError::TooManyDestinations,
		];

		for (i, variant) in variants.iter().enumerate() {
			let encoded = variant.encode();
			assert_eq!(encoded[0] as usize, i, "variant {:?} should have index {}", variant, i);
		}
	}

	#[test]
	fn clone_and_eq() {
		let err = SpeculativeMessagingError::RootMismatch;
		let cloned = err.clone();
		assert_eq!(err, cloned);
	}

	#[test]
	fn debug_output_contains_variant_name() {
		let err = SpeculativeMessagingError::InvalidMerkleProof;
		let debug: String = format!("{:?}", err);
		assert!(debug.contains("InvalidMerkleProof"));
	}
}
