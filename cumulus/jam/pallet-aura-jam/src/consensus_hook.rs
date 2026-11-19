// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! A consensus hook that ties the Aura slot to the JAM anchor slot.

use crate::{Config, Pallet, SlotInfo};
use core::{marker::PhantomData, num::NonZeroU32};
use cumulus_pallet_parachain_system::{
	self as parachain_system,
	consensus_hook::{ConsensusHook, UnincludedSegmentCapacity},
	relay_state_snapshot::RelayChainStateProof,
};
use frame_support::pallet_prelude::*;
use sp_consensus_aura::Slot;

/// Like aura-ext's `FixedVelocityConsensusHook`, but the clock is the anchor slot of the block's
/// verified `JamParent` digest instead of the relay slot in the (collator-built) state proof.
///
/// - `V`: blocks allowed per anchor slot (plus one to fill the unincluded segment).
/// - `C`: unincluded segment capacity.
pub struct JamFixedVelocityConsensusHook<T, const V: u32, const C: u32>(PhantomData<T>);

impl<T: Config + parachain_system::Config, const V: u32, const C: u32> ConsensusHook
	for JamFixedVelocityConsensusHook<T, V, C>
{
	fn on_state_proof(_: &RelayChainStateProof) -> (Weight, UnincludedSegmentCapacity) {
		let anchor = Pallet::<T>::anchor_slot()
			.expect("a JAM block always carries its JamParent digest; qed");
		Pallet::<T>::check_anchor_slot(anchor, V);
		(
			T::DbWeight::get().reads_writes(3, 1),
			NonZeroU32::new(C.max(1))
				.expect("1 is the minimum value and non-zero; qed")
				.into(),
		)
	}
}

impl<T: Config + parachain_system::Config, const V: u32, const C: u32>
	JamFixedVelocityConsensusHook<T, V, C>
{
	/// Whether a block in Aura slot `new_slot` may extend the chain, with `included_hash` the
	/// latest included block. Consistent with [`ConsensusHook::on_state_proof`].
	pub fn can_build_upon(included_hash: T::Hash, new_slot: Slot) -> bool {
		let velocity = V.max(1);
		let Some((last_anchor, authored)) = SlotInfo::<T>::get() else { return true };
		if parachain_system::Pallet::<T>::unincluded_segment_size_after(included_hash) >= C {
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
