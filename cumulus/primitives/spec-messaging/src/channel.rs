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

//! Channel state primitives: the key and the two per-direction states the messaging
//! pallet keeps in `OutChannels` / `InChannels` and returns from the runtime API's `out_channels()`
//! / `in_channels()`.
//!
//! State shapes, not wire types. The channel protocol (the in-band [`crate::SpecMsgSignal`]s and
//! the out-of-band [`Register`]) is in [`crate::flow_control`]; the logic is the pallet's.

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use scale_info::TypeInfo;

use crate::{flow_control::Register, mmr::MessagePosition};

/// Channel discriminator.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	TypeInfo,
)]
pub struct ChannelId {
	/// The other end: the recipient of an outbound channel, the sender of an inbound one.
	pub peer: ParaId,
	/// Allocation-convention field, `0` by default (see `StreamId`).
	pub domain: u8,
	pub num: u16,
}

/// A channel's phase, derived from [`OutChannelState`]. Not stored.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChannelPhase {
	/// Sent `OpenChannel`, no register read yet.
	Opening,
	/// A register has been read and neither side has closed.
	Open,
	/// We sent `CloseChannel`, or the peer's register says `closed`.
	Closed,
}

/// A register as read: the peer's latest `Register` and its position on the peer's `Ack` stream.
/// Kept together so no register can be stored without its position.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
	TypeInfo,
)]
pub struct RegisterRead {
	/// Position of `register` on the peer's `Ack` stream. A read below it is ignored, so an older
	/// grant or `closed` flag cannot replace a newer one: `up_to` and `version` stay unchanged
	/// across a suspend, a resume or a close, so they cannot order registers. Sound only because
	/// the stream root binds the leaf count.
	pub at: MessagePosition,
	/// The peer's watermark, credit, version announcement and closed flag.
	pub register: Register,
}

/// Sender side, per outbound channel.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
	TypeInfo,
)]
pub struct OutChannelState {
	/// Whether we sent `CloseChannel`. The peer's close arrives in the register; this is the one
	/// phase bit the register cannot carry.
	pub closed_by_us: bool,
	/// Our latest in-band version announcement.
	pub announced_version: u8,
	/// Latest register read, with its position. `None` until the first read.
	pub register: Option<RegisterRead>,
}

impl OutChannelState {
	/// The lower of our announced version and the peer's. `None` while `Opening`: no register has
	/// been read, so the peer's is unknown.
	pub fn effective_version(&self) -> Option<u8> {
		self.register.map(|read| self.announced_version.min(read.register.version))
	}

	pub fn phase(&self) -> ChannelPhase {
		if self.closed_by_us || self.register.is_some_and(|read| read.register.closed) {
			ChannelPhase::Closed
		} else if self.register.is_none() {
			ChannelPhase::Opening
		} else {
			ChannelPhase::Open
		}
	}
}

/// Receiver side, per inbound channel.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
	TypeInfo,
)]
pub struct InChannelState {
	/// The register we last published on our `Ack` stream. This is our channel state as the peer
	/// sees it, and it decides when the next publish is due.
	pub published: Register,
	/// The sender's latest in-band version announcement (from consumed `OpenChannel` /
	/// `Upgrade` signals).
	pub peer_version: u8,
	/// Upper-layer consumption switch. While set, the STF refuses this channel's messages,
	/// `consumed_streams()` omits the stream, and published registers grant zero. A pause, not a
	/// close: all state persists.
	pub suspended: bool,
}

impl InChannelState {
	pub fn effective_version(&self) -> u8 {
		self.published.version.min(self.peer_version)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flow_control::WindowGrant;

	fn register(version: u8, closed: bool) -> Register {
		Register { version, up_to: MessagePosition(0), grant: WindowGrant::default(), closed }
	}

	fn read(version: u8, closed: bool) -> RegisterRead {
		RegisterRead { at: MessagePosition(0), register: register(version, closed) }
	}

	#[test]
	fn out_channel_phase_is_a_view() {
		let mut out = OutChannelState { closed_by_us: false, announced_version: 1, register: None };
		assert_eq!(out.phase(), ChannelPhase::Opening);
		out.register = Some(read(1, false));
		assert_eq!(out.phase(), ChannelPhase::Open);
		// Either side's close closes the channel. Ours counts before any register read.
		out.register = Some(read(1, true));
		assert_eq!(out.phase(), ChannelPhase::Closed);
		let ours = OutChannelState { closed_by_us: true, announced_version: 1, register: None };
		assert_eq!(ours.phase(), ChannelPhase::Closed);
	}

	#[test]
	fn effective_version_is_the_min() {
		let inbound =
			InChannelState { published: register(3, false), peer_version: 2, suspended: false };
		assert_eq!(inbound.effective_version(), 2);

		let mut out = OutChannelState { closed_by_us: false, announced_version: 3, register: None };
		assert_eq!(out.effective_version(), None, "unknown until the first register read");
		out.register = Some(read(2, false));
		assert_eq!(out.effective_version(), Some(2));
	}

	#[test]
	fn channel_id_orders_by_peer_then_domain_then_num() {
		let a = ChannelId { peer: ParaId::from(1), domain: 1, num: 9 };
		let b = ChannelId { peer: ParaId::from(2), domain: 0, num: 0 };
		let c = ChannelId { peer: ParaId::from(2), domain: 0, num: 1 };
		assert!(a < b && b < c);
	}
}
