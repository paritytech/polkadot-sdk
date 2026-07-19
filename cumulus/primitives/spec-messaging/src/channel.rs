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

use crate::{mmr::MessagePosition, stream_id::StreamId};

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

/// One stream this chain currently consumes, as the `consumed_streams()`
/// runtime API serves it (grouped by source): stream identity and
/// consumption discipline in one enum, plus the per-stream resume cursor —
/// what the own collators fetch, from which position on.
///
/// The full [`StreamId`]'s `recipient` field is absent: by the uniform
/// addressing rule a consumed stream is always addressed to the consuming
/// chain itself — [`ConsumedStream::stream_id`] reconstructs the full id.
/// `Ack` streams never appear: ack registers carry no resume state (lossy,
/// latest-wins head reads), and *which* registers to read follows from the
/// `out_channels()` view. Private kinds cannot appear either.
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
pub enum ConsumedStream {
	/// Ordered channel consumption: fetch the payloads from `from` on.
	Channel {
		/// The channel's `domain` discriminator.
		domain: u8,
		/// The channel's `num` discriminator.
		num: u16,
		/// Resume cursor: the position of the next message to consume —
		/// projects the stream's inbound frontier (cursor = leaf count;
		/// storage keeps frontiers because verification needs peaks, the
		/// API returns positions because fetching addresses by position).
		from: MessagePosition,
	},
	/// Lossy latest-wins event consumption (broadcast streams are post-MVP;
	/// the discipline ships because ack-register reads are exactly it).
	Broadcast {
		/// The feed's `domain` discriminator.
		domain: u16,
		/// The feed's `subdomain` discriminator.
		subdomain: u8,
		/// The feed's `num` discriminator.
		num: u32,
		/// Positions below `from` are no longer of interest.
		from: MessagePosition,
	},
}

impl ConsumedStream {
	/// Projects a full consumed stream key onto the API view, given the
	/// stream's resume cursor. `None` for the kinds the view never carries
	/// (`Ack`, `Private`).
	pub fn project(stream: &StreamId, from: MessagePosition) -> Option<Self> {
		match *stream {
			StreamId::Channel { domain, num, .. } => Some(Self::Channel { domain, num, from }),
			StreamId::Broadcast { domain, subdomain, num } => {
				Some(Self::Broadcast { domain, subdomain, num, from })
			},
			StreamId::Ack { .. } | StreamId::Private { .. } => None,
		}
	}

	/// Reconstructs the full [`StreamId`] (in the source's key space);
	/// `recipient` is the consuming chain — the uniform addressing rule.
	pub fn stream_id(&self, recipient: ParaId) -> StreamId {
		match *self {
			Self::Channel { domain, num, .. } => StreamId::Channel { recipient, domain, num },
			Self::Broadcast { domain, subdomain, num, .. } => {
				StreamId::Broadcast { domain, subdomain, num }
			},
		}
	}
}

/// The phase of an outbound channel — a *view* over [`OutChannelState`]'s
/// fields ([`OutChannelState::phase`]), never stored: `Opening` until the
/// peer's register is first read (its very existence is the acceptance),
/// `Open` while neither side closed, `Closed` on either side's close.
/// Close and reopen only layer over the eternal stream/frontier state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelPhase {
	/// `OpenChannel` sent, no register read yet: the peer has not
	/// (visibly) accepted. No credit exists, nothing but the open signal
	/// was sendable.
	Opening,
	/// A register was read and neither side closed: the channel carries
	/// credit-gated traffic.
	Open,
	/// This chain half-closed (`closed_by_us`) or the peer's register did
	/// (`register.closed`). Advisory and reversible: reopening emits
	/// `OpenChannel` again over the surviving frontier.
	Closed,
}

/// Sender-side state of one OUTBOUND channel — the value of the
/// `out_channels()` runtime API view and of the messaging pallet's
/// `OutChannels` storage. Phases are views over the fields
/// ([`Self::phase`]): `Opening` = `register` is `None`; `Open` = `Some`
/// with neither side closed; `Closed` = `closed_by_us` or
/// `register.closed`.
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
pub struct OutChannelState {
	/// Whether this chain half-closed the channel (`CloseChannel` sent).
	pub closed_by_us: bool,
	/// This chain's latest version announcement (monotonic; MVP always 0).
	pub announced_version: u8,
	/// The latest verified read of the peer's [`Register`]; `None` until
	/// the first read — the register's very existence is the peer's
	/// acceptance. Credit/watermark standing lives here; *which* ack
	/// registers the own collators read follows from this view's keys.
	pub register: Option<Register>,
}

