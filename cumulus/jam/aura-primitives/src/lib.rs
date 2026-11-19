// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! Runtime API and slot arithmetic shared by the runtime and the collator for Aura authority sets
//! whose changes are enacted at JAM anchor slots.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;

pub use cumulus_aura_authorizer_utils::AURA_SLOT_OFFSET;

/// Milliseconds per JAM timeslot.
pub const JAM_SLOT_DURATION_MS: u64 = 6000;

/// The Unix timestamp in milliseconds at the start of JAM slot `anchor_slot`.
///
/// A block built on an anchor carries this timestamp, which pins its Aura slot to the anchor.
pub fn timestamp_for_anchor(anchor_slot: u32) -> u64 {
	(AURA_SLOT_OFFSET + anchor_slot as u64) * JAM_SLOT_DURATION_MS
}

/// The Aura slot of a block built on `anchor_slot`, for an Aura slot of `slot_duration_ms`.
pub fn aura_slot_for_anchor(anchor_slot: u32, slot_duration_ms: u64) -> u64 {
	timestamp_for_anchor(anchor_slot) / slot_duration_ms.max(1)
}

sp_api::decl_runtime_apis! {
	/// The Aura authority sets JAM authorizes, by JAM anchor slot.
	pub trait JamAuraApi<AuthorityId: codec::Codec> {
		/// The set JAM's authorizer commits to for a package anchored at `anchor_slot`.
		fn authorities_at(anchor_slot: u32) -> Vec<AuthorityId>;
		/// The announced sets that are not active yet, with the JAM slot each one starts at.
		fn scheduled_changes() -> Vec<(u32, Vec<AuthorityId>)>;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn aura_slot_matches_the_authorizer_round_robin_works() {
		// For d = 1 the Aura slot is exactly the slot the authorizer round-robins on.
		for anchor in [0, 1, 7, 1_000_003] {
			assert_eq!(aura_slot_for_anchor(anchor, 6000), AURA_SLOT_OFFSET + anchor as u64);
			assert_eq!(aura_slot_for_anchor(anchor, 12000), (AURA_SLOT_OFFSET + anchor as u64) / 2);
		}
	}
}
