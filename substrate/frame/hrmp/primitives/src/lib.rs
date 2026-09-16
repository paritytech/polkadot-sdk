// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! # HRMP-channel shared primitives
//!
//! Types shared by the parachain pallet (`pallet-hrmp-para`) and relay-chain pallet
//! (`pallet-hrmp-relay`). This crate is deliberately free of any FRAME, XCM, or network-specific
//! dependency, so a single version of the wire types serves Westend, Kusama and Polkadot, and so
//! both pallets can depend on it without forming a dependency cycle. Parachains driving their
//! own channels depend on it too, for [`ParaRequest`].

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// A parachain id.
///
/// Byte-compatible with the relay chain's `Id`, which is a transparent `u32` newtype.
pub type ParaId = u32;

/// The highest id that belongs to the system.
///
/// Mirrors `polkadot_parachain_primitives`' `SYSTEM_INDEX_END`, which this crate does not depend
/// on. Both ends of the protocol must agree on which paras pay no deposit.
const SYSTEM_INDEX_END: ParaId = 1999;

/// Whether `para_id` belongs to the system, as the relay chain's `IsSystem` decides it.
pub fn is_system(para_id: ParaId) -> bool {
	para_id <= SYSTEM_INDEX_END
}

/// One end of a channel, in the order the relay chain names them.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Copy,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Debug,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct ChannelId {
	/// The para that sends on this channel.
	pub sender: ParaId,
	/// The para that receives on this channel.
	pub recipient: ParaId,
}

impl ChannelId {
	/// Whether `para_id` is one of the two ends.
	pub fn is_participant(&self, para_id: ParaId) -> bool {
		self.sender == para_id || self.recipient == para_id
	}

	/// Whether either end belongs to the system, which is what makes a channel deposit-free.
	pub fn is_system(&self) -> bool {
		is_system(self.sender) || is_system(self.recipient)
	}

	pub fn reversed(&self) -> Self {
		Self { sender: self.recipient, recipient: self.sender }
	}
}

/// A parachain's own HRMP request, forwarded by the relay chain to the parachain that runs
/// `pallet-hrmp-para`.
///
/// The route a para uses when it cannot reach the control-plane parachain itself, which is every
/// para that has no channel to it yet. The asking para is not in the payload: the relay chain
/// authenticated it from the origin and passes it alongside.
///
/// The variant's `#[codec(index)]` is the on-wire version tag.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum ParaRequest {
	/// Version 1 of the forwarded parachain requests.
	#[codec(index = 0)]
	V1(ParaRequestV1),
}

/// Version 1 payloads for [`ParaRequest`].
///
/// One variant per call a para may make on its own behalf, carrying what the matching
/// `pallet-hrmp-para` extrinsic takes.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum ParaRequestV1 {
	/// Open a channel from the asking para to `recipient`.
	#[codec(index = 0)]
	InitOpenChannel {
		/// The other end of the channel.
		recipient: ParaId,
		/// How many messages the channel may hold at once.
		proposed_max_capacity: u32,
		/// The largest message the channel will carry.
		proposed_max_message_size: u32,
	},
	/// Accept a channel `sender` asked to open to the asking para.
	#[codec(index = 1)]
	AcceptOpenChannel {
		/// The para that asked.
		sender: ParaId,
	},
	/// Close a channel the asking para is one end of.
	#[codec(index = 2)]
	CloseChannel {
		/// The channel to close.
		channel: ChannelId,
	},
	/// Withdraw an open request the recipient has not accepted.
	#[codec(index = 3)]
	CancelOpenRequest {
		/// The channel the request is for.
		channel: ChannelId,
		/// The asking para's count of open requests, checked against storage.
		open_requests: u32,
	},
	/// Open both deposit-free directions between the asking para and a system chain.
	#[codec(index = 4)]
	EstablishChannelWithSystem {
		/// The system chain to open with.
		target_system_chain: ParaId,
	},
}

