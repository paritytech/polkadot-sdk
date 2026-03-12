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

//! Off-chain message types exchanged between collators.
//!
//! [`OutgoingMessage`] represents a single message stored as a leaf in a
//! sender's per-destination MMR. [`MessageBatch`] groups several such
//! messages together with the Merkle proof needed to verify that the
//! per-destination subtree root is part of the sender's top-level
//! provides commitment.

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use scale_info::TypeInfo;
use sp_core::H256;

use crate::{
	error::SpeculativeMessagingError,
	merkle_tree::{DestinationMerkleTree, MerkleProof},
};

/// A single message in a sender's per-destination MMR.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct OutgoingMessage {
	/// Destination parachain.
	pub destination: ParaId,
	/// Message payload (XCM or other data).
	pub payload: Vec<u8>,
	/// Position in sender's per-destination MMR for this destination.
	pub position: u64,
}

impl OutgoingMessage {
	/// Compute the leaf hash for this message.
	///
	/// Returns `blake2_256(self.encode())` as an `H256`. This is the value
	/// stored as a leaf in the per-destination MMR.
	pub fn leaf_hash(&self) -> H256 {
		H256::from(sp_core::hashing::blake2_256(&self.encode()))
	}
}

/// A batch of messages from one source chain to one destination, shared
/// off-chain between collators.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct MessageBatch {
	/// Source parachain.
	pub source: ParaId,
	/// Hash of the source block that produced these messages.
	pub source_block: H256,
	/// The provides root for the source block.
	pub provides_root: H256,
	/// Per-destination MMR root for the receiver.
	pub subtree_root: H256,
	/// Proof that `subtree_root` is included in `provides_root`.
	pub subtree_inclusion_proof: MerkleProof,
	/// The actual messages, ordered by position.
	pub messages: Vec<OutgoingMessage>,
}

impl MessageBatch {
	/// Returns `true` if the batch contains no messages.
	pub fn is_empty(&self) -> bool {
		self.messages.is_empty()
	}

	/// Returns the number of messages in this batch.
	pub fn message_count(&self) -> usize {
		self.messages.len()
	}

	/// Verify that [`Self::subtree_root`] is included in
	/// [`Self::provides_root`] for the given `receiver` parachain.
	pub fn verify_subtree_inclusion(
		&self,
		receiver: ParaId,
	) -> Result<(), SpeculativeMessagingError> {
		DestinationMerkleTree::verify_proof(
			self.provides_root,
			receiver,
			self.subtree_root,
			&self.subtree_inclusion_proof,
		)
	}

	/// Verify that message positions are sequential starting from
	/// `expected_start`.
	///
	/// An empty batch is always considered valid.
	pub fn verify_sequential(&self, expected_start: u64) -> Result<(), SpeculativeMessagingError> {
		for (i, msg) in self.messages.iter().enumerate() {
			let expected_pos = expected_start
				.checked_add(i as u64)
				.ok_or(SpeculativeMessagingError::InvalidMessagePosition)?;
			if msg.position != expected_pos {
				return Err(SpeculativeMessagingError::InvalidMessagePosition);
			}
		}
		Ok(())
	}

