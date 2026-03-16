// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <http://www.gnu.org/licenses/>.

//! Request/response protocol types for speculative message forwarding.
//!
//! These types define the wire format for the `/polkadot/spec-msg/1`
//! request-response protocol used to forward [`MessageBatch`]es between
//! relay chain peers.

use codec::{Decode, Encode};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use polkadot_primitives_speculative_messaging::MessageBatch;

/// Protocol name for speculative message forwarding.
pub const PROTOCOL_NAME: &str = "/polkadot/spec-msg/1";

/// Maximum request size in bytes. Message batches can contain many
/// messages, so we allow up to 16 MiB.
pub const MAX_REQUEST_SIZE: u64 = 16 * 1024 * 1024;

/// Maximum response size in bytes. Responses are lightweight status
/// messages.
pub const MAX_RESPONSE_SIZE: u64 = 1024;

/// Request to forward a speculative message batch through the relay
/// chain peer network.
///
/// The source collator sends this to its relay chain peer, which
/// forwards it to the destination parachain's relay peer.
#[derive(Debug, Clone, Encode, Decode)]
pub struct ForwardMessageRequest {
	/// Source parachain that produced the messages.
	pub source_para: ParaId,
	/// Destination parachain that should receive the messages.
	pub destination_para: ParaId,
	/// The message batch with Merkle proofs.
	pub batch: MessageBatch,
}

/// Response to a [`ForwardMessageRequest`].
#[derive(Debug, Clone, Encode, Decode)]
pub enum ForwardMessageResponse {
	/// Batch accepted by the final destination collator.
	Accepted,
	/// Batch forwarded to the next relay peer hop.
	Forwarded,
	/// Batch rejected.
	Rejected {
		/// Human-readable reason for the rejection.
		reason: Vec<u8>,
	},
}

impl ForwardMessageResponse {
	/// Create a rejected response from a string.
	pub fn rejected(reason: &str) -> Self {
		Self::Rejected { reason: reason.as_bytes().to_vec() }
	}

	/// Returns `true` if the response indicates success.
	pub fn is_ok(&self) -> bool {
		matches!(self, Self::Accepted | Self::Forwarded)
	}

	/// Returns the rejection reason, if any.
	pub fn rejection_reason(&self) -> Option<String> {
		match self {
			Self::Rejected { reason } => Some(String::from_utf8_lossy(reason).into_owned()),
			_ => None,
		}
	}
}

/// The role this node plays in the speculative messaging network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRole {
	/// A collator that produces and consumes messages.
	Collator,
	/// A relay chain peer that forwards messages between collators.
	RelayPeer,
}

#[cfg(test)]
mod tests {
	use super::*;
	use polkadot_primitives_speculative_messaging::{MerkleProof, OutgoingMessage};
	use sp_core::H256;

	fn make_request() -> ForwardMessageRequest {
		ForwardMessageRequest {
			source_para: ParaId::from(100),
			destination_para: ParaId::from(200),
			batch: MessageBatch {
				source: ParaId::from(100),
				source_block: H256::from([0xAA; 32]),
				provides_root: H256::from([0xBB; 32]),
				subtree_root: H256::from([0xCC; 32]),
				subtree_inclusion_proof: MerkleProof {
					leaf_index: 0,
					leaf_count: 1,
					siblings: vec![],
				},
				messages: vec![OutgoingMessage {
					destination: ParaId::from(200),
					payload: b"hello".to_vec(),
					position: 0,
				}],
			},
		}
	}

	#[test]
	fn request_encode_decode_roundtrip() {
		let req = make_request();
		let encoded = req.encode();
		let decoded =
			ForwardMessageRequest::decode(&mut &encoded[..]).expect("should decode request");
		assert_eq!(decoded.source_para, ParaId::from(100));
		assert_eq!(decoded.destination_para, ParaId::from(200));
		assert_eq!(decoded.batch.messages.len(), 1);
	}

	#[test]
	fn response_encode_decode_roundtrip() {
		for response in [
			ForwardMessageResponse::Accepted,
			ForwardMessageResponse::Forwarded,
			ForwardMessageResponse::rejected("test reason"),
		] {
			let encoded = response.encode();
			let decoded = ForwardMessageResponse::decode(&mut &encoded[..])
				.expect("should decode response");
			assert_eq!(response.is_ok(), decoded.is_ok());
		}
	}

	#[test]
	fn response_helpers() {
		assert!(ForwardMessageResponse::Accepted.is_ok());
		assert!(ForwardMessageResponse::Forwarded.is_ok());
		assert!(!ForwardMessageResponse::rejected("oops").is_ok());

		assert_eq!(ForwardMessageResponse::Accepted.rejection_reason(), None);
		assert_eq!(
			ForwardMessageResponse::rejected("bad batch").rejection_reason(),
			Some("bad batch".to_string()),
		);
	}
}
