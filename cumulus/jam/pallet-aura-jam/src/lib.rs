// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! Aura for JAM parachains.
//!
//! Replaces `pallet-aura` and `cumulus-pallet-aura-ext` on JAM. JAM authorizes a parachain's work
//! packages with an authorizer that commits to one Aura authority set and switches a core's
//! authorizer at a JAM slot, so the authority set of a block is the set JAM authorizes at the
//! block's anchor slot (from its verified `JamParent` digest):
//!
//! - The pallet is the session handler of the `aura` key: every session change schedules the set
//!   pallet-session queued for the next session at the next switch slot, one session ahead, so
//!   coretime can schedule the matching authorizer.
//! - [`JamSlotSessions`] ends a session once the block's anchor slot reaches the next switch slot.
//! - [`Pallet::authorities_at`] is the set for an anchor slot, for the seal check, the author
//!   lookup and the collator.
//! - [`BlockExecutor`] checks the seal against that set, so the first block anchored at a switch
//!   slot is already sealed by the new set.
//! - [`JamFixedVelocityConsensusHook`], the `cumulus-pallet-jam-system` consensus hook, checks that
//!   the block's Aura slot is the anchor's.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod block_executor;
mod consensus_hook;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use block_executor::BlockExecutor;
pub use consensus_hook::JamFixedVelocityConsensusHook;
pub use pallet::*;

use alloc::vec::Vec;
use cumulus_primitives_core::CumulusDigestItem;
use frame_support::{
	pallet_prelude::*,
	traits::{EstimateNextSessionRotation, FindAuthor, OnTimestampSet, OneSessionHandler},
	BoundedVec, ConsensusEngineId,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_consensus_aura::{digests::CompatibleDigestItem, Slot, AURA_ENGINE_ID};
use sp_runtime::{
	traits::{Get, MaybeSerializeDeserialize, Member, SaturatedConversion},
	BoundToRuntimeAppPublic, Permill, RuntimeAppPublic,
};

type AuthoritySetOf<T> = BoundedVec<<T as Config>::AuthorityId, <T as Config>::MaxAuthorities>;

/// How many scheduled sets are kept: the one switching at the current boundary and the next one,
/// plus slack for a chain that rotates faster than JAM reaches the slots.
pub type MaxPendingSets = ConstU32<4>;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// The identifier type for an authority.
		type AuthorityId: Member
			+ Parameter
			+ RuntimeAppPublic
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen;
		/// The maximum number of authorities in a set.
		#[pallet::constant]
		type MaxAuthorities: Get<u32>;
		/// Milliseconds per Aura slot.
		#[pallet::constant]
		type SlotDuration: Get<u64>;
		/// JAM slots between two switches.
		#[pallet::constant]
		type RotationPeriod: Get<u32>;
	}

	/// The set JAM authorizes now.
	#[pallet::storage]
	pub type Authorities<T: Config> = StorageValue<_, AuthoritySetOf<T>, ValueQuery>;

	/// Scheduled sets with the JAM slot each starts at, ascending.
	#[pallet::storage]
	pub type Pending<T: Config> =
		StorageValue<_, BoundedVec<(u32, AuthoritySetOf<T>), MaxPendingSets>, ValueQuery>;

	/// The JAM slot at which JAM switches to the next scheduled set.
	#[pallet::storage]
	pub type NextSwitch<T> = StorageValue<_, u32, OptionQuery>;

	/// The Aura slot of the current block.
	#[pallet::storage]
	pub type CurrentSlot<T> = StorageValue<_, Slot, ValueQuery>;

	/// The last anchor slot a block was built on and how many blocks were built on it.
	#[pallet::storage]
	pub type SlotInfo<T> = StorageValue<_, (u32, u32), OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// `authorities` take over at JAM slot `from_slot`.
		AuthoritiesScheduled { from_slot: u32, authorities: Vec<T::AuthorityId> },
		/// The set scheduled for `from_slot` is now the one JAM authorizes.
		AuthoritiesActivated { from_slot: u32 },
	}

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// The initial set, unless pallet-session provides it.
		pub authorities: Vec<T::AuthorityId>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			if !self.authorities.is_empty() {
				Authorities::<T>::put(BoundedVec::truncate_from(self.authorities.clone()));
			}
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			if let Some(slot) = Self::slot_from_digest() {
				assert!(CurrentSlot::<T>::get() <= slot, "Slot must not decrease");
				CurrentSlot::<T>::put(slot);
			}
			if let Some(anchor) = Self::anchor_slot() {
				// `BlockExecutor` checks the seal against the set at the anchor, read from the
				// parent's state. Read it before this block changes it, to get it into the storage
				// proof of the PoV.
				Self::authorities_at(anchor);
				Self::activate_due(anchor);
			}
			T::DbWeight::get().reads_writes(4, 3)
		}

		fn integrity_test() {
			assert!(T::SlotDuration::get() > 0, "Aura slot duration cannot be zero.");
		}
	}
}

