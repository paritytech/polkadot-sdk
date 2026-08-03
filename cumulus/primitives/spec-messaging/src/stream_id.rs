// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! The structured, relay-invisible stream identifier and its canonical
//! encoding.

use polkadot_parachain_primitives::primitives::Id as ParaId;

/// Length of the canonical [`StreamId`] encoding, in bytes.
pub const STREAM_ID_LEN: usize = 8;

/// First kind byte of the private-use range.
const PRIVATE_KIND_START: u8 = 0x80;

const KIND_CHANNEL: u8 = 0x00;
const KIND_ACK: u8 = 0x01;
const KIND_BROADCAST: u8 = 0x02;

/// Stream identifier, scoped to the sending chain: the full stream key is
/// `(sender ParaId, StreamId)`, with the sender implicit wherever the
/// context is the sender's own candidate.
///
/// `StreamId` has a MANUAL, CANONICAL SCALE ENCODING (no derive), and that
/// encoding is used everywhere — wire, storage, and as the commitment-tree
/// key. One format, always exactly 8 bytes:
///
/// ```text
/// Channel   → 0x00 ++ recipient.to_be_bytes() ++ domain ++ num.to_be_bytes()
/// Ack       → 0x01 ++ recipient.to_be_bytes() ++ domain ++ num.to_be_bytes()
/// Broadcast → 0x02 ++ domain.to_be_bytes() ++ subdomain ++ num.to_be_bytes()
/// Private   → kind ++ body
///             kinds 0x03..=0x7F: reserved for future standard kinds
///             kinds 0x80..=0xFF: private use — body semantics defined by
///                                the chain, never assigned by the standard
/// ```
///
/// The kind byte doubles as the variant discriminant; multi-byte fields are
/// big-endian, so the encoding, compared lexicographically, sorts like the
/// field tuple compared numerically (kind subtrees cluster in the trie;
/// sequential ids are neighbors). This deliberately deviates from default
/// SCALE integer encoding (little-endian) — which is exactly why the impl is
/// manual: SCALE-encoding a `StreamId` IS the sanctioned key derivation, and
/// there is no second format to confuse it with.
///
/// This encoding is CONSENSUS-CRITICAL and frozen: every implementation must
/// reproduce it bit-identically (a receiver derives the same trie path the
/// sender used), locked by the test vectors below. Decode enforces
/// canonicality (`decode ∘ encode` = identity; fixed length, no redundant
/// encodings) and REJECTS reserved kinds `0x03..=0x7F`: no correct consensus
/// path ever decodes a kind it does not know, so an unknown kind is a loud
/// boundary error, not a value. Tooling that walks foreign trees parses raw
/// key bytes with its own lenient presentation parser.
///
/// One uniform rule: a stream id's `ParaId` field names the chain the stream
/// is ADDRESSED TO — its `recipient`, the party that reads it. Broadcast
/// streams have no addressee, hence no field.
///
/// The derived `Ord` equals the lexicographic order of the canonical
/// encoding (variant declaration order matches the kind bytes; big-endian
/// field comparison matches numeric comparison) — pinned by a test below.
/// The commitment tree, the consumption record and the lift transport all
/// sort by this order.
///
/// Note: `Private` with `kind < 0x80` is NOT a valid id — construct via
/// [`StreamId::private`], which enforces the range. Encoding such a value is
/// a programming error (debug-asserted); decoding one is impossible.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum StreamId {
	/// Ordered, flow-controlled, guaranteed-delivery, unidirectional data
	/// stream — the HRMP-replacement workhorse.
	Channel {
		/// The chain this channel's messages are addressed to.
		recipient: ParaId,
		/// Reserved delegation field, 0 by default.
		domain: u8,
		/// Channel discriminator among channels to the same recipient.
		num: u16,
	},
	/// The lossy confirmation register the *receiver* of a channel
	/// maintains, addressed to the channel's sender — same discriminator as
	/// the channel, kind flipped, recipient swapped to the other end.
	Ack {
		/// The channel's sender (who reads this register).
		recipient: ParaId,
		/// Mirrors the channel's `domain`.
		domain: u8,
		/// Mirrors the channel's `num`.
		num: u16,
	},
	/// Sender-wide event stream, no addressee, lossy latest-wins
	/// consumption by any interested chain.
	Broadcast {
		/// Reserved delegation field, 0 by default.
		domain: u16,
		/// Reserved delegation field, 0 by default.
		subdomain: u8,
		/// Feed discriminator.
		num: u32,
	},
	/// Private-use kind (`kind >= 0x80`); body semantics chain-defined.
	Private {
		/// The kind byte; must be in `0x80..=0xFF`.
		kind: u8,
		/// Chain-defined discriminator bytes.
		body: [u8; 7],
	},
}

/// Errors decoding or constructing a [`StreamId`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamIdError {
	/// The kind byte is in the reserved range `0x03..=0x7F`.
	ReservedKind(u8),
}

