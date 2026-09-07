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
//! both pallets can depend on it without forming a dependency cycle.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// A parachain id.
///
/// Byte-compatible with the relay chain's `Id`, which is a transparent `u32` newtype.
pub type ParaId = u32;

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
}

/// HRMP control-plane messages sent to the relay chain.
///
/// The variant's `#[codec(index)]` is the on-wire version tag.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToRelay {
	/// Version 1 of the HRMP control-plane messages to the relay chain.
	#[codec(index = 0)]
	V1(MessageToRelayV1),
}

/// Version 1 payloads for [`MessageToRelay`].
///
/// Every variant carries `message_id`, the parachain's id for the request, echoed back in the
/// response so the two chains' events tie together.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToRelayV1 {
	/// Record an open-channel request. The sender's deposit is already held on the parachain.
	#[codec(index = 0)]
	InitOpenChannel {
		/// Which channel is being opened.
		channel: ChannelId,
		/// The parachain's id for this message.
		message_id: u64,
		/// How many messages the channel may hold at once.
		max_capacity: u32,
		/// The largest message the channel will carry.
		max_message_size: u32,
	},
	/// Confirm an open-channel request on the recipient's behalf.
	#[codec(index = 1)]
	AcceptOpenChannel {
		/// Which channel is being accepted.
		channel: ChannelId,
		/// The parachain's id for this message.
		message_id: u64,
	},
	/// Close an open channel.
	#[codec(index = 2)]
	CloseChannel {
		/// Which channel is being closed.
		channel: ChannelId,
		/// The parachain's id for this message.
		message_id: u64,
		/// Which end asked. Either may close.
		initiator: ParaId,
	},
	/// Drop an open-channel request the recipient never confirmed.
	#[codec(index = 3)]
	CancelOpenRequest {
		/// Which request is being withdrawn.
		channel: ChannelId,
		/// The parachain's id for this message.
		message_id: u64,
	},
	/// Open a deposit-free channel in both directions between two paras, one of them a system
	/// chain.
	#[codec(index = 4)]
	EstablishSystemChannel {
		/// One end of the pair. Both directions are opened.
		channel: ChannelId,
		/// The parachain's id for this message.
		message_id: u64,
	},
	/// Open a channel without the recipient's consent.
	#[codec(index = 5)]
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
	/// Drop every channel and request belonging to a para.
	#[codec(index = 6)]
	ForceClean {
		/// The para whose channels are being dropped.
		para_id: ParaId,
		/// The parachain's id for this message.
		message_id: u64,
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
	/// Answer an [`MessageToRelayV1::InitOpenChannel`].
	#[codec(index = 0)]
	OpenResponse {
		/// The channel the report is about.
		channel: ChannelId,
		/// The id of the request this answers.
		message_id: u64,
		/// Whether the request was recorded.
		outcome: Outcome,
	},
	/// Answer an [`MessageToRelayV1::AcceptOpenChannel`].
	#[codec(index = 1)]
	AcceptResponse {
		/// The channel the report is about.
		channel: ChannelId,
		/// The id of the request this answers.
		message_id: u64,
		/// Whether the acceptance was recorded.
		outcome: Outcome,
	},
	/// Answer a [`MessageToRelayV1::CloseChannel`]. `Ok(())` releases both deposits.
	#[codec(index = 2)]
	CloseResponse {
		/// The channel the report is about.
		channel: ChannelId,
		/// The id of the request this answers.
		message_id: u64,
		/// Whether the channel was closed.
		outcome: Outcome,
	},
	/// Answer a [`MessageToRelayV1::CancelOpenRequest`]. `Ok(())` releases the sender's deposit.
	#[codec(index = 3)]
	CancelResponse {
		/// The channel the report is about.
		channel: ChannelId,
		/// The id of the request this answers.
		message_id: u64,
		/// Whether the request was dropped.
		outcome: Outcome,
	},
	/// Answer an [`MessageToRelayV1::EstablishSystemChannel`].
	#[codec(index = 4)]
	SystemChannelResponse {
		/// The channel the report is about. Both directions share one answer.
		channel: ChannelId,
		/// The id of the request this answers.
		message_id: u64,
		/// Whether both directions were opened, and at what sizes.
		outcome: Result<(u32, u32), FailureReason>,
	},
	/// Answer a [`MessageToRelayV1::ForceOpenChannel`].
	#[codec(index = 5)]
	ForceOpenResponse {
		/// The channel the report is about.
		channel: ChannelId,
		/// The id of the request this answers.
		message_id: u64,
		/// Whether the channel was opened.
		outcome: Outcome,
	},
	/// Answer a [`MessageToRelayV1::ForceClean`].
	#[codec(index = 6)]
	ForceCleanResponse {
		/// The para the report is about.
		para_id: ParaId,
		/// The id of the request this answers.
		message_id: u64,
		/// Whether the para's channels were dropped.
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
	///
	/// On a close or cancel this is the end state the request asks for, so the parachain must
	/// still settle and release deposits on it. It is also the only signal a parachain gets that
	/// a counterparty was offboarded.
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
/// Every method is deposit-free: the parachain holds the money now, so the relay chain must
/// record channels and requests with a zero deposit.
///
/// Implementations are not required to be atomic on failure, so the caller runs every method
/// inside its own storage layer.
pub trait HrmpRegistry {
	/// Record an open-channel request.
	fn init_open_channel(
		channel: ChannelId,
		max_capacity: u32,
		max_message_size: u32,
	) -> Result<(), FailureReason>;

	/// Confirm an open-channel request on the recipient's behalf.
	fn accept_open_channel(channel: ChannelId) -> Result<(), FailureReason>;

	/// Close an open channel. `initiator` must be one of its two ends.
	fn close_channel(channel: ChannelId, initiator: ParaId) -> Result<(), FailureReason>;

	/// Drop an open-channel request that was never confirmed.
	fn cancel_open_request(channel: ChannelId) -> Result<(), FailureReason>;

	/// Open a channel in both directions, at the sizes configured for system chains.
	///
	/// Returns the `(max_capacity, max_message_size)` it used.
	fn establish_system_channel(channel: ChannelId) -> Result<(u32, u32), FailureReason>;

	/// Open a channel without the recipient's consent.
	fn force_open_channel(
		channel: ChannelId,
		max_capacity: u32,
		max_message_size: u32,
	) -> Result<(), FailureReason>;

	/// Drop every channel and request belonging to `para_id`.
	fn force_clean(para_id: ParaId) -> Result<(), FailureReason>;

	/// Whether there is a channel or a pending request for `channel`.
	fn exists(channel: ChannelId) -> bool;

	/// Arrange for `channel` to be openable, so the request paths can be benchmarked.
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_openable(channel: ChannelId);
}
