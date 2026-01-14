// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT-0

// Permission is hereby granted, free of charge, to any person obtaining a copy of
// this software and associated documentation files (the "Software"), to deal in
// the Software without restriction, including without limitation the rights to
// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
// of the Software, and to permit persons to whom the Software is furnished to do
// so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! # Pallet

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

pub mod benchmarking;
pub mod weights;

#[frame_support::pallet]
pub mod pallet {
	use crate::weights::WeightInfo;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type WeightInfo: WeightInfo;
		// No BenchmarkHelper
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type NextId<T> = StorageValue<_, u32, ValueQuery>;

	#[pallet::storage]
	pub type Registered<T> = StorageMap<_, Blake2_128Concat, u32, (), ValueQuery>;

	#[pallet::error]
	pub enum Error<T> {
		AlreadyRegistered,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::register())]
		pub fn register(origin: OriginFor<T>, id: u32) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			ensure!(!Registered::<T>::contains_key(id), Error::<T>::AlreadyRegistered);
			Registered::<T>::insert(id, ());
			Ok(())
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::{assert_ok, derive_impl};

	type Block = frame_system::mocking::MockBlock<Test>;

	frame_support::construct_runtime!(
		pub enum Test {
			System: frame_system,
			MyPallet: pallet,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Test {
		type Block = Block;
	}

	impl pallet::Config for Test {
		type WeightInfo = ();
	}

	impl benchmarking::BenchmarkConfig for Test {
		type Helper = ();
	}

	#[allow(unused)]
	pub fn new_test_ext() -> sp_io::TestExternalities {
		sp_io::TestExternalities::new(Default::default())
	}

	#[test]
	fn new_registration_works() {
		new_test_ext().execute_with(|| {
			assert_eq!(NextId::<Test>::get(), 0);
			NextId::<Test>::put(10);
			let id = NextId::<Test>::get();
			assert_ok!(MyPallet::register(RuntimeOrigin::signed(1), id));
			assert_eq!(Registered::<Test>::contains_key(id), true);
		});
	}
}
