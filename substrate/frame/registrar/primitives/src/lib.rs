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

//! # Parachain-registrar shared primitives
//!
//! Types shared by the parachain registrar pallet (`pallet-registrar-para`) and relay-chain
//! registrar pallet (`pallet-registrar-relay`). This crate is deliberately free of any FRAME,
//! XCM, or network-specific dependency, so a single version of the wire types serves Westend,
//! Kusama and Polkadot, and so both pallets can depend on it without forming a dependency cycle.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;

/// A parachain id.
///
/// Byte-compatible with the relay chain's `Id`, which is a transparent `u32` newtype.
pub type ParaId = u32;

/// Registrar control-plane messages sent to the relay chain.
///
/// The variant's `#[codec(index)]` is the on-wire version tag.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToRelay {
	/// Version 1 of the registrar control-plane messages to the relay chain.
	#[codec(index = 0)]
	V1(MessageToRelayV1),
}

/// Version 1 payloads for [`MessageToRelay`].
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToRelayV1 {
	#[codec(index = 0)]
	TODO,
}

/// Registrar report messages sent back to the parachain.
///
/// The variant's `#[codec(index)]` is the on-wire version tag.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToPara {
	/// Version 1 of the registrar report messages to the parachain.
	#[codec(index = 0)]
	V1(MessageToParaV1),
}

/// Version 1 payloads for [`MessageToPara`].
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToParaV1 {
	#[codec(index = 0)]
	TODO,
}

/// State of migrated para at the source chain.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MigratedParaState {
	/// The para id is held by its manager, but nothing is registered on the relay chain.
	Reserved,
	/// The relay chain has onboarded this para.
	Registered {
		/// Length of the para's current head data, so the destination prices the registration
		/// the way it prices a fresh one.
		head_len: u32,
	},
}

/// One para, as it arrives from source chain that used to own the registry.
///
/// Carries no deposit. [`ReceiveMigratedParas::receive_para`] takes the reservation deposit,
/// and for a registered para the registration deposit, from `manager` at the destination's own
/// prices.
///
/// Note: We recreate even if there is not enough fund to pay for the deposit so no RC para
/// is dropped.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub struct MigratedPara<AccountId> {
	/// The para id.
	pub para_id: ParaId,
	/// The account that reserved the para id and controls it.
	pub manager: AccountId,
	/// Where this para id sits in the registration flow.
	pub state: MigratedParaState,
	/// Whether the manager is locked out of controlling this para. `None` until the lock is set
	/// for the first time, and read as unlocked.
	pub locked: Option<bool>,
}

/// Takes migrated para ids into the pallet that owns registration on the destination.
pub trait ReceiveMigratedParas<AccountId> {
	/// Take one para, charging its deposits at this chain's prices.
	///
	/// Note: We recreate even if there is not enough fund to pay for the deposit so no RC para
	/// gets dropped.
	fn receive_para(para: MigratedPara<AccountId>) -> sp_runtime::DispatchResult;

	/// Adopt the next free para id from the source chain.
	fn receive_next_free_para_id(para_id: ParaId);
}

impl<AccountId> ReceiveMigratedParas<AccountId> for () {
	fn receive_para(_: MigratedPara<AccountId>) -> sp_runtime::DispatchResult {
		Err(sp_runtime::DispatchError::Unavailable)
	}

	fn receive_next_free_para_id(_: ParaId) {}
}
