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

//! # Root Testing Pallet
//!
//! Pallet that contains extrinsics that can be usefull in testing.
//!
//! NOTE: This pallet should only be used for testing purposes and should not be used in production
//! runtimes!

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{dispatch::DispatchResult, sp_runtime::Perbill};

pub use pallet::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Event dispatched when the trigger_defensive extrinsic is called.
		DefensiveTestCall,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A dispatch that will fill the block weight up to the given ratio.
		#[pallet::call_index(0)]
		#[pallet::weight(*_ratio * T::BlockWeights::get().max_block)]
		pub fn fill_block(origin: OriginFor<T>, _ratio: Perbill) -> DispatchResult {
			ensure_root(origin)?;
			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(0)]
		pub fn trigger_defensive(origin: OriginFor<T>) -> DispatchResult {
			ensure_root(origin)?;
			frame_support::defensive!("root_testing::trigger_defensive was called.");
			Self::deposit_event(Event::DefensiveTestCall);
			Ok(())
		}
	}
}
