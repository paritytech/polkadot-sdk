// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
#![cfg_attr(not(feature = "std"), no_std)]

pub mod impls;
pub mod weights;

use frame_system::pallet_prelude::*;
use crate::impls::RewardLedger;
pub use weights::WeightInfo;

pub use pallet::*;

pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
#[frame_support::pallet]
pub mod pallet {
	use sp_core::U256;
	use frame_support::pallet_prelude::*;

	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	pub enum Event<T: Config> {

	}

	#[pallet::error]
	pub enum Error<T> {

	}

	#[pallet::storage]
	pub type RewardsMapping<T: Config> =
	StorageMap<_, Identity, AccountIdOf<T>, U256, ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight((T::WeightInfo::claim(), DispatchClass::Operational))]
		pub fn claim(
			origin: OriginFor<T>,
			deposit_address: AccountIdOf<T>,
		) -> DispatchResult {
			ensure_signed(origin)?;
			let _ = Self::process_rewards(deposit_address);
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		fn process_rewards(_deposit_address: AccountIdOf<T>) -> DispatchResult {

			Ok(())
		}
	}

	impl<T: Config> RewardLedger<T> for Pallet<T> {
		fn deposit(_account: AccountIdOf<T>, _value: U256) {}
	}
}
