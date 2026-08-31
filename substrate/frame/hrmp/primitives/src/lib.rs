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

//! # HRMP control-plane shared primitives
//!
//! Types shared by the parachain-side HRMP pallet (`pallet-hrmp-para`) and the relay-chain-side
//! one (`pallet-hrmp-relay`), on the same terms as `registrar-primitives`: no FRAME, no XCM, no
//! network-specific dependency, so one version of the wire types serves every network and neither
//! pallet has to depend on the other.
//!
//! ## What actually moves
//!
//! HRMP is not a signed-extrinsic problem today — the user-facing calls are dispatched by the
//! parachain itself inside an XCM `Transact`. What forces the migration is the money: the channel
//! deposits are DOT held on the relay chain, in the paras' sovereign accounts. Those move to the
//! Coretime chain, and the intent follows them, so that the relay chain can end up accepting
//! system origins only.
//!
//! ## Who may ask
//!
//! Coretime accepts either the para itself, arriving as a `Transact` from the sibling chain, or
//! the para's registrar manager as a signed account. The para-origin path is what preserves
//! today's trust model: a parachain still speaks for itself, it just retargets its message from
//! the relay chain to Coretime. The manager path is the recovery route when a para cannot build
//! the message at all.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// A parachain id.
///
/// Byte-compatible with the relay chain's `Id`, which is a transparent `u32` newtype.
pub type ParaId = u32;

