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
//!
//! For the same reason the types here are plain: a para id is a `u32` (byte-compatible with the
//! relay chain's `Id` newtype), head data and validation code are `Vec<u8>`, and a validation
//! code hash is an [`H256`] (what `ValidationCodeHash` wraps). Conversion to the relay chain's
//! own types happens in the adapter that bridges `pallet-registrar-relay` to `paras_registrar`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::H256;

/// A parachain id.
///
/// Byte-compatible with the relay chain's `Id`, which is a transparent `u32` newtype.
pub type ParaId = u32;

/// Registrar control-plane messages sent to the relay chain.
///
/// The variant's `#[codec(index)]` is the on-wire version tag.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToRelay<AccountId> {
	/// Version 1 of the registrar control-plane messages to the relay chain.
	#[codec(index = 0)]
	V1(MessageToRelayV1<AccountId>),
}

/// Version 1 payloads for [`MessageToRelay`].
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToRelayV1<AccountId> {
	/// Ask the relay chain to accept a registration for `para_id`.
	///
	/// The deposit for this registration is already held on the parachain; the relay chain takes
	/// nothing. The validation code itself is not included: only its hash and length are, and the
	/// blob is uploaded to the relay chain separately.
	#[codec(index = 0)]
	Register {
		/// The para id being registered. Already reserved on the parachain.
		para_id: ParaId,
		/// The account that manages this registration and holds the deposit on the parachain.
		manager: AccountId,
		/// The genesis head data of the new parachain.
		genesis_head: Vec<u8>,
		/// Blake2-256 hash of the validation code that will be uploaded.
		code_hash: H256,
		/// Length of the validation code that will be uploaded, in bytes.
		///
		/// The deposit on the parachain was computed from this, so the relay chain must reject any
		/// blob whose length differs.
		code_len: u32,
	},
}

/// Registrar report messages sent back to the parachain.
///
/// The variant's `#[codec(index)]` is the on-wire version tag.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum MessageToPara {
	/// Version 1 of the registrar report messages to the parachain.
	#[codec(index = 0)]
	V1(MessageToParaV1),
}

/// Version 1 payloads for [`MessageToPara`].
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum MessageToParaV1 {
	/// Report how a registration requested with [`MessageToRelayV1::Register`] ended.
	///
	/// `para_id` alone correlates the response with its request: a parachain only sends
	/// [`MessageToRelayV1::Register`] for a para id that is reserved and otherwise idle, so at
	/// most one request per para id is ever in flight.
	#[codec(index = 0)]
	RegisterResponse {
		/// The para id the report is about.
		para_id: ParaId,
		/// Whether the registration was applied on the relay chain.
		outcome: Outcome,
	},
}

/// How a request ended.
///
/// `Ok(())` means the relay chain applied it, `Err(reason)` that it did not. Shared by every
/// response in this protocol rather than one outcome type per request, the same way a pallet has
/// one `Error` enum instead of one per extrinsic. Encodes as `0x00` for success and `0x01` plus
/// the reason for failure.
pub type Outcome = Result<(), FailureReason>;

/// Why a request was rejected by the relay chain.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum FailureReason {
	/// The relay chain already knows this para id.
	#[codec(index = 0)]
	AlreadyRegistered,
	/// The head data or the declared code length is not acceptable to the relay chain.
	#[codec(index = 1)]
	InvalidOnboardingData,
	/// The validation code was not uploaded before the request expired.
	#[codec(index = 2)]
	Expired,
	/// The relay chain is already holding as many pending registrations as it will accept.
	#[codec(index = 3)]
	TooManyPending,
}
