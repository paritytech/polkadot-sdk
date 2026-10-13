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
pub type ParaId = u32;

/// A balance, as the relay chain counts it.
pub type Balance = u128;

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

/// HRMP control-plane messages sent to the relay chain.
///
/// The variant's `#[codec(index)]` is the on-wire version tag.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToRelay {
	/// Version 1 of the HRMP control-plane messages to the relay chain.
	#[codec(index = 0)]
	V1(MessageToRelayV1),
}

/// Version 1 payloads for [`MessageToRelay`].
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToRelayV1 {
	#[codec(index = 0)]
	TODO,
}

/// HRMP report messages sent back to the parachain.
///
/// The variant's `#[codec(index)]` is the on-wire version tag.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToPara {
	/// Version 1 of the HRMP report messages to the parachain.
	#[codec(index = 0)]
	V1(MessageToParaV1),
}

/// Version 1 payloads for [`MessageToPara`].
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToParaV1 {
	#[codec(index = 0)]
	TODO,
}

/// One channel, as it arrives at the destination from the chain that used to hold its deposits.
///
/// Carries no deposit. [`ReceiveMigratedChannels::receive_channel`] takes the sender's, and for a
/// confirmed channel the recipient's, deposit at the destination's own prices.
///
/// Note: We recreate even if there is not enough fund to pay for the deposit so no RC channel
/// is dropped.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub struct MigratedChannel {
	/// Which channel.
	pub channel: ChannelId,
	/// Whether the source chain has the channel itself, or only an open request the recipient
	/// has not accepted.
	pub confirmed: bool,
}

/// Takes migrated channels into the pallet that owns HRMP on the destination.
pub trait ReceiveMigratedChannels {
	/// Take one channel, charging its deposits at this chain's prices.
	fn receive_channel(channel: MigratedChannel) -> sp_runtime::DispatchResult;
}

impl ReceiveMigratedChannels for () {
	fn receive_channel(_: MigratedChannel) -> sp_runtime::DispatchResult {
		Err(sp_runtime::DispatchError::Unavailable)
	}
}

/// The channel call a deposit request is made for.
///
/// Sent with the request in [`ChannelDepositProvider::try_set_deposit`] and returned with its
/// outcome in [`OnDepositSet::on_deposit_set`], so the pallet that owns the channels knows which
/// call to finish. A provider carries it along without looking at it.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum DepositAction {
	/// A request to open a channel, recorded once the sender's deposit is taken.
	InitOpenChannel { max_capacity: u32, max_message_size: u32, max_total_size: u32 },
	/// The recipient accepting an open channel request.
	AcceptOpenChannel,
	/// Bringing the deposits of an open channel in line with the current prices.
	PokeChannelDeposits,
	/// Dropping an open channel request.
	CancelOpenRequest,
	/// Closing a channel.
	CloseChannel,
}

/// The deposit each side of a channel should hold once a request is done.
///
/// The relay chain's HRMP pallet does not hold channel deposits itself. It asks its
/// `DepositProvider`, a [`ChannelDepositProvider`], to set them. The provider may hold the funds
/// locally or on another chain, where they are settled later. This type carries the request in
/// [`ChannelDepositProvider::try_set_deposit`] and comes back in
/// [`OnDepositSet::on_deposit_set`] once the deposits are set.
///
/// A side set to `Some(amount)` must end up holding exactly `amount`, so the provider reserves or
/// releases the difference from what that side holds now. A side set to `None` is not touched,
/// e.g. only the sender's side is set when a channel is requested.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Copy,
	Eq,
	PartialEq,
	Debug,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct DepositTargets {
	/// Target deposit of the channel's sender, or `None` to leave it unchanged.
	pub sender: Option<Balance>,
	/// Target deposit of the channel's recipient, or `None` to leave it unchanged.
	pub recipient: Option<Balance>,
}

/// Reserves and releases HRMP channel deposits on behalf of the pallet that owns the channels.
///
/// Implemented by whoever holds the funds: the owning pallet itself when they are on the relay
/// chain, or a pallet that forwards the request to another chain. Setting a deposit may complete
/// later, so the outcome is reported back through [`Self::OnDepositSet`] rather than returned
/// here.
pub trait ChannelDepositProvider {
	/// The callback invoked once the deposits of an accepted request are set.
	type OnDepositSet: OnDepositSet;

	/// Requests that every side of `channel_id` named in `targets` holds its amount, for `action`.
	/// Sides left as `None` are not touched.
	///
	/// `Ok` only means the request was accepted, not that the deposits are set. The provider
	/// calls [`OnDepositSet::on_deposit_set`] once they are, and until then another request
	/// touching the same side of `channel_id` must be refused.
	fn try_set_deposit(
		channel_id: ChannelId,
		targets: DepositTargets,
		action: DepositAction,
	) -> sp_runtime::DispatchResult;
}

/// Callback through which a [`ChannelDepositProvider`] reports the outcome of a deposit request.
///
/// Implemented by the pallet that issued the request via
/// [`ChannelDepositProvider::try_set_deposit`]. The provider calls it once the deposits are
/// actually reserved or released, which may be in a later block or on another chain. The
/// implementor then resumes the `action` the request was made for, e.g. opening, accepting or
/// closing the channel.
pub trait OnDepositSet {
	/// Called once every side of `channel_id` named in `targets` holds its amount. Sides left as
	/// `None` were not touched.
	///
	/// `action` is the action passed to [`ChannelDepositProvider::try_set_deposit`] and tells the
	/// implementor which pending call to finish. Returns the weight spent finishing it.
	fn on_deposit_set(
		channel_id: ChannelId,
		targets: DepositTargets,
		action: DepositAction,
	) -> sp_runtime::Weight;
}
