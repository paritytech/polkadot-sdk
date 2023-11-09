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

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::pallet_macros::*;

pub use pallet::*;

mod first {
	use super::*;

	#[pallet_section]
	mod section {
		#[pallet::event]
		#[pallet::generate_deposit(pub(super) fn deposit_event)]
		pub enum Event<T: Config> {
			SomethingDone,
		}
	}
}

mod second {
	use super::*;
	
	#[pallet_section(section2)]
	mod section {
		#[pallet::error]
		pub enum Error<T> {
			NoneValue,
		}
	}
}

#[import_section(first::section)]
#[import_section(second::section2)]
#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn my_call(_origin: OriginFor<T>) -> DispatchResult {
			Self::deposit_event(Event::SomethingDone);
			Ok(())
		}

		pub fn my_call_2(_origin: OriginFor<T>) -> DispatchResult {
			return Err(Error::<T>::NoneValue.into())
		}
	}
}

fn main() {
}
