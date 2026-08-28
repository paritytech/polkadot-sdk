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
	/// Ask the relay chain to deregister `para_id`.
	///
	/// Carries no manager: the parachain holds the deposits and is the authority on who may ask,
	/// and it has already checked. The relay chain removes the para or reports why it cannot.
	/// Answered with [`MessageToParaV1::DeregisterResponse`].
	#[codec(index = 2)]
	Deregister {
		/// The para id to deregister.
		para_id: ParaId,
		/// The parachain's id for this message, echoed back in the response.
		message_id: u64,
	},
	/// Ask the relay chain whether a deregistration whose verdict never arrived went through.
	///
	/// Sent when the manager gives up waiting on a [`MessageToRelayV1::Deregister`] answer. The
	/// relay chain answers with what actually happened, and only that answer settles the
	/// parachain's state. Answered with [`MessageToParaV1::CancelDeregistrationResponse`].
	#[codec(index = 3)]
	CancelDeregistration {
		/// The para id whose deregistration is being chased up.
		para_id: ParaId,
		/// The parachain's id for this message, echoed back in the response.
		message_id: u64,
	},
	/// Ask the relay chain to accept a validation code upgrade for `para_id`.
	///
	/// Carries only the hash and length, for the same reason
	/// [`MessageToRelayV1::Register`] does; the blob is uploaded to the relay chain separately.
	/// No deposit changes hands: a registration is priced at the largest code the relay chain
	/// will accept, so an upgrade is always already paid for.
	///
	/// Unanswered. Nothing on the parachain is staked on the outcome, so a refusal is reported
	/// as a relay-chain event rather than costing a round trip.
	#[codec(index = 4)]
	AuthorizeCodeUpgrade {
		/// The para id being upgraded.
		para_id: ParaId,
		/// The parachain's id for this message, for tying the two chains' events together.
		message_id: u64,
		/// Blake2-256 hash of the validation code that will be uploaded.
		code_hash: H256,
		/// Length of the validation code that will be uploaded, in bytes.
		code_len: u32,
	},
	/// Ask the relay chain to drop `para_id`'s upgrade cooldown.
	///
	/// The cost was already burned on the parachain, so the relay chain charges nothing. It is a
	/// flat price there rather than the relay chain's "time left times a multiplier", because the
	/// parachain cannot see how much of the cooldown is left without another round trip.
	///
	/// Unanswered. Nothing on the parachain is staked on the outcome — the cost is burned, not
	/// held — so a refusal is reported as a relay-chain event.
	#[codec(index = 6)]
	RemoveUpgradeCooldown {
		/// The para whose cooldown should be dropped.
		para_id: ParaId,
		/// The parachain's id for this message, for tying the two chains' events together.
		message_id: u64,
	},
	/// Ask the relay chain to set `para_id`'s head data.
	///
	/// Head data is small enough in practice to travel inline, so this needs no upload step.
	/// Unanswered, for the same reason as [`MessageToRelayV1::AuthorizeCodeUpgrade`].
	#[codec(index = 5)]
	SetCurrentHead {
		/// The para id whose head is being set.
		para_id: ParaId,
		/// The parachain's id for this message, for tying the two chains' events together.
		message_id: u64,
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
	/// Report how a deregistration requested with [`MessageToRelayV1::Deregister`] ended.
	///
	/// `Ok(())` means the para is gone from the relay chain (or was never there), so the deposits
	/// can be released.
	#[codec(index = 2)]
	DeregisterResponse {
		/// The para id the report is about.
		para_id: ParaId,
		/// The id of the [`MessageToRelayV1::Deregister`] this answers, echoed back.
		message_id: u64,
		/// Whether the deregistration was applied on the relay chain.
		outcome: Outcome,
	},
	/// Answer a [`MessageToRelayV1::CancelDeregistration`].
	///
	/// `Ok(())` means the para is still registered on the relay chain, so the deregistration
	/// never happened and the parachain can go back to normal. The only refusal is
	/// [`FailureReason::NotRegistered`]: the deregistration did go through and its report was
	/// lost, so the deposits go back after all.
	#[codec(index = 3)]
	CancelDeregistrationResponse {
		/// The para id the answer is about.
		para_id: ParaId,
		/// The id of the [`MessageToRelayV1::CancelDeregistration`] this answers, echoed back.
		message_id: u64,
		/// Whether the deregistration was called off.
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
	/// The relay chain is already holding as many pending registrations as it will accept.
	#[codec(index = 3)]
	TooManyPending,
	/// The request's manager does not match the one in the relay chain's registry.
	#[codec(index = 4)]
	NotManager,
	/// The para is locked from manager control on the relay chain.
	#[codec(index = 5)]
	Locked,
	/// The registry refused to deregister the para: it is a live parachain, known to the relay
	/// chain outside the registry, mid-upgrade, or its upward message queue is not drained.
	#[codec(index = 6)]
	CannotDeregister,
	/// The answer to a [`MessageToRelayV1::CancelDeregistration`] that came too late: the para is
	/// no longer registered, so the deregistration went through.
	#[codec(index = 7)]
	NotRegistered,
	/// The registry refused the code upgrade: the para is mid-upgrade, or the code is not
	/// acceptable to the relay chain's live configuration.
	#[codec(index = 8)]
	CannotUpgrade,
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
	#[allow(clippy::result_unit_err)]
	fn check_onboarding(head_len: u32, code_len: u32) -> Result<(), ()>;

	/// Whether the relay chain already knows this para id.
	fn is_registered(para_id: ParaId) -> bool;

	/// The manager of `para_id`, if the registry knows it.
	///
	/// `None` both for an id the registry never saw and for a para the relay chain knows outside
	/// the registry (distinguish those with [`Self::is_registered`]).
	fn manager_of(para_id: ParaId) -> Option<Self::AccountId>;

	/// Whether `para_id` is locked from manager control.
	fn is_locked(para_id: ParaId) -> bool;

	/// Onboard `para_id` under `manager`.
	///
	/// No deposit is taken: the manager's funds are held on the chain running
	/// `pallet-registrar-para`.
	fn register(
		manager: Self::AccountId,
		para_id: ParaId,
		genesis_head: Vec<u8>,
		validation_code: Vec<u8>,
	) -> sp_runtime::DispatchResult;

	/// Remove `para_id` from the registry and free its relay-chain state.
	///
	/// No deposit is released here for the same reason [`Self::register`] takes none. Not
	/// required to be atomic on failure: the caller runs it under a storage layer.
	fn deregister(para_id: ParaId) -> sp_runtime::DispatchResult;

	/// Schedule `new_code` as `para_id`'s validation code.
	///
	/// No deposit is taken or topped up: a registration already paid for the largest code the
	/// relay chain accepts. Not required to be atomic on failure; the caller runs it under a
	/// storage layer.
	fn schedule_code_upgrade(para_id: ParaId, new_code: Vec<u8>) -> sp_runtime::DispatchResult;

	/// Set `para_id`'s current head data.
	fn set_current_head(para_id: ParaId, head: Vec<u8>) -> sp_runtime::DispatchResult;

	/// Drop `para_id`'s upgrade cooldown, charging nothing.
	///
	/// The caller has already been charged on the chain that holds the money. Returns whether
	/// there was a cooldown to remove, so a request for a para that was not in one can be
	/// reported as such rather than looking like a success.
	fn remove_upgrade_cooldown(para_id: ParaId) -> bool;

	/// Arrange for `para_id` to be registered under `manager`, unlocked and deregisterable, so
	/// the deregistration path can be benchmarked.
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_deregisterable(manager: Self::AccountId, para_id: ParaId);
}
