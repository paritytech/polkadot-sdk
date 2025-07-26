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

//! Simple pallet that stores the preset that was used to generate the genesis state in the state.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame::pallet]
pub mod pallet {
	extern crate alloc;
	use frame::prelude::*;

	#[pallet::storage]
	#[pallet::getter(fn preset)]
	#[pallet::unbounded]
	pub type Preset<T: Config> = StorageValue<_, alloc::string::String, OptionQuery>;

	#[pallet::genesis_config]
	#[derive(DefaultNoBound, DebugNoBound, CloneNoBound, PartialEqNoBound, EqNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub preset: alloc::string::String,
		pub _marker: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			Preset::<T>::put(self.preset.clone());
		}
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);
}
