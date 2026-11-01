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

//! Consumer-side stream identifiers and per-direction channel state primitives.
//!
//! Distinct ID types per direction and kind ([`OutChannelId`], [`InChannelId`], [`BroadcastId`])
//! prevent an outbound channel ID from indexing inbound state, and let the runtime API and
//! inherent maps fix each entry's discipline by type.
//!
//! Helper convention: a bare [`StreamId`] is one of this chain's own streams (a key in its own
//! commitment tree and sender-side storage). A foreign stream is returned with its owner,
//! `(ParaId, StreamId)`.

use core::ops::RangeInclusive;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use scale_info::TypeInfo;

use crate::{
	flow_control::{Register, RegisterStatus},
	mmr::{MessagePosition, MmrFrontier},
	stream::StreamId,
};

/// A channel this chain sends on, named by its recipient.
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
	Hash,
	Debug,
	TypeInfo,
)]
pub struct OutChannelId {
	pub recipient: ParaId,
	pub domain: u8,
	pub num: u16,
}

impl OutChannelId {
	/// Our data stream: `Channel { recipient, domain, num }`.
	pub fn data_stream(&self) -> StreamId {
		StreamId::Channel { recipient: self.recipient, domain: self.domain, num: self.num }
	}

	/// The recipient's `Ack { us, domain, num }`, read for its register.
	pub fn peer_ack_stream(&self, us: ParaId) -> (ParaId, StreamId) {
		(self.recipient, StreamId::Ack { recipient: us, domain: self.domain, num: self.num })
	}
}

/// A channel this chain receives on, named by its sender.
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
	Hash,
	Debug,
	TypeInfo,
)]
pub struct InChannelId {
	pub sender: ParaId,
	pub domain: u8,
	pub num: u16,
}

impl InChannelId {
	/// Our `Ack { sender, domain, num }`, where we publish the register.
	pub fn ack_stream(&self) -> StreamId {
		StreamId::Ack { recipient: self.sender, domain: self.domain, num: self.num }
	}

	/// The sender's `Channel { us, domain, num }`, consumed in order.
	pub fn sender_data_stream(&self, us: ParaId) -> (ParaId, StreamId) {
		(self.sender, StreamId::Channel { recipient: us, domain: self.domain, num: self.num })
	}
}

/// A broadcast stream: its sender plus the discriminator. Broadcast has no recipient, so this is
/// the stream's global identity.
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
	Hash,
	Debug,
	TypeInfo,
)]
pub struct BroadcastId {
	pub sender: ParaId,
	pub domain: u16,
	pub subdomain: u8,
	pub num: u32,
}

impl BroadcastId {
	/// The sender's `Broadcast { domain, subdomain, num }`, with its owner.
	pub fn stream(&self) -> (ParaId, StreamId) {
		(
			self.sender,
			StreamId::Broadcast { domain: self.domain, subdomain: self.subdomain, num: self.num },
		)
	}
}

/// A session's negotiated parameters, fixed from `Accepting` to `Closed`.
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
pub struct SessionParams {
	pub version: u8,
	pub max_message_size: u32,
}

/// A register applied by the sender, with its position on the recipient's `Ack` stream (the ack
/// highwater).
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
pub struct AppliedRegister {
	pub position: MessagePosition,
	pub register: Register,
}

/// The session the sender took part in (sent `Hello` for), its parameters, and whether the sender
/// has applied its `Opened` register.
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
pub struct OpenSession {
	pub session: u32,
	pub params: SessionParams,
	pub opened: bool,
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
	Default,
	TypeInfo,
)]
pub struct OutChannelState {
	/// Own intent: set by `open()`, cleared by `close()`. Survives receiver-initiated closes so
	/// the sender reopens on the next `Accepting`.
	pub want_open: bool,
	/// Latest session number seen in `Accepting` or `Closed` (register validation).
	pub session: u32,
	/// Latest register applied, with its position on the recipient's `Ack` stream.
	pub register: Option<AppliedRegister>,
	/// The session we took part in (sent `Hello` for), its parameters, and whether we applied its
	/// `Opened`.
	pub open: Option<OpenSession>,
	/// The session we sent `CloseChannel` for: no `Data` in it any more, and no second
	/// `CloseChannel`.
	pub closed_session: Option<u32>,
}

impl OutChannelState {
	/// Whether we took part in (sent `Hello` for) session `s`.
	pub fn took_part_in(&self, s: u32) -> bool {
		self.open.is_some_and(|o| o.session == s)
	}

	/// Whether we already sent `CloseChannel` for session `s`.
	pub fn closed(&self, s: u32) -> bool {
		self.closed_session == Some(s)
	}

	/// The active session parameters if we may send `Data` right now: `open` is `Some(o)` with
	/// `o.opened`, we have not closed `o.session`, and `o.params.version` is in
	/// `supported_versions`.
	pub fn active_session(&self, supported_versions: RangeInclusive<u8>) -> Option<SessionParams> {
		let o = self.open?;
		if o.opened && !self.closed(o.session) && supported_versions.contains(&o.params.version) {
			Some(o.params)
		} else {
			None
		}
	}

