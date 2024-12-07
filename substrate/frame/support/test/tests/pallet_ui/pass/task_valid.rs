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

#[frame_support::pallet(dev_mode)]
mod pallet {
	use frame_support::{ensure, pallet_prelude::DispatchResult};

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(core::marker::PhantomData<T>);

    #[pallet::tasks_experimental]
	impl<T: Config> Pallet<T> {
		#[pallet::task_index(0)]
		#[pallet::task_condition(|i, j| i == 0u32 && j == 2u64)]
		#[pallet::task_list(vec![(0u32, 2u64), (2u32, 4u64)].iter())]
		#[pallet::task_weight(0.into())]
		fn foo(i: u32, j: u64) -> DispatchResult {
			ensure!(i == 0, "i must be 0");
			ensure!(j == 2, "j must be 2");
			Ok(())
		}
	}
}

#[frame_support::pallet(dev_mode)]
mod pallet_with_instance {
	use frame_support::pallet_prelude::{ValueQuery, StorageValue};

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::storage]
	pub type SomeStorage<T, I = ()> = StorageValue<_, u32, ValueQuery>;

    #[pallet::tasks_experimental]
	impl<T: Config<I>, I> Pallet<T, I> {
		#[pallet::task_index(0)]
		#[pallet::task_condition(|i, j| i == 0u32 && j == 2u64)]
		#[pallet::task_list(vec![(0u32, 2u64), (2u32, 4u64)].iter())]
		#[pallet::task_weight(0.into())]
		fn foo(_i: u32, _j: u64) -> frame_support::pallet_prelude::DispatchResult {
			<SomeStorage<T, I>>::get();
			Ok(())
		}
	}
}

fn main() {
}
