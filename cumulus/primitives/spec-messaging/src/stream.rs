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

//! `StreamId`: the parachain-structured stream key.
//!
//! Each stream a chain maintains is one MMR keyed by a `StreamId`. The relay chain never sees an
//! id; only the `StreamsRoot` reaches it. Ids live in tree paths, proofs and networking. The full
//! stream key is `(sender ParaId, StreamId)`, with the sender implicit in its own candidate.
//!
//! # Tree key (consensus-critical, frozen)
//!
//! [`StreamId::to_bytes`] is the commitment-tree key. It is big-endian, so `Ord` equals the key's
//! byte order: kinds cluster and sequential ids are neighbours.

use codec::{Decode, DecodeWithMemTracking, Encode, Error as CodecError, Input, MaxEncodedLen};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use scale_info::TypeInfo;

pub const STREAM_ID_LEN: usize = 8;

/// A private-use kind byte.
///
/// Byte 0 of the tree key is the discriminant for standard kinds and the payload for `Private`, so
/// a kind below [`Self::MIN`] would collide with a standard kind. [`Self::new`] is the only check.
#[derive(
	Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Encode, MaxEncodedLen, TypeInfo,
)]
pub struct PrivateKind(u8);

impl PrivateKind {
	/// Lowest private-use kind byte; everything below is standard or reserved.
	pub const MIN: u8 = 0x80;

	pub const fn new(kind: u8) -> Option<Self> {
		if kind >= Self::MIN {
			Some(Self(kind))
		} else {
			None
		}
	}

	pub const fn get(self) -> u8 {
		self.0
	}
}

impl Decode for PrivateKind {
	fn decode<I: Input>(input: &mut I) -> Result<Self, CodecError> {
		Self::new(u8::decode(input)?).ok_or_else(|| "PrivateKind: below MIN".into())
	}
}

impl DecodeWithMemTracking for PrivateKind {}

/// A parachain-structured stream key.
///
/// `recipient` is the party that reads the stream: a channel's messages are for the recipient, an
/// ack register is for the chain it grants credit to, broadcasts have no addressee.
///
/// SCALE variant indices follow the tree key's kind bytes.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Hash,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub enum StreamId {
	/// Ordered, flow-controlled, guaranteed, unidirectional channel to `recipient`.
	#[codec(index = 0)]
	Channel { recipient: ParaId, domain: u8, num: u16 },
	/// The receiver's lossy confirmation register for a channel, addressed to `recipient` (the
	/// sender).
	#[codec(index = 1)]
	Ack { recipient: ParaId, domain: u8, num: u16 },
	/// Sender-wide event stream (pub-sub), no addressee.
	#[codec(index = 2)]
	Broadcast { domain: u16, subdomain: u8, num: u32 },
	/// Private-use kind; 7 chain-defined body bytes.
	#[codec(index = 0x80)]
	Private { kind: PrivateKind, body: [u8; 7] },
}

impl StreamId {
	pub fn recipient(&self) -> Option<ParaId> {
		match self {
			StreamId::Channel { recipient, .. } | StreamId::Ack { recipient, .. } => {
				Some(*recipient)
			},
			StreamId::Broadcast { .. } | StreamId::Private { .. } => None,
		}
	}

	/// The commitment-tree key.
	pub fn to_bytes(&self) -> [u8; STREAM_ID_LEN] {
		let mut out = [0u8; STREAM_ID_LEN];
		match self {
			StreamId::Channel { recipient, domain, num } => {
				out[0] = 0x00;
				out[1..5].copy_from_slice(&u32::from(*recipient).to_be_bytes());
				out[5] = *domain;
				out[6..8].copy_from_slice(&num.to_be_bytes());
			},
			StreamId::Ack { recipient, domain, num } => {
				out[0] = 0x01;
				out[1..5].copy_from_slice(&u32::from(*recipient).to_be_bytes());
				out[5] = *domain;
				out[6..8].copy_from_slice(&num.to_be_bytes());
			},
			StreamId::Broadcast { domain, subdomain, num } => {
				out[0] = 0x02;
				out[1..3].copy_from_slice(&domain.to_be_bytes());
				out[3] = *subdomain;
				out[4..8].copy_from_slice(&num.to_be_bytes());
			},
			StreamId::Private { kind, body } => {
				out[0] = kind.get();
				out[1..8].copy_from_slice(body);
			},
		}
		out
	}

