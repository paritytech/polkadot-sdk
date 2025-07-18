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

//! This pallet demonstrates the use of the `pallet::view_functions` api for service
//! work.
#![cfg_attr(not(feature = "std"), no_std)]

pub mod tests;

use frame_support::Parameter;
use scale_info::TypeInfo;

pub struct SomeType1;
impl From<SomeType1> for u64 {
	fn from(_t: SomeType1) -> Self {
		0u64
	}
}

pub trait SomeAssociation1 {
	type _1: Parameter + codec::MaxEncodedLen + TypeInfo;
}
impl SomeAssociation1 for u64 {
	type _1 = u64;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;

	#[pallet::error]
	pub enum Error<T> {}

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type SomeValue<T: Config> = StorageValue<_, u32>;

	#[pallet::storage]
	pub type SomeMap<T: Config> = StorageMap<_, Twox64Concat, u32, u32, OptionQuery>;

	#[pallet::view_functions]
	impl<T: Config> Pallet<T>
	where
		T::AccountId: From<SomeType1> + SomeAssociation1,
	{
		/// Query value with no input args.
		pub fn get_value() -> Option<u32> {
			SomeValue::<T>::get()
		}

		/// Query value with input args.
		pub fn get_value_with_arg(key: u32) -> Option<u32> {
			SomeMap::<T>::get(key)
		}
	}
}

#[frame_support::pallet]
pub mod pallet2 {
	use super::*;
	use frame_support::pallet_prelude::*;

	#[pallet::error]
	pub enum Error<T, I = ()> {}

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::storage]
	pub type SomeValue<T: Config<I>, I: 'static = ()> = StorageValue<_, u32>;

	#[pallet::storage]
	pub type SomeMap<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, u32, u32, OptionQuery>;

	#[pallet::view_functions]
	impl<T: Config<I>, I: 'static> Pallet<T, I>
	where
		T::AccountId: From<SomeType1> + SomeAssociation1,
	{
		/// Query value with no input args.
		pub fn get_value() -> Option<u32> {
			SomeValue::<T, I>::get()
		}

		/// Query value with input args.
		pub fn get_value_with_arg(key: u32) -> Option<u32> {
			SomeMap::<T, I>::get(key)
		}
	}
}
