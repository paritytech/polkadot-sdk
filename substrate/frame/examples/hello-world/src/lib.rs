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

//! # Hello World Example Pallet
//!
//! A simple pallet demonstrating basic Substrate concepts for workshops.
//!
//! **This pallet serves as an example and is not meant to be used in production.**
//!
//! ## Features Demonstrated
//!
//! - Events: Emitting events when actions occur
//! - Calls: Public dispatchable functions
//! - Hooks: Block lifecycle hooks
//!
//! ## Pallet API
//!
//! - `say_hello()`: A public call that anyone can execute

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
use log::info;

// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;
pub use weights::*;

// Definition of the pallet logic, to be aggregated at runtime definition through
// `construct_runtime`.
#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// Our pallet's configuration trait. All our types and constants go in here.
	/// If the pallet is dependent on specific other pallets, then their configuration
	/// traits should be added to our implied traits list.
	///
	/// `frame_system::Config` should always be included.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Type representing the weight of this pallet
		type WeightInfo: WeightInfo;
	}

	// Simple declaration of the `Pallet` type. It is placeholder we use to implement traits and
	// method.
	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// The call declaration. This states the entry points that we handle.
	#[pallet::call(weight(<T as Config>::WeightInfo))]
	impl<T: Config> Pallet<T> {
		/// Say hello! This is a public call that anyone can execute.
		/// It emits an event and logs a message.
		#[pallet::call_index(0)]
		pub fn say_hello(origin: OriginFor<T>) -> DispatchResult {
			// Ensure the call is from a signed account
			let who = ensure_signed(origin)?;

			// Log the action
			info!("Account {:?} said hello!", who);

			// Emit an event
			Self::deposit_event(Event::HelloSaid { who });

			Ok(())
		}
	}

	/// Events are a simple means of reporting specific conditions and
	/// circumstances that have happened that users, Dapps and/or chain explorers would find
	/// interesting and otherwise difficult to detect.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Someone said hello!
		HelloSaid {
			who: T::AccountId,
		},
	}



}


