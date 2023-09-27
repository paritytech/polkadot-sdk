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

//! # Basic Example Pallet
//!
//! A pallet demonstrating concepts, APIs and structures common to most FRAME runtimes.
//!
//! **This pallet serves as an example and is not meant to be used in production.**
//!
//! > Made with *Substrate*, for *Polkadot*.
//!
//! [![github]](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/frame/examples/basic)
//! [![polkadot]](https://polkadot.network)
//!
//! [polkadot]: https://img.shields.io/badge/polkadot-E6007A?style=for-the-badge&logo=polkadot&logoColor=white
//! [github]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Overview
//!
//! This pallet provides basic examples of using:
//!
//! - Pallet hooks to implement some logic to be executed at the start and end of block execution
//!   (see: [`frame_support::traits::Hooks`])
//! - The [`StorageValue`](frame_support::storage::types::StorageValue) API to demonstrate it's use
//!   of `mutate` on:
//! 	- A storage value that stores some `Balance` and uses the default
//!    [`OptionQuery`](frame_support::storage::types::OptionQuery) which will always either return
//!    `Option<T>` or `None` when queried
//! 	- A storage value that stores a `u32` and uses
//!    [`ValueQuery`](frame_support::storage::types::ValueQuery) to always return `T` or
//!    `Default::default` if the stored value is removed, which in this case will be `0`
//!    (`u32::default()`)
//! - A storage map of AccountIds and Balances
//! - A custom weight calculator able to classify a call's dispatch class (see:
//!   [`frame_support::dispatch::DispatchClass`])
//! - Inherited weight annotation for pallet calls, used to create less repetition for calls that
//!   use the [`Config::WeightInfo`] trait to calculate call weights. This can also be overridden,
//!   as demonstrated by [`Call::set_dummy`].
//! - A simple signed extension implementation (see: [`sp_runtime::traits::SignedExtension`]) which
//!   increases the priority of the [`Call::set_dummy`] if it's present and drops any transaction
//!   with an encoded length higher than 200 bytes.
//!
//! ## Examples
//!
//! 1. Construct a signed call to accumulate the value in Dummy:
#![doc = docify::embed!("src/tests.rs", accumulate_dummy_works)]
//! 2. Construct a root call to execute `set_dummy`:
#![doc = docify::embed!("src/tests.rs", set_dummy_works)]
// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::{
	dispatch::{ClassifyDispatch, DispatchClass, DispatchResult, Pays, PaysFee, WeighData},
	traits::IsSubType,
	weights::Weight,
};
use frame_system::ensure_signed;
use log::info;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{Bounded, DispatchInfoOf, SaturatedConversion, Saturating, SignedExtension},
	transaction_validity::{
		InvalidTransaction, TransactionValidity, TransactionValidityError, ValidTransaction,
	},
};
use sp_std::{marker::PhantomData, prelude::*};

// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;

#[cfg(test)]
mod tests;

mod benchmarking;
pub mod weights;
pub use weights::*;

/// A type alias for the balance type from this pallet's point of view.
type BalanceOf<T> = <T as pallet_balances::Config>::Balance;

