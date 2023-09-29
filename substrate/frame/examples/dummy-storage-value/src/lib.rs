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

//! # Dummy Storage Value Example
//!
//! A pallet demonstrating basic usage of FRAME's
//! [`StorageValue`](frame_support::storage::types::StorageValue) API alongside other commonly used
//! FRAME features, such as ensuring the origin of a call and configuring the genesis storage of the
//! pallet.
//!
//! **WARNING: This pallet is not meant to be used in production.** The pallet is in `dev_mode`, so we don't need to care about specifying call indices or call
//! weights. Read more about configuring weights [here](https://docs.substrate.io/test/benchmark/).
//!
//! ## Overview
//!
//! We demonstrate the [`StorageValue`](frame_support::storage::types::StorageValue) API by showing
//! it's use of `mutate` on:
//! - A storage item that stores some `Balance` and uses the default
//!   [`OptionQuery`](frame_support::storage::types::OptionQuery) which will always either return
//!   `Option<T>` or `None` when queried
#![doc = docify::embed!("src/tests.rs", accumulate_dummy_works)]
//! - A storage item that stores a `u32` and uses
//!   [`ValueQuery`](frame_support::storage::types::ValueQuery) to always return `T` or
//!   `Default::default` if the stored value is removed, which in this case will be `0`
//!   (`u32::default()`)
#![doc = docify::embed!("src/tests.rs", accumulate_dummy_value_query_works)]
//! We provide an example of a call that can only set a new value in Dummy if it is called by the root origin:
#![doc = docify::embed!("src/tests.rs", set_dummy_works)]
//! We also configure our pallet's genesis state using [`GenesisConfig`] which we demonstrate in the mock runtime
//! environment to run our tests against:
#![doc = docify::embed!("src/tests.rs", new_test_ext)]

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{dispatch::DispatchResult, sp_runtime::Saturating};
use frame_system::ensure_signed;

// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;

#[cfg(test)]
mod tests;

/// A type alias for the balance type from this pallet's point of view.
type BalanceOf<T> = <T as pallet_balances::Config>::Balance;

// Definition of the pallet logic, to be aggregated in a chain's runtime definition.
// Note: we're using this palelt in `dev_mode`.
#[frame_support::pallet(dev_mode)]
pub mod pallet {
	// Import various types used to declare pallet in scope.
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// Our pallet's configuration trait.
	///
	/// Because all FRAME pallets depend on some core utility types from the System pallet,
	/// [`frame_system::Config`] should always be included. This pallet example uses the `Balances`
	/// type from `pallet_balances` which we make available by bounding this trait with
	/// `pallet_balances::Config`.
	#[pallet::config]
	pub trait Config: pallet_balances::Config + frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	// The `Pallet` type is a placeholder we use to implement traits and methods for the pallet.
	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// An example storage item to store a single value, in our case, some Balance.
	/// This storage item uses [`OptionQuery`] by default which will return what is in actual state
	/// provided by [`sp_io::storage`]. If a value `v` exists in state, it returns `Some(v)`,
	/// otherwise it returns `None`.
	///
	/// The getter attribute generates a function on the `Pallet` struct that we can use to
	/// conveniently retrieve the current value stored.
	#[doc = docify::embed!("src/tests.rs", accumulate_dummy_works)]
	#[pallet::storage]
	#[pallet::getter(fn dummy)]
	pub(super) type Dummy<T: Config> = StorageValue<_, T::Balance>;