impl OutChannelState {
	/// The channel's phase, derived from the fields (see [`ChannelPhase`]).
	pub fn phase(&self) -> ChannelPhase {
		if self.closed_by_us || self.register.map_or(false, |register| register.closed) {
			ChannelPhase::Closed
		} else if self.register.is_none() {
			ChannelPhase::Opening
		} else {
			ChannelPhase::Open
		}
	}

	/// The effective protocol version: the min of both sides' latest
	/// announcements (a variant is usable only after *reading* the
	/// announcement permitting it). `None` until the peer's register was
	/// first read.
	pub fn effective_version(&self) -> Option<u8> {
		self.register.map(|register| self.announced_version.min(register.version))
	}
}

/// Receiver-side state of one INBOUND channel — the value of the
/// `in_channels()` runtime API view and of the messaging pallet's
/// `InChannels` storage: which channels are due a register publish check
/// (compare [`Self::published`] against the stream's consumption
/// progress) and the suspension standing. An entry's existence is the
/// acceptance.
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
pub struct InChannelState {
	/// The register this chain last published on its ack stream.
	pub published: Register,
	/// The peer's latest version announcement (`OpenChannel` / `Upgrade`;
	/// monotonic).
	pub peer_version: u8,
	/// Upper-layer off switch (pause, not close): while suspended the STF
	/// refuses the channel's messages, `consumed_streams()` omits the
	/// stream and published registers grant zero.
	pub suspended: bool,
}

impl InChannelState {
	/// The effective protocol version: the min of both sides' latest
	/// announcements (the peer's arrived in-band via
	/// `OpenChannel`/`Upgrade`, this side's rides the published register).
	pub fn effective_version(&self) -> u8 {
		self.published.version.min(self.peer_version)
	}
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
	fn consumed_stream_projection_round_trips() {
		// `recipient` is redundant in the view (uniform addressing rule):
		// projecting and reconstructing with the consuming chain's id is
		// the identity on the kinds the view carries.
		let me = ParaId::from(2001);
		let cursor = MessagePosition(42);
		let carried = [
			StreamId::Channel { recipient: me, domain: 3, num: 7 },
			StreamId::Broadcast { domain: 1, subdomain: 2, num: 3 },
		];
		for stream in carried {
			let view = ConsumedStream::project(&stream, cursor).unwrap();
			assert_eq!(view.stream_id(me), stream);
			let (ConsumedStream::Channel { from, .. } | ConsumedStream::Broadcast { from, .. }) =
				view;
			assert_eq!(from, cursor);
		}

		// Ack registers (no resume state; they follow from `out_channels()`)
		// and private kinds never appear in the view.
		let never = [
			StreamId::Ack { recipient: me, domain: 0, num: 0 },
			StreamId::private(0x80, [0; 7]).unwrap(),
		];
		for stream in never {
			assert_eq!(ConsumedStream::project(&stream, cursor), None);
		}
	}

	#[test]
	fn phases_are_views_over_the_fields() {
		let mut state = OutChannelState::default();
		assert_eq!(state.phase(), ChannelPhase::Opening);
		assert_eq!(state.effective_version(), None);

		// The register's existence is the acceptance: reading one opens.
		state.register = Some(Register { version: 3, ..Default::default() });
		assert_eq!(state.phase(), ChannelPhase::Open);
		// Effective = min of the two latest announcements.
		assert_eq!(state.effective_version(), Some(0));
		state.announced_version = 7;
		assert_eq!(state.effective_version(), Some(3));

		// Either side's close closes; both flags are independent views.
		state.closed_by_us = true;
		assert_eq!(state.phase(), ChannelPhase::Closed);
		state.closed_by_us = false;
		state.register = Some(Register { closed: true, ..Default::default() });
		assert_eq!(state.phase(), ChannelPhase::Closed);
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