// Definition of the pallet logic, to be aggregated in a chain's runtime definition.
#[frame_support::pallet]
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

		/// A type representing the weight information for this pallet's callable functions.
		type WeightInfo: WeightInfo;
	}

	// The `Pallet` type is a placeholder we use to implement traits and methods for the pallet.
	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// This pallet implements the [`frame_support::traits::Hooks`] trait to demonstrate how we could
	// define some logic to execute in some context.
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		// `on_initialize` is executed at the beginning of the block before any extrinsics are
		// dispatched.
		//
		// This function must return the weight consumed by `on_initialize` and `on_finalize`.
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			// Anything that needs to be done at the start of the block.
			// We don't do anything here.
			Weight::zero()
		}

		// `on_finalize` is executed at the end of block after all extrinsics are dispatched.
		fn on_finalize(_n: BlockNumberFor<T>) {
			// Perform necessary data/state clean up here.
		}
	}

	/// An example storage item to store a single value, in our case, some Balance.
	/// This storage item uses [`OptionQuery`] by default which will return what is in actual state
	/// provided by [`sp_io::storage`]. If a value `v` exists in state, it returns `Some(v)`,
	/// otherwise it returns `None`.
	///
	/// The getter attribute generates a function on the `Pallet` struct that we can use to
	/// conveniently retrieve the current value stored.
	/// For example: 
	#[doc = docify::embed!("src/tests.rs", accumulate_dummy_works)]
	#[pallet::storage]
	#[pallet::getter(fn dummy)]
	pub(super) type Dummy<T: Config> = StorageValue<_, T::Balance>;

	/// An example storage item that stores a `u32` value.
	/// Here, we're using [`ValueQuery`] instead of the default [`OptionQuery`]. If a value exists
	/// in state, it will return that raw `u32` value, otherwise it will return `u32::default()`.
	/// For example:
	#[doc = docify::embed!("src/tests.rs", accumulate_dummy_value_query_works)]
	#[pallet::storage]
	#[pallet::getter(fn dummy_value_query)]
	pub(super) type DummyValueQuery<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// An example storage map that has enumerable entries. In our case this is a mapping of
	/// AccountIds to Balances.
	///
	/// We don't actually use this anywhere.
	#[pallet::storage]
	#[pallet::getter(fn accounts_to_balances_map)]
	pub(super) type AccountsToBalancesMap<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, T::Balance>;

	/// The genesis configuration type.
	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub dummy: T::Balance,
		pub dummy_value_query: u32,
		pub accounts_to_balances_map: Vec<(T::AccountId, T::Balance)>,
	}

	/// The genesis build for this pallet.
	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			Dummy::<T>::put(&self.dummy);
			DummyValueQuery::<T>::put(&self.dummy_value_query);
			for (a, b) in &self.accounts_to_balances_map {
				AccountsToBalancesMap::<T>::insert(a, b);
			}
		}
	}

	/// Events are a simple means of reporting specific conditions and
	/// circumstances that have happened that users, Dapps and/or chain explorers would find
	/// interesting and otherwise difficult to detect.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The Dummy value has been accumulated.
		AccumulateDummy {
			/// The new value of Dummy.
			balance: BalanceOf<T>,
		},
		/// The Dummy value has been set.
		SetDummy {
			/// The new value of Dummy.
			balance: BalanceOf<T>,
		},
	}

	// The call declaration block of our pallet. This states the entry points that we handle. The
	// macro takes care of the marshalling of arguments and dispatch.
	//
	// Each call must define a `#[pallet::weight(..)]` attribute to convey a set of
	// static information about its dispatch. The FRAME System and FRAME Executive pallets then use
	// this information to properly execute transaction, whilst keeping the total load of
	// the chain in a moderate rate.
	//
	// The parenthesized value of the `#[pallet::weight(..)]` attribute can be any type that
	// implements the following traits:
	// - [`WeighData`]: conveys the weight (a numeric representation of pure
	// execution time and difficulty) of the transaction
	// - [`ClassifyDispatch`]: demonstrates the [`DispatchClass`] of the call
	// - [`PaysFee`]: indicates whether an extrinsic must pay transaction fees or not
	//
	// Larger weights imply larger execution time a block can handle (less of which can be placed in
	// a single block).
	//
	// The weight for these calls relies on `WeightInfo`, which is auto-generated from the
	// benchmark toolchain.
	#[pallet::call(weight(<T as Config>::WeightInfo))]
	impl<T: Config> Pallet<T> {
		/// A public call that increases the value of `Dummy` in storage.
		///
		/// This can be called by any signed origin. The example uses the [`StorageValue::mutate`]
		/// method to demonstrate a safe and elegant way to accumulate the stored value.
		#[pallet::call_index(0)]
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
			Self::deposit_event(Event::AccumulateDummy { balance: increase_by });

			// all good, no refund
			Ok(())
		}

		/// A privileged call that can set the value in `Dummy` to a new value.
		///
		/// This must be called with a `Root` origin, implying that only the system as a whole has
		/// decided to execute this call. Different runtimes have different reasons to allow
		/// privileged calls to be executed - we don't need to care why. Because it's privileged, we
		/// can assume it's a one-off operation and substantial processing/storage/memory can be
		/// used without worrying about gameability or attack scenarios.
		#[pallet::call_index(1)]
		pub fn set_dummy(
			origin: OriginFor<T>,
			#[pallet::compact] new_value: T::Balance,
		) -> DispatchResult {
			// we ensure that the caller is a root origin
			ensure_root(origin)?;

			// put the new value into storage
			Dummy::<T>::put(new_value);

			// deposit an event
			Self::deposit_event(Event::SetDummy { balance: new_value });

			// we can also print out a log message to the client console via log::{error, warn,
			// info, debug, trace}, accepting format strings similar to `println!`
			info!("New value is now: {:?}", new_value);

			// all good, no refund
			Ok(())
		}
	}
}

// The main implementation block for the pallet.
impl<T: Config> Pallet<T> {
	/// Removes the values in our Dummy and DummyValueQuery storage items.
	///
	/// We use this function in our unit tests to showcase the behavior of [`OptionQuery`] and
	/// [`ValueQuery`].
	#[warn(dead_code)]
	fn do_reset_dummy(origin: T::RuntimeOrigin) -> DispatchResult {
		let _sender = ensure_signed(origin)?;

		Dummy::<T>::kill();
		DummyValueQuery::<T>::kill();

		Ok(())
	}

