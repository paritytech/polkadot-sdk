// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! V3 scheduling signature verifier backed by parachain Aura authorities.
//!
//! Implements [`VerifySchedulingSignature`] for parachains running Aura: derives the
//! parachain slot from the BABE pre-digest of the relay header at
//! `internal_scheduling_parent`, looks up the eligible Aura author from this pallet's
//! cached authority set, and verifies the 64-byte signature in [`SignedSchedulingInfo`]
//! over the encoded [`SchedulingInfoPayload`].

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
/// global [`polkadot_primitives::RELAY_CHAIN_SLOT_DURATION_MILLIS`] (6000 ms),
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

	fn verify(
		signed_info: &SignedSchedulingInfo,
		internal_scheduling_parent_header: &RelayChainHeader,
	) -> bool {
		if signed_info.payload.internal_scheduling_parent !=
			internal_scheduling_parent_header.hash()
		{
			return false;
		}

		// 1. Decode relay slot from the BABE pre-digest of the internal_scheduling_parent header.
		//    The eligible parachain author is determined by *this* slot, anchoring the signature to
		//    a specific block (the one being submitted/resubmitted) rather than to a moving relay
		//    tip. `check_scheduling` proves this header is the actual relay block at
		//    internal_scheduling_parent — it can't be substituted.
		let relay_slot: Slot = match internal_scheduling_parent_header
			.digest
			.logs()
			.iter()
			.find_map(|log| BabeDigestItem::as_babe_pre_digest(log))
		{
			Some(pre_digest) => pre_digest.slot(),
			None => return false,
		};

		// 2. Convert relay slot to parachain slot. Both slot durations are in milliseconds; the
		//    relay slot duration is the global Polkadot/Kusama/Westend/Rococo value re-exported by
		//    polkadot-primitives, and the para slot duration is read from pallet-aura. Fail closed
		//    on overflow rather than saturating, so an out-of-range relay slot can't quietly
		//    produce a wrong author index.
		let para_slot_duration: u64 =
			match TryInto::<u64>::try_into(pallet_aura::Pallet::<T>::slot_duration()) {
				Ok(d) if d > 0 => d,
				_ => return false,
			};

		let para_slot: u64 = match u64::from(relay_slot)
			.checked_mul(RELAY_CHAIN_SLOT_DURATION_MILLIS)
			.and_then(|product| product.checked_div(para_slot_duration))
		{
			Some(s) => s,
			None => return false,
		};

		// 3. Look up the eligible Aura author. Use the cached authority set rather than
		//    `pallet_aura::Authorities` because aura-ext's cache is captured at on_initialize for
		//    verification of the current PoV.
		let authorities = Authorities::<T>::get();
		if authorities.is_empty() {
			return false;
		}
		let author_idx = (para_slot % authorities.len() as u64) as usize;
		let author = &authorities[author_idx];

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