/// What a parachain is told about a channel it is one end of.
///
/// The first three are delivered as the XCM `HrmpNewChannelOpenRequest`, `HrmpChannelAccepted`
/// and `HrmpChannelClosing` instructions, whose fields they mirror. The last two conclude a
/// request and have no instruction of their own, so how they reach a para is up to the transport.
/// Versioned by the message carrying it, as [`ChannelId`] and [`FailureReason`] are.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum ParaNotification {
	/// A para asked to open a channel to the para being told.
	#[codec(index = 0)]
	NewChannelOpenRequest {
		/// The para that asked.
		sender: ParaId,
		/// The largest message the channel will carry.
		max_message_size: u32,
		/// How many messages the channel may hold at once.
		max_capacity: u32,
	},
	/// The channel the para being told asked for was accepted.
	#[codec(index = 1)]
	ChannelAccepted {
		/// The para that accepted.
		recipient: ParaId,
	},
	/// The other end of an open channel decided to close it.
	#[codec(index = 2)]
	ChannelClosing {
		/// Which end asked.
		initiator: ParaId,
		/// The para that sends on the channel.
		sender: ParaId,
		/// The para that receives on the channel.
		recipient: ParaId,
	},
	/// The channel both ends agreed on is open.
	#[codec(index = 3)]
	ChannelOpened {
		/// The channel.
		channel: ChannelId,
	},
	/// The channel both ends agreed on was refused, and their deposits are being released.
	#[codec(index = 4)]
	ChannelOpenFailure {
		/// The channel.
		channel: ChannelId,
		/// Why the relay chain refused.
		reason: FailureReason,
	},
}

/// HRMP control-plane messages sent to the relay chain.
///
/// The variant's `#[codec(index)]` is the on-wire version tag.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum MessageToRelay {
	/// Version 1 of the HRMP control-plane messages to the relay chain.
	#[codec(index = 0)]
	V1(MessageToRelayV1),
}

/// Version 1 payloads for [`MessageToRelay`].
///
/// Every variant that expects an answer carries `message_id`, the parachain's id for the request,
/// echoed back in the response so the two chains' events tie together.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum MessageToRelayV1 {
	/// Open a channel both ends have agreed to. Their deposits are already held on the parachain.
	#[codec(index = 0)]
	OpenChannel {
		/// Which channel is being opened.
		channel: ChannelId,
		/// The parachain's id for this message.
		message_id: u64,
		/// How many messages the channel may hold at once.
		max_capacity: u32,
		/// The largest message the channel will carry.
		max_message_size: u32,
	},
	/// Open a channel the recipient never agreed to.
	#[codec(index = 1)]
	ForceOpenChannel {
		/// Which channel is being opened.
		channel: ChannelId,
		/// The parachain's id for this message.
		message_id: u64,
		/// How many messages the channel may hold at once.
		max_capacity: u32,
		/// The largest message the channel will carry.
		max_message_size: u32,
	},
	/// Open one deposit-free direction between two system chains, at the relay's configured sizes.
	#[codec(index = 2)]
	OpenSystemChannel {
		/// Which direction is being opened.
		channel: ChannelId,
		/// The parachain's id for this message.
		message_id: u64,
	},
	/// Open both deposit-free directions between a para and a system chain.
	#[codec(index = 3)]
	OpenSystemPair {
		/// One direction of the pair; the other is its reverse.
		channel: ChannelId,
		/// The parachain's id for this message.
		message_id: u64,
		/// How many messages each direction may hold at once.
		max_capacity: u32,
		/// The largest message each direction will carry.
		max_message_size: u32,
	},
	/// Close an open channel. Both deposits are already released on the parachain.
	#[codec(index = 4)]
	CloseChannel {
		/// Which channel is being closed.
		channel: ChannelId,
		/// The parachain's id for this message.
		message_id: u64,
		/// Which end asked. Either may close.
		initiator: ParaId,
	},
	/// Drop every channel belonging to a para.
	#[codec(index = 5)]
	ForceClean {
		/// The para whose channels are being dropped.
		para_id: ParaId,
		/// The parachain's id for this message.
		message_id: u64,
	},
	/// Deliver a channel notification to a para. Nothing is answered.
	///
	/// The parachain has no channel to most paras, so what it has to tell them goes out through
	/// the relay chain, which reaches every para.
	#[codec(index = 6)]
	NotifyPara {
		/// The para to tell.
		para_id: ParaId,
		/// What it is told.
		notification: ParaNotification,
	},
}