/// One end of a channel, in the order the relay chain names them.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Copy, Eq, PartialEq, Debug, TypeInfo,
	MaxEncodedLen, Ord, PartialOrd,
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
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToRelayV1 {
	/// Ask the relay chain to record an open-channel request.
	///
	/// The sender's deposit is already held on the parachain, so the relay chain takes nothing.
	/// Answered with [`MessageToParaV1::OpenResponse`].
	#[codec(index = 0)]
	InitOpenChannel {
		/// Which channel is being opened.
		channel: ChannelId,
		/// The parachain's id for this message, echoed back in the response.
		message_id: u64,
		/// How many messages the channel may hold at once.
		max_capacity: u32,
		/// The largest message the channel will carry.
		max_message_size: u32,
	},
	/// Ask the relay chain to confirm an open-channel request on the recipient's behalf.
	///
	/// Answered with [`MessageToParaV1::AcceptResponse`]. The channel itself only comes into
	/// existence at the relay chain's next session boundary, which is not something either side
	/// waits for: the deposits are settled by this answer, not by the channel opening.
	#[codec(index = 1)]
	AcceptOpenChannel {
		/// Which channel is being accepted.
		channel: ChannelId,
		/// The parachain's id for this message, echoed back in the response.
		message_id: u64,
	},
	/// Ask the relay chain to close an open channel.
	///
	/// Answered with [`MessageToParaV1::CloseResponse`], and only that answer releases the
	/// deposits: a close that is merely requested must not hand the money back, or a para gets
	/// its deposit while the channel still carries messages.
	#[codec(index = 2)]
	CloseChannel {
		/// Which channel is being closed.
		channel: ChannelId,
		/// The parachain's id for this message, echoed back in the response.
		message_id: u64,
		/// Which end asked. Either may close.
		initiator: ParaId,
	},
	/// Ask the relay chain to drop an open-channel request the recipient never confirmed.
	///
	/// Answered with [`MessageToParaV1::CancelResponse`].
	#[codec(index = 3)]
	CancelOpenRequest {
		/// Which request is being withdrawn.
		channel: ChannelId,
		/// The parachain's id for this message, echoed back in the response.
		message_id: u64,
	},
	/// Ask the relay chain to open a deposit-free channel in both directions.
	///
	/// Used for channels with or amongst system chains, including the one the Coretime chain
	/// opens with every para it registers. Answered with
	/// [`MessageToParaV1::SystemChannelResponse`].
	///
	/// No deposit is staked on the outcome, so the round trip is not about money — it is about the
	/// parachain not being able to tell "opened" from "refused" otherwise. It cannot see the relay
	/// chain's state, and the most common refusal is routine rather than exceptional: the chain
	/// that owns the registry asks for this channel the moment a registration is applied, while the
	/// new para is still onboarding and the relay chain will not yet open a channel to it.
	#[codec(index = 4)]
	EstablishSystemChannel {
		/// One end of the pair. Both directions are opened.
		channel: ChannelId,
		/// The parachain's id for this message, for tying the two chains' events together.
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
/// per channel, so the pair of para ids is enough. `message_id` echoes the request's id on top,
/// tying the two chains' events together.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum MessageToParaV1 {
	/// Report how an [`MessageToRelayV1::InitOpenChannel`] ended.
	///
	/// `Ok(())` means the relay chain is holding the request, so the sender's deposit is owed.
	#[codec(index = 0)]
	OpenResponse {
		/// The channel the report is about.
		channel: ChannelId,
		/// The id of the request this answers, echoed back.
		message_id: u64,
		/// Whether the request was recorded on the relay chain.
		outcome: Outcome,
	},
	/// Report how an [`MessageToRelayV1::AcceptOpenChannel`] ended.
	#[codec(index = 1)]
	AcceptResponse {
		/// The channel the report is about.
		channel: ChannelId,
		/// The id of the request this answers, echoed back.
		message_id: u64,
		/// Whether the acceptance was recorded on the relay chain.
		outcome: Outcome,
	},
	/// Report how a [`MessageToRelayV1::CloseChannel`] ended.
	///
	/// `Ok(())` means the channel is gone from the relay chain, so both deposits can be released.
	#[codec(index = 2)]
	CloseResponse {
		/// The channel the report is about.
		channel: ChannelId,
		/// The id of the request this answers, echoed back.
		message_id: u64,
		/// Whether the channel was closed.
		outcome: Outcome,
	},
	/// Report how a [`MessageToRelayV1::CancelOpenRequest`] ended.
	///
	/// `Ok(())` means the request is gone, so the sender's deposit can be released.
	#[codec(index = 3)]
	CancelResponse {
		/// The channel the report is about.
		channel: ChannelId,
		/// The id of the request this answers, echoed back.
		message_id: u64,
		/// Whether the request was dropped.
		outcome: Outcome,
	},
	/// Report how a [`MessageToRelayV1::EstablishSystemChannel`] ended, for **both** directions.
	///
	/// `Ok(())` means the relay chain has the pair, so both may be recorded open.
	/// [`FailureReason::AlreadyExists`] counts as success: it means the channel is there, which is
	/// the outcome that was asked for. Any other refusal leaves the pair unconfirmed for a retry —
	/// nothing is staked on it, so there is nothing to release.
	#[codec(index = 4)]
	SystemChannelResponse {
		/// One end of the pair the report is about. It covers both directions.
		channel: ChannelId,
		/// The id of the request this answers, echoed back.
		message_id: u64,
		/// Whether the relay chain has the pair.
		outcome: Outcome,
	},
}

/// How a request ended.
///
/// `Ok(())` means the relay chain applied it, `Err(reason)` that it did not. One outcome type for
/// every response in this protocol, the way a pallet has one `Error` enum rather than one per
/// extrinsic.
pub type Outcome = Result<(), FailureReason>;

/// Any dispatch error becomes [`FailureReason::Refused`].
///
/// Needed so the caller can run registry calls inside a storage layer, which requires the error
/// type to carry a `DispatchError`. Collapsing is the right answer anyway: this enum names the
/// outcomes a parachain can act on, and a diagnosis it cannot act on is just a refusal.
impl From<sp_runtime::DispatchError> for FailureReason {
	fn from(_: sp_runtime::DispatchError) -> Self {
		FailureReason::Refused
	}
}

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
	/// The relay chain's HRMP pallet refused for a reason this protocol does not name.
	///
	/// A catch-all rather than a mirror of the relay chain's error enum: the parachain acts on
	/// the outcome, not on the diagnosis, and mirroring would tie the wire format to another
	/// pallet's errors.
	#[codec(index = 5)]
	Refused,
}

