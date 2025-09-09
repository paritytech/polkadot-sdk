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

//! # Cumulus Slot Configuration Pallet
//!
//! This pallet enables dynamic configuration of slot duration in Cumulus parachains
//! without requiring runtime upgrades. It provides secure governance-controlled
//! mechanisms to adjust consensus timing parameters.
//!
//! ## Overview
//!
//! The Slot Config pallet allows authorized origins (typically Root, Sudo, or Democracy)
//! to modify the slot duration used by the AuRa consensus algorithm. This enables
//! network operators to:
//!
//! - Adjust block time without code changes
//! - Respond to network conditions dynamically  
//! - Coordinate with relay chain timing changes
//! - Test different performance parameters
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! - `set_slot_duration` - Update the slot duration (requires UpdateOrigin)
//!
//! ### Public Functions
//!
//! - `current_slot_duration` - Get the current slot duration value
//!
//! ## Usage
//!
//! ```rust,ignore
//! // In your runtime configuration:
//! impl cumulus_pallet_slot_config::Config for Runtime {
//!     type UpdateOrigin = EnsureRoot<AccountId>;
//!     type DefaultSlotDuration = ConstU64<6000>; // 6 seconds default
//!     type WeightInfo = ();
//! }
//!
//! // For pallet_aura integration:
//! impl pallet_aura::Config for Runtime {
//!     type SlotDuration = cumulus_pallet_slot_config::DynamicSlotDuration<Runtime>;
//!     // ... other config
//! }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
use frame_support::traits::Get;
use sp_consensus_aura::Slot;
// Imports for types only

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod slot_adjustment;
pub mod traits_impl;
pub mod weights;

pub use weights::WeightInfo;

/// Interface for interacting with pallet-aura's CurrentSlot.
pub trait AuraInterface {
	/// Get the current slot from pallet-aura.
	fn current_slot() -> Slot;
	
	/// Set the current slot in pallet-aura.
	fn set_current_slot(slot: Slot);
}

/// Interface for getting current timestamp.
pub trait TimestampProvider<Moment> {
	/// Get the current timestamp.
	fn now() -> Moment;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
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

	/// Configuration trait for the slot config pallet.
	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// The origin that can update slot duration.
		/// 
		/// Typically `EnsureRoot<AccountId>` for Root access,
		/// or `pallet_sudo::EnsureSudo<AccountId>` for sudo access,
		/// or democracy/collective origins for governance.
		type UpdateOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Default slot duration in milliseconds.
		/// 
		/// This value is used during genesis initialization and
		/// serves as a fallback if storage is corrupted.
		#[pallet::constant]
		type DefaultSlotDuration: Get<u64>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// The Aura pallet type for current slot access.
		/// 
		/// This is needed to update CurrentSlot when SlotDuration changes.
		/// Should be set to `pallet_aura::Pallet<Self>`.
		type AuraPallet: AuraInterface;

		/// The Timestamp pallet for getting current time.
		/// 
		/// This is needed to recalculate the correct slot when
		/// SlotDuration changes.
		type TimestampProvider: TimestampProvider<Self::Moment>;

		/// The moment type from pallet_timestamp.
		type Moment: Copy + Default + TryInto<u64>;
	}

	/// Current slot duration in milliseconds.
	/// 
	/// This value is read by the `DynamicSlotDuration` type and used
	/// by consensus algorithms like AuRa.
	#[pallet::storage]
	#[pallet::getter(fn slot_duration)]
	pub type SlotDuration<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// Genesis configuration for the pallet.
	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// Initial slot duration in milliseconds.
		/// If 0, uses the DefaultSlotDuration constant.
		pub slot_duration: u64,
		#[serde(skip)]
		pub _config: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			let initial_duration = if self.slot_duration > 0 {
				self.slot_duration
			} else {
				T::DefaultSlotDuration::get()
			};
			SlotDuration::<T>::put(initial_duration);
		}
	}

	/// Events emitted by this pallet.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Slot duration was updated.
		SlotDurationUpdated { 
			/// Previous slot duration in milliseconds
			old_duration: u64, 
			/// New slot duration in milliseconds
			new_duration: u64 
		},
	}

	/// Errors that can be returned by this pallet.
	#[pallet::error]
	pub enum Error<T> {
		/// Slot duration cannot be zero.
		ZeroSlotDuration,
		/// Slot duration is too small. Minimum is 1000ms (1 second).
		SlotDurationTooSmall,
		/// Slot duration is too large. Maximum is 60000ms (60 seconds).
		SlotDurationTooLarge,
	}

	/// Dispatchable functions for this pallet.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Update the slot duration.
		/// 
		/// The origin must be the configured `UpdateOrigin` (typically Root or Sudo).
		/// 
		/// The new duration must be between 1000ms (1 second) and 60000ms (60 seconds).
		/// 
		/// **Important**: This function also adjusts the CurrentSlot in pallet-aura
		/// to ensure consistency when the slot duration changes.
		/// 
		/// Emits `SlotDurationUpdated` event on success.
		/// 
		/// # Parameters
		/// 
		/// - `origin`: The origin of the call (must match UpdateOrigin)
		/// - `new_duration`: The new slot duration in milliseconds
		/// 
		/// # Errors
		/// 
		/// - `ZeroSlotDuration`: If new_duration is 0
		/// - `SlotDurationTooSmall`: If new_duration < 1000ms
		/// - `SlotDurationTooLarge`: If new_duration > 60000ms
		/// - `BadOrigin`: If origin is not authorized
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_slot_duration())]
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
			
			// Only update if the value actually changed
			if old_duration != new_duration {
				// Get current timestamp from pallet-timestamp
				let current_timestamp_ms: u64 = T::TimestampProvider::now()
					.try_into()
					.unwrap_or_default();

				// Get current slot from pallet-aura
				let current_slot = T::AuraPallet::current_slot();

				// Calculate the new slot based on current time and new duration
				let new_slot = crate::slot_adjustment::recalculate_current_slot(
					*current_slot,
					old_duration,
					new_duration,
					current_timestamp_ms,
				);

				// Update the slot duration
				SlotDuration::<T>::put(new_duration);

				// Update the current slot in pallet-aura to maintain consistency
				T::AuraPallet::set_current_slot(Slot::from(new_slot));

				Self::deposit_event(Event::SlotDurationUpdated {
					old_duration,
					new_duration,
				});
			}

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Get the current slot duration in milliseconds.
		/// 
		/// This is the primary interface used by consensus algorithms
		/// and other runtime components that need to know the current
		/// slot timing.
		pub fn current_slot_duration() -> u64 {
			SlotDuration::<T>::get()
		}
	}
}

/// Dynamic SlotDuration type for integration with pallet_aura.
/// 
/// This type implements the `Get<u64>` trait and reads the slot duration
/// from storage instead of using a compile-time constant.
/// 
/// # Usage
/// 
/// ```rust,ignore
/// impl pallet_aura::Config for Runtime {
///     type SlotDuration = cumulus_pallet_slot_config::DynamicSlotDuration<Runtime>;
///     // ... other config
/// }
/// ```
pub struct DynamicSlotDuration<T>(core::marker::PhantomData<T>);

impl<T: Config> Get<u64> for DynamicSlotDuration<T> {
	fn get() -> u64 {
		Pallet::<T>::current_slot_duration()
	}
}