	/// Cursor for reading the recipient's register in `consumed_streams()`, if watched
	/// (`want_open`): the applied register's position + 1, or 0 before the first read.
	pub fn register_cursor(&self) -> Option<MessagePosition> {
		if !self.want_open {
			return None;
		}
		match self.register {
			None => Some(MessagePosition(0)),
			Some(applied) => applied.position.0.checked_add(1).map(MessagePosition),
		}
	}
}

/// Receiver side, per inbound channel.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, PartialEq, Eq, Debug, TypeInfo,
)]
pub struct InChannelState {
	/// Our copy of the sender's channel stream: incoming messages are appended to it; its leaf
	/// count is the position of the next message to consume.
	pub frontier: MmrFrontier,
	/// Current session.
	pub session: u32,
	/// Current status.
	pub status: RegisterStatus,
	/// The register last published, verbatim (`None` before the first).
	pub published: Option<Register>,
	/// The session's parameters, set by the session's `Hello` and cleared when it closes (`Some`
	/// while `Opened` or `Draining`).
	pub open: Option<SessionParams>,
	/// Draining: `Closed` but still consuming toward `CloseChannel { session }`.
	pub draining: bool,
}

impl InChannelState {
	/// Whether the channel is in a watched phase (`Accepting`, `Opened`, or `Draining`).
	/// `consumed_streams()` lists it when this holds and its delivery queue has room.
	pub fn is_watched(&self) -> bool {
		matches!(self.status, RegisterStatus::Accepting { .. } | RegisterStatus::Opened) ||
			self.draining
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn typed_ids_map_to_own_and_foreign_streams() {
		let us = ParaId::from(1000);
		let peer = ParaId::from(2000);

		let out_id = OutChannelId { recipient: peer, domain: 1, num: 2 };
		assert_eq!(out_id.data_stream(), StreamId::Channel { recipient: peer, domain: 1, num: 2 });
		assert_eq!(
			out_id.peer_ack_stream(us),
			(peer, StreamId::Ack { recipient: us, domain: 1, num: 2 }),
		);

		let in_id = InChannelId { sender: peer, domain: 1, num: 2 };
		assert_eq!(in_id.ack_stream(), StreamId::Ack { recipient: peer, domain: 1, num: 2 });
		assert_eq!(
			in_id.sender_data_stream(us),
			(peer, StreamId::Channel { recipient: us, domain: 1, num: 2 }),
		);

		let bcast_id = BroadcastId { sender: peer, domain: 3, subdomain: 4, num: 5 };
		assert_eq!(
			bcast_id.stream(),
			(peer, StreamId::Broadcast { domain: 3, subdomain: 4, num: 5 }),
		);
	}

	#[test]
	fn out_channel_state_helpers_reflect_lifecycle_terms() {
		let params = SessionParams { version: 2, max_message_size: 1024 };
		let mut out = OutChannelState {
			want_open: true,
			session: 1,
			register: None,
			open: Some(OpenSession { session: 1, params, opened: false }),
			closed_session: None,
		};
		assert!(out.took_part_in(1));
		assert!(!out.took_part_in(2));
		assert_eq!(out.active_session(1..=3), None, "not opened yet");
		assert_eq!(out.register_cursor(), Some(MessagePosition(0)));

		out.open.as_mut().unwrap().opened = true;
		assert_eq!(out.active_session(1..=3), Some(params));
		assert_eq!(out.active_session(3..=4), None, "version dropped by upgrade");

		out.closed_session = Some(1);
		assert!(out.closed(1));
		assert_eq!(out.active_session(1..=3), None, "closed by us");

		out.register = Some(AppliedRegister {
			position: MessagePosition(4),
			register: Register { up_to: MessagePosition(0), status: RegisterStatus::Opened },
		});
		assert_eq!(out.register_cursor(), Some(MessagePosition(5)));
		out.want_open = false;
		assert_eq!(out.register_cursor(), None);
	}

	#[test]
	fn in_channel_watching_rule() {
		let mut st = InChannelState {
			frontier: MmrFrontier::new(),
			session: 1,
			status: RegisterStatus::Accepting {
				session: 1,
				min: 1,
				max: 1,
				max_message_size: 1024,
			},
			published: None,
			open: None,
			draining: false,
		};
		assert!(st.is_watched());
		st.status = RegisterStatus::Opened;
		assert!(st.is_watched());
		st.status = RegisterStatus::Closed { session: 1 };
		st.draining = true;
		assert!(st.is_watched());
		st.draining = false;
		assert!(!st.is_watched());
	}

	#[test]
	fn id_ordering_is_by_peer_then_discriminator() {
		let a = OutChannelId { recipient: ParaId::from(1), domain: 1, num: 9 };
		let b = OutChannelId { recipient: ParaId::from(2), domain: 0, num: 0 };
		let c = OutChannelId { recipient: ParaId::from(2), domain: 0, num: 1 };
		assert!(a < b && b < c);
	}
}
