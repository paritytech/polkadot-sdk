// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! V3 scheduling signature verifier backed by parachain Aura authorities.
//!
//! Implements [`VerifySchedulingSignature`] for parachains running Aura: derives the
//! parachain slot from the BABE pre-digest of the relay header at
//! `internal_scheduling_parent`, looks up the eligible Aura author from this pallet's
//! cached authority set, and verifies the 64-byte signature in [`SignedSchedulingInfo`]
//! over the encoded `SchedulingInfoPayload`.

use crate::{Authorities, Config};
use codec::{Decode, Encode};
use cumulus_primitives_core::{
	relay_chain::{Header as RelayChainHeader, RELAY_CHAIN_SLOT_DURATION_MILLIS},
	SignedSchedulingInfo, VerifySchedulingSignature,
};
use sp_application_crypto::RuntimeAppPublic;
use sp_consensus_aura::Slot;
use sp_consensus_babe::digests::CompatibleDigestItem as BabeDigestItem;

/// Verifier for V3 [`SignedSchedulingInfo`] against parachain Aura authorities.
///
/// Wired by the parachain runtime as
/// `type SchedulingSignatureVerifier = AuraSchedulingVerifier<Runtime>;` on
/// [`cumulus_pallet_parachain_system::Config`]. The relay slot duration is the
/// global `RELAY_CHAIN_SLOT_DURATION_MILLIS` (6000 ms),
/// which is fixed across Polkadot, Kusama, Westend, and Rococo.
///
/// `T` is the runtime; the Aura crypto is derived from
/// [`pallet_aura::Config::AuthorityId`] (typically `sr25519` or `ed25519`). The
/// signature blob in [`SignedSchedulingInfo`] is decoded into
/// `<T::AuthorityId as RuntimeAppPublic>::Signature` and verified with the
/// authority's own `verify` method, matching the existing Aura seal verification path.
pub struct AuraSchedulingVerifier<T>(core::marker::PhantomData<T>);

impl<T> VerifySchedulingSignature for AuraSchedulingVerifier<T>
where
	T: Config,
	T: pallet_timestamp::Config,
{
	const V3_SCHEDULING_ENABLED: bool = true;

	/// Verify that `signed_info` was produced by the Aura author eligible at the parachain slot
	/// derived from `internal_scheduling_parent_header`.
	///
	/// Returns `true` only when every step succeeds; all error paths return `false` (fail-closed)
	/// so the PVF rejects the candidate without panicking on adversarial input.
	///
	/// Binds the signature to `internal_scheduling_parent_header` by asserting the payload's
	/// `internal_scheduling_parent` field matches its hash. Derives the para slot from the
	/// header's BABE pre-digest, then looks up the eligible Aura author in the cached authority
	/// set and verifies the signature over the encoded `SchedulingInfoPayload`.
	fn verify(
		signed_info: &SignedSchedulingInfo,
		internal_scheduling_parent_header: &RelayChainHeader,
	) -> bool {
		if signed_info.payload.internal_scheduling_parent !=
			internal_scheduling_parent_header.hash()
		{
			return false;
		}

		// 1. Relay slot at internal scheduling parent gives the para slot that determines the valid
		//    author.
		let relay_slot: Slot = match internal_scheduling_parent_header
			.digest
			.logs()
			.iter()
			.find_map(|log| BabeDigestItem::as_babe_pre_digest(log))
		{
			Some(pre_digest) => pre_digest.slot(),
			None => return false,
		};

		// 2. Determine the para slot.
		let para_slot_duration: u64 =
			match TryInto::<u64>::try_into(pallet_aura::Pallet::<T>::slot_duration()) {
				Ok(d) if d > 0 => d,
				_ => return false,
			};

		let para_slot: u64 = match u64::from(relay_slot)
			.checked_mul(RELAY_CHAIN_SLOT_DURATION_MILLIS)
			.map(|product| product / para_slot_duration)
		{
			Some(s) => s,
			None => return false,
		};

		// 3. Look up the eligible Aura author.
		let authorities = Authorities::<T>::get();
		let author_idx = match pallet_aura::Pallet::<T>::slot_author_index(Slot::from(para_slot)) {
			Some(idx) => idx as usize,
			None => return false,
		};
		let author = match authorities.get(author_idx) {
			Some(author) => author,
			None => return false,
		};

		// 4. Decode the 64-byte signature blob as the authority's expected signature type and
		//    verify over the encoded SchedulingInfoPayload.
		let signature = match <T::AuthorityId as RuntimeAppPublic>::Signature::decode(
			&mut &signed_info.signature[..],
		) {
			Ok(sig) => sig,
			Err(_) => return false,
		};

		author.verify(&signed_info.payload.encode(), &signature)
	}
}
