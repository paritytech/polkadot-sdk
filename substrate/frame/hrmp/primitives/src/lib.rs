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

/// A deposit amount, in the relay chain's native token.
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

/// Which end of a channel pays a deposit.
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
pub enum DepositSide {
	/// The para that sends on the channel.
	#[codec(index = 0)]
	Sender,
	/// The para that receives on the channel.
	#[codec(index = 1)]
	Recipient,
}

impl DepositSide {
	/// The para of `channel` that pays this side's deposit.
	pub fn para(&self, channel: &ChannelId) -> ParaId {
		match self {
			Self::Sender => channel.sender,
			Self::Recipient => channel.recipient,
		}
	}
}

/// Why the parachain could not set the deposits it was asked to.
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
pub enum DepositError {
	/// A side cannot cover its deposit.
	#[codec(index = 0)]
	InsufficientFunds,
	/// Any other reason.
	#[codec(index = 1)]
	Other,
}

/// Messages from the relay chain to the parachain that holds the deposits.
///
/// The variant's `#[codec(index)]` is the on-wire version tag.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum MessageToPara {
	/// Version 1.
	#[codec(index = 0)]
	V1(MessageToParaV1),
}

/// Version 1 payloads for [`MessageToPara`].
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum MessageToParaV1 {
	/// Move each named side of `channel` to what it should hold, both or neither. `None` leaves
	/// that side alone. Answered with [`MessageToRelayV1::DepositsSet`] carrying `id`.
	#[codec(index = 0)]
	SetDeposits {
		/// Chosen by the relay chain, echoed back in the answer.
		id: u64,
		/// The channel whose deposits change.
		channel: ChannelId,
		/// What the sender should hold.
		sender: Option<Balance>,
		/// What the recipient should hold.
		recipient: Option<Balance>,
	},
	/// Release everything held for the named sides of `channel`. Never fails, never answered.
	#[codec(index = 1)]
	Release {
		/// The channel whose deposits are given back.
		channel: ChannelId,
		/// Whether the sender's deposit is given back.
		sender: bool,
		/// Whether the recipient's deposit is given back.
		recipient: bool,
	},
}

/// Messages from the parachain that holds the deposits to the relay chain.
///
/// The variant's `#[codec(index)]` is the on-wire version tag.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum MessageToRelay {
	/// Version 1.
	#[codec(index = 0)]
	V1(MessageToRelayV1),
}

/// Version 1 payloads for [`MessageToRelay`].
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum MessageToRelayV1 {
	/// The answer to [`MessageToParaV1::SetDeposits`] with this `id`.
	#[codec(index = 0)]
	DepositsSet {
		/// The `id` of the request.
		id: u64,
		/// The `result` of the request.
		result: Result<(), DepositError>,
	},
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
