// Copyright 2022 Parity Technologies (UK) Ltd.
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

use codec::{Decode, Encode};
use cumulus_pallet_parachain_system as parachain_system;
use frame_support::{dispatch::DispatchResult, pallet_prelude::*, weights::DispatchInfo};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use polkadot_primitives::v1::PersistedValidationData;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{DispatchInfoOf, Dispatchable, SignedExtension},
	transaction_validity::{
		InvalidTransaction, TransactionLongevity, TransactionPriority, TransactionValidity,
		TransactionValidityError, ValidTransaction,
	},
};
use sp_std::{prelude::*, vec::Vec};

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config:
		frame_system::Config + parachain_system::Config + pallet_sudo::Config
	{
		type Event: From<Event> + IsType<<Self as frame_system::Config>::Event>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	/// In case of a scheduled migration, this storage field contains the custom head data to be applied.
	#[pallet::storage]
	pub(super) type PendingCustomValidationHeadData<T: Config> =
		StorageValue<_, Vec<u8>, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event {
		/// The custom validation head data has been scheduled to apply.
		CustomValidationHeadDataStored,
		/// The custom validation head data was applied as of the contained relay chain block number.
		CustomValidationHeadDataApplied,
	}

	#[pallet::error]
	pub enum Error<T> {
		/// CustomHeadData is not stored in storage.
		NoCustomHeadData,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(0)]
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

		/// Set pending custom head data as head data that will be returned by `validate_block`. on the relay chain.
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

	/// Ensure that signed transactions are only valid if they are signed by root.
	#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo, Default)]
	#[scale_info(skip_type_params(T))]
	pub struct CheckSudo<T: Config + Send + Sync>(sp_std::marker::PhantomData<T>);

	impl<T: Config + Send + Sync> CheckSudo<T> {
		pub fn new() -> Self {
			Self(Default::default())
		}
	}

	impl<T: Config + Send + Sync> sp_std::fmt::Debug for CheckSudo<T> {
		#[cfg(feature = "std")]
		fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
			write!(f, "CheckSudo")
		}

		#[cfg(not(feature = "std"))]
		fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
			Ok(())
		}
	}

	impl<T: Config + Send + Sync> SignedExtension for CheckSudo<T>
	where
		<T as frame_system::Config>::Call: Dispatchable<Info = DispatchInfo>,
	{
		type AccountId = T::AccountId;
		type Call = <T as frame_system::Config>::Call;
		type AdditionalSigned = ();
		type Pre = ();
		const IDENTIFIER: &'static str = "CheckSudo";

		fn additional_signed(&self) -> sp_std::result::Result<(), TransactionValidityError> {
			Ok(())
		}

		fn pre_dispatch(
			self,
			who: &Self::AccountId,
			call: &Self::Call,
			info: &DispatchInfoOf<Self::Call>,
			len: usize,
		) -> Result<Self::Pre, TransactionValidityError> {
			Ok(self.validate(who, call, info, len).map(|_| ())?)
		}

		fn validate(
			&self,
			who: &Self::AccountId,
			_call: &Self::Call,
			info: &DispatchInfoOf<Self::Call>,
			_len: usize,
		) -> TransactionValidity {
			let root_account = match pallet_sudo::Pallet::<T>::key() {
				Some(account) => account,
				None => return Err(InvalidTransaction::BadSigner.into()),
			};

			if *who == root_account {
				Ok(ValidTransaction {
					priority: info.weight as TransactionPriority,
					longevity: TransactionLongevity::max_value(),
					propagate: true,
					..Default::default()
				})
			} else {
				Err(InvalidTransaction::BadSigner.into())
			}
		}
	}
}
