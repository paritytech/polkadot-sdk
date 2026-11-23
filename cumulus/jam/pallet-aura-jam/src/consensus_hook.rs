// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! A consensus hook that ties the Aura slot to the JAM anchor slot.

use crate::{Config, Pallet, SlotInfo};
use core::{marker::PhantomData, num::NonZeroU32};
use cumulus_pallet_jam_system::{self as jam_system, ConsensusHook};
use cumulus_primitives_core::JamParent;
use frame_support::pallet_prelude::*;
use sp_consensus_aura::Slot;

/// Like aura-ext's `FixedVelocityConsensusHook`, but the clock is the anchor slot of the block's
/// verified `JamParent` digest instead of the relay slot in a relay-chain state proof.
///
/// `JamSystem` calls it at the start of its `on_initialize`, which runs before this pallet's, so
/// the check reads the block's Aura slot from its pre-runtime digest (see
/// [`Pallet::check_anchor_slot`]).
///
/// - `V`: blocks allowed per anchor slot (plus one to fill the unincluded segment).
/// - `C`: unincluded segment capacity.
pub struct JamFixedVelocityConsensusHook<T, const V: u32, const C: u32>(PhantomData<T>);

impl<T: Config, const V: u32, const C: u32> ConsensusHook
	for JamFixedVelocityConsensusHook<T, V, C>
{
	fn on_block(parent: &JamParent) -> (Weight, NonZeroU32) {
		Pallet::<T>::check_anchor_slot(parent.anchor_slot, V);
		(
			// Reads: the digest and `SlotInfo`. Writes: `SlotInfo`.
			T::DbWeight::get().reads_writes(2, 1),
			NonZeroU32::new(C.max(1)).expect("1 is the minimum value and non-zero; qed"),
		)
	}
}

impl<T: Config + jam_system::Config, const V: u32, const C: u32>
	JamFixedVelocityConsensusHook<T, V, C>
{
	/// Whether a block in Aura slot `new_slot` may extend the chain, with `included_hash` the
	/// latest included block. Consistent with [`ConsensusHook::on_block`].
	pub fn can_build_upon(included_hash: T::Hash, new_slot: Slot) -> bool {
		let velocity = V.max(1);
		let Some((last_anchor, authored)) = SlotInfo::<T>::get() else { return true };
		if jam_system::Pallet::<T>::unincluded_segment_size_after(included_hash) >= C {
			return false;
		}
		let last_slot = cumulus_jam_aura_primitives::aura_slot_for_anchor(
			last_anchor,
			Pallet::<T>::slot_duration(),
		);
		if last_slot == *new_slot {
			authored < velocity + 1
		} else {
			last_slot < *new_slot
		}
	}
}
