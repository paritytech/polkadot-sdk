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
//! code hash is an [`H256`] (what `ValidationCodeHash` wraps). The same holds for
//! [`ParachainRegistrar`], the interface the relay pallet drives the registry through: conversion
//! to the relay chain's own types happens in the pallet implementing it.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::DispatchResult;

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
		/// The parachain's id for this message, echoed back in the response.
		message_id: u64,
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
	/// Ask the relay chain to drop the authorization it is holding for `para_id`.
	///
	/// Sent when the manager gives up on a registration whose validation code never arrived. The
	/// relay chain never abandons an authorization by itself, so this is what ends a registration
	/// that is going nowhere, and the manager pays for it. Answered with
	/// [`MessageToParaV1::CancelResponse`].
	#[codec(index = 1)]
	CancelRegistration {
		/// The para id whose authorization should be dropped.
		para_id: ParaId,
		/// The parachain's id for this message, echoed back in the response.
		message_id: u64,
	},
	/// Ask the relay chain to drop `para_id` from the registry.
	///
	/// Ownership and Coretime are checked on the parachain, so nothing about the caller travels
	/// with this. Answered with [`MessageToParaV1::DeregisterResponse`].
	#[codec(index = 2)]
	Deregister {
		/// The para id to drop.
		para_id: ParaId,
		/// The parachain's id for this message, echoed back in the response.
		message_id: u64,
	},
	/// Ask the relay chain to abandon a [`MessageToRelayV1::Deregister`] it has not acted on.
	///
	/// Nothing is parked there, so this takes nothing back: it asks which way the deregistration
	/// went. Answered with [`MessageToParaV1::CancelDeregistrationResponse`].
	#[codec(index = 3)]
	CancelDeregistration {
		/// The para id whose deregistration should be abandoned.
		para_id: ParaId,
		/// The parachain's id for this message, echoed back in the response.
		message_id: u64,
	},
	/// Ask the relay chain to authorize a validation code upgrade for `para_id`.
	///
	/// As with a registration, the blob itself is not sent: the relay chain is told which bytes to
	/// accept and they are uploaded to it separately. Answered with
	/// [`MessageToParaV1::CodeUpgradeResponse`].
	#[codec(index = 4)]
	AuthorizeCodeUpgrade {
		/// The para id being upgraded.
		para_id: ParaId,
		/// The parachain's id for this message, echoed back in the response.
		message_id: u64,
		/// The account that manages this para on the parachain.
		manager: AccountId,
		/// Blake2-256 hash of the validation code that will be uploaded.
		code_hash: H256,
		/// Length of the validation code that will be uploaded, in bytes.
		code_len: u32,
	},
	/// Ask the relay chain to set the current head data of `para_id`.
	///
	/// The head data is small enough to travel with the request, so unlike a code upgrade this is
	/// a single round trip. Answered with [`MessageToParaV1::SetHeadResponse`].
	#[codec(index = 5)]
	SetCurrentHead {
		/// The para id whose head is being set.
		para_id: ParaId,
		/// The parachain's id for this message, echoed back in the response.
		message_id: u64,
		/// The account that manages this para on the parachain.
		manager: AccountId,
		/// The new head data.
		head: Vec<u8>,
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
	/// `para_id` correlates the response with its request: a parachain only sends
	/// [`MessageToRelayV1::Register`] for a para id that is reserved and otherwise idle, so at
	/// most one request per para id is ever in flight. `message_id` echoes the request's id on
	/// top, tying the two together across chains and in events.
	#[codec(index = 0)]
	RegisterResponse {
		/// The para id the report is about.
		para_id: ParaId,
		/// The id of the [`MessageToRelayV1::Register`] this answers, echoed back.
		message_id: u64,
		/// Whether the registration was applied on the relay chain.
		outcome: Outcome,
	},
	/// Answer a [`MessageToRelayV1::CancelRegistration`].
	///
	/// `Ok(())` means the relay chain is no longer holding an authorization for this para id, so
	/// the deposit can be released. The only refusal is
	/// [`FailureReason::AlreadyRegistered`]: the code did land after all and the para is
	/// registered, so the deposit stays where it is.
	#[codec(index = 1)]
	CancelResponse {
		/// The para id the answer is about.
		para_id: ParaId,
		/// The id of the [`MessageToRelayV1::CancelRegistration`] this answers, echoed back.
		message_id: u64,
		/// Whether the authorization was dropped.
		outcome: Outcome,
	},
	/// Answer a [`MessageToRelayV1::Deregister`].
	///
	/// `Ok(())` means the para is gone from the relay chain's registry, so every deposit the
	/// parachain is holding for it can be released.
	#[codec(index = 2)]
	DeregisterResponse {
		/// The para id the answer is about.
		para_id: ParaId,
		/// The id of the [`MessageToRelayV1::Deregister`] this answers, echoed back.
		message_id: u64,
		/// Whether the para was dropped.
		outcome: Outcome,
	},
	/// Answer a [`MessageToRelayV1::CancelDeregistration`].
	///
	/// `Ok(())` means the para is still registered, so the deposits stay. The one refusal is
	/// [`FailureReason::NotRegistered`]: it did go after all, so they can be released.
	#[codec(index = 3)]
	CancelDeregistrationResponse {
		/// The para id the answer is about.
		para_id: ParaId,
		/// The id of the [`MessageToRelayV1::CancelDeregistration`] this answers, echoed back.
		message_id: u64,
		/// Whether the deregistration was abandoned.
		outcome: Outcome,
	},
	/// Answer a [`MessageToRelayV1::AuthorizeCodeUpgrade`].
	///
	/// `Ok(expire_at)` means the relay chain is holding the authorization and will accept the blob
	/// until that relay-chain block. The upgrade is not scheduled yet: that is reported separately
	/// with [`MessageToParaV1::CodeUpgradeScheduled`] once the code lands.
	#[codec(index = 4)]
	CodeUpgradeResponse {
		/// The para id the answer is about.
		para_id: ParaId,
		/// The id of the [`MessageToRelayV1::AuthorizeCodeUpgrade`] this answers, echoed back.
		message_id: u64,
		/// The relay-chain block the authorization lapses at, or why it was refused.
		outcome: Result<u32, FailureReason>,
	},
	/// Report that an authorized code upgrade has been scheduled on the relay chain.
	///
	/// Sent when the blob is uploaded, which may be many blocks after the authorization and is not
	/// something the parachain asked for, so this carries no outcome: a failure to upload simply
	/// leaves the authorization to lapse.
	#[codec(index = 5)]
	CodeUpgradeScheduled {
		/// The para id the report is about.
		para_id: ParaId,
		/// The id of the [`MessageToRelayV1::AuthorizeCodeUpgrade`] this concludes, echoed back.
		message_id: u64,
	},
	/// Answer a [`MessageToRelayV1::SetCurrentHead`].
	///
	/// The parachain checks the head against its own mirror of the relay chain's limits first, so
	/// a refusal here means the two have drifted apart.
	#[codec(index = 6)]
	SetHeadResponse {
		/// The para id the answer is about.
		para_id: ParaId,
		/// The id of the [`MessageToRelayV1::SetCurrentHead`] this answers, echoed back.
		message_id: u64,
		/// Whether the head was set.
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
	///
	/// Also the answer to a [`MessageToRelayV1::CancelRegistration`] that came too late, because
	/// the validation code landed first.
	#[codec(index = 0)]
	AlreadyRegistered,
	/// The head data or the declared code length is not acceptable to the relay chain.
	#[codec(index = 1)]
	InvalidOnboardingData,
	/// The relay chain does not know this para id.
	///
	/// Also the answer to a [`MessageToRelayV1::CancelDeregistration`] that came too late.
	#[codec(index = 2)]
	NotRegistered,
	/// The relay chain is already holding as many pending registrations as it will accept.
	#[codec(index = 3)]
	TooManyPending,
	/// The relay chain will not drop this para: not an on-demand parathread, or code in
	/// pre-checking.
	#[codec(index = 4)]
	NotDeregisterable,
}

/// The parachain registry, as `pallet-registrar-relay` needs to see it.
///
/// Implemented by whichever pallet owns parachain registration, typically `paras_registrar` on the
/// relay chain. Lives here so neither side of the protocol has to depend on the other.
pub trait ParachainRegistrar {
	/// The account id used to identify a registration's manager.
	type AccountId;

	/// Whether head data and code of these sizes could be onboarded right now.
	///
	/// Checked against the relay chain's live configuration so a doomed request can be rejected
	/// before the user goes and uploads megabytes of code.
	fn check_onboarding(head_len: u32, code_len: u32) -> Result<(), FailureReason>;

	/// Whether the relay chain already knows this para id.
	fn is_registered(para_id: ParaId) -> bool;

	/// Whether `para_id` has left the registry, or is on its way out.
	///
	/// Not the inverse of [`Self::is_registered`]: a para being cleaned up is both.
	fn is_deregistering(para_id: ParaId) -> bool;

	/// Onboard `para_id` under `manager`.
	///
	/// No deposit is taken: the manager's funds are held on the chain running
	/// `pallet-registrar-para`. A refusal here fails the extrinsic that carried the validation
	/// code rather than being reported to the parachain, so this one speaks `DispatchError`.
	fn register(
		manager: Self::AccountId,
		para_id: ParaId,
		genesis_head: Vec<u8>,
		validation_code: Vec<u8>,
	) -> DispatchResult;

	/// Drop `para_id` from the registry. An id it does not know is dropped as a no-op.
	fn deregister(para_id: ParaId) -> Result<(), FailureReason>;
}
