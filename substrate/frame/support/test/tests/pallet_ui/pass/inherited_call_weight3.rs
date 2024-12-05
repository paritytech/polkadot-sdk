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

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;

// If, for whatever reason, you dont to not use a `WeightInfo` trait - it will still work.
struct Impl;

impl Impl {
	fn foo() -> Weight {
		Weight::zero()
	}
}

#[frame_support::pallet]
mod parentheses {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
	}

	#[pallet::pallet]
	pub struct Pallet<T>(core::marker::PhantomData<T>);

	#[pallet::call(weight(crate::Impl))]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		pub fn foo(_: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}
}

#[frame_support::pallet]
mod assign {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
	}

	#[pallet::pallet]
	pub struct Pallet<T>(core::marker::PhantomData<T>);

	#[pallet::call(weight = crate::Impl)]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		pub fn foo(_: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}
}

fn main() {
}
