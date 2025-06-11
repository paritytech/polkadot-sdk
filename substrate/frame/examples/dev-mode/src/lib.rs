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

//! <!-- markdown-link-check-disable -->
//! # Dev Mode Example Pallet
//!
//! A simple example of a FRAME pallet demonstrating
//! the ease of requirements for a pallet in dev mode.
//!
//! Run `cargo doc --package pallet-dev-mode --open` to view this pallet's documentation.
//!
//! **Dev mode is not meant to be used in production.**

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{vec, vec::Vec};
use frame_support::dispatch::DispatchResult;
use frame_system::ensure_signed;

// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;

#[cfg(test)]
mod tests;

/// A type alias for the balance type from this pallet's point of view.
type BalanceOf<T> = <T as pallet_balances::Config>::Balance;

/// Enable `dev_mode` for this pallet.
#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: pallet_balances::Config + frame_system::Config {}

	// Simple declaration of the `Pallet` type. It is placeholder we use to implement traits and
	// method.
	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		// No need to define a `call_index` attribute here because of `dev_mode`.
		// No need to define a `weight` attribute here because of `dev_mode`.
		pub fn add_dummy(origin: OriginFor<T>, id: T::AccountId) -> DispatchResult {
			ensure_root(origin)?;

			if let Some(mut dummies) = Dummy::<T>::get() {
				dummies.push(id.clone());
				Dummy::<T>::set(Some(dummies));
			} else {
				Dummy::<T>::set(Some(vec![id.clone()]));
			}

			// Let's deposit an event to let the outside world know this happened.
			Self::deposit_event(Event::AddDummy { account: id });

			Ok(())
		}

		// No need to define a `call_index` attribute here because of `dev_mode`.
		// No need to define a `weight` attribute here because of `dev_mode`.
		pub fn set_bar(
			origin: OriginFor<T>,
			#[pallet::compact] new_value: T::Balance,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			// Put the new value into storage.
			<Bar<T>>::insert(&sender, new_value);

			Self::deposit_event(Event::SetBar { account: sender, balance: new_value });

			Ok(())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AddDummy { account: T::AccountId },
		SetBar { account: T::AccountId, balance: BalanceOf<T> },
	}

	/// The MEL requirement for bounded pallets is skipped by `dev_mode`.
	/// This means that all storages are marked as unbounded.
	/// This is equivalent to specifying `#[pallet::unbounded]` on this type definitions.
	/// When the dev_mode is removed, we would need to implement implement `MaxEncodedLen`.
	#[pallet::storage]
	pub type Dummy<T: Config> = StorageValue<_, Vec<T::AccountId>>;

	/// The Hasher requirement is skipped by `dev_mode`. So, second parameter can be `_`
	/// and `Blake2_128Concat` is used as a default.
	/// When the dev_mode is removed, we would need to specify the hasher like so:
	/// `pub type Bar<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, T::Balance>;`.
	#[pallet::storage]
	pub type Bar<T: Config> = StorageMap<_, _, T::AccountId, T::Balance>;
}