	/// The inverse of [`Self::to_bytes`]. Returns `Err` if the kind is reserved.
	pub fn from_bytes(bytes: [u8; STREAM_ID_LEN]) -> Result<Self, CodecError> {
		let [kind, rest @ ..] = bytes;
		let recipient = || ParaId::from(u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]));
		match kind {
			0x00 => Ok(StreamId::Channel {
				recipient: recipient(),
				domain: rest[4],
				num: u16::from_be_bytes([rest[5], rest[6]]),
			}),
			0x01 => Ok(StreamId::Ack {
				recipient: recipient(),
				domain: rest[4],
				num: u16::from_be_bytes([rest[5], rest[6]]),
			}),
			0x02 => Ok(StreamId::Broadcast {
				domain: u16::from_be_bytes([rest[0], rest[1]]),
				subdomain: rest[2],
				num: u32::from_be_bytes([rest[3], rest[4], rest[5], rest[6]]),
			}),
			0x80..=0xFF => {
				let kind = PrivateKind::new(kind).ok_or("StreamId: private kind out of range")?;
				Ok(StreamId::Private { kind, body: rest })
			},
			_ => Err(CodecError::from("StreamId: reserved/unknown kind")),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn sample_ids() -> Vec<StreamId> {
		vec![
			StreamId::Channel { recipient: ParaId::from(1000), domain: 0, num: 0 },
			StreamId::Channel { recipient: ParaId::from(1000), domain: 0, num: 1 },
			StreamId::Channel { recipient: ParaId::from(1000), domain: 1, num: 0 },
			StreamId::Channel { recipient: ParaId::from(2000), domain: 0, num: 0 },
			StreamId::Ack { recipient: ParaId::from(1000), domain: 0, num: 0 },
			StreamId::Broadcast { domain: 0, subdomain: 0, num: 0 },
			StreamId::Broadcast { domain: 1, subdomain: 0, num: 0 },
			StreamId::Private { kind: PrivateKind::new(0x80).unwrap(), body: [0; 7] },
			StreamId::Private { kind: PrivateKind::new(0xFF).unwrap(), body: [7; 7] },
		]
	}

	#[test]
	fn private_kind_rejects_standard_and_reserved_bytes() {
		for byte in 0x00..PrivateKind::MIN {
			assert!(PrivateKind::new(byte).is_none(), "0x{byte:02X} must not be a private kind");
		}
		for byte in PrivateKind::MIN..=0xFF {
			assert_eq!(PrivateKind::new(byte).map(PrivateKind::get), Some(byte));
		}
	}

	#[test]
	fn tree_key_layout_is_exact() {
		assert_eq!(
			StreamId::Channel { recipient: ParaId::from(2000), domain: 0, num: 0 }.to_bytes(),
			[0x00, 0x00, 0x00, 0x07, 0xD0, 0x00, 0x00, 0x00],
		);
		assert_eq!(
			StreamId::Ack { recipient: ParaId::from(1000), domain: 0, num: 5 }.to_bytes(),
			[0x01, 0x00, 0x00, 0x03, 0xE8, 0x00, 0x00, 0x05],
		);
		assert_eq!(
			StreamId::Broadcast { domain: 0, subdomain: 0, num: 1 }.to_bytes(),
			[0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01],
		);
		assert_eq!(
			StreamId::Private {
				kind: PrivateKind::new(0x80).unwrap(),
				body: [1, 2, 3, 4, 5, 6, 7]
			}
			.to_bytes(),
			[0x80, 1, 2, 3, 4, 5, 6, 7],
		);
	}

	#[test]
	fn from_bytes_inverts_to_bytes() {
		for id in sample_ids() {
			assert_eq!(StreamId::from_bytes(id.to_bytes()).unwrap(), id);
		}
	}

	#[test]
	fn from_bytes_rejects_reserved_kinds() {
		for kind in 0x03u8..=0x7F {
			assert!(
				StreamId::from_bytes([kind, 0, 0, 0, 0, 0, 0, 0]).is_err(),
				"reserved kind {kind:#x} must reject"
			);
		}
	}

	#[test]
	fn decode_round_trips() {
		for id in sample_ids() {
			let encoded = id.encode();
			assert!(encoded.len() <= StreamId::max_encoded_len());
			assert_eq!(StreamId::decode(&mut &encoded[..]).unwrap(), id);
		}
	}

	#[test]
	fn decode_rejects_private_kind_below_min() {
		let mut encoded =
			StreamId::Private { kind: PrivateKind::new(0x80).unwrap(), body: [0; 7] }.encode();
		encoded[1] = PrivateKind::MIN - 1;
		assert!(StreamId::decode(&mut &encoded[..]).is_err());
	}

	#[test]
	fn decode_rejects_truncated() {
		let full = StreamId::Channel { recipient: ParaId::from(7), domain: 0, num: 0 }.encode();
		for len in 0..full.len() {
			assert!(StreamId::decode(&mut &full[..len]).is_err(), "len {len} must reject");
		}
	}

	#[test]
	fn ord_equals_tree_key_order() {
		let mut by_ord = sample_ids();
		by_ord.sort();
		let mut by_key = sample_ids();
		by_key.sort_by_key(StreamId::to_bytes);
		assert_eq!(by_ord, by_key, "Ord must match the tree key's lexicographic order");
	}

	#[test]
	fn recipient_addressing() {
		assert_eq!(
			StreamId::Channel { recipient: ParaId::from(9), domain: 0, num: 0 }.recipient(),
			Some(ParaId::from(9)),
		);
		assert_eq!(
			StreamId::Ack { recipient: ParaId::from(9), domain: 0, num: 0 }.recipient(),
			Some(ParaId::from(9)),
		);
		assert_eq!(StreamId::Broadcast { domain: 0, subdomain: 0, num: 0 }.recipient(), None);
		assert_eq!(
			StreamId::Private { kind: PrivateKind::new(0x80).unwrap(), body: [0; 7] }.recipient(),
			None
		);
	}
}
