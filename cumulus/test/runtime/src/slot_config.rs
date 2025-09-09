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

//! Pallet for dynamic slot duration configuration

pub use pallet::*;
use frame_support::traits::Get;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{
		dispatch::DispatchResult,
		pallet_prelude::*,
	};
	use frame_system::pallet_prelude::*;

	/// The current pallet version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// The origin that can update slot duration (Root, Sudo, or Democracy)
		type UpdateOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Default slot duration in milliseconds
		#[pallet::constant]
		type DefaultSlotDuration: Get<u64>;
	}

	/// Storage for the current slot duration in milliseconds
	#[pallet::storage]
	#[pallet::getter(fn slot_duration)]
	pub type SlotDuration<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// Genesis configuration for the pallet
	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Initial slot duration in milliseconds
		pub slot_duration: u64,
		#[serde(skip)]
		pub _config: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			// Set initial slot duration or use default
			let initial_duration = if self.slot_duration > 0 {
				self.slot_duration
			} else {
				T::DefaultSlotDuration::get()
			};
			SlotDuration::<T>::put(initial_duration);
		}
	}

	/// Events emitted by the pallet
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Slot duration was updated. [old_duration, new_duration]
		SlotDurationUpdated { old_duration: u64, new_duration: u64 },
	}

	/// Errors that can occur in this pallet
	#[pallet::error]
	pub enum Error<T> {
		/// Slot duration cannot be zero
		ZeroSlotDuration,
		/// Slot duration is too small (minimum 1000ms = 1 second)
		SlotDurationTooSmall,
		/// Slot duration is too large (maximum 60000ms = 1 minute)
		SlotDurationTooLarge,
	}

	/// Dispatchable functions (extrinsics) for the pallet
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Update the slot duration
		/// 
		/// The origin must be the configured UpdateOrigin (typically Root or Sudo).
		/// 
		/// Parameters:
		/// - `new_duration`: The new slot duration in milliseconds
		/// 
		/// Emits `SlotDurationUpdated` event.
		#[pallet::call_index(0)]
		#[pallet::weight(T::DbWeight::get().reads_writes(1, 1))]
		pub fn set_slot_duration(
			origin: OriginFor<T>,
			new_duration: u64,
		) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;

			// Validate the new duration
			ensure!(new_duration > 0, Error::<T>::ZeroSlotDuration);
			ensure!(new_duration >= 1000, Error::<T>::SlotDurationTooSmall);
			ensure!(new_duration <= 60000, Error::<T>::SlotDurationTooLarge);

			let old_duration = SlotDuration::<T>::get();
			SlotDuration::<T>::put(new_duration);

			Self::deposit_event(Event::SlotDurationUpdated {
				old_duration,
				new_duration,
			});

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Get the current slot duration in milliseconds
		pub fn current_slot_duration() -> u64 {
			SlotDuration::<T>::get()
		}
	}
}

/// Custom SlotDuration type that reads from storage
pub struct DynamicSlotDuration<T>(core::marker::PhantomData<T>);

impl<T: Config> Get<u64> for DynamicSlotDuration<T> {
	fn get() -> u64 {
		Pallet::<T>::current_slot_duration()
	}
}
