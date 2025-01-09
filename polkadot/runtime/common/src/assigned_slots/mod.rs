// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! This pallet allows to assign permanent (long-lived) or temporary
//! (short-lived) parachain slots to paras, leveraging the existing
//! parachain slot lease mechanism. Temporary slots are given turns
//! in a fair (though best-effort) manner.
//! The dispatchables must be called from the configured origin
//! (typically `Sudo` or a governance origin).
//! This pallet should not be used on a production relay chain,
//! only on a test relay chain (e.g. Rococo).

#![cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
pub mod migration;

use crate::{
	slots::{self, Pallet as Slots, WeightInfo as SlotsWeightInfo},
	traits::{LeaseError, Leaser, Registrar},
};
use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::*, traits::Currency};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use polkadot_primitives::Id as ParaId;
use polkadot_runtime_parachains::{
	configuration,
	paras::{self},
};
use scale_info::TypeInfo;
use sp_runtime::traits::{One, Saturating, Zero};

const LOG_TARGET: &str = "runtime::assigned_slots";

/// Lease period an assigned slot should start from (current, or next one).
#[derive(Encode, Decode, Clone, Copy, Eq, PartialEq, RuntimeDebug, TypeInfo)]
pub enum SlotLeasePeriodStart {
	Current,
	Next,
}

/// Information about a temporary parachain slot.
#[derive(Encode, Decode, Clone, PartialEq, Eq, Default, MaxEncodedLen, RuntimeDebug, TypeInfo)]
pub struct ParachainTemporarySlot<AccountId, LeasePeriod> {
	/// Manager account of the para.
	pub manager: AccountId,
	/// Lease period the parachain slot should ideally start from,
	/// As slot are allocated in a best-effort manner, this could be later,
	/// but not earlier than the specified period.
	pub period_begin: LeasePeriod,
	/// Number of lease period the slot lease will last.
	/// This is set to the value configured in `TemporarySlotLeasePeriodLength`.
	pub period_count: LeasePeriod,
	/// Last lease period this slot had a turn in (incl. current).
	/// This is set to the beginning period of a slot.
	pub last_lease: Option<LeasePeriod>,
	/// Number of leases this temporary slot had (incl. current).
	pub lease_count: u32,
}

pub trait WeightInfo {
	fn assign_perm_parachain_slot() -> Weight;
	fn assign_temp_parachain_slot() -> Weight;
	fn unassign_parachain_slot() -> Weight;
	fn set_max_permanent_slots() -> Weight;
	fn set_max_temporary_slots() -> Weight;
}

pub struct TestWeightInfo;
impl WeightInfo for TestWeightInfo {
	fn assign_perm_parachain_slot() -> Weight {
		Weight::zero()
	}
	fn assign_temp_parachain_slot() -> Weight {
		Weight::zero()
	}
	fn unassign_parachain_slot() -> Weight {
		Weight::zero()
	}
	fn set_max_permanent_slots() -> Weight {
		Weight::zero()
	}
	fn set_max_temporary_slots() -> Weight {
		Weight::zero()
	}
}

type BalanceOf<T> = <<<T as Config>::Leaser as Leaser<BlockNumberFor<T>>>::Currency as Currency<
	<T as frame_system::Config>::AccountId,