	/// An example storage item that stores a `u32` value.
	/// Here, we're using [`ValueQuery`] instead of the default [`OptionQuery`]. If a value exists
	/// in state, it will return that raw `u32` value, otherwise it will return `u32::default()`.
	#[doc = docify::embed!("src/tests.rs", accumulate_dummy_value_query_works)]
	#[pallet::storage]
	#[pallet::getter(fn dummy_value_query)]
	pub(super) type DummyValueQuery<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// The optional genesis configuration type, which we use here to demonstrate how to configure
	/// building the genesis storage for this pallet.
	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub dummy: T::Balance,
		pub dummy_value_query: u32,
	}

	/// The genesis build for this pallet.
	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			Dummy::<T>::put(&self.dummy);
			DummyValueQuery::<T>::put(&self.dummy_value_query);
		}
	}

	/// Events are a simple means of providing some metadata about specific state changes that have
	/// been made that can be useful to the outside world (for e.g. apps or chain explorers).
	///
	/// All events are stored in the System pallet at the end of each block.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The Dummy value has been accumulated.
		AccumulateDummy {
			/// The amount `Dummy` has been increased by.
			increase_by: BalanceOf<T>,
		},
		/// The Dummy value has been set.
		SetDummy {
			/// The new value of `Dummy`.
			new_balance: BalanceOf<T>,
		},
	}

	/// The call declaration block of our pallet.
	impl<T: Config> Pallet<T> {
		/// A public call that increases the value in `Dummy`.
		///
		/// This can be called by any signed origin. The example uses the [`StorageValue::mutate`]
		/// method to demonstrate a safe and elegant way to accumulate the stored value.
		pub fn accumulate_dummy(origin: OriginFor<T>, increase_by: T::Balance) -> DispatchResult {
			// we ensure that the origin is signed
			let _sender = ensure_signed(origin)?;

			// using `mutate`, we can query the value in storage and update it in just a few lines
			Dummy::<T>::mutate(|dummy| {
				// we use `saturating_add` instead of a regular `+` to avoid overflowing
				let new_dummy = dummy.map_or(increase_by, |d| d.saturating_add(increase_by));
				*dummy = Some(new_dummy);
			});

			// Here's another way to accumulate the value in `Dummy`:
			// read the value from storage using the generated getter function
			// let dummy = Self::dummy();
			// calculate the new value
			// let new_dummy = dummy.map_or(increase_by, |dummy| dummy.saturating_add(increase_by));
			// put the new value into storage
			// Dummy::<T>::put(new_dummy);

			// deposit an event to let the outside world know this storage update has
			// happened
			Self::deposit_event(Event::AccumulateDummy { increase_by });

			// no errors returned
			Ok(())
		}

		/// A privileged call that can set the value in `Dummy` to a new value.
		///
		/// This must be called with a `Root` origin, implying that only the system as a whole has
		/// decided to execute this call. Different runtimes have different reasons to allow
		/// privileged calls to be executed - we don't need to care why. Because it's privileged, we
		/// can assume it's a one-off operation and substantial processing/storage/memory can be
		/// used without worrying about gameability or attack scenarios.
		pub fn set_dummy(origin: OriginFor<T>, new_balance: T::Balance) -> DispatchResult {
			// we ensure that the caller is a root origin
			ensure_root(origin)?;

			// put the new value into storage
			Dummy::<T>::put(new_balance);

			// deposit an event
			Self::deposit_event(Event::SetDummy { new_balance });

			// no errors returned
			Ok(())
		}
	}
}

/// The main implementation block for our pallet.
impl<T: Config> Pallet<T> {
	/// Removes the values in our Dummy and DummyValueQuery storage items.
	///
	/// We use this function in our unit tests to showcase the behavior of `OptionQuery` and
	/// `ValueQuery`.
	#[warn(dead_code)]
	fn do_reset_dummy(origin: T::RuntimeOrigin) -> DispatchResult {
		let _sender = ensure_signed(origin)?;

		Dummy::<T>::kill();
		DummyValueQuery::<T>::kill();

		assert_eq!(Self::dummy(), None);
		assert_eq!(Self::dummy_value_query(), u32::default());

		Ok(())
	}

	/// Accumulates the value in [`DummyValueQuery`].
	///
	/// This demonstrates using the `mutate` method from the
	/// [`StorageValue`](frame_support::storage::types::StorageValue) API on
	/// [`ValueQuery`](frame_support::storage::types::ValueQuery)
	#[warn(dead_code)]
	fn accumulate_value_query(origin: T::RuntimeOrigin, increase_by: u32) -> DispatchResult {
		let _sender = ensure_signed(origin)?;

		let prev = DummyValueQuery::<T>::get();
		// because DummyValueQuery uses [`ValueQuery`], 'value' is the raw type
		let result = DummyValueQuery::<T>::mutate(|value| {
			*value = value.saturating_add(increase_by);
			*value
		});
		assert!(prev + increase_by == result);

		Ok(())
	}
}