impl StreamId {
	/// Constructs a private-use id, enforcing `kind >= 0x80`.
	pub fn private(kind: u8, body: [u8; 7]) -> Result<Self, StreamIdError> {
		if kind < PRIVATE_KIND_START {
			return Err(StreamIdError::ReservedKind(kind));
		}
		Ok(Self::Private { kind, body })
	}

	/// The canonical 8-byte encoding. This IS the SCALE encoding, the wire
	/// format and the commitment-tree key.
	pub fn to_bytes(&self) -> [u8; STREAM_ID_LEN] {
		let mut b = [0u8; STREAM_ID_LEN];
		match self {
			Self::Channel { recipient, domain, num } | Self::Ack { recipient, domain, num } => {
				b[0] = if matches!(self, Self::Channel { .. }) { KIND_CHANNEL } else { KIND_ACK };
				b[1..5].copy_from_slice(&u32::from(*recipient).to_be_bytes());
				b[5] = *domain;
				b[6..8].copy_from_slice(&num.to_be_bytes());
			},
			Self::Broadcast { domain, subdomain, num } => {
				b[0] = KIND_BROADCAST;
				b[1..3].copy_from_slice(&domain.to_be_bytes());
				b[3] = *subdomain;
				b[4..8].copy_from_slice(&num.to_be_bytes());
			},
			Self::Private { kind, body } => {
				debug_assert!(
					*kind >= PRIVATE_KIND_START,
					"Private StreamId with reserved kind byte; construct via StreamId::private"
				);
				b[0] = *kind;
				b[1..8].copy_from_slice(body);
			},
		}
		b
	}

	/// Decodes the canonical 8-byte encoding, rejecting reserved kinds.
	pub fn from_bytes(b: [u8; STREAM_ID_LEN]) -> Result<Self, StreamIdError> {
		match b[0] {
			KIND_CHANNEL | KIND_ACK => {
				let recipient = ParaId::from(u32::from_be_bytes([b[1], b[2], b[3], b[4]]));
				let domain = b[5];
				let num = u16::from_be_bytes([b[6], b[7]]);
				Ok(if b[0] == KIND_CHANNEL {
					Self::Channel { recipient, domain, num }
				} else {
					Self::Ack { recipient, domain, num }
				})
			},
			KIND_BROADCAST => Ok(Self::Broadcast {
				domain: u16::from_be_bytes([b[1], b[2]]),
				subdomain: b[3],
				num: u32::from_be_bytes([b[4], b[5], b[6], b[7]]),
			}),
			kind if kind >= PRIVATE_KIND_START => {
				let mut body = [0u8; 7];
				body.copy_from_slice(&b[1..8]);
				Ok(Self::Private { kind, body })
			},
			reserved => Err(StreamIdError::ReservedKind(reserved)),
		}
	}
}

impl codec::Encode for StreamId {
	fn size_hint(&self) -> usize {
		STREAM_ID_LEN
	}

	fn encode_to<T: codec::Output + ?Sized>(&self, dest: &mut T) {
		dest.write(&self.to_bytes());
	}

	fn encoded_size(&self) -> usize {
		STREAM_ID_LEN
	}
}

impl codec::EncodeLike for StreamId {}

impl codec::Decode for StreamId {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let mut b = [0u8; STREAM_ID_LEN];
		input.read(&mut b)?;
		Self::from_bytes(b).map_err(|_| codec::Error::from("reserved StreamId kind"))
	}
}

impl codec::DecodeWithMemTracking for StreamId {}

impl codec::MaxEncodedLen for StreamId {
	fn max_encoded_len() -> usize {
		STREAM_ID_LEN
	}
}

// Metadata describes the id as what it is on the wire: 8 opaque bytes with
// a manual consensus encoding — NOT the Rust enum shape, which would mislead
// generic decoders about field endianness.
impl scale_info::TypeInfo for StreamId {
	type Identity = Self;