	/// Accumulates the value in DummyValueQuery.
	///
	/// This demonstrates using the `mutate` method from the [`StorageValue`] API.
	#[warn(dead_code)]
	fn accumulate_foo(origin: T::RuntimeOrigin, increase_by: u32) -> DispatchResult {
		let _sender = ensure_signed(origin)?;

		let prev = DummyValueQuery::<T>::get();
		// because DummyValueQuery uses [`ValueQuery`], 'value' in the closure is the raw type
		// instead of an Option<> type
		let result = DummyValueQuery::<T>::mutate(|value| {
			*value = value.saturating_add(increase_by);
			*value
		});
		assert!(prev + increase_by == result);

		Ok(())
	}
}

// Similar to other FRAME pallets, your pallet can also define a signed extension and perform some
// checks and [pre/post]processing [before/after] the transaction. A signed extension can be any
// decodable type that implements `SignedExtension`. See the trait definition for the full list of
// bounds. As a convention, you can follow this approach to create an extension for your pallet:
//   - If the extension does not carry any data, then use a tuple struct with just a `marker`
//     (needed for the compiler to accept `T: Config`) will suffice.
//   - Otherwise, create a tuple struct which contains the external data. Of course, for the entire
//     struct to be decodable, each individual item also needs to be decodable.
//
// Note that a signed extension can also indicate that a particular data must be present in the
// _signing payload_ of a transaction by providing an implementation for the `additional_signed`
// method. This example will not cover this type of extension. See `CheckSpecVersion` in
// [FRAME System](https://github.com/paritytech/substrate/tree/master/frame/system#signed-extensions)
// for an example.
//
// Using the extension, you can add some hooks to the life cycle of each transaction. Note that by
// default, an extension is applied to all `Call` functions (i.e. all transactions). The `Call` enum
// variant is given to each function of `SignedExtension`. Hence, you can filter based on pallet or
// a particular call if needed.
//
// Some extra information, such as encoded length, some static dispatch info like weight and the
// sender of the transaction (if signed) are also provided.
//
// The full list of hooks that can be added to a signed extension can be found
// [here](https://paritytech.github.io/polkadot-sdk/master/sp_runtime/traits/trait.SignedExtension.html).
//
// The signed extensions are aggregated in the runtime file of a substrate chain. All extensions
// should be aggregated in a tuple and passed to the `CheckedExtrinsic` and `UncheckedExtrinsic`
// types defined in the runtime. Lookup `pub type SignedExtra = (...)` in `node/runtime` and
// `node-template` for an example of this.

/// A simple signed extension that checks for the `set_dummy` call. In that case, it increases the
/// priority and prints some log.
///
/// Additionally, it drops any transaction with an encoded length higher than 200 bytes. No
/// particular reason why, just to demonstrate the power of signed extensions.
#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct WatchSetDummy<T: Config + Send + Sync>(PhantomData<T>);

impl<T: Config + Send + Sync> sp_std::fmt::Debug for WatchSetDummy<T> {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		write!(f, "WatchSetDummy")
	}
}

impl<T: Config + Send + Sync> SignedExtension for WatchSetDummy<T>
where
	<T as frame_system::Config>::RuntimeCall: IsSubType<Call<T>>,
{
	const IDENTIFIER: &'static str = "WatchSetDummy";
	type AccountId = T::AccountId;
	type Call = <T as frame_system::Config>::RuntimeCall;
	type AdditionalSigned = ();
	type Pre = ();

	fn additional_signed(&self) -> sp_std::result::Result<(), TransactionValidityError> {
		Ok(())
	}

	fn pre_dispatch(
		self,
		who: &Self::AccountId,
		call: &Self::Call,
		info: &DispatchInfoOf<Self::Call>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		self.validate(who, call, info, len).map(|_| ())
	}

	fn validate(
		&self,
		_who: &Self::AccountId,
		call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		len: usize,
	) -> TransactionValidity {
		// if the transaction is too big, just drop it.
		if len > 200 {
			return InvalidTransaction::ExhaustsResources.into()
		}

		// check for `set_dummy`
		match call.is_sub_type() {
			Some(Call::set_dummy { .. }) => {
				sp_runtime::print("set_dummy was received.");

				let valid_tx =
					ValidTransaction { priority: Bounded::max_value(), ..Default::default() };
				Ok(valid_tx)
			},
			_ => Ok(Default::default()),
		}
	}
}