/// HRMP report messages sent back to the parachain.
///
/// The variant's `#[codec(index)]` is the on-wire version tag.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum MessageToPara {
	/// Version 1 of the HRMP report messages to the parachain.
	#[codec(index = 0)]
	V1(MessageToParaV1),
}

/// Version 1 payloads for [`MessageToPara`].
///
/// `channel` correlates a response with its request: a parachain only has one request in flight
/// per channel, so the pair of para ids is enough. `message_id` echoes the request's id on top.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum MessageToParaV1 {
	/// Answer any of the four open requests; the parachain knows which it sent from its own state.
	#[codec(index = 0)]
	OpenChannelResponse {
		/// The channel the report is about. For a pair, both directions share one answer.
		channel: ChannelId,
		/// The id of the request this answers.
		message_id: u64,
		/// The `(max_capacity, max_message_size)` opened, or why it was refused.
		outcome: Result<(u32, u32), FailureReason>,
	},
	/// Answer a [`MessageToRelayV1::CloseChannel`].
	#[codec(index = 1)]
	CloseResponse {
		/// The channel the report is about.
		channel: ChannelId,
		/// The id of the request this answers.
		message_id: u64,
		/// Whether the channel was closed.
		outcome: Outcome,
	},
}

/// How a request ended.
///
/// One outcome type for every response in this protocol, the way a pallet has one `Error` enum
/// rather than one per extrinsic.
pub type Outcome = Result<(), FailureReason>;

/// Why the relay chain refused a request.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum FailureReason {
	/// One of the two paras is not one the relay chain will open a channel for.
	#[codec(index = 0)]
	InvalidPara,
	/// The requested capacity or message size is outside the relay chain's configured limits.
	#[codec(index = 1)]
	InvalidParameters,
	/// A request for this channel is already recorded, or the channel already exists.
	#[codec(index = 2)]
	AlreadyExists,
	/// The para has as many channels or pending requests as the relay chain allows.
	#[codec(index = 3)]
	LimitExceeded,
	/// There is no request or channel here to act on.
	#[codec(index = 4)]
	NotFound,
	/// Refused for a reason this protocol does not name.
	#[codec(index = 5)]
	Refused,
}

/// Any dispatch error becomes [`FailureReason::Refused`], so registry calls can run inside a
/// storage layer.
impl From<sp_runtime::DispatchError> for FailureReason {
	fn from(_: sp_runtime::DispatchError) -> Self {
		FailureReason::Refused
	}
}

/// The relay chain's HRMP channel registry, as `pallet-hrmp-relay` needs to see it.
///
/// Implemented by whichever pallet owns HRMP, which on a relay chain is
/// `polkadot-runtime-parachains`' `hrmp`. Lives here so neither side of the protocol depends on
/// the other.
///
/// Implementations are not required to be atomic on failure, so the caller runs every method
/// inside its own storage layer.
pub trait HrmpRegistry {
	/// Open a channel, forced or agreed. Both cases are the same here.
	fn open_channel(
		channel: ChannelId,
		max_capacity: u32,
		max_message_size: u32,
	) -> Result<(), FailureReason>;

	/// Open one direction between two system chains, returning the sizes it used.
	fn open_system_channel(channel: ChannelId) -> Result<(u32, u32), FailureReason>;

	/// Open both `channel` and its reverse, rolling both back if either is refused.
	fn open_system_pair(
		channel: ChannelId,
		max_capacity: u32,
		max_message_size: u32,
	) -> Result<(), FailureReason>;

	/// Close an open channel. `initiator` must be one of its two ends.
	fn close_channel(channel: ChannelId, initiator: ParaId) -> Result<(), FailureReason>;

	/// Drop every channel belonging to `para_id`.
	fn force_clean(para_id: ParaId) -> Result<(), FailureReason>;

	/// Whether there is a channel or a pending request for `channel`.
	fn exists(channel: ChannelId) -> bool;

	/// Arrange for `channel` to be openable, so the request paths can be benchmarked.
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_openable(channel: ChannelId);
}