	fn type_info() -> scale_info::Type {
		scale_info::Type::builder()
			.path(scale_info::Path::new("StreamId", module_path!()))
			.docs(&["Canonical 8-byte stream id (manual consensus encoding)"])
			.composite(
				scale_info::build::Fields::unnamed()
					.field(|f| f.ty::<[u8; STREAM_ID_LEN]>().type_name("[u8; 8]")),
			)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::{Decode, Encode};

	fn channel(recipient: u32, domain: u8, num: u16) -> StreamId {
		StreamId::Channel { recipient: recipient.into(), domain, num }
	}

	fn ack(recipient: u32, domain: u8, num: u16) -> StreamId {
		StreamId::Ack { recipient: recipient.into(), domain, num }
	}

	fn broadcast(domain: u16, subdomain: u8, num: u32) -> StreamId {
		StreamId::Broadcast { domain, subdomain, num }
	}

	/// Consensus-critical test vectors: these bytes are frozen. A failure
	/// here means the encoding changed, which is a protocol break — never
	/// "fix" the vector, fix the code. Values from the design's worked
	/// example (ParaId 2001 = 0x7D1, 2002 = 0x7D2).
	#[test]
	fn encoding_test_vectors() {
		let vectors: &[(StreamId, [u8; 8])] = &[
			(channel(2001, 0, 0), [0x00, 0x00, 0x00, 0x07, 0xD1, 0x00, 0x00, 0x00]),
			(channel(2002, 0, 0), [0x00, 0x00, 0x00, 0x07, 0xD2, 0x00, 0x00, 0x00]),
			(ack(2001, 0, 0), [0x01, 0x00, 0x00, 0x07, 0xD1, 0x00, 0x00, 0x00]),
			(broadcast(0, 0, 0), [0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
			// Big-endian fields, non-zero everywhere.
			(channel(0x0A0B0C0D, 0xEE, 0x1234), [0x00, 0x0A, 0x0B, 0x0C, 0x0D, 0xEE, 0x12, 0x34]),
			(broadcast(0x0102, 0x03, 0x04050607), [0x02, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]),
			(
				StreamId::private(0x80, [1, 2, 3, 4, 5, 6, 7]).unwrap(),
				[0x80, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07],
			),
			(
				StreamId::private(0xFF, [0; 7]).unwrap(),
				[0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
			),
		];

		for (id, expected) in vectors {
			assert_eq!(&id.to_bytes(), expected, "encoding of {id:?}");
			assert_eq!(&id.encode()[..], &expected[..], "SCALE encoding of {id:?}");
			assert_eq!(StreamId::from_bytes(*expected), Ok(*id), "decoding of {id:?}");
		}
	}

	#[test]
	fn decode_encode_is_identity() {
		// All kind bytes, arbitrary bodies: every non-reserved byte string
		// decodes, and re-encoding reproduces the input bytes exactly.
		for kind in 0..=u8::MAX {
			let bytes = [kind, 0xA1, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6, 0x07];
			match StreamId::from_bytes(bytes) {
				Ok(id) => assert_eq!(id.to_bytes(), bytes, "kind {kind:#x}"),
				Err(StreamIdError::ReservedKind(k)) => {
					assert_eq!(k, kind);
					assert!((0x03..=0x7F).contains(&kind), "kind {kind:#x} wrongly reserved");
				},
			}
		}
	}

	#[test]
	fn reserved_kinds_rejected() {
		for kind in 0x03..=0x7Fu8 {
			let bytes = [kind, 0, 0, 0, 0, 0, 0, 0];
			assert_eq!(StreamId::from_bytes(bytes), Err(StreamIdError::ReservedKind(kind)));
			assert!(StreamId::decode(&mut &bytes[..]).is_err(), "SCALE decode of kind {kind:#x}");
		}
		assert_eq!(StreamId::private(0x03, [0; 7]), Err(StreamIdError::ReservedKind(0x03)));
	}

	#[test]
	fn scale_decode_consumes_exactly_eight_bytes() {
		let mut bytes = channel(7, 1, 2).encode();
		assert_eq!(bytes.len(), STREAM_ID_LEN);
		bytes.push(0xAB);
		let mut input = &bytes[..];
		let id = StreamId::decode(&mut input).unwrap();
		assert_eq!(id, channel(7, 1, 2));
		assert_eq!(input, &[0xAB]);
	}

	#[test]
	fn ord_matches_encoding_order() {
		// Representative sample crossing every boundary that could diverge:
		// variant order, big-endian field order, private kinds.
		let sample = [
			channel(0, 0, 0),
			channel(1, 0, 0),
			channel(1, 0, 1),
			channel(1, 1, 0),
			channel(2001, 0, 0),
			channel(2002, 0, 0),
			channel(0x0100, 0, 0),
			channel(u32::MAX, u8::MAX, u16::MAX),
			ack(0, 0, 0),
			ack(2001, 0, 0),
			ack(2001, 0, 1),
			broadcast(0, 0, 0),
			broadcast(0, 0, 1),
			broadcast(0, 1, 0),
			broadcast(1, 0, 0),
			broadcast(0x0100, 0, 0),
			StreamId::private(0x80, [0; 7]).unwrap(),
			StreamId::private(0x80, [0, 0, 0, 0, 0, 0, 1]).unwrap(),
			StreamId::private(0x81, [0; 7]).unwrap(),
			StreamId::private(0xFF, [0xFF; 7]).unwrap(),
		];

		for a in &sample {
			for b in &sample {
				assert_eq!(
					a.cmp(b),
					a.to_bytes().cmp(&b.to_bytes()),
					"Ord vs encoding order for {a:?} / {b:?}"
				);
			}
		}
	}
}
