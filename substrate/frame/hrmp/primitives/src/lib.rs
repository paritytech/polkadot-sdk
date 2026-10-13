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
//! (`pallet-hrmp-relay`), and the [`ChannelDepositProvider`] interface through which the relay
//! chain's `hrmp` pallet has its channel deposits held. This crate is deliberately free of any
//! FRAME, XCM, or network-specific dependency, so a single version of the wire types serves
//! Westend, Kusama and Polkadot, and so both pallets can depend on it without forming a
//! dependency cycle.

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

/// Reserves and releases HRMP channel deposits on behalf of the pallet that owns the channels.
///
/// Implemented by whoever holds the funds: the owning pallet itself when they are on the relay
/// chain, or a pallet that forwards the request to another chain. The latter cannot set the
/// deposits within the call, so it accepts the request and reports the outcome through
/// [`Self::OnDepositSet`] later; until then it must refuse another request touching the same
/// side of the channel.
pub trait ChannelDepositProvider {
	/// Where the provider reports a pending request once its deposits are set.
	type OnDepositSet: OnDepositSet;

	/// Requests that every side of `channel_id` named in `changes` ends up holding its target.
	/// `action` is handed back in [`OnDepositSet::on_deposit_set`] if the request is pending.
	fn try_set_deposit(
		channel_id: ChannelId,
		changes: DepositChanges,
		action: DepositAction,
	) -> Result<DepositStatus, sp_runtime::DispatchError>;
}

/// Callback through which a [`ChannelDepositProvider`] reports that the deposits of a pending
/// request are set.
///
/// Implemented by the pallet that owns the channels, which then completes the call that asked
/// for the change.
pub trait OnDepositSet {
	/// Called once every side of `channel_id` named in `changes` holds its target. `action` is
	/// the one passed to [`ChannelDepositProvider::try_set_deposit`]. Returns the weight spent.
	fn on_deposit_set(
		channel_id: ChannelId,
		changes: DepositChanges,
		action: DepositAction,
	) -> sp_runtime::Weight;
}

/// Whether a deposit request is done when [`ChannelDepositProvider::try_set_deposit`] returns.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum DepositStatus {
	/// The deposits are set. Nothing will be reported.
	Set,
	/// The request is accepted and its outcome is reported through [`OnDepositSet`].
	Pending,
}

/// The deposit changes a call asks for, one per side of the channel. `None` leaves that side
/// untouched, e.g. only the sender's side changes when a channel is requested.
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
pub struct DepositChanges {
	pub sender: Option<DepositChange>,
	pub recipient: Option<DepositChange>,
}

/// A change to the deposit one side of a channel holds.
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
pub struct DepositChange {
	/// What the side holds now, by the records of the pallet that owns the channels.
	pub held: Balance,
	/// What the side must hold once the change is done.
	pub target: Balance,
}

/// Which HRMP call asked for a deposit change.
///
/// Passed to [`ChannelDepositProvider::try_set_deposit`] and handed back unchanged in
/// [`OnDepositSet::on_deposit_set`], so the pallet that owns the channels knows which call to
/// complete. A provider never inspects it.
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
pub enum DepositAction {
	/// A new open channel request, carrying the limits it is recorded with.
	InitOpenChannel { max_capacity: u32, max_message_size: u32, max_total_size: u32 },
	/// The recipient accepting an open channel request.
	AcceptOpenChannel,
	/// Bringing the deposits of an open channel in line with the current prices.
	PokeChannelDeposits,
	/// Giving deposits back. Nothing to complete.
	Release,
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
