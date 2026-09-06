//! `StreamId` — the relay-invisible, parachain-structured stream key (spec-msg v0.5).
//!
//! Each stream a chain maintains is one MMR keyed by a `StreamId`. The relay chain never sees an id
//! — only the `StreamsRoot` (the keyed-tree root over stream roots) reaches it; ids live in tree
//! paths, proofs and networking. The full stream key is `(sender ParaId, StreamId)`, with the
//! sender implicit in the sender's own candidate.
//!
//! # Canonical encoding (CONSENSUS-CRITICAL, frozen)
//!
//! Manual SCALE, always **exactly 8 bytes**, big-endian — so lexicographic byte order equals
//! numeric field-tuple order (kinds cluster; sequential ids are neighbours). SCALE-encoding a
//! `StreamId` *is* the commitment-tree key derivation; there is no second format to confuse it
//! with, which is why the impl is manual (default SCALE ints are little-endian).
//!
//! ```text
//! Channel   → 0x00 ++ recipient(be32) ++ domain(u8) ++ num(be16)
//! Ack       → 0x01 ++ recipient(be32) ++ domain(u8) ++ num(be16)
//! Broadcast → 0x02 ++ domain(be16) ++ subdomain(u8) ++ num(be32)
//! Private   → kind(0x80..=0xFF) ++ body[7]
//! ```
//!
//! `Decode` enforces canonicality (fixed 8-byte length, no redundant encodings) and REJECTS
//! reserved kinds `0x03..=0x7F`: a correct consensus path only ever keys streams it knows, so an
//! unknown kind is a loud boundary error, not a value. `Ord` equals the canonical-encoding order
//! (verified by test).

use codec::{
	Decode, DecodeWithMemTracking, Encode, EncodeLike, Error as CodecError, Input, MaxEncodedLen,
	Output,
};
use polkadot_parachain_primitives::primitives::Id as ParaId;

/// Encoded length of every `StreamId`.
pub const STREAM_ID_LEN: usize = 8;

/// A relay-invisible, parachain-structured stream key. See the module docs for the frozen encoding.
///
/// The `recipient` / addressee is the party that *reads* the stream: a channel's messages are for
/// the recipient; an ack register is for the chain it grants credit to; broadcasts have no
/// addressee.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, scale_info::TypeInfo)]
pub enum StreamId {
	/// Ordered, flow-controlled, guaranteed, unidirectional channel to `recipient`.
	Channel { recipient: ParaId, domain: u8, num: u16 },
	/// The receiver's lossy confirmation register for a channel, addressed to `recipient` (the
	/// sender).
	Ack { recipient: ParaId, domain: u8, num: u16 },
	/// Sender-wide event stream (pub-sub), no addressee.
	Broadcast { domain: u16, subdomain: u8, num: u32 },
	/// Private-use kind (`0x80..=0xFF`); 7 chain-defined body bytes.
	Private { kind: u8, body: [u8; 7] },
}

impl StreamId {
	/// The chain the stream is ADDRESSED TO (its `recipient`), if the kind has one.
	pub fn recipient(&self) -> Option<ParaId> {
		match self {
			StreamId::Channel { recipient, .. } | StreamId::Ack { recipient, .. } => {
				Some(*recipient)
			},
			StreamId::Broadcast { .. } | StreamId::Private { .. } => None,
		}
	}
}

impl Encode for StreamId {
	fn size_hint(&self) -> usize {
		STREAM_ID_LEN
	}

	fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
		match self {
			StreamId::Channel { recipient, domain, num } => {
				dest.push_byte(0x00);
				dest.write(&u32::from(*recipient).to_be_bytes());
				dest.push_byte(*domain);
				dest.write(&num.to_be_bytes());
			},
			StreamId::Ack { recipient, domain, num } => {
				dest.push_byte(0x01);
				dest.write(&u32::from(*recipient).to_be_bytes());
				dest.push_byte(*domain);
				dest.write(&num.to_be_bytes());
			},
			StreamId::Broadcast { domain, subdomain, num } => {
				dest.push_byte(0x02);
				dest.write(&domain.to_be_bytes());
				dest.push_byte(*subdomain);
				dest.write(&num.to_be_bytes());
			},
			StreamId::Private { kind, body } => {
				dest.push_byte(*kind);
				dest.write(body);
			},
		}
	}
}

