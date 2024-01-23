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

#[frame::pallet(dev_mode)]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: IsType<<Self as frame_system::Config>::RuntimeEvent> + From<Event<Self>>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::event]
	pub enum Event<T: Config> {}

	#[pallet::storage]
	pub type Value<T> = StorageValue<Value = u32>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn some_dispatchable(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::pallet as my_pallet;
	use frame::testing_prelude::*;

	construct_runtime!(
		pub enum Runtime {
			System: frame_system,
			MyPallet: my_pallet,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = MockBlock<Self>;
	}

	impl my_pallet::Config for Runtime {
		type RuntimeEvent = RuntimeEvent;
	}
}
