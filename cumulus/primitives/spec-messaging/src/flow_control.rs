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

//! Channel leaf and register formats: the sender's channel-stream leaf payload
//! ([`ChannelMessage`]) and the receiver's out-of-band confirmation [`Register`] with its
//! [`RegisterStatus`]. The pallet owns the lifecycle and flow-control logic.
//!
//! # Consensus surface
//!
//! - [`ChannelMessage`] is the SCALE payload of every channel-stream MMR leaf, inside
//! [`crate::message::leaf_hash`]'s `LEAF_TAG ++ LEAF_VERSION ++ payload` framing.
//! - [`Register`] is the SCALE payload of every ack-stream leaf: the receiver's complete voice in
//! the channel, lossy and latest-wins, read out-of-band by the sender.
//!
//! # Frozen core
//!
//! [`ChannelMessage::Hello`] (index 0), [`ChannelMessage::CloseChannel`] (index 1) and the
//! [`Register`] / [`RegisterStatus`] layout parse at every channel version. Variants beyond the
//! frozen core (such as [`ChannelMessage::Data`] at index 2) are interpreted under the session's
//! negotiated channel version. The tests pin their byte layouts.

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

use crate::mmr::MessagePosition;

/// The payload of every channel-stream MMR leaf: session signalling or userspace data.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, TypeInfo)]
pub enum ChannelMessage {
	/// Frozen core (index 0). Opens session `session` at `version`, chosen from the range the
	/// receiver advertised in [`RegisterStatus::Accepting`].
	#[codec(index = 0)]
	Hello { session: u32, version: u8 },
	/// Frozen core (index 1). Last leaf of a session the sender took part in; nothing further
	/// until a new session opens.
	#[codec(index = 1)]
	CloseChannel { session: u32 },
	/// Channel version 1 (index 2). Userspace payload bytes, delivered in order. Bounds apply to
	/// the encoded [`ChannelMessage`] leaf payload; the variant byte and compact length take up to
	/// 5 bytes.
	#[codec(index = 2)]
	Data(Vec<u8>),
}

/// The receiver's complete voice in a channel, published as a leaf on its `Ack` stream.
/// Latest-wins: every register is self-contained.
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
pub struct Register {
	/// Cumulative watermark: all positions `< up_to` of the channel's data stream (data and
	/// signals alike) have been consumed.
	pub up_to: MessagePosition,
	/// Current session status.
	pub status: RegisterStatus,
}

/// Receiver-side session status carried in a [`Register`].
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
pub enum RegisterStatus {
	/// Session `session` accepted; the sender may open it with a [`ChannelMessage::Hello`] at a
	/// version in `min..=max` and encoded leaf size at most `max_message_size`.
	#[codec(index = 0)]
	Accepting { session: u32, min: u8, max: u8, max_message_size: u32 },
	/// The receiver consumed the session's `Hello`; data may flow.
	#[codec(index = 1)]
	Opened,
	/// Session `session` is closing or closed; `up_to` reports the drain's progress.
	#[codec(index = 2)]
	Closed { session: u32 },
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Decode;

	#[test]
	fn channel_message_frozen_core_variant_indices() {
		assert_eq!(
			ChannelMessage::Hello { session: 3, version: 7 }.encode(),
			vec![0x00, 0x03, 0, 0, 0, 0x07],
		);
		assert_eq!(ChannelMessage::CloseChannel { session: 5 }.encode(), vec![0x01, 0x05, 0, 0, 0]);
		assert_eq!(ChannelMessage::Data(vec![0xAA]).encode(), vec![0x02, 0x04, 0xAA]);
	}

	#[test]
	fn data_at_the_protocol_bound_takes_five_bytes_of_framing() {
		// The bound is on the encoded leaf. At this size the compact length takes 4 bytes, so 5
		// bytes less data fill it exactly and one more overflows it.
		let max = crate::message::MAX_SPECULATIVE_MESSAGE_LEN as usize;
		assert_eq!(ChannelMessage::Data(vec![0; max - 5]).encoded_size(), max);
		assert_eq!(ChannelMessage::Data(vec![0; max - 4]).encoded_size(), max + 1);
	}

	#[test]
	fn register_frozen_layout() {
		let accepting = Register {
			up_to: MessagePosition(5),
			status: RegisterStatus::Accepting {
				session: 1,
				min: 1,
				max: 3,
				max_message_size: 1024,
			},
		};
		assert_eq!(
			accepting.encode(),
			vec![
				0x05, 0, 0, 0, 0, 0, 0, 0,    // up_to: u64 LE = 5
				0x00, // status: Accepting (index 0)
				0x01, 0, 0, 0,    // session: u32 LE = 1
				0x01, // min: u8 = 1
				0x03, // max: u8 = 3
				0x00, 0x04, 0, 0, // max_message_size: u32 LE = 1024
			],
		);
		assert_eq!(Register::decode(&mut &accepting.encode()[..]).unwrap(), accepting);

		let opened = Register { up_to: MessagePosition(9), status: RegisterStatus::Opened };
		assert_eq!(opened.encode(), vec![0x09, 0, 0, 0, 0, 0, 0, 0, 0x01]);
		assert_eq!(Register::decode(&mut &opened.encode()[..]).unwrap(), opened);

		let closed =
			Register { up_to: MessagePosition(12), status: RegisterStatus::Closed { session: 2 } };
		assert_eq!(closed.encode(), vec![0x0c, 0, 0, 0, 0, 0, 0, 0, 0x02, 0x02, 0, 0, 0]);
		assert_eq!(Register::decode(&mut &closed.encode()[..]).unwrap(), closed);
	}
}
