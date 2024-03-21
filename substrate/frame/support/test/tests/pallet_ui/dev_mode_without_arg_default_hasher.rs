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

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;

	// The struct on which we build all of our Pallet logic.
	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// Your Pallet's configuration trait, representing custom external types and interfaces.
	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::storage]
	type MyStorage<T: Config> = StorageValue<_, Vec<u8>>;

	#[pallet::storage]
	type MyStorageMap<T: Config> = StorageMap<_, _, u32, u64>;

	#[pallet::storage]
	type MyStorageDoubleMap<T: Config> = StorageDoubleMap<_, _, u32, _, u64, u64>;

	#[pallet::storage]
	type MyCountedStorageMap<T: Config> = CountedStorageMap<_, _, u32, u64>;

	// Your Pallet's internal functions.
	impl<T: Config> Pallet<T> {}
}

fn main() {}
