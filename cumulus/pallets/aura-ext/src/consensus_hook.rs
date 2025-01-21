// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! The definition of a [`FixedVelocityConsensusHook`] for consensus logic to manage
//! block velocity.
//!
//! The velocity `V` refers to the rate of block processing by the relay chain.
use super::{pallet, Aura};
use core::{marker::PhantomData, num::NonZeroU32};
use cumulus_pallet_parachain_system::{
	self as parachain_system,
	consensus_hook::{ConsensusHook, UnincludedSegmentCapacity},
	relay_state_snapshot::RelayChainStateProof,
};
use frame_support::pallet_prelude::*;
use sp_consensus_aura::{Slot, SlotDuration};

/// A consensus hook for a fixed block processing velocity and unincluded segment capacity.
///
/// Relay chain slot duration must be provided in milliseconds.
pub struct FixedVelocityConsensusHook<
	T,
	const RELAY_CHAIN_SLOT_DURATION_MILLIS: u32,
	const V: u32,
	const C: u32,
>(PhantomData<T>);

impl<
		T: pallet::Config,
		const RELAY_CHAIN_SLOT_DURATION_MILLIS: u32,
		const V: u32,
		const C: u32,
	> ConsensusHook for FixedVelocityConsensusHook<T, RELAY_CHAIN_SLOT_DURATION_MILLIS, V, C>
where
	<T as pallet_timestamp::Config>::Moment: Into<u64>,
{
	// Validates the number of authored blocks within the slot with respect to the `V + 1` limit.
	fn on_state_proof(state_proof: &RelayChainStateProof) -> (Weight, UnincludedSegmentCapacity) {
		// Ensure velocity is non-zero.
		let velocity = V.max(1);
		let relay_chain_slot = state_proof.read_slot().expect("failed to read relay chain slot");

		let (relay_chain_slot, authored_in_relay) = match pallet::RelaySlotInfo::<T>::get() {
			Some((slot, authored)) if slot == relay_chain_slot => (slot, authored),
			Some((slot, _)) if slot < relay_chain_slot => (relay_chain_slot, 0),
			Some((slot, _)) => {
				panic!("Slot moved backwards: stored_slot={slot:?}, relay_chain_slot={relay_chain_slot:?}")
			},
			None => (relay_chain_slot, 0),
		};

		// We need to allow one additional block to be built to fill the unincluded segment.
		if authored_in_relay > velocity {
			panic!("authored blocks limit is reached for the slot: relay_chain_slot={relay_chain_slot:?}, authored={authored_in_relay:?}, velocity={velocity:?}");
		}

		pallet::RelaySlotInfo::<T>::put((relay_chain_slot, authored_in_relay + 1));

		let para_slot = pallet_aura::CurrentSlot::<T>::get();

		// Convert relay chain timestamp.
		let relay_chain_timestamp =
			u64::from(RELAY_CHAIN_SLOT_DURATION_MILLIS).saturating_mul(*relay_chain_slot);

		let para_slot_duration = SlotDuration::from_millis(Aura::<T>::slot_duration().into());
		let para_slot_from_relay =
			Slot::from_timestamp(relay_chain_timestamp.into(), para_slot_duration);

		// Check that we are not too far in the future. Since we expect `V` parachain blocks
		// during the relay chain slot, we can allow for `V` parachain slots into the future.
		if *para_slot > *para_slot_from_relay + u64::from(velocity) {
			panic!(
				"Parachain slot is too far in the future: parachain_slot={:?}, derived_from_relay_slot={:?} velocity={:?}, relay_chain_slot={:?}",
				para_slot,
				para_slot_from_relay,
				velocity,
				relay_chain_slot
			);
		}

		let weight = T::DbWeight::get().reads(1);

		(
			weight,
			NonZeroU32::new(core::cmp::max(C, 1))
				.expect("1 is the minimum value and non-zero; qed")
				.into(),
		)
	}
}

impl<
		T: pallet::Config + parachain_system::Config,
		const RELAY_CHAIN_SLOT_DURATION_MILLIS: u32,
		const V: u32,
		const C: u32,
	> FixedVelocityConsensusHook<T, RELAY_CHAIN_SLOT_DURATION_MILLIS, V, C>
{
	/// Whether it is legal to extend the chain, assuming the given block is the most
	/// recently included one as-of the relay parent that will be built against, and
	/// the given slot.
	///
	/// This should be consistent with the logic the runtime uses when validating blocks to
	/// avoid issues.
	///
	/// When the unincluded segment is empty, i.e. `included_hash == at`, where at is the block
	/// whose state we are querying against, this must always return `true` as long as the slot
	/// is more recent than the included block itself.
	pub fn can_build_upon(included_hash: T::Hash, new_slot: Slot) -> bool {
		let velocity = V.max(1);
		let (last_slot, authored_so_far) = match pallet::RelaySlotInfo::<T>::get() {
			None => return true,
			Some(x) => x,
		};

		let size_after_included =
			parachain_system::Pallet::<T>::unincluded_segment_size_after(included_hash);

		// can never author when the unincluded segment is full.
		if size_after_included >= C {
			return false
		}

		// Check that we have not authored more than `V + 1` parachain blocks in the current relay
		// chain slot.
		if last_slot == new_slot {
			authored_so_far < velocity + 1
		} else {
			// disallow slot from moving backwards.
			last_slot < new_slot
		}
	}
}
