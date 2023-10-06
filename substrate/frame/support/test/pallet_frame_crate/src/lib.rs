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

//! A basic pallet that can be used to test `construct_runtime!` when `frame_system` and
//! `frame_support` are reexported by a `frame` crate.

// Ensure docs are propagated properly by the macros.
#![warn(missing_docs)]

pub use pallet::*;

use frame::deps::{frame_support, frame_system};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	// The only valid syntax here is the following or
	// ```
	// pub trait Config: frame_system::Config {}
	// ```
	// if `frame_system` is brought into scope.
	pub trait Config: frame_system::Config {}

	#[pallet::storage]
	pub type Value<T> = StorageValue<_, u32>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		#[serde(skip)]
		_config: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {}
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Something failed
		Test,
	}
}
