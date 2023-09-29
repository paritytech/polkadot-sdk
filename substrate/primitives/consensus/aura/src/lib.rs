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

//! Primitives for Aura.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Codec, Decode, Encode};
use scale_info::TypeInfo;

use sp_application_crypto::RuntimeAppPublic;
use sp_runtime::{generic::DigestItem, traits::Header, ConsensusEngineId};
use sp_std::vec::Vec;

pub use sp_consensus_slots::{Slot, SlotDuration};

pub mod digests;
pub mod inherents;

/// Key type for AURA module.
pub const KEY_TYPE: sp_application_crypto::sp_core::crypto::KeyTypeId =
	sp_application_crypto::key_types::AURA;

pub mod sr25519 {
	mod app_sr25519 {
		use sp_application_crypto::{app_crypto, key_types::AURA, sr25519};
		app_crypto!(sr25519, AURA);
	}

	sp_application_crypto::with_pair! {
		/// An Aura authority keypair using S/R 25519 as its crypto.
		pub type AuthorityPair = app_sr25519::Pair;
	}

	/// An Aura authority signature using S/R 25519 as its crypto.
	pub type AuthoritySignature = app_sr25519::Signature;

	/// An Aura authority identifier using S/R 25519 as its crypto.
	pub type AuthorityId = app_sr25519::Public;
}

pub mod ed25519 {
	mod app_ed25519 {
		use sp_application_crypto::{app_crypto, ed25519, key_types::AURA};
		app_crypto!(ed25519, AURA);
	}

	sp_application_crypto::with_pair! {
		/// An Aura authority keypair using Ed25519 as its crypto.
		pub type AuthorityPair = app_ed25519::Pair;
	}

	/// An Aura authority signature using Ed25519 as its crypto.
	pub type AuthoritySignature = app_ed25519::Signature;

	/// An Aura authority identifier using Ed25519 as its crypto.
	pub type AuthorityId = app_ed25519::Public;
}

/// The `ConsensusEngineId` of AuRa.
pub const AURA_ENGINE_ID: ConsensusEngineId = [b'a', b'u', b'r', b'a'];

/// The index of an authority.
pub type AuthorityIndex = u32;

/// An equivocation proof for multiple block authorships on the same slot.
pub type EquivocationProof<H, AuthorityId> = sp_consensus_slots::EquivocationProof<H, AuthorityId>;

/// Opaque type representing the key ownership proof at runtime API boundary.
///
/// The inner value is an encoded representation of the actual key
/// ownership proof which will be parameterized when defining the runtime. At
/// the runtime API boundary this type is unknown and as such we keep this
/// opaque representation, implementors of the runtime API will have to make
/// sure that all usages of `OpaqueKeyOwnershipProof` refer to the same type.
#[derive(Decode, Encode, PartialEq, TypeInfo)]
pub struct OpaqueKeyOwnershipProof(pub Vec<u8>);

/// An consensus log item for Aura.
#[derive(Decode, Encode)]
pub enum ConsensusLog<AuthorityId: Codec> {
	/// The authorities have changed.
	#[codec(index = 1)]
	AuthoritiesChange(Vec<AuthorityId>),
	/// Disable the authority with given index.
	#[codec(index = 2)]
	OnDisabled(AuthorityIndex),
}

/// Verifies the equivocation proof.
///
/// Makes sure that both headers have different hashes, are targetting the same slot,
/// and have valid signatures by the same authority.
pub fn check_equivocation_proof<H, AuthorityId: RuntimeAppPublic>(
	proof: EquivocationProof<H, AuthorityId>,
) -> bool
where
	H: Header,
{
	use digests::CompatibleDigestItem;

	let find_pre_digest = |header: &H| {
		header.digest().logs().iter().find_map(|log| {
			<DigestItem as CompatibleDigestItem<AuthorityId::Signature>>::as_aura_pre_digest(log)
		})
	};

	let verify_signature = |mut header: H, offender: &AuthorityId| {
		let seal = header.digest_mut().pop()?.as_aura_seal()?;
		let pre_hash = header.hash();
		offender.verify(&pre_hash.as_ref(), &seal).then(|| ())
	};

	let verify_proof = || {
		// We must have different headers for the equivocation to be valid
		if proof.first_header.hash() == proof.second_header.hash() {
			return None
		}

		let first_slot = find_pre_digest(&proof.first_header)?;
		let second_slot = find_pre_digest(&proof.second_header)?;

		// Both headers must be targetting the same slot and it must
		// be the same as the one in the proof.
		if proof.slot != first_slot || first_slot != second_slot {
			return None
		}

		// Finally verify that the expected authority has signed both headers and
		// that the signature is valid.
		verify_signature(proof.first_header, &proof.offender)?;
		verify_signature(proof.second_header, &proof.offender)?;

		Some(())
	};

	verify_proof().is_some()
}

sp_api::decl_runtime_apis! {
	/// API necessary for block authorship with aura.
	pub trait AuraApi<AuthorityId: Codec> {
		/// Returns the slot duration for Aura.
		///
		/// Currently, only the value provided by this type at genesis will be used.
		fn slot_duration() -> SlotDuration;

		/// Return the current set of authorities.
		fn authorities() -> Vec<AuthorityId>;

		/// Generates a proof of key ownership for the given authority in the
		/// current epoch.
		///
		/// An example usage of this module is coupled with the session historical
		/// module to prove that a given authority key is tied to a given staking
		/// identity during a specific session. Proofs of key ownership are necessary
		/// for submitting equivocation reports.
		fn generate_key_ownership_proof(
			slot: Slot,
			authority_id: AuthorityId,
		) -> Option<OpaqueKeyOwnershipProof>;

		/// Submits an unsigned extrinsic to report an equivocation.
		///
		/// The caller must provide the equivocation proof and a key ownership
		/// proof (should be obtained using `generate_key_ownership_proof`).
		/// The extrinsic will be unsigned and should only be accepted for local
		/// authorship (not to be broadcast to the network). This method returns
		/// `None` when creation of the extrinsic fails, e.g. if equivocation
		/// reporting is disabled for the given runtime (i.e. this method is
		/// hardcoded to return `None`). Only useful in an offchain context.
		fn submit_report_equivocation_unsigned_extrinsic(
			equivocation_proof: EquivocationProof<Block::Header, AuthorityId>,
			key_owner_proof: OpaqueKeyOwnershipProof,
		) -> Option<()>;
	}
}