>>::Balance;
type LeasePeriodOf<T> = <<T as Config>::Leaser as Leaser<BlockNumberFor<T>>>::LeasePeriod;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: configuration::Config + paras::Config + slots::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Origin for assigning slots.
		type AssignSlotOrigin: EnsureOrigin<<Self as frame_system::Config>::RuntimeOrigin>;

		/// The type representing the leasing system.
		type Leaser: Leaser<
			BlockNumberFor<Self>,
			AccountId = Self::AccountId,
			LeasePeriod = BlockNumberFor<Self>,
		>;

		/// The number of lease periods a permanent parachain slot lasts.
		#[pallet::constant]
		type PermanentSlotLeasePeriodLength: Get<u32>;

		/// The number of lease periods a temporary parachain slot lasts.
		#[pallet::constant]
		type TemporarySlotLeasePeriodLength: Get<u32>;

		/// The max number of temporary slots to be scheduled per lease periods.
		#[pallet::constant]
		type MaxTemporarySlotPerLeasePeriod: Get<u32>;

		/// Weight Information for the Extrinsics in the Pallet
		type WeightInfo: WeightInfo;
	}

	/// Assigned permanent slots, with their start lease period, and duration.
	#[pallet::storage]
	pub type PermanentSlots<T: Config> =
		StorageMap<_, Twox64Concat, ParaId, (LeasePeriodOf<T>, LeasePeriodOf<T>), OptionQuery>;

	/// Number of assigned (and active) permanent slots.
	#[pallet::storage]
	pub type PermanentSlotCount<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Assigned temporary slots.
	#[pallet::storage]
	pub type TemporarySlots<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ParaId,
		ParachainTemporarySlot<T::AccountId, LeasePeriodOf<T>>,
		OptionQuery,
	>;

	/// Number of assigned temporary slots.
	#[pallet::storage]
	pub type TemporarySlotCount<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Number of active temporary slots in current slot lease period.
	#[pallet::storage]
	pub type ActiveTemporarySlotCount<T: Config> = StorageValue<_, u32, ValueQuery>;

	///  The max number of temporary slots that can be assigned.
	#[pallet::storage]
	pub type MaxTemporarySlots<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// The max number of permanent slots that can be assigned.
	#[pallet::storage]
	pub type MaxPermanentSlots<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub max_temporary_slots: u32,
		pub max_permanent_slots: u32,
		#[serde(skip)]
		pub _config: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			MaxPermanentSlots::<T>::put(&self.max_permanent_slots);
			MaxTemporarySlots::<T>::put(&self.max_temporary_slots);
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A parachain was assigned a permanent parachain slot
		PermanentSlotAssigned(ParaId),
		/// A parachain was assigned a temporary parachain slot
		TemporarySlotAssigned(ParaId),
		/// The maximum number of permanent slots has been changed
		MaxPermanentSlotsChanged { slots: u32 },
		/// The maximum number of temporary slots has been changed
		MaxTemporarySlotsChanged { slots: u32 },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The specified parachain is not registered.
		ParaDoesntExist,
		/// Not a parathread (on-demand parachain).
		NotParathread,
		/// Cannot upgrade on-demand parachain to lease holding
		/// parachain.
		CannotUpgrade,
		/// Cannot downgrade lease holding parachain to
		/// on-demand.
		CannotDowngrade,
		/// Permanent or Temporary slot already assigned.
		SlotAlreadyAssigned,
		/// Permanent or Temporary slot has not been assigned.
		SlotNotAssigned,
		/// An ongoing lease already exists.
		OngoingLeaseExists,
		// The maximum number of permanent slots exceeded
		MaxPermanentSlotsExceeded,
		// The maximum number of temporary slots exceeded
		MaxTemporarySlotsExceeded,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			if let Some((lease_period, first_block)) = Self::lease_period_index(n) {
				// If we're beginning a new lease period then handle that.
				if first_block {
					return Self::manage_lease_period_start(lease_period)
				}
			}

			// We didn't return early above, so we didn't do anything.
			Weight::zero()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Assign a permanent parachain slot and immediately create a lease for it.
		#[pallet::call_index(0)]
		#[pallet::weight((<T as Config>::WeightInfo::assign_perm_parachain_slot(), DispatchClass::Operational))]
		pub fn assign_perm_parachain_slot(origin: OriginFor<T>, id: ParaId) -> DispatchResult {
			T::AssignSlotOrigin::ensure_origin(origin)?;

			let manager = T::Registrar::manager_of(id).ok_or(Error::<T>::ParaDoesntExist)?;

			ensure!(T::Registrar::is_parathread(id), Error::<T>::NotParathread);

			ensure!(
				!Self::has_permanent_slot(id) && !Self::has_temporary_slot(id),
				Error::<T>::SlotAlreadyAssigned
			);

			let current_lease_period: BlockNumberFor<T> = Self::current_lease_period_index();
			ensure!(
				!T::Leaser::already_leased(
					id,
					current_lease_period,
					// Check current lease & next one
					current_lease_period.saturating_add(
						BlockNumberFor::<T>::from(2u32)
							.saturating_mul(T::PermanentSlotLeasePeriodLength::get().into())
					)
				),
				Error::<T>::OngoingLeaseExists
			);

			ensure!(
				PermanentSlotCount::<T>::get() < MaxPermanentSlots::<T>::get(),
				Error::<T>::MaxPermanentSlotsExceeded
			);

			// Permanent slot assignment fails if a lease cannot be created
			Self::configure_slot_lease(
				id,
				manager,
				current_lease_period,
				T::PermanentSlotLeasePeriodLength::get().into(),
			)
			.map_err(|_| Error::<T>::CannotUpgrade)?;

			PermanentSlots::<T>::insert(
				id,
				(
					current_lease_period,
					LeasePeriodOf::<T>::from(T::PermanentSlotLeasePeriodLength::get()),
				),
			);
			PermanentSlotCount::<T>::mutate(|count| count.saturating_inc());

			Self::deposit_event(Event::<T>::PermanentSlotAssigned(id));
			Ok(())
		}

		/// Assign a temporary parachain slot. The function tries to create a lease for it
		/// immediately if `SlotLeasePeriodStart::Current` is specified, and if the number
		/// of currently active temporary slots is below `MaxTemporarySlotPerLeasePeriod`.
		#[pallet::call_index(1)]
		#[pallet::weight((<T as Config>::WeightInfo::assign_temp_parachain_slot(), DispatchClass::Operational))]
		pub fn assign_temp_parachain_slot(
			origin: OriginFor<T>,
			id: ParaId,
			lease_period_start: SlotLeasePeriodStart,
		) -> DispatchResult {
			T::AssignSlotOrigin::ensure_origin(origin)?;

			let manager = T::Registrar::manager_of(id).ok_or(Error::<T>::ParaDoesntExist)?;

			ensure!(T::Registrar::is_parathread(id), Error::<T>::NotParathread);

			ensure!(
				!Self::has_permanent_slot(id) && !Self::has_temporary_slot(id),
				Error::<T>::SlotAlreadyAssigned
			);

			let current_lease_period: BlockNumberFor<T> = Self::current_lease_period_index();
			ensure!(
				!T::Leaser::already_leased(
					id,
					current_lease_period,
					// Check current lease & next one
					current_lease_period.saturating_add(
						BlockNumberFor::<T>::from(2u32)
							.saturating_mul(T::TemporarySlotLeasePeriodLength::get().into())
					)
				),
				Error::<T>::OngoingLeaseExists
			);

			ensure!(
				TemporarySlotCount::<T>::get() < MaxTemporarySlots::<T>::get(),
				Error::<T>::MaxTemporarySlotsExceeded
			);

			let mut temp_slot = ParachainTemporarySlot {
				manager: manager.clone(),
				period_begin: match lease_period_start {
					SlotLeasePeriodStart::Current => current_lease_period,
					SlotLeasePeriodStart::Next => current_lease_period + One::one(),
				},
				period_count: T::TemporarySlotLeasePeriodLength::get().into(),
				last_lease: None,
				lease_count: 0,
			};

			if lease_period_start == SlotLeasePeriodStart::Current &&
				ActiveTemporarySlotCount::<T>::get() < T::MaxTemporarySlotPerLeasePeriod::get()
			{
				// Try to allocate slot directly
				match Self::configure_slot_lease(
					id,
					manager,
					temp_slot.period_begin,
					temp_slot.period_count,
				) {
					Ok(_) => {
						ActiveTemporarySlotCount::<T>::mutate(|count| count.saturating_inc());
						temp_slot.last_lease = Some(temp_slot.period_begin);
						temp_slot.lease_count += 1;
					},
					Err(err) => {
						// Treat failed lease creation as warning .. slot will be allocated a lease
						// in a subsequent lease period by the `allocate_temporary_slot_leases`
						// function.
						log::warn!(
							target: LOG_TARGET,
							"Failed to allocate a temp slot for para {:?} at period {:?}: {:?}",
							id,
							current_lease_period,
							err
						);
					},
				}
			}

			TemporarySlots::<T>::insert(id, temp_slot);
			TemporarySlotCount::<T>::mutate(|count| count.saturating_inc());

			Self::deposit_event(Event::<T>::TemporarySlotAssigned(id));

			Ok(())
		}

		/// Unassign a permanent or temporary parachain slot
		#[pallet::call_index(2)]
		#[pallet::weight((<T as Config>::WeightInfo::unassign_parachain_slot(), DispatchClass::Operational))]
		pub fn unassign_parachain_slot(origin: OriginFor<T>, id: ParaId) -> DispatchResult {
			T::AssignSlotOrigin::ensure_origin(origin.clone())?;

			ensure!(
				Self::has_permanent_slot(id) || Self::has_temporary_slot(id),
				Error::<T>::SlotNotAssigned
			);

			// Check & cache para status before we clear the lease
			let is_parachain = Self::is_parachain(id);

			// Remove perm or temp slot
			Self::clear_slot_leases(origin.clone(), id)?;

			if PermanentSlots::<T>::contains_key(id) {
				PermanentSlots::<T>::remove(id);
				PermanentSlotCount::<T>::mutate(|count| *count = count.saturating_sub(One::one()));
			} else if TemporarySlots::<T>::contains_key(id) {
				TemporarySlots::<T>::remove(id);
				TemporarySlotCount::<T>::mutate(|count| *count = count.saturating_sub(One::one()));
				if is_parachain {
					ActiveTemporarySlotCount::<T>::mutate(|active_count| {
						*active_count = active_count.saturating_sub(One::one())
					});
				}
			}

			// Force downgrade to on-demand parachain (if needed) before end of lease period
			if is_parachain {
				if let Err(err) = polkadot_runtime_parachains::schedule_parachain_downgrade::<T>(id)
				{
					// Treat failed downgrade as warning .. slot lease has been cleared,
					// so the parachain will be downgraded anyway by the slots pallet
					// at the end of the lease period .
					log::warn!(
						target: LOG_TARGET,
						"Failed to downgrade parachain {:?} at period {:?}: {:?}",
						id,
						Self::current_lease_period_index(),
						err
					);
				}
			}

			Ok(())
		}

		/// Sets the storage value [`MaxPermanentSlots`].
		#[pallet::call_index(3)]
		#[pallet::weight((<T as Config>::WeightInfo::set_max_permanent_slots(), DispatchClass::Operational))]
		pub fn set_max_permanent_slots(origin: OriginFor<T>, slots: u32) -> DispatchResult {
			ensure_root(origin)?;

			MaxPermanentSlots::<T>::put(slots);

			Self::deposit_event(Event::<T>::MaxPermanentSlotsChanged { slots });
			Ok(())
		}

		/// Sets the storage value [`MaxTemporarySlots`].
		#[pallet::call_index(4)]
		#[pallet::weight((<T as Config>::WeightInfo::set_max_temporary_slots(), DispatchClass::Operational))]
		pub fn set_max_temporary_slots(origin: OriginFor<T>, slots: u32) -> DispatchResult {
			ensure_root(origin)?;

			MaxTemporarySlots::<T>::put(slots);

			Self::deposit_event(Event::<T>::MaxTemporarySlotsChanged { slots });
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Allocate temporary slot leases up to `MaxTemporarySlotPerLeasePeriod` per lease period.
	/// Beyond the already active temporary slot leases, this function will activate more leases
	/// in the following order of preference:
	/// - Assigned slots that didn't have a turn yet, though their `period_begin` has passed.
	/// - Assigned slots that already had one (or more) turn(s): they will be considered for the
	/// current slot lease if they weren't active in the preceding one, and will be ranked by
	/// total number of lease (lower first), and then when they last a turn (older ones first).
	/// If any remaining ex-aequo, we just take the para ID in ascending order as discriminator.
	///
	/// Assigned slots with a `period_begin` bigger than current lease period are not considered
	/// (yet).
	///
	/// The function will call out to `Leaser::lease_out` to create the appropriate slot leases.
	fn allocate_temporary_slot_leases(lease_period_index: LeasePeriodOf<T>) -> DispatchResult {
		let mut active_temp_slots = 0u32;
		let mut pending_temp_slots = Vec::new();
		TemporarySlots::<T>::iter().for_each(|(para, slot)| {
				match slot.last_lease {
					Some(last_lease)
						if last_lease <= lease_period_index &&
							lease_period_index <
								(last_lease.saturating_add(slot.period_count)) =>
					{
						// Active slot lease
						active_temp_slots += 1;
					}
					Some(last_lease)
						// Slot w/ past lease, only consider it every other slot lease period (times period_count)
						if last_lease.saturating_add(slot.period_count.saturating_mul(2u32.into())) <= lease_period_index => {
							pending_temp_slots.push((para, slot));
					},
					None if slot.period_begin <= lease_period_index => {
						// Slot hasn't had a lease yet
						pending_temp_slots.insert(0, (para, slot));
					},
					_ => {
						// Slot not being considered for this lease period (will be for a subsequent one)
					},
				}
		});

		let mut newly_created_lease = 0u32;
		if active_temp_slots < T::MaxTemporarySlotPerLeasePeriod::get() &&
			!pending_temp_slots.is_empty()
		{
			// Sort by lease_count, favoring slots that had no or less turns first
			// (then by last_lease index, and then Para ID)
			pending_temp_slots.sort_by(|a, b| {
				a.1.lease_count
					.cmp(&b.1.lease_count)
					.then_with(|| a.1.last_lease.cmp(&b.1.last_lease))
					.then_with(|| a.0.cmp(&b.0))
			});

			let slots_to_be_upgraded = pending_temp_slots.iter().take(
				(T::MaxTemporarySlotPerLeasePeriod::get().saturating_sub(active_temp_slots))
					as usize,
			);

			for (id, temp_slot) in slots_to_be_upgraded {
				TemporarySlots::<T>::try_mutate::<_, _, Error<T>, _>(id, |s| {
					// Configure temp slot lease
					Self::configure_slot_lease(
						*id,
						temp_slot.manager.clone(),
						lease_period_index,
						temp_slot.period_count,
					)
					.map_err(|_| Error::<T>::CannotUpgrade)?;

					// Update temp slot lease info in storage
					*s = Some(ParachainTemporarySlot {
						manager: temp_slot.manager.clone(),
						period_begin: temp_slot.period_begin,
						period_count: temp_slot.period_count,
						last_lease: Some(lease_period_index),
						lease_count: temp_slot.lease_count + 1,
					});

					newly_created_lease += 1;

					Ok(())
				})?;
			}
		}

		ActiveTemporarySlotCount::<T>::set(active_temp_slots + newly_created_lease);

		Ok(())
	}

	/// Clear out all slot leases for both permanent & temporary slots.
	/// The function merely calls out to `Slots::clear_all_leases`.
	fn clear_slot_leases(origin: OriginFor<T>, id: ParaId) -> DispatchResult {
		Slots::<T>::clear_all_leases(origin, id)
	}

	/// Create a parachain slot lease based on given params.
	/// The function merely calls out to `Leaser::lease_out`.
	fn configure_slot_lease(
		para: ParaId,
		manager: T::AccountId,
		lease_period: LeasePeriodOf<T>,
		lease_duration: LeasePeriodOf<T>,
	) -> Result<(), LeaseError> {
		T::Leaser::lease_out(para, &manager, BalanceOf::<T>::zero(), lease_period, lease_duration)
	}

	/// Returns whether a para has been assigned a permanent slot.
	fn has_permanent_slot(id: ParaId) -> bool {
		PermanentSlots::<T>::contains_key(id)
	}

	/// Returns whether a para has been assigned temporary slot.
	fn has_temporary_slot(id: ParaId) -> bool {
		TemporarySlots::<T>::contains_key(id)
	}

	/// Returns whether a para is currently a lease holding parachain.
	fn is_parachain(id: ParaId) -> bool {
		T::Registrar::is_parachain(id)
	}

	/// Returns current lease period index.
	fn current_lease_period_index() -> LeasePeriodOf<T> {
		T::Leaser::lease_period_index(frame_system::Pallet::<T>::block_number())
			.and_then(|x| Some(x.0))
			.unwrap()
	}

	/// Returns lease period index for block
	fn lease_period_index(block: BlockNumberFor<T>) -> Option<(LeasePeriodOf<T>, bool)> {
		T::Leaser::lease_period_index(block)
	}

	/// Handles start of a lease period.
	fn manage_lease_period_start(lease_period_index: LeasePeriodOf<T>) -> Weight {
		// Note: leases that have ended in previous lease period, should have been cleaned in slots
		// pallet.
		if let Err(err) = Self::allocate_temporary_slot_leases(lease_period_index) {
			log::error!(
				target: LOG_TARGET,
				"Allocating slots failed for lease period {:?}, with: {:?}",
				lease_period_index,
				err
			);
		}
		<T as slots::Config>::WeightInfo::force_lease() *
			(T::MaxTemporarySlotPerLeasePeriod::get() as u64)
	}
}

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;
