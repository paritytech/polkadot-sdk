// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! # Pallet for simulation of reentrancy attack

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
pub use pallet_multisig::{self as Multisig, Call as MultisigCall};


use frame_support::{
	dispatch::{
		GetDispatchInfo,
		PostDispatchInfo,
	},
	traits::ReservableCurrency,
};
use sp_runtime::traits::Dispatchable;


/// The log target of this pallet.
pub const LOG_TARGET: &'static str = "runtime::reentrancy-attack";


// impl pallet_multisig::Config for Runtime {
// 	type RuntimeEvent = RuntimeEvent;
// 	// type Currency = dyn frame_support::traits::Currency<AccountId>;
// 	// type RuntimeCall = Self::RuntimeCall;
// 	type DepositBase = Self::DepositBase;
// 	type DepositFactor = Self::DepositFactor;
// 	type MaxSignatories = Self::MaxSignatories;
// 	type WeightInfo = ();
// }


#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{dispatch, pallet_prelude::*};
	use frame_system::pallet_prelude::OriginFor;
	use frame_support::traits::Currency;
	use frame_system::ensure_signed;


	pub use frame_system::{
		pallet_prelude::BlockNumberFor, Config as SystemConfig, Pallet as SystemPallet,
	};

	type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as SystemConfig>::AccountId>>::Balance;

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_multisig::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The overarching call type.
		type RuntimeCall: Parameter
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin, PostInfo = PostDispatchInfo>
			+ GetDispatchInfo
			+ From<frame_system::Call<Self>>;

		/// The currency mechanism.
		type Currency: ReservableCurrency<Self::AccountId>;
	}

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::error]
	pub enum Error<T> {
		/// Threshold must be 2 or greater.
		MinimumThreshold,
		/// Call is already approved by this signatory.
		AlreadyApproved,
	}

	#[pallet::event]
	pub enum Event<T: Config> {
		AccountsCreated(T::AccountId, T::AccountId),
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {

		#[pallet::weight(10_000)]
		pub fn create_accounts(origin: OriginFor<T>, amount: BalanceOf<T>, account: T::AccountId,) -> DispatchResult {
			let mut SEED = [1u8; 32];
			let sender = ensure_signed(origin)?;

			let mut account_1: u64 = 1;
			let mut account_2: u64 = 2;
			// let multi = Multisig::multi_account_id(&[1, 2, 3][..], 2);
			// pallet_multisig::

            // Call the as_multi function from the pallet_multisig pallet
            Multisig::Pallet::<T>::as_multi(
                frame_system::RawOrigin::Signed(sender).into(),
                2,
                vec![account.clone()],
                None,
                call,
                Weight::zero(),
            );
			// T::Currency::transfer(&sender, dest, value, existence_requirement);

			Ok(())
		}
	}
}