	/// Returns `(first_position, last_position)` or `None` if the batch
	/// is empty.
	///
	/// **Note:** This returns positions from the first and last messages
	/// in array order. Call [`Self::verify_sequential`] first to ensure
	/// the messages are properly ordered.
	pub fn positions_range(&self) -> Option<(u64, u64)> {
		let first = self.messages.first()?.position;
		let last = self.messages.last()?.position;
		Some((first, last))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::vec;
	use codec::{Decode, Encode};

	fn para(id: u32) -> ParaId {
		ParaId::from(id)
	}

	fn make_message(destination: ParaId, payload: &[u8], position: u64) -> OutgoingMessage {
		OutgoingMessage { destination, payload: payload.to_vec(), position }
	}

	fn dummy_batch(messages: Vec<OutgoingMessage>) -> MessageBatch {
		MessageBatch {
			source: para(100),
			source_block: H256::from([0xAA; 32]),
			provides_root: H256::from([0xBB; 32]),
			subtree_root: H256::from([0xCC; 32]),
			subtree_inclusion_proof: MerkleProof { leaf_index: 0, leaf_count: 1, siblings: vec![] },
			messages,
		}
	}

	// ---------------------------------------------------------------
	// OutgoingMessage::leaf_hash
	// ---------------------------------------------------------------

	#[test]
	fn outgoing_message_leaf_hash_deterministic() {
		let msg = make_message(para(1), b"hello", 0);
		let h1 = msg.leaf_hash();
		let h2 = msg.leaf_hash();
		assert_eq!(h1, h2);
	}

	#[test]
	fn outgoing_message_leaf_hash_differs_by_payload() {
		let a = make_message(para(1), b"alpha", 0);
		let b = make_message(para(1), b"beta", 0);
		assert_ne!(a.leaf_hash(), b.leaf_hash());
	}

	#[test]
	fn outgoing_message_leaf_hash_differs_by_position() {
		let a = make_message(para(1), b"same", 0);
		let b = make_message(para(1), b"same", 1);
		assert_ne!(a.leaf_hash(), b.leaf_hash());
	}

	#[test]
	fn outgoing_message_leaf_hash_differs_by_destination() {
		let a = make_message(para(1), b"same", 0);
		let b = make_message(para(2), b"same", 0);
		assert_ne!(a.leaf_hash(), b.leaf_hash());
	}

	// ---------------------------------------------------------------
	// MessageBatch helpers
	// ---------------------------------------------------------------

	#[test]
	fn message_batch_is_empty() {
		let batch = dummy_batch(vec![]);
		assert!(batch.is_empty());

		let batch = dummy_batch(vec![make_message(para(1), b"x", 0)]);
		assert!(!batch.is_empty());
	}

	#[test]
	fn message_batch_message_count() {
		let batch = dummy_batch(vec![]);
		assert_eq!(batch.message_count(), 0);

		let batch = dummy_batch(vec![
			make_message(para(1), b"a", 0),
			make_message(para(1), b"b", 1),
			make_message(para(1), b"c", 2),
		]);
		assert_eq!(batch.message_count(), 3);
	}

	// ---------------------------------------------------------------
	// verify_sequential
	// ---------------------------------------------------------------

	#[test]
	fn message_batch_verify_sequential_valid() {
		let batch = dummy_batch(vec![
			make_message(para(1), b"a", 5),
			make_message(para(1), b"b", 6),
			make_message(para(1), b"c", 7),
		]);
		assert!(batch.verify_sequential(5).is_ok());
	}

	#[test]
	fn message_batch_verify_sequential_gap() {
		let batch =
			dummy_batch(vec![make_message(para(1), b"a", 5), make_message(para(1), b"b", 7)]);
		assert_eq!(
			batch.verify_sequential(5),
			Err(SpeculativeMessagingError::InvalidMessagePosition),
		);
	}

	#[test]
	fn message_batch_verify_sequential_wrong_start() {
		let batch =
			dummy_batch(vec![make_message(para(1), b"a", 6), make_message(para(1), b"b", 7)]);
		assert_eq!(
			batch.verify_sequential(5),
			Err(SpeculativeMessagingError::InvalidMessagePosition),
		);
	}

	#[test]
	fn message_batch_verify_sequential_empty() {
		let batch = dummy_batch(vec![]);
		assert!(batch.verify_sequential(0).is_ok());
		assert!(batch.verify_sequential(42).is_ok());
		assert!(batch.verify_sequential(u64::MAX).is_ok());
	}

	// ---------------------------------------------------------------
	// positions_range
	// ---------------------------------------------------------------

	#[test]
	fn message_batch_positions_range() {
		// Empty batch.
		let batch = dummy_batch(vec![]);
		assert_eq!(batch.positions_range(), None);

		// Single message.
		let batch = dummy_batch(vec![make_message(para(1), b"x", 10)]);
		assert_eq!(batch.positions_range(), Some((10, 10)));

		// Multiple messages.
		let batch = dummy_batch(vec![
			make_message(para(1), b"a", 3),
			make_message(para(1), b"b", 4),
			make_message(para(1), b"c", 5),
		]);
		assert_eq!(batch.positions_range(), Some((3, 5)));
	}

	// ---------------------------------------------------------------
	// verify_subtree_inclusion (with real Merkle proof)
	// ---------------------------------------------------------------

	#[test]
	fn message_batch_verify_subtree_inclusion() {
		let receiver = para(200);
		let subtree_root = H256::from([0xDD; 32]);

		// Build a real destination Merkle tree with several entries.
		let destinations = [
			(para(100), H256::from([0xAA; 32])),
			(receiver, subtree_root),
			(para(300), H256::from([0xCC; 32])),
		];

		let (root, proof) = DestinationMerkleTree::generate_proof(&destinations, receiver)
			.expect("proof generation should succeed");

		let batch = MessageBatch {
			source: para(1),
			source_block: H256::from([0x11; 32]),
			provides_root: root,
			subtree_root,
			subtree_inclusion_proof: proof,
			messages: vec![make_message(receiver, b"hello", 0)],
		};

		assert!(batch.verify_subtree_inclusion(receiver).is_ok());

		// Verifying with the wrong receiver should fail.
		let wrong_receiver = para(999);
		assert!(batch.verify_subtree_inclusion(wrong_receiver).is_err());
	}

	// ---------------------------------------------------------------
	// Encode / decode round-trip
	// ---------------------------------------------------------------

	#[test]
	fn encode_decode_roundtrip() {
		// OutgoingMessage round-trip.
		let msg = make_message(para(42), b"payload", 7);
		let encoded = msg.encode();
		let decoded =
			OutgoingMessage::decode(&mut &encoded[..]).expect("OutgoingMessage should decode");
		assert_eq!(msg, decoded);

		// MessageBatch round-trip.
		let batch =
			dummy_batch(vec![make_message(para(1), b"a", 0), make_message(para(1), b"b", 1)]);
		let encoded = batch.encode();
		let decoded = MessageBatch::decode(&mut &encoded[..]).expect("MessageBatch should decode");
		assert_eq!(batch, decoded);
	}
}