impl EncodeLike for StreamId {}
impl DecodeWithMemTracking for StreamId {}

impl MaxEncodedLen for StreamId {
	fn max_encoded_len() -> usize {
		STREAM_ID_LEN
	}
}

impl Decode for StreamId {
	fn decode<I: Input>(input: &mut I) -> Result<Self, CodecError> {
		let kind = input.read_byte()?;
		match kind {
			0x00 | 0x01 => {
				let mut r = [0u8; 4];
				input.read(&mut r)?;
				let recipient = ParaId::from(u32::from_be_bytes(r));
				let domain = input.read_byte()?;
				let mut n = [0u8; 2];
				input.read(&mut n)?;
				let num = u16::from_be_bytes(n);
				Ok(if kind == 0x00 {
					StreamId::Channel { recipient, domain, num }
				} else {
					StreamId::Ack { recipient, domain, num }
				})
			},
			0x02 => {
				let mut d = [0u8; 2];
				input.read(&mut d)?;
				let domain = u16::from_be_bytes(d);
				let subdomain = input.read_byte()?;
				let mut n = [0u8; 4];
				input.read(&mut n)?;
				let num = u32::from_be_bytes(n);
				Ok(StreamId::Broadcast { domain, subdomain, num })
			},
			0x80..=0xFF => {
				let mut body = [0u8; 7];
				input.read(&mut body)?;
				Ok(StreamId::Private { kind, body })
			},
			// 0x03..=0x7F are reserved for future standard kinds and REJECTED (canonicality).
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
			StreamId::Private { kind: 0x80, body: [0; 7] },
			StreamId::Private { kind: 0xFF, body: [7; 7] },
		]
	}

	#[test]
	fn encoding_layout_is_exact() {
		// Frozen test vectors — any change here is a consensus break.
		assert_eq!(
			StreamId::Channel { recipient: ParaId::from(2000), domain: 0, num: 0 }.encode(),
			vec![0x00, 0x00, 0x00, 0x07, 0xD0, 0x00, 0x00, 0x00],
		);
		assert_eq!(
			StreamId::Ack { recipient: ParaId::from(1000), domain: 0, num: 5 }.encode(),
			vec![0x01, 0x00, 0x00, 0x03, 0xE8, 0x00, 0x00, 0x05],
		);
		assert_eq!(
			StreamId::Broadcast { domain: 0, subdomain: 0, num: 1 }.encode(),
			vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01],
		);
		assert_eq!(
			StreamId::Private { kind: 0x80, body: [1, 2, 3, 4, 5, 6, 7] }.encode(),
			vec![0x80, 1, 2, 3, 4, 5, 6, 7],
		);
		for id in sample_ids() {
			assert_eq!(
				id.encode().len(),
				STREAM_ID_LEN,
				"{id:?} must encode to {STREAM_ID_LEN} bytes"
			);
		}
	}

	#[test]
	fn decode_round_trips() {
		for id in sample_ids() {
			assert_eq!(StreamId::decode(&mut &id.encode()[..]).unwrap(), id);
		}
	}

	#[test]
	fn decode_rejects_reserved_kinds() {
		for kind in 0x03u8..=0x7F {
			let bytes = [kind, 0, 0, 0, 0, 0, 0, 0];
			assert!(
				StreamId::decode(&mut &bytes[..]).is_err(),
				"reserved kind {kind:#x} must reject"
			);
		}
	}

	#[test]
	fn decode_rejects_truncated() {
		let full = StreamId::Channel { recipient: ParaId::from(7), domain: 0, num: 0 }.encode();
		for len in 0..full.len() {
			assert!(StreamId::decode(&mut &full[..len]).is_err(), "len {len} must reject");
		}
	}

	#[test]
	fn ord_equals_canonical_encoding_order() {
		let mut by_ord = sample_ids();
		by_ord.sort();
		let mut by_bytes = sample_ids();
		by_bytes.sort_by_key(|id| id.encode());
		assert_eq!(by_ord, by_bytes, "Ord must match canonical-encoding lexicographic order");
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
		assert_eq!(StreamId::Private { kind: 0x80, body: [0; 7] }.recipient(), None);
	}
}
