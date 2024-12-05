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

#![cfg_attr(not(feature = "std"), no_std)]

use cumulus_pallet_parachain_system as parachain_system;
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;
use polkadot_primitives::PersistedValidationData;
use sp_std::vec::Vec;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config:
		frame_system::Config + parachain_system::Config + pallet_sudo::Config
	{
		type RuntimeEvent: From<Event> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	/// In case of a scheduled migration, this storage field contains the custom head data to be
	/// applied.
	#[pallet::storage]
	pub(super) type PendingCustomValidationHeadData<T: Config> =
		StorageValue<_, Vec<u8>, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event {
		/// The custom validation head data has been scheduled to apply.
		CustomValidationHeadDataStored,
		/// The custom validation head data was applied as of the contained relay chain block
		/// number.
		CustomValidationHeadDataApplied,
	}

	#[pallet::error]
	pub enum Error<T> {
		/// CustomHeadData is not stored in storage.
		NoCustomHeadData,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight({0})]
		pub fn schedule_migration(
			origin: OriginFor<T>,
			code: Vec<u8>,
			head_data: Vec<u8>,
		) -> DispatchResult {
			ensure_root(origin)?;

			parachain_system::Pallet::<T>::schedule_code_upgrade(code)?;
			Self::store_pending_custom_validation_head_data(head_data);
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Set a custom head data that should only be applied when upgradeGoAheadSignal from
		/// the Relay Chain is GoAhead
		fn store_pending_custom_validation_head_data(head_data: Vec<u8>) {
			PendingCustomValidationHeadData::<T>::put(head_data);
			Self::deposit_event(Event::CustomValidationHeadDataStored);
		}

		/// Set pending custom head data as head data that will be returned by `validate_block`. on
		/// the relay chain.
		fn set_pending_custom_validation_head_data() {
			if let Some(head_data) = <PendingCustomValidationHeadData<T>>::take() {
				parachain_system::Pallet::<T>::set_custom_validation_head_data(head_data);
				Self::deposit_event(Event::CustomValidationHeadDataApplied);
			}
		}
	}

	impl<T: Config> parachain_system::OnSystemEvent for Pallet<T> {
		fn on_validation_data(_data: &PersistedValidationData) {}
		fn on_validation_code_applied() {
			crate::Pallet::<T>::set_pending_custom_validation_head_data();
		}
	}
}