impl<T: Config> Pallet<T> {
	/// The anchor slot from this block's `JamParent` digest.
	pub fn anchor_slot() -> Option<u32> {
		CumulusDigestItem::find_jam_parent_info(&frame_system::Pallet::<T>::digest())
			.map(|parent| parent.anchor_slot)
	}

	/// The Aura slot from this block's pre-runtime digest.
	fn slot_from_digest() -> Option<Slot> {
		frame_system::Pallet::<T>::digest()
			.logs()
			.iter()
			.find_map(|item| CompatibleDigestItem::<()>::as_aura_pre_digest(item))
	}

	/// Milliseconds per Aura slot.
	pub fn slot_duration() -> u64 {
		T::SlotDuration::get()
	}

	/// The set JAM authorizes for a package anchored at `anchor_slot`.
	pub fn authorities_at(anchor_slot: u32) -> Vec<T::AuthorityId> {
		Pending::<T>::get()
			.into_iter()
			.rev()
			.find(|(slot, _)| *slot <= anchor_slot)
			.map(|(_, set)| set)
			.unwrap_or_else(Authorities::<T>::get)
			.into_inner()
	}

	/// The scheduled sets with the slot each starts at.
	pub fn scheduled_changes() -> Vec<(u32, Vec<T::AuthorityId>)> {
		Pending::<T>::get()
			.into_iter()
			.map(|(slot, set)| (slot, set.into_inner()))
			.collect()
	}

	/// A session started in a block anchored at `anchor`: `active` starts at the boundary that
	/// ended the last session, `queued` at the boundary after it.
	fn on_rotation(anchor: u32, active: AuthoritySetOf<T>, queued: AuthoritySetOf<T>) {
		let switch = NextSwitch::<T>::get().unwrap_or(anchor);
		Self::schedule(switch, active);

		// Re-based on this block rather than on a fixed grid, so a stalled chain rotates once.
		let next = switch.max(anchor).saturating_add(T::RotationPeriod::get());
		NextSwitch::<T>::put(next);
		Self::schedule(next, queued);
		Self::activate_due(anchor);
	}

	fn schedule(slot: u32, set: AuthoritySetOf<T>) {
		if Self::authorities_at(slot) == set.as_slice() {
			return;
		}
		let mut pending = Pending::<T>::get();
		pending.retain(|(at, _)| *at < slot);
		if pending.is_full() {
			let (_, oldest) = pending.remove(0);
			Authorities::<T>::put(oldest);
		}
		let authorities = set.to_vec();
		pending.try_push((slot, set)).expect("one entry was freed above; qed");
		Pending::<T>::put(pending);
		Self::deposit_event(Event::AuthoritiesScheduled { from_slot: slot, authorities });
	}

	fn activate_due(anchor: u32) {
		let mut pending = Pending::<T>::get();
		let due = pending.iter().take_while(|(slot, _)| *slot <= anchor).count();
		if due == 0 {
			return;
		}
		for (slot, set) in pending.drain(..due) {
			Authorities::<T>::put(set);
			Self::deposit_event(Event::AuthoritiesActivated { from_slot: slot });
		}
		Pending::<T>::put(pending);
	}

