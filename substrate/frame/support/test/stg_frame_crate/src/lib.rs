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

// ! A basic pallet to test it compiles along with a runtime using it when `frame_system` and
// `frame_support` are reexported by a `frame` crate.

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
	// pub trait Config: frame::deps::frame_system::Config {}
	// ```
	pub trait Config: frame_system::Config {}

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
}

#[cfg(test)]
// Dummy test to make sure a runtime would compile.
mod tests {
	use super::{
		frame_support::{construct_runtime, derive_impl},
		frame_system, pallet,
	};

	type Block = frame_system::mocking::MockBlock<Runtime>;

	impl crate::pallet::Config for Runtime {}

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = Block;
	}

	construct_runtime! {
		pub struct Runtime
		{
			System: frame_system,
			Pallet: pallet,
		}
	}
}
