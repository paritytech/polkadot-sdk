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

//! Channel-layer payload types: message kinds, lifecycle signals and the
//! flow-control register.
//!
//! Nothing here involves the relay chain: channels are a bilateral affair
//! between two parachain runtimes, conducted over the very transport they
//! govern — lifecycle signals are ordinary messages in the ordered streams,
//! confirmations live in the ack registers.

use alloc::vec::Vec;
use polkadot_parachain_primitives::primitives::Id as ParaId;

use crate::mmr::MessagePosition;

/// The payload of every channel-stream MMR leaf. SCALE-encoded; this
/// encoding is what the leaf preimage's `payload` field contains — the
/// preimage layout itself (`LEAF_TAG ++ LEAF_VERSION ++ payload`) is
/// untouched and `LEAF_VERSION` governs the preimage framing only.
#[derive(
	Clone,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	Debug,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub enum SpecMsgKind {
	/// Protocol-level signalling: the sender-side channel lifecycle.
	/// Emitted and consumed by the messaging pallet itself, never by
	/// applications. Ordinary messages in every respect: window-counted,
	/// ordered, confirmed by the watermark like any other position.
	#[codec(index = 0)]
	Signal(SpecMsgSignal),
	/// Userspace payload. The transport delivers the bytes in order and is
	/// deliberately blind to their meaning. Demultiplexing among userspace
	/// protocols is an upper-layer convention; XCM rides here under a
	/// well-known envelope defined by the XCM integration.
	#[codec(index = 1)]
	Data(Vec<u8>),
}

/// Lifecycle signals — the SENDER side of the channel; in-band, ordered,
/// window-counted messages on the `Channel` stream. The receiver side
/// (acceptance, credit, watermark, close) lives entirely out-of-band in its
/// [`Register`].
///
/// Variant indices are FROZEN (the versioning machinery rides on them):
/// `OpenChannel` is index 0 — it must parse before any version announcement
/// exists — and together with `CloseChannel`, `Upgrade` and the [`Register`]
/// format forms the frozen core, parseable at every protocol version.
#[derive(
	Clone,
	Copy,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	codec::MaxEncodedLen,
	Debug,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub enum SpecMsgSignal {
	/// Request/announce a channel. `version` is the sender's initial
	/// version announcement (highest supported). The one message sendable
	/// without credit — necessarily, since no register exists yet.
	#[codec(index = 0)]
	OpenChannel {
		/// The sender's initial (highest supported) protocol version.
		version: u8,
	},
	/// Half-close: the sender sends nothing further after this leaf (until
	/// a later reopen).
	#[codec(index = 1)]
	CloseChannel,
	/// Raise the sender's version announcement mid-channel (monotonic;
	/// lower-than-current values are invalid). Genuine downgrades require
	/// close + reopen.
	#[codec(index = 2)]
	Upgrade {
		/// The sender's new (higher) version announcement.
		version: u8,
	},
}

/// Receiver-granted credit (carried in the [`Register`]) — **advice, not
/// enforcement**. Registers are lossy and read with delay, so a sender may
/// legitimately act on an older grant. Honoring the grant is the sender's
/// own interest; the sender's own STF turns the advice into its local gate.
///
/// Both limits apply simultaneously, mirroring weight's two dimensions:
/// message count bounds per-item processing, bytes the per-block weight/POV
/// budget. `max_message_size` is likewise advice; the *hard* size bound is
/// the consensus constant `MaxMsgLen`, enforced in the sender's STF.
#[derive(
	Clone,
	Copy,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	codec::MaxEncodedLen,
	Debug,
	Default,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct WindowGrant {
	/// Maximum unconfirmed messages beyond the watermark.
	pub max_messages: u32,
	/// Maximum unconfirmed bytes beyond the watermark.
	pub max_bytes: u64,
	/// Advisory per-message size cap.
	pub max_message_size: u32,
}

/// The complete receiver-side state of one channel, published as a leaf on
/// the `Ack` stream. Only the LATEST leaf matters (the stream is consumed
/// lossily, latest-wins); each publish supersedes all earlier ones. Its
/// very existence is the sender-visible channel acceptance.
///
/// `up_to` and `version` are monotonic — a register that regresses either
/// is a protocol violation: the sender ignores the regressed leaf and keeps
/// its previous read. `grant` is NOT monotonic: the receiver may shrink it
/// at any time (shrinking only gates new sends).
#[derive(
	Clone,
	Copy,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	codec::MaxEncodedLen,
	Debug,
	Default,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct Register {
	/// Receiver's protocol version announcement (monotonic).
	pub version: u8,
	/// Cumulative watermark: all positions `< up_to` of the channel's data
	/// stream (messages AND lifecycle signals — it is a stream position)
	/// have been consumed. Monotonic.
	pub up_to: MessagePosition,
	/// Absolute credit for messages beyond `up_to`.
	pub grant: WindowGrant,
	/// Receiver-side close. A closed register's grant is void; `up_to`
	/// still reports what was consumed.
	pub closed: bool,
}

/// Channel discriminator; `peer` is the other end of the channel — the
/// recipient for outbound channels, the channel's sender for inbound ones.
/// Also keys the channel views of the runtime API.
#[derive(
	Clone,
	Copy,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	codec::MaxEncodedLen,
	Debug,
	Eq,
	Ord,
	PartialEq,
	PartialOrd,
	scale_info::TypeInfo,
)]
pub struct ChannelId {
	/// The other end of the channel.
	pub peer: ParaId,
	/// The channel's `domain` discriminator (reserved, 0 by default).
	pub domain: u8,
	/// The channel's `num` discriminator.
	pub num: u16,
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::{Decode, Encode};

	/// The frozen core's variant indices are consensus-critical: an
	/// `OpenChannel` leaf must decode identically at every protocol version
	/// forever.
	#[test]
	fn signal_variant_indices_are_frozen() {
		assert_eq!(SpecMsgSignal::OpenChannel { version: 7 }.encode(), alloc::vec![0x00, 0x07]);
		assert_eq!(SpecMsgSignal::CloseChannel.encode(), alloc::vec![0x01]);
		assert_eq!(SpecMsgSignal::Upgrade { version: 9 }.encode(), alloc::vec![0x02, 0x09]);

		assert_eq!(
			SpecMsgKind::Signal(SpecMsgSignal::CloseChannel).encode(),
			alloc::vec![0x00, 0x01]
		);
		assert_eq!(SpecMsgKind::Data(alloc::vec![0xAB]).encode(), alloc::vec![0x01, 0x04, 0xAB]);
	}

	#[test]
	fn register_round_trips() {
		let register = Register {
			version: 1,
			up_to: MessagePosition(42),
			grant: WindowGrant { max_messages: 10, max_bytes: 4096, max_message_size: 1024 },
			closed: false,
		};
		let decoded = Register::decode(&mut &register.encode()[..]).unwrap();
		assert_eq!(register, decoded);
	}
}