/// The relay chain's HRMP channel registry, as `pallet-hrmp-relay` needs to see it.
///
/// Implemented by whichever pallet owns HRMP, which on a relay chain is
/// `polkadot-runtime-parachains`' `hrmp`. Lives here so neither side of the protocol depends on
/// the other, and stated in plain `u32` para ids for the same reason the messages are.
///
/// Every method here is deposit-free. The parachain holds the money now, so the relay chain must
/// record channels and requests with a zero deposit — otherwise a para would pay twice, and the
/// relay chain would try to reserve from a sovereign account the migration has emptied.
///
/// Implementations are **not** required to be atomic on failure. A real registry validates as it
/// goes and can write before it refuses, so the caller runs every method inside its own storage
/// layer — a refusal is reported to the parachain as "nothing happened", and a partial write that
/// survived one would leave the two chains disagreeing with no way to notice.
pub trait HrmpRegistry {
	/// Record an open-channel request, taking no deposit.
	fn init_open_channel(
		channel: ChannelId,
		max_capacity: u32,
		max_message_size: u32,
	) -> Result<(), FailureReason>;

	/// Confirm an open-channel request on the recipient's behalf, taking no deposit.
	fn accept_open_channel(channel: ChannelId) -> Result<(), FailureReason>;

	/// Close an open channel. `initiator` must be one of its two ends.
	fn close_channel(channel: ChannelId, initiator: ParaId) -> Result<(), FailureReason>;

	/// Drop an open-channel request that was never confirmed.
	fn cancel_open_request(channel: ChannelId) -> Result<(), FailureReason>;

	/// Open a deposit-free channel in both directions between two paras.
	fn establish_system_channel(channel: ChannelId) -> Result<(), FailureReason>;

	/// Whether the relay chain has a channel or a pending request for `channel`.
	fn exists(channel: ChannelId) -> bool;

	/// Arrange for `channel` to be openable, so the request paths can be benchmarked.
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_openable(channel: ChannelId);
}

/// Who manages a para on the chain that now holds its deposits.
///
/// Implemented by `pallet-registrar-para`. Lives here rather than in `registrar-primitives` so the
/// HRMP pallets do not have to depend on the registrar's wire types just to ask one question.
pub trait ParaManager {
	/// The account id a manager is identified by.
	type AccountId;

	/// The manager of `para_id`, if this chain knows the para at all.
	fn manager_of(para_id: ParaId) -> Option<Self::AccountId>;
}

impl ParaManager for () {
	type AccountId = sp_runtime::AccountId32;

	fn manager_of(_para_id: ParaId) -> Option<Self::AccountId> {
		None
	}
}

/// Told when a para finishes registering, so a channel can be opened with it.
///
/// The Coretime chain is the control plane for every para it registers, so it needs a route to
/// each one — and a para can only `Transact` into Coretime if a channel already exists, which is
/// what makes the para-origin HRMP path reachable at all.
///
/// Deliberately returns nothing. Registration has already succeeded by the time this runs, and a
/// channel that could fail it would make onboarding depend on HRMP capacity. Implementations
/// report their own failures and leave a way to retry.
pub trait OnParaRegistered {
	/// `para_id` has just been confirmed as registered.
	fn on_registered(para_id: ParaId);
}

impl OnParaRegistered for () {
	fn on_registered(_para_id: ParaId) {}
}

/// One channel, as it arrives from the chain that used to hold its deposits.
///
/// Carries no deposit, for the same reason [`MigratedPara`]-style records do not: the deposits are
/// re-taken here at this chain's prices, from the sovereign accounts the money already arrived on.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub struct MigratedChannel {
	/// Which channel.
	pub channel: ChannelId,
	/// Whether the relay chain has the channel itself, or only an unconfirmed request for it.
	///
	/// The difference decides how many deposits are owed: a request that the recipient has not
	/// accepted is the sender's alone.
	pub confirmed: bool,
}

/// Takes migrated channels into the pallet that will own them.
///
/// Same reasoning as `registrar-primitives`' equivalent: which deposits a channel holds is a
/// function of its state, that rule is enforced inside the pallet, and a migrator rebuilding it
/// from outside is how it gets broken.
pub trait ReceiveMigratedChannels {
	/// Take one channel, charging its deposits at this chain's prices.
	///
	/// Fails if the channel is already known here, or if a sovereign account cannot pay. Either
	/// way the caller is expected to park the record rather than lose it.
	fn receive_channel(channel: MigratedChannel) -> sp_runtime::DispatchResult;
}
