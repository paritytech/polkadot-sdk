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

//! # Custom Config Pallet

#![cfg_attr(not(feature = "std"), no_std)]

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type SetValue<T> = StorageValue<_, u32>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(100)]
		pub fn set_value(origin: OriginFor<T>, value: u32) -> DispatchResult {
			ensure_signed(origin)?;

			SetValue::<T>::put(value);

			Ok(())
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{pallet, pallet::*};
	use crate::tests::runtime::*;
	use frame_support::{assert_ok, derive_impl};
	use sp_runtime::BuildStorage;

	mod runtime {
		use super::*;

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

		impl Config for Test {}
	}

	pub fn new_test_ext() -> sp_io::TestExternalities {
		let t = RuntimeGenesisConfig { system: Default::default() }.build_storage().unwrap();
		t.into()
	}

	#[test]
	fn set_value_works() {
		new_test_ext().execute_with(|| {
			assert_eq!(SetValue::<Test>::get(), None);
			let val = 45;
			assert_ok!(MyPallet::set_value(RuntimeOrigin::signed(1), val));
			assert_eq!(SetValue::<Test>::get(), Some(45));
		});
	}
}
