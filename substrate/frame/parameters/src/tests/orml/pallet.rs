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

//! Pallet for testing the ORML parameters store.

// FAIL-CI remove
#![allow(dead_code)]

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;

use frame_support::traits::dynamic_params::ParameterStore;

pub use pallet::*;

// This macro will be moved to ORML:
frame_support::define_parameters! {
	pub Parameters = {
		DynamicMagicNumber: u32 = 0,
	}
}

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// This is how ORML pallets would use it:
		type ParameterStore: ParameterStore<Parameters>;
	}

	#[pallet::event]
	pub enum Event<T: Config> {}

	#[pallet::error]
	pub enum Error<T> {
		WrongValue,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn check_param(_origin: OriginFor<T>, want: Option<u32>) -> DispatchResult {
			let got = T::ParameterStore::get(DynamicMagicNumber);

			ensure!(got == want, Error::<T>::WrongValue);

			Ok(())
		}
	}
}
