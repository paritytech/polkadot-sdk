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

//! # HRMP deposit primitives
//!
//! Types shared by the relay-chain pallet (`pallet-hrmp-relay`) and the parachain pallet that
//! holds HRMP channel deposits (`pallet-hrmp-para`). No FRAME, XCM or network-specific
//! dependency, so one version of the wire types serves every network.
//!
//! The relay chain owns every HRMP channel, request and decision. The parachain holds the
//! deposits: the relay chain asks it to hold or release one, and it answers whether a hold
//! succeeded.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// A parachain id.
///
/// Byte-compatible with the relay chain's `Id`, which is a transparent `u32` newtype.
pub type ParaId = u32;

/// A deposit amount, in the relay chain's native token.
pub type Balance = u128;

/// One direction of a channel, in the order the relay chain names them.
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

/// One deposit: a channel, and which end of it pays.
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
pub struct DepositKey {
	/// The channel the deposit is for.
	pub channel: ChannelId,
	/// Which end pays it.
	pub side: DepositSide,
}

impl DepositKey {
	/// The para that pays this deposit.
	pub fn para(&self) -> ParaId {
		match self.side {
			DepositSide::Sender => self.channel.sender,
			DepositSide::Recipient => self.channel.recipient,
		}
	}
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
	/// Hold `amount` from the para that pays `key`. Answered with
	/// [`MessageToRelayV1::HoldResult`].
	#[codec(index = 0)]
	Hold {
		/// The deposit.
		key: DepositKey,
		/// How much to hold.
		amount: Balance,
	},
	/// Release `amount` of what is held for `key`, or everything if `None`. Not answered.
	#[codec(index = 1)]
	Release {
		/// The deposit.
		key: DepositKey,
		/// How much to release.
		amount: Option<Balance>,
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
	/// Whether a [`MessageToParaV1::Hold`] succeeded.
	#[codec(index = 0)]
	HoldResult {
		/// The deposit.
		key: DepositKey,
		/// `true` if the amount is now held.
		held: bool,
	},
	/// `poke_channel_deposits`, asked for by a user on the parachain.
	#[codec(index = 1)]
	PokeChannelDeposits {
		/// The channel to reprice.
		channel: ChannelId,
	},
	/// `establish_system_channel`, asked for by a user on the parachain.
	#[codec(index = 2)]
	EstablishSystemChannel {
		/// The channel to open. Both ends must be system chains.
		channel: ChannelId,
	},
}

/// A parachain's own HRMP request, sent to the relay chain.
///
/// The asking para is not in the payload: the relay chain takes it from the origin.
///
/// The variant's `#[codec(index)]` is the on-wire version tag.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum ParaRequest {
	/// Version 1.
	#[codec(index = 0)]
	V1(ParaRequestV1),
}

/// Version 1 payloads for [`ParaRequest`], one per para-facing call of the relay chain's `hrmp`.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum ParaRequestV1 {
	/// `hrmp_init_open_channel`.
	#[codec(index = 0)]
	InitOpenChannel {
		/// The other end of the channel.
		recipient: ParaId,
		/// How many messages the channel may hold at once.
		proposed_max_capacity: u32,
		/// The largest message the channel will carry.
		proposed_max_message_size: u32,
	},
	/// `hrmp_accept_open_channel`.
	#[codec(index = 1)]
	AcceptOpenChannel {
		/// The para that asked.
		sender: ParaId,
	},
	/// `hrmp_close_channel`.
	#[codec(index = 2)]
	CloseChannel {
		/// The channel to close.
		channel: ChannelId,
	},
	/// `hrmp_cancel_open_request`.
	#[codec(index = 3)]
	CancelOpenRequest {
		/// The channel the request is for.
		channel: ChannelId,
		/// The number of open requests on the relay chain, as witness.
		open_requests: u32,
	},
	/// `establish_channel_with_system`.
	#[codec(index = 4)]
	EstablishChannelWithSystem {
		/// The system chain to open both directions with.
		target_system_chain: ParaId,
	},
}

/// The relay chain's HRMP, as `pallet-hrmp-relay` dispatches into it.
///
/// Every method has the same checks and events as the matching `hrmp` call. The ones taking a
/// `para` act as that para.
pub trait RelayHrmp {
	/// `hrmp_init_open_channel` from `para`.
	fn init_open_channel(
		para: ParaId,
		recipient: ParaId,
		proposed_max_capacity: u32,
		proposed_max_message_size: u32,
	) -> sp_runtime::DispatchResult;
	/// `hrmp_accept_open_channel` from `para`.
	fn accept_open_channel(para: ParaId, sender: ParaId) -> sp_runtime::DispatchResult;
	/// `hrmp_close_channel` from `para`.
	fn close_channel(para: ParaId, channel: ChannelId) -> sp_runtime::DispatchResult;
	/// `hrmp_cancel_open_request` from `para`.
	fn cancel_open_request(
		para: ParaId,
		channel: ChannelId,
		open_requests: u32,
	) -> sp_runtime::DispatchResult;
	/// `establish_channel_with_system` from `para`.
	fn establish_channel_with_system(
		para: ParaId,
		target_system_chain: ParaId,
	) -> sp_runtime::DispatchResult;
	/// `poke_channel_deposits`.
	fn poke_channel_deposits(channel: ChannelId) -> sp_runtime::DispatchResult;
	/// `establish_system_channel`.
	fn establish_system_channel(channel: ChannelId) -> sp_runtime::DispatchResult;
}

/// Receives the outcome of a hold the relay chain asked for.
pub trait OnDepositHeld {
	/// `held` is `true` if the deposit for `key` is now held.
	fn on_deposit_held(key: DepositKey, held: bool);
}

/// Takes deposits migrated from the relay chain into the pallet that holds them.
///
/// `()` refuses every deposit, so a migrator running ahead of the pallet parks each record
/// instead of losing it.
pub trait ReceiveMigratedDeposits {
	/// Hold up to `amount` for `key`, from what the paying para has.
	fn receive_deposit(key: DepositKey, amount: Balance) -> sp_runtime::DispatchResult;
}

impl ReceiveMigratedDeposits for () {
	fn receive_deposit(_: DepositKey, _: Balance) -> sp_runtime::DispatchResult {
		Err(sp_runtime::DispatchError::Unavailable)
	}
}