	/// Count the block against its anchor slot and check that its Aura slot is the anchor's.
	///
	/// The Aura slot comes from the block's pre-runtime digest, not [`CurrentSlot`]: the consensus
	/// hook calls this from `JamSystem`'s `on_initialize`, before this pallet's `on_initialize`
	/// stores the block's slot.
	///
	/// Panics when more than `velocity + 1` blocks build on one anchor slot, when the anchor slot
	/// moves backwards, when the block has no Aura pre-digest, or when its Aura slot is not the one
	/// of the anchor.
	pub fn check_anchor_slot(anchor: u32, velocity: u32) {
		let velocity = velocity.max(1);
		let authored = match SlotInfo::<T>::get() {
			Some((slot, authored)) if slot == anchor => authored,
			Some((slot, _)) if slot < anchor => 0,
			Some((slot, _)) => {
				panic!("Anchor slot moved backwards: stored={slot}, anchor={anchor}")
			},
			None => 0,
		};
		if authored > velocity {
			panic!("authored blocks limit is reached for anchor slot {anchor}: authored={authored}, velocity={velocity}");
		}
		SlotInfo::<T>::put((anchor, authored + 1));

		let expected =
			cumulus_jam_aura_primitives::aura_slot_for_anchor(anchor, T::SlotDuration::get());
		let para_slot = *Self::slot_from_digest().expect("Could not find an AuRa pre-digest!");
		if para_slot != expected {
			panic!("Aura slot must be the anchor's: aura_slot={para_slot}, expected={expected}, anchor_slot={anchor}");
		}
	}
}

impl<T: Config> BoundToRuntimeAppPublic for Pallet<T> {
	type Public = T::AuthorityId;
}

impl<T: Config> OneSessionHandler<T::AccountId> for Pallet<T> {
	type Key = T::AuthorityId;

	fn on_genesis_session<'a, I: 'a>(validators: I)
	where
		I: Iterator<Item = (&'a T::AccountId, T::AuthorityId)>,
	{
		Authorities::<T>::put(BoundedVec::truncate_from(validators.map(|(_, k)| k).collect()));
	}

	fn on_new_session<'a, I: 'a>(_changed: bool, validators: I, queued_validators: I)
	where
		I: Iterator<Item = (&'a T::AccountId, T::AuthorityId)>,
	{
		let active = BoundedVec::truncate_from(validators.map(|(_, k)| k).collect());
		let queued = BoundedVec::truncate_from(queued_validators.map(|(_, k)| k).collect());
		match Self::anchor_slot() {
			Some(anchor) => Self::on_rotation(anchor, active, queued),
			// Without an anchor there is no JAM slot to bind to; enact right away.
			None => Authorities::<T>::put(active),
		}
	}

	fn on_disabled(_: u32) {}
}

impl<T: Config> FindAuthor<u32> for Pallet<T> {
	/// The index in the set JAM authorizes at this block's anchor slot.
	fn find_author<'a, I>(digests: I) -> Option<u32>
	where
		I: 'a + IntoIterator<Item = (ConsensusEngineId, &'a [u8])>,
	{
		let slot = digests.into_iter().find_map(|(id, mut data)| {
			(id == AURA_ENGINE_ID).then(|| Slot::decode(&mut data).ok()).flatten()
		})?;
		let len = Self::authorities_at(Self::anchor_slot()?).len() as u64;
		(len > 0).then(|| (*slot % len) as u32)
	}
}

impl<T: Config + pallet_timestamp::Config> OnTimestampSet<T::Moment> for Pallet<T> {
	fn on_timestamp_set(moment: T::Moment) {
		let timestamp_slot = Slot::from(moment.saturated_into::<u64>() / T::SlotDuration::get());
		assert_eq!(
			CurrentSlot::<T>::get(),
			timestamp_slot,
			"Timestamp slot must match `CurrentSlot`.",
		);
	}
}

/// Ends a session once the block's anchor slot reaches the next switch.
pub struct JamSlotSessions<T>(PhantomData<T>);

impl<T: Config> pallet_session::ShouldEndSession<BlockNumberFor<T>> for JamSlotSessions<T> {
	fn should_end_session(_: BlockNumberFor<T>) -> bool {
		let Some(anchor) = Pallet::<T>::anchor_slot() else { return false };
		match NextSwitch::<T>::get() {
			None => {
				NextSwitch::<T>::put(anchor.saturating_add(T::RotationPeriod::get()));
				false
			},
			Some(switch) => anchor >= switch,
		}
	}
}

impl<T: Config> EstimateNextSessionRotation<BlockNumberFor<T>> for JamSlotSessions<T> {
	fn average_session_length() -> BlockNumberFor<T> {
		T::RotationPeriod::get().into()
	}

	fn estimate_current_session_progress(_: BlockNumberFor<T>) -> (Option<Permill>, Weight) {
		(None, T::DbWeight::get().reads(1))
	}

	fn estimate_next_session_rotation(_: BlockNumberFor<T>) -> (Option<BlockNumberFor<T>>, Weight) {
		(None, T::DbWeight::get().reads(1))
	}
}
