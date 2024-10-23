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

use frame::prelude::*;

#[cfg(test)]
mod mock_relay;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo)]
pub enum Role {
	Relay,
	AssetHub,
}

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo)]
pub enum Phase {
	Waiting,
}

#[frame::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: IsType<<Self as frame_system::Config>::RuntimeEvent> + From<Event<Self>>;

		#[pallet::constant]
		type Role: Get<Role>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type Phase<T: Config> = StorageValue<_, super::Phase>;

	#[pallet::event]
	pub enum Event<T: Config> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(123)]
		pub fn some_dispatchable(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}
}
