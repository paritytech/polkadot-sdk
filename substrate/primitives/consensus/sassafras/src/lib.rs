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

//! Primitives for Sassafras consensus.

#![deny(warnings)]
#![forbid(unsafe_code, missing_docs, unused_variables, unused_imports)]
#![cfg_attr(not(feature = "std"), no_std)]

use scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::crypto::KeyTypeId;
use sp_runtime::{ConsensusEngineId, RuntimeDebug};
use sp_std::vec::Vec;

pub use sp_consensus_slots::{Slot, SlotDuration};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub mod digests;
pub mod ticket;
pub mod vrf;

pub use ticket::{
	ticket_id_threshold, EphemeralPublic, EphemeralSignature, TicketBody, TicketClaim,
	TicketEnvelope, TicketId,
};

mod app {
	use sp_application_crypto::{app_crypto, bandersnatch, key_types::SASSAFRAS};
	app_crypto!(bandersnatch, SASSAFRAS);
}

/// Key type identifier.
pub const KEY_TYPE: KeyTypeId = sp_application_crypto::key_types::SASSAFRAS;

/// Consensus engine identifier.
pub const SASSAFRAS_ENGINE_ID: ConsensusEngineId = *b"SASS";

/// VRF output length for per-slot randomness.
pub const RANDOMNESS_LENGTH: usize = 32;

/// Index of an authority.
pub type AuthorityIndex = u32;

/// Sassafras authority keypair. Necessarily equivalent to the schnorrkel public key used in
/// the main Sassafras module. If that ever changes, then this must, too.
#[cfg(feature = "std")]
pub type AuthorityPair = app::Pair;

/// Sassafras authority signature.
pub type AuthoritySignature = app::Signature;

/// Sassafras authority identifier. Necessarily equivalent to the schnorrkel public key used in
/// the main Sassafras module. If that ever changes, then this must, too.
pub type AuthorityId = app::Public;

/// Weight of a Sassafras block.
/// Primary blocks have a weight of 1 whereas secondary blocks have a weight of 0.
pub type SassafrasBlockWeight = u32;

/// An equivocation proof for multiple block authorships on the same slot (i.e. double vote).
pub type EquivocationProof<H> = sp_consensus_slots::EquivocationProof<H, AuthorityId>;

/// Randomness required by some protocol's operations.
pub type Randomness = [u8; RANDOMNESS_LENGTH];

/// Protocol configuration that can be modified on epoch change.
///
/// Mostly tweaks to the ticketing system parameters.
#[derive(
	Copy, Clone, PartialEq, Eq, Encode, Decode, RuntimeDebug, MaxEncodedLen, TypeInfo, Default,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct EpochConfiguration {
	/// Tickets redundancy factor.
	///
	/// Expected ratio between epoch's slots and the cumulative number of tickets which can
	/// be submitted by the set of epoch validators.
	pub redundancy_factor: u32,
	/// Tickets max attempts for each validator.
	///
	/// Influences the anonymity of block producers. As all published tickets have a public
	/// attempt number less than `attempts_number` if two tickets share an attempt number
	/// then they must belong to two different validators, which reduces anonymity late as
	/// we approach the epoch tail.
	///
	/// This anonymity loss already becomes small when `attempts_number = 64` or `128`.
	pub attempts_number: u32,
}

/// Sassafras epoch information
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct Epoch {
	/// Epoch index.
	pub index: u64,
	/// Starting slot of the epoch.
	pub start: Slot,
	/// Number of slots in the epoch.
	pub length: u32,
	/// Randomness value.
	pub randomness: Randomness,
	/// Authorities list.
	pub authorities: Vec<AuthorityId>,
	/// Epoch configuration.
	pub config: EpochConfiguration,
}

/// An opaque type used to represent the key ownership proof at the runtime API boundary.
///
/// The inner value is an encoded representation of the actual key ownership proof which will be
/// parameterized when defining the runtime. At the runtime API boundary this type is unknown and
/// as such we keep this opaque representation, implementors of the runtime API will have to make
/// sure that all usages of `OpaqueKeyOwnershipProof` refer to the same type.
#[derive(Decode, Encode, PartialEq, TypeInfo)]
#[repr(transparent)]
pub struct OpaqueKeyOwnershipProof(Vec<u8>);

// Runtime API.
sp_api::decl_runtime_apis! {
	/// API necessary for block authorship with Sassafras.
	pub trait SassafrasApi {
		/// Get ring context to be used for ticket construction and verification.
		fn ring_context() -> Option<vrf::RingContext>;

		/// Submit next epoch validator tickets via an unsigned extrinsic.
		/// This method returns `false` when creation of the extrinsics fails.
		fn submit_tickets_unsigned_extrinsic(tickets: Vec<TicketEnvelope>) -> bool;

		/// Get ticket id associated to the given slot.
		fn slot_ticket_id(slot: Slot) -> Option<TicketId>;

		/// Get ticket id and data associated to the given slot.
		fn slot_ticket(slot: Slot) -> Option<(TicketId, TicketBody)>;

		/// Current epoch information.
		fn current_epoch() -> Epoch;

		/// Next epoch information.
		fn next_epoch() -> Epoch;

		/// Generates a proof of key ownership for the given authority in the current epoch.
		///
		/// An example usage of this module is coupled with the session historical module to prove
		/// that a given authority key is tied to a given staking identity during a specific
		/// session.
		///
		/// Proofs of key ownership are necessary for submitting equivocation reports.
		fn generate_key_ownership_proof(authority_id: AuthorityId) -> Option<OpaqueKeyOwnershipProof>;

		/// Submits an unsigned extrinsic to report an equivocation.
		///
		/// The caller must provide the equivocation proof and a key ownership proof (should be
		/// obtained using `generate_key_ownership_proof`). The extrinsic will be unsigned and
		/// should only be accepted for local authorship (not to be broadcast to the network). This
		/// method returns `false` when creation of the extrinsic fails.
		///
		/// Only useful in an offchain context.
		fn submit_report_equivocation_unsigned_extrinsic(
			equivocation_proof: EquivocationProof<Block::Header>,
			key_owner_proof: OpaqueKeyOwnershipProof,
		) -> bool;
	}
}
