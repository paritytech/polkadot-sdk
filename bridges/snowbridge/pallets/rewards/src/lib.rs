// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
#![cfg_attr(not(feature = "std"), no_std)]

pub mod impls;

use frame_support::{
	traits::{
		fungible::{Inspect},
	},
};
use frame_system::pallet_prelude::*;
use sp_core::{RuntimeDebug, H160, H256};
use frame_support::pallet_prelude::DispatchClass;
use sp_runtime::DispatchResult;
use crate::impls::RewardLedger;
use frame_support::Identity;
use frame_support::StorageMap;
use frame_system::WeightInfo;
use frame_support::pallet_prelude::IsType;

pub use pallet::*;

pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
pub type BalanceOf<T> =
<<T as pallet::Config>::Token as Inspect<<T as frame_system::Config>::AccountId>>::Balance;
#[frame_support::pallet]
pub mod pallet {
	use sp_core::U256;

	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {

	}

	#[pallet::error]
	pub enum Error<T> {

	}

	#[pallet::storage]
	pub type RewardsMapping<T: Config> =
	StorageMap<_, Identity, H256, U256, ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight((T::WeightInfo::claim(), DispatchClass::Operational))]
		pub fn claim(
			origin: OriginFor<T>,
			deposit_address: AccountIdOf<T>,
		) -> DispatchResult {
			ensure_signed(origin)?;
			Self::process_rewards(deposit_address);
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		fn process_rewards(deposit_address: AccountIdOf<T>) -> DispatchResult {

			Ok(())
		}
	}

	impl<T: Config> RewardLedger<T> for Pallet<T> {
		fn deposit(account: AccountIdOf<T>, value: BalanceOf<T>) {}
	}
}
