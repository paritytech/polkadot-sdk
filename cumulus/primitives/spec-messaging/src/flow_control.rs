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

//! Channel flow-control types (spec-msg v0.5): the sender's channel-stream leaf payload
//! ([`SpecMsgKind`] / [`SpecMsgSignal`]) and the receiver's out-of-band confirmation [`Register`]
//! with its advisory [`WindowGrant`]. The pallet owns the logic: windowing, version enforcement,
//! channel storage, the accept extrinsic.
//!
//! # Consensus surface
//!
//! - [`SpecMsgKind`] is the SCALE payload of every channel-stream MMR leaf, inside
//! [`crate::message::leaf_hash`]'s `LEAF_TAG ++ LEAF_VERSION ++ payload` framing. It can evolve
//! without a `LEAF_VERSION` bump.
//! - [`Register`] is the SCALE payload of every ack-stream leaf: the receiver's whole channel
//! state, lossy and latest-wins, read out-of-band by the sender.
//!
//! # Frozen core
//!
//! These must parse at every protocol version: `OpenChannel` (variant 0, so it decodes before any
//! version is announced), `CloseChannel`, `Upgrade`, and the `Register` format. The tests pin their
//! bytes. Versioning is by monotonic per-side announcements (sender in-band, receiver via
//! [`Register::version`]); the effective version is the lower of the two.

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

use crate::mmr::MessagePosition;

/// The payload of every channel-stream MMR leaf: protocol signalling or userspace data. The
/// transport acts only on `Signal` vs `Data` (window accounting, pallet-internal consumption);
/// demultiplexing within `Data` is an upper-layer convention.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub enum SpecMsgKind {
	/// Channel lifecycle signalling. Emitted and consumed by the pallet, never by applications;
	/// window-counted and ordered like any other leaf.
	Signal(SpecMsgSignal),
	/// Userspace payload bytes, delivered in order. Bounded by
	/// [`crate::message::MAX_SPECULATIVE_MESSAGE_LEN`] in the sender STF, not by the type.
	Data(Vec<u8>),
}

/// Sender-side channel lifecycle signal. Frozen core: `OpenChannel` must stay variant 0, since it
/// parses before any version announcement exists. Only variants beyond these three could ever be
/// version-gated.
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
pub enum SpecMsgSignal {
	/// Open the channel and announce the sender's initial protocol version. Variant 0.
	OpenChannel {
		/// The sender's initial protocol-version announcement.
		version: u8,
	},
	/// Sender-side half-close.
	CloseChannel,
	/// Raise the sender's version announcement mid-channel (announcements never decrease).
	Upgrade {
		/// The sender's new (higher) protocol-version announcement.
		version: u8,
	},
}

/// Advisory send-window credit a receiver grants a sender beyond the confirmed watermark. Not
/// enforced: registers are lossy and read late, and on an ordered stream the receiver cannot reject
/// without stalling. The hard bound is the sender STF's message-size constant. A grant may shrink
/// between publishes.
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
	Default,
	TypeInfo,
)]
pub struct WindowGrant {
	/// Max messages the sender may have in flight beyond the watermark.
	pub max_messages: u32,
	/// Max cumulative bytes in flight beyond the watermark.
	pub max_bytes: u64,
	/// Max size of a single message.
	pub max_message_size: u32,
}

/// The receiver's whole channel state as the sender sees it. Frozen core. Published as the
/// ack-stream's lossy, latest-wins leaf payload and read out-of-band by the sender via an inclusion
/// proof. The first publish is the channel acceptance.
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
	Default,
	TypeInfo,
)]
pub struct Register {
	/// The receiver's monotonic protocol-version announcement.
	pub version: u8,
	/// Cumulative confirmation watermark: consumed up to this position. Monotonic and idempotent,
	/// so lost or delayed confirmations are harmless; the latest supersedes.
	pub up_to: MessagePosition,
	/// Advisory send-window credit beyond `up_to`.
	pub grant: WindowGrant,
	/// Receiver-side close / rejection.
	pub closed: bool,
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Decode;

	#[test]
	fn signal_frozen_core_variant_indices() {
		// Frozen core: `OpenChannel` is variant 0. Renumbering is a consensus break.
		assert_eq!(SpecMsgSignal::OpenChannel { version: 7 }.encode(), vec![0x00, 0x07]);
		assert_eq!(SpecMsgSignal::CloseChannel.encode(), vec![0x01]);
		assert_eq!(SpecMsgSignal::Upgrade { version: 9 }.encode(), vec![0x02, 0x09]);
	}

	#[test]
	fn spec_msg_kind_variant_layout() {
		// The transport distinguishes only Signal (variant 0) vs Data (variant 1).
		assert_eq!(
			SpecMsgKind::Signal(SpecMsgSignal::OpenChannel { version: 1 }).encode(),
			vec![0x00, 0x00, 0x01],
		);
		// Data: variant 1 ++ compact(len) ++ bytes.
		assert_eq!(SpecMsgKind::Data(vec![0xAA]).encode(), vec![0x01, 0x04, 0xAA]);
	}

	#[test]
	fn register_frozen_layout() {
		// Frozen core: `Register` is read cross-chain. Pin its byte layout; any reorder or width
		// change is a consensus break.
		let r = Register {
			version: 2,
			up_to: MessagePosition(5),
			grant: WindowGrant { max_messages: 10, max_bytes: 1000, max_message_size: 100 },
			closed: true,
		};
		assert_eq!(
			r.encode(),
			vec![
				0x02, // version: u8
				0x05, 0, 0, 0, 0, 0, 0, 0, // up_to: u64 LE = 5
				0x0a, 0, 0, 0, // grant.max_messages: u32 LE = 10
				0xe8, 0x03, 0, 0, 0, 0, 0, 0, // grant.max_bytes: u64 LE = 1000
				0x64, 0, 0, 0,    // grant.max_message_size: u32 LE = 100
				0x01, // closed: bool = true
			],
		);
		assert_eq!(Register::decode(&mut &r.encode()[..]).unwrap(), r);
	}
}
