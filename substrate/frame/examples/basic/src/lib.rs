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

//! # Basic Example Pallet
//!
//! A pallet demonstrating concepts, APIs and structures common to most FRAME runtimes.
//!
//! **This pallet serves as an example and is not meant to be used in production.**
//!
//! > Made with *Substrate*, for *Polkadot*.
//!
//! [![github]](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/frame/examples/basic)
//! [![polkadot]](https://polkadot.com)
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
//! - A custom weight calculator able to classify a call's dispatch class (see:
//!   [`frame_support::dispatch::DispatchClass`])
//! - Pallet hooks to implement some custom logic that's executed before and after a block is
//!   imported (see: [`frame_support::traits::Hooks`])
//! - Inherited weight annotation for pallet calls, used to create less repetition for calls that
//!   use the [`Config::WeightInfo`] trait to calculate call weights. This can also be overridden,
//!   as demonstrated by [`Call::set_dummy`].
//! - Performing storage migration with version upgrade of the Dummy Storage Value away from:
//! - #[pallet::storage]
//! 	pub(super) type Dummy<T: Config> = StorageValue<_, T::Balance>;
//!  - Into:
//!  - #[pallet::storage]
//! 	pub(super) type Dummy<T: Config> = StorageMap<_,Twox64Concat, T::AccountId, T::Balance,
//! OptionQuery>;
//! - A private function that performs a storage update.
//! - A simple transaction extension implementation (see:
//!   [`sp_runtime::traits::TransactionExtension`]) which increases the priority of the
//!   [`Call::set_dummy`] if it's present and drops any transaction with an encoded length higher
//!   than 200 bytes.

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use core::marker::PhantomData;
use frame_support::{
	dispatch::{ClassifyDispatch, DispatchClass, DispatchResult, Pays, PaysFee, WeighData},
	traits::{IsSubType, Get},
	pallet_prelude::TransactionSource,
	weights::Weight,
};
use frame_system::ensure_signed;
use log::info;
use scale_info::TypeInfo;
use sp_runtime::{
	impl_tx_ext_default,
	traits::{
		Bounded, DispatchInfoOf, DispatchOriginOf, SaturatedConversion, Saturating, StaticLookup,
		TransactionExtension, ValidateResult,
	},
	transaction_validity::{InvalidTransaction, ValidTransaction},
};
use sp_runtime::traits::Zero;

// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;

#[cfg(test)]
mod tests;

mod benchmarking;
pub mod weights;
pub use weights::*;
mod migration;

const LOG_TARGET: &str = "runtime::example-basic";

type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;
/// A type alias for the balance type from this pallet's point of view.
type BalanceOf<T> = <T as pallet_balances::Config>::Balance;
const MILLICENTS: u32 = 1_000_000_000;

// A custom weight calculator tailored for the dispatch call `set_dummy()`. This actually examines
// the arguments and makes a decision based upon them.
//
// The `WeightData<T>` trait has access to the arguments of the dispatch that it wants to assign a
// weight to. Nonetheless, the trait itself cannot make any assumptions about what the generic type
// of the arguments (`T`) is. Based on our needs, we could replace `T` with a more concrete type
// while implementing the trait. The `pallet::weight` expects whatever implements `WeighData<T>` to
// replace `T` with a tuple of the dispatch arguments. This is exactly how we will craft the
// implementation below.
//
// The rules of `WeightForSetDummy` are as follows:
// - The final weight of each dispatch is calculated as the argument of the call multiplied by the
//   parameter given to the `WeightForSetDummy`'s constructor.
// - assigns a dispatch class `operational` if the argument of the call is more than 1000.
//
// More information can be read at:
//   - https://docs.substrate.io/main-docs/build/tx-weights-fees/
//
// Manually configuring weight is an advanced operation and what you really need may well be
//   fulfilled by running the benchmarking toolchain. Refer to `benchmarking.rs` file.
struct WeightForSetDummy<T: pallet_balances::Config>(BalanceOf<T>);

impl<T: pallet_balances::Config> WeighData<(&AccountIdLookupOf<T>, &BalanceOf<T>)>
	for WeightForSetDummy<T>
{
	fn weigh_data(&self, target: (&AccountIdLookupOf<T>, &BalanceOf<T>)) -> Weight {
		let multiplier = self.0;
		// *target.1 is the amount passed into the extrinsic
		let cents = *target.1 / <BalanceOf<T>>::from(MILLICENTS);
		Weight::from_parts((cents * multiplier).saturated_into::<u64>(), 0)
	}
}

impl<T: pallet_balances::Config> ClassifyDispatch<(&AccountIdLookupOf<T>, &BalanceOf<T>)>
	for WeightForSetDummy<T>
{
	fn classify_dispatch(&self, target: (&AccountIdLookupOf<T>, &BalanceOf<T>)) -> DispatchClass {
		// current_balance + target amount passed into the extrinsic.
		if self.0.saturating_add(*target.1) > <BalanceOf<T>>::from(1000u32) {
			DispatchClass::Operational
		} else {
			DispatchClass::Normal
		}
	}
}

impl<T: pallet_balances::Config> PaysFee<(&AccountIdLookupOf<T>, &BalanceOf<T>)>
	for WeightForSetDummy<T>
{
	fn pays_fee(&self, target: (&AccountIdLookupOf<T>, &BalanceOf<T>)) -> Pays {
		if *target.1 > <BalanceOf<T>>::from(1000u32) {
			Pays::Yes
		} else {
			Pays::No
		}
	}
}

/// No actual amount is entered into the users account(i.e this is done by this Traits anf functions
/// to actaully change a users balance., this just mainly for example purposes)

// Definition of the pallet logic, to be aggregated at runtime definition through
// `construct_runtime`.
#[frame_support::pallet]
pub mod pallet {
	// Import various types used to declare pallet in scope.
	use super::*;
	use frame_support::{pallet_prelude::*, weights::WeightMeter};
	use frame_system::pallet_prelude::*;

	/// Our pallet's configuration trait. All our types and constants go in here. If the
	/// pallet is dependent on specific other pallets, then their configuration traits
	/// should be added to our implied traits list.
	///
	/// `frame_system::Config` should always be included.
	#[pallet::config]
	pub trait Config: pallet_balances::Config + frame_system::Config {
		// Setting a constant config parameter from the runtime(MagicNumber, the Counted storage
		// must be cleared before new values are added fo some operation to take place.)
		#[pallet::constant]
		type MagicNumber: Get<Self::Balance>;
		
		#[pallet::constant]
		type OperationMax: Get<u16>;

		/// Type representing the weight of this pallet
		type WeightInfo: WeightInfo;
	}

	// Simple declaration of the `Pallet` type. It is placeholder we use to implement traits and
	// method.
	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// This pallet implements the [`frame_support::traits::Hooks`] trait to define some logic to
	// execute in some context.
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		// `on_initialize` is executed at the beginning of the block before any extrinsic are
		// dispatched.
		//
		// This function must return the weight consumed by `on_initialize` and `on_finalize`.
		fn on_initialize(_n: BlockNumberFor<T>) -> Weight {
			// Anything that needs to be done at the start of the block.
			// We don't do anything here.
			// Reset the temporary balances at the start of each block.

			for (key, _) in Dummy::<T>::iter() {
				Dummy::<T>::remove(key.clone());
				Bar::<T>::remove(key);
			}
			Foo::<T>::kill();
			let _ = Holds::<T>::clear(u32::MAX, None);
			
			Weight::zero()
		}

		// `on_finalize` is executed at the end of block after all extrinsic are dispatched.
		fn on_finalize(_n: BlockNumberFor<T>) {
			// Perform necessary data/state clean up here.
			Self::check_win_condition();
		}

		// The on_idle hook is called when the system is idle, typically
		// during periods when no extrinsics are being processed.
		// More details about the hook can be found here: https://github.com/paritytech/substrate/issues/4064.
		fn on_idle(n: BlockNumberFor<T>, _remaining_weight: Weight) -> Weight {
			// Log the block number at which the on_idle hook is being called
			log::info!("on_idle called at block number {:?}", n);

			// Return weight for the operation; this indicates the computational cost
			// In this case, we return Weight::zero() because we are not performing any heavy
			// operations.
			Weight::zero()
		}

		fn on_poll(_n: BlockNumberFor<T>, _weight: &mut WeightMeter) {
			// Polling logic for offchain workers.
			log::info!("on_poll called");
		}

		fn on_runtime_upgrade() -> Weight {
			// Logic to handle runtime upgrades.
			log::info!("on_runtime_upgrade called");
			Weight::zero()
		}

		// Assert invariants that must always be held in a pallet with non-trivial.
		// More details about implementing the `try-state`
		// can be found here: https://github.com/paritytech/polkadot-sdk/issues/239.
		// This pallet contains trivial logic hence no invariant checking required.
		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), TryRuntimeError> {
			// Invariant checks for try-runtime.
			// Verify that the total balance matches the sum of all user balances.
			let total_balance = Foo::<T>::get();
			let balances: T::Balance = Bar::<T>::iter_values().sum();
			let hols: T::Balance = Holds::<T>::iter_values().sum();
			let sum = balances.saturating_add(holds);

			ensure!(total_balance == sum, "Total balance does not match sum of all user balances");

			Ok(())
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			// Pre-upgrade logic for try-runtime.
			log::info!("pre_upgrade called");
			Ok(Vec::new())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
			// Post-upgrade logic for try-runtime.
			log::info!("post_upgrade called");
			Ok(())
		}

		// A runtime code run after every block and have access to extended set of APIs.
		//
		// For instance you can generate extrinsics for the upcoming produced block.
		fn offchain_worker(_n: BlockNumberFor<T>) {
			// We don't do anything here.
			// but we could dispatch extrinsic (transaction/unsigned/inherent) using
			// sp_io::submit_extrinsic.
			// To see example on offchain worker, please refer to example-offchain-worker pallet
			// accompanied in this repository.
			log::info!("offchain_worker called at block number {:?}", _n);
		}

		fn integrity_test() {
			// Integrity Check 4: Ensure correct logging during upgrades.
			log::info!("Integrity checks passed successfully.");
		}
	}

	// The call declaration. This states the entry points that we handle. The
	// macro takes care of the marshalling of arguments and dispatch.
	//
	// Anyone can have these functions execute by signing and submitting
	// an extrinsic. Ensure that calls into each of these execute in a time, memory and
	// using storage space proportional to any costs paid for by the caller or otherwise the
	// difficulty of forcing the call to happen.
	//
	// Generally you'll want to split these into three groups:
	// - Public calls that are signed by an external account.
	// - Root calls that are allowed to be made only by the governance system.
	// - Unsigned calls that can be of two kinds:
	//   * "Inherent extrinsics" that are opinions generally held by the block authors that build
	//     child blocks.
	//   * Unsigned Transactions that are of intrinsic recognizable utility to the network, and are
	//     validated by the runtime.
	//
	// Information about where this dispatch initiated from is provided as the first argument
	// "origin". As such functions must always look like:
	//
	// `fn foo(origin: OriginFor<T>, bar: Bar, baz: Baz) -> DispatchResultWithPostInfo { ... }`
	//
	// The `DispatchResultWithPostInfo` is required as part of the syntax (and can be found at
	// `pallet_prelude::DispatchResultWithPostInfo`).
	//
	// There are three entries in the `frame_system::Origin` enum that correspond
	// to the above bullets: `::Signed(AccountId)`, `::Root` and `::None`. You should always match
	// against them as the first thing you do in your function. There are three convenience calls
	// in system that do the matching for you and return a convenient result: `ensure_signed`,
	// `ensure_root` and `ensure_none`.
	#[pallet::call(weight(<T as Config>::WeightInfo))]
	impl<T: Config> Pallet<T> {
		/// This is your public interface. Be extremely careful.
		/// This is just a simple example of how to interact with the pallet from the external
		/// world.
		// This just increases the value of `Dummy` by `increase_by`.
		//
		// Since this is a dispatched function there are two extremely important things to
		// remember:
		//
		// - MUST NOT PANIC: Under no circumstances (save, perhaps, storage getting into an
		// irreparably damaged state) must this function panic.
		// - NO SIDE-EFFECTS ON ERROR: This function must either complete totally (and return
		// `Ok(())` or it must have no side-effects on storage and return `Err('Some reason')`.
		//
		// The first is relatively easy to audit for - just ensure all panickers are removed from
		// logic that executes in production (which you do anyway, right?!). To ensure the second
		// is followed, you should do all tests for validity at the top of your function. This
		// is stuff like checking the sender (`origin`) or that state is such that the operation
		// makes sense.
		//
		// Once you've determined that it's all good, then enact the operation and change storage.
		// If you can't be certain that the operation will succeed without substantial computation
		// then you have a classic blockchain attack scenario. The normal way of managing this is
		// to attach a bond to the operation. As the first major alteration of storage, reserve
		// some value from the sender's account (`Balances` Pallet has a `reserve` function for
		// exactly this scenario). This amount should be enough to cover any costs of the
		// substantial execution in case it turns out that you can't proceed with the operation.
		//
		// If it eventually transpires that the operation is fine and, therefore, that the
		// expense of the checks should be borne by the network, then you can refund the reserved
		// deposit. If, however, the operation turns out to be invalid and the computation is
		// wasted, then you can burn it or repatriate elsewhere.
		//
		// Security bonds ensure that attackers can't game it by ensuring that anyone interacting
		// with the system either progresses it or pays for the trouble of faffing around with
		// no progress.
		//
		// If you don't respect these rules, it is likely that your chain will be attackable.
		//
		// Each transaction must define a `#[pallet::weight(..)]` attribute to convey a set of
		// static information about its dispatch. FRAME System and FRAME Executive pallet then use
		// this information to properly execute the transaction, whilst keeping the total load of
		// the chain in a moderate rate.
		//
		// The parenthesized value of the `#[pallet::weight(..)]` attribute can be any type that
		// implements a set of traits, namely [`WeighData`], [`ClassifyDispatch`], and
		// [`PaysFee`]. The first conveys the weight (a numeric representation of pure
		// execution time and difficulty) of the transaction and the second demonstrates the
		// [`DispatchClass`] of the call, the third gives whereas extrinsic must pay fees or not.
		// A higher weight means a larger transaction (less of which can be placed in a single
		// block).
		//
		// The weight for this extrinsic we rely on the auto-generated `WeightInfo` from the
		// benchmark toolchain.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::accumulate_dummy())]
		pub fn accumulate_dummy(who: OriginFor<T>, increase_by: T::Balance) -> DispatchResult {
			Self::do_accumulate_dummy(who, increase_by)
		}

		/// A privileged call; in this case it resets our dummy value to something new.
		// Implementation of a privileged call. The `origin` parameter is ROOT because
		// it's not (directly) from an extrinsic, but rather the system as a whole has decided
		// to execute it. Different runtimes have different reasons for allow privileged
		// calls to be executed - we don't need to care why. Because it's privileged, we can
		// assume it's a one-off operation and substantial processing/storage/memory can be used
		// without worrying about gameability or attack scenarios.
		//
		// The weight for this extrinsic we use our own weight object `WeightForSetDummy` to
		// determine its weight
		#[pallet::call_index(1)]
		#[pallet::weight(WeightForSetDummy::<T>(<BalanceOf<T>>::from(100u32)))]
		pub fn set_dummy(
			origin: OriginFor<T>,
			who: AccountIdLookupOf<T>,
			#[pallet::compact] new_value: T::Balance,
		) -> DispatchResult {
			ensure_root(origin)?;
			let who = T::Lookup::lookup(who)?;

			let set_dummy_operation_key: u8 = 2; // TODO: set key to a byte string instead

			// assert no value exixsts for that user.
			// Assert no value exists for the user.
			ensure!(Dummy::<T>::get(who.clone()).is_none(), Error::<T>::ValueAlreadySet);

			// Print out log or debug message in the console via log::{error, warn, info, debug,
			// trace}, accepting format strings similar to `println!`.
			// https://paritytech.github.io/substrate/master/sp_io/logging/fn.log.html
			// https://paritytech.github.io/substrate/master/frame_support/constant.LOG_TARGET.html
			info!("New value is now: {:?}", new_value);

			// Put the new value into storage.
			<Dummy<T>>::insert(who, new_value);
			Self::record_operation(set_dummy_operation_key)?;

			Self::deposit_event(Event::SetDummy { balance: new_value });

			// All good, no refund.
			Ok(())
		}

		#[pallet::call_index(2)]		
		#[pallet::weight(10_000)] // based on how much is cleared the weight calculkated using the custom weight function or// use the wightinfo file.			
		pub fn clear_temporary_balance(
			origin: OriginFor<T>,
			who: AccountIdLookupOf<T>,
		) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			let who = T::Lookup::lookup(who)?;
			let clear_dummy_operation_key: u8 = 3;

			Self::record_operation(clear_dummy_operation_key)?;
			Dummy::<T>::remove(&who);
			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(10_000)]
		pub fn update_balance(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let update_permanent_operation_key: u8 = 4;

			// Get the new balance from Dummy storage
			let new_balance = Dummy::<T>::get(&who).ok_or(Error::<T>::TempoaryBalanceNotFound)?;

			// Update the user's balance
			Bar::<T>::try_mutate(&who, |balance| -> DispatchResult {
				// Ensure that the balance is initialized to avoid issues with None
				let current_balance = balance.unwrap_or_else(Zero::zero);
				let updated_balance = current_balance.saturating_add(new_balance);
				Self::record_operation(update_permanent_operation_key)?;
				*balance = Some(updated_balance);
				Ok(())
			})?;

			// Update the total balance in Foo storage
			Foo::<T>::mutate(|total| {
				*total = total.saturating_add(new_balance);
			});

			// Emit the event
			Self::deposit_event(Event::BalanceUpdated(who.clone(), new_balance));
			Ok(())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(10_000)]
		pub fn burn(origin: OriginFor<T>, amount: T::Balance) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let burn_key: u8 = 5;

			// Ensure the withdrawal amount is above the minimum value defined by MILLICENTS.
			let min_amount = T::Balance::from(MILLICENTS);
			ensure!(amount >= min_amount, Error::<T>::InsufficientAmount);

			Bar::<T>::try_mutate(&who, |balance| -> DispatchResult {
				let current_balance = balance.unwrap_or_else(Zero::zero);
				ensure!(current_balance > Zero::zero(), Error::<T>::InsufficientBalance);
				let held_amount = Holds::<T>::get(&who).unwrap_or_else(T::Balance::zero);
				let available_balance = current_balance.saturating_sub(held_amount);

				// Ensure the amount to be burned does not exceed the available balance after
				// considering holds.
				ensure!(amount <= available_balance, Error::<T>::InsufficientBalance);
				Self::record_operation(burn_key)?;
				// Proceed with the burn operation if the check passes.
				*balance = Some(current_balance.saturating_sub(amount));
				Foo::<T>::mutate(|total| *total = total.saturating_sub(amount));

				Self::deposit_event(Event::Burnt(who.clone(), amount));
				Ok(())
			})
		}
		/// In real usecase use these traits, Hold, Freeze e.t.c.

		#[pallet::call_index(5)]
		#[pallet::weight(10_000)]
		pub fn place_temporary_hold(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Bar::<T>::try_mutate(&who, |balance| -> DispatchResult {
				let current_balance = balance.unwrap_or_else(Zero::zero);
				// check if balance is greater than zero
				ensure!(current_balance > Zero::zero(), Error::<T>::InsufficientBalance);
				ensure!(amount <= current_balance, Error::<T>::InsufficientBalance);

				*balance = Some(current_balance.saturating_sub(amount));
				Holds::<T>::insert(&who, amount);

				Self::deposit_event(Event::TemporaryHoldPlaced(who.clone(), amount));
				Ok(())
			})
		}

		#[pallet::call_index(6)]
		#[pallet::weight(10_000)]
		pub fn release_temporary_hold(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Holds::<T>::try_mutate_exists(&who, |hold| -> DispatchResult {
				let held_amount = hold.take().ok_or(Error::<T>::NoHoldFound)?;

				Bar::<T>::mutate(&who, |balance| {
					let current_balance = balance.unwrap_or_else(Zero::zero);
					let new_balance = current_balance.saturating_add(held_amount);
					*balance = Some(new_balance);
				});

				Self::deposit_event(Event::TemporaryHoldReleased(who.clone(), held_amount));
				Ok(())
			})
		}
	}

	/// Events are a simple means of reporting specific conditions and
	/// circumstances that have happened that users, Dapps and/or chain explorers would find
	/// interesting and otherwise difficult to detect.
	#[pallet::event]
	/// This attribute generate the function `deposit_event` to deposit one of this pallet event,
	/// it is optional, it is also possible to provide a custom implementation.
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		// Just a normal `enum`, here's a dummy event to ensure it compiles.
		/// Dummy event, just here so there's a generic type that's used.
		AccumulateDummy {
			balance: T::Balance,
		},
		SetDummy {
			balance: T::Balance,
		},

		WithdrawalAttempt(T::AccountId, T::Balance),
		TemporaryHoldPlaced(T::AccountId, T::Balance),
		TemporaryHoldReleased(T::AccountId, T::Balance),
		BalanceUpdated(T::AccountId, T::Balance),
		Burnt(T::AccountId, T::Balance),
		GameWon(T::AccountId, T::Balance),
		OperationCountUpdated(u8, u16)
		// TODO: Max Operation reached, wait till next block, with the Magic Nuimber.
	}

	#[pallet::error]
	pub enum Error<T> {
		NoEntryForSender,
		ValueAlreadySet,
		InsufficientBalance,
		ExceedsWithdrawalLimit,
		NoHoldFound,
		InsufficientAmount, // New error for amount below MILLICENTS
		TempoaryBalanceNotFound,
		OperationLimitExceeded,
	}

	// pallet::storage attributes allow for type-safe usage of the Substrate storage database,
	// so you can keep things around between blocks.
	//
	// Any storage must be one of `StorageValue`, `StorageMap`, or `StorageDoubleMap`.
	// The first generic holds the prefix to use and is generated by the macro.
	// The query kind is either `OptionQuery` (the default) or `ValueQuery`.
	// Below are examples with the correct methods for each type of storage:

	// Example for `StorageMap` using `Twox64Concat` hasher:
	// `type Dummy<T: Config> = StorageMap<_, Twox64Concat, T::AccountId, T::Balance, OptionQuery>`;
	// Methods:
	// - `Dummy::insert(who: T::AccountId, new_value: T::Balance);`  // Inserts a new value.
	// - `Dummy::remove(who: &T::AccountId);`                        // Removes the value associated
	//   with the key.
	// - `Dummy::contains_key(who: &T::AccountId) -> bool;`          // Checks if a key exists.
	// - `Dummy::get(who: &T::AccountId) -> Option<T::Balance>;`     // Retrieves the value
	//   associated with a key.

	// Example for `StorageMap` using `Blake2_128Concat` hasher:
	// `type Bar<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, T::Balance>`;
	// Methods:
	// - `Bar::insert(who: T::AccountId, new_value: T::Balance);`  // Inserts a new value.
	// - `Bar::remove(who: &T::AccountId);`                        // Removes the value associated
	//   with the key.
	// - `Bar::get(who: &T::AccountId) -> Option<T::Balance>;`     // Retrieves the value associated
	//   with a key.

	// Example for `StorageValue` with `ValueQuery`:
	// `type Foo<T: Config> = StorageValue<_, T::Balance, ValueQuery>`;
	// Methods:
	// - `Foo::get() -> T::Balance;`              // Retrieves the stored value.
	// - `Foo::put(new_value: T::Balance);`       // Stores a new value.
	// - `Foo::mutate(|v| *v += 1);`              // Mutates the stored value.
	// - `Foo::kill();`                           // Removes the stored value, sets it to default.

	// Example for `CountedStorageMap`:
	// `type CountedMap<T: Config> = CountedStorageMap<_, Blake2_128Concat, u8, u16>`;
	// Methods:
	// - `CountedMap::insert(key: u8, value: u16);`       // Inserts a key-value pair.
	// - `CountedMap::remove(key: &u8);`                 // Removes a key-value pair.
	// - `CountedMap::get(key: &u8) -> Option<u16>;`     // Retrieves the value for a key.
	// - `CountedMap::iter() -> impl Iterator<Item=(u8, u16)>;` // Iterates over all key-value
	//   pairs.
	// - `CountedMap::count() -> u32;`                   // Returns the count of stored items.
	#[pallet::storage]
	pub(super) type Bar<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, T::Balance>;
	/// Holds tempoary balance.
	#[pallet::storage]
	pub(super) type Dummy<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, T::Balance, OptionQuery>;
	/// Account for an operation to take place(Intents).
	#[pallet::storage]
	pub type CountedMap<T> = CountedStorageMap<_, Blake2_128Concat, u8, u16>;
	/// Store the total value of Balances held in storage, this one uses the query kind:
	/// `ValueQuery`, we'll demonstrate the usage of 'mutate' API.
	#[pallet::storage]
	pub(super) type Foo<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

	#[pallet::storage]
	pub(super) type Holds<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, T::Balance>;

	// The genesis config type.
	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub dummy: Vec<(T::AccountId, T::Balance)>,
	}

	// The build of genesis for the pallet.
	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			for (a, b) in &self.dummy {
				<Dummy<T>>::insert(a, b);
			}
		}
	}
}

// The main implementation block for the pallet. Functions here fall into three broad
// categories:
// - Public interface. These are functions that are `pub` and generally fall into inspector
// functions that do not write to storage and operation functions that do.
// - Private functions. These are your usual private utilities unavailable to other pallets.
impl<T: Config> Pallet<T> {
	// Add public immutables and private mutables.
	#[allow(dead_code)]
	fn do_accumulate_dummy(
		origin: frame_system::pallet_prelude::OriginFor<T>,
		increase_by: T::Balance,
	) -> DispatchResult {
		// Ensure that the origin is signed, meaning the sender is an account.
		let who = ensure_signed(origin)?;

		// Check if the sender already has an entry in the Dummy map.
		// We want to ensure the sender is the owner of the entry.
		let current_dummy = Dummy::<T>::get(&who);

		if current_dummy.is_none() {
			// If no entry exists, we can create a new one or return an error
			// indicating the sender does not own an entry.
			return Err(Error::<T>::NoEntryForSender.into());
		}

		// If an entry exists, we can mutate it.
		<Dummy<T>>::mutate(&who, |dummy_opt| {
			if let Some(dummy) = dummy_opt {
				// Using `saturating_add` to avoid overflow and safely update the value.
				*dummy = dummy.saturating_add(increase_by);
			} else {
				// If it's None, initialize it to the increase value.
				*dummy_opt = Some(increase_by);
			}
		});

		// Deposit an event to inform the outside world of the change.
		Self::deposit_event(Event::AccumulateDummy { balance: increase_by });

		// Return Ok() since the operation was successful.
		Ok(())
	}

	// so each operation has a specific key, withdraw operation, set_dummy_operation..
	// that is what this does.. so this should be a helper function that records main extrinsic
	// operatrions.
	pub fn record_operation(key: u8) -> DispatchResult {
		// Fetch the current count from the storage.
		let current_count = CountedMap::<T>::get(key).unwrap_or_default();
		
		// // Check if the current count exceeds the magic number..
		frame_support::ensure!(current_count <= T::OperationMax::get(), Error::<T>::OperationLimitExceeded);
		
		// Increment the count if the limit has not been reached.
		let new_count = current_count + 1;
		CountedMap::<T>::insert(key, new_count);
	
		// Emit an event indicating that the operation count has been updated.
		Self::deposit_event(Event::OperationCountUpdated(key, new_count));
	
		Ok(())
	}

	/// Checks if a player has won the game based on specific criteria.
	/// Checks if any player has reached the magic number in their balance or if the total balance
    /// reaches the magic number, and emits events accordingly.
    pub fn check_win_condition() {
        let total_balance = Foo::<T>::get();

        // Check if any player's balance reaches the magic number and emit event
        for (account, balance) in Bar::<T>::iter() {
            if balance >= <T as Config>::MagicNumber::get() {
                Self::deposit_event(Event::GameWon(account.clone(), balance));
            }
        }

        // Emit event for all accounts if the total balance reaches the magic number
        if total_balance >= <T as Config>::MagicNumber::get() {
            for (account, balance) in Bar::<T>::iter() {
                Self::deposit_event(Event::GameWon(account.clone(), balance));
            }
        }
    }
}

// Similar to other FRAME pallets, your pallet can also define a transaction extension and perform
// some checks and [pre/post]processing [before/after] the transaction. A transaction extension can
// be any decodable type that implements `TransactionExtension`. See the trait definition for the
// full list of bounds. As a convention, you can follow this approach to create an extension for
// your pallet:
//   - If the extension does not carry any data, then use a tuple struct with just a `marker`
//     (needed for the compiler to accept `T: Config`) will suffice.
//   - Otherwise, create a tuple struct which contains the external data. Of course, for the entire
//     struct to be decodable, each individual item also needs to be decodable.
//
// Note that a transaction extension can also indicate that a particular data must be present in the
// _signing payload_ of a transaction by providing an implementation for the `implicit` method. This
// example will not cover this type of extension. See `CheckSpecVersion` in [FRAME
// System](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/frame/system#signed-extensions)
// for an example.
//
// Using the extension, you can add some hooks to the life cycle of each transaction. Note that by
// default, an extension is applied to all `Call` functions (i.e. all transactions). the `Call` enum
// variant is given to each function of `TransactionExtension`. Hence, you can filter based on
// pallet or a particular call if needed.
//
// Some extra information, such as encoded length, some static dispatch info like weight and the
// sender of the transaction (if signed) are also provided.
//
// The full list of hooks that can be added to a transaction extension can be found in the
// `TransactionExtension` trait definition.
//
// The transaction extensions are aggregated in the runtime file of a substrate chain. All
// extensions should be aggregated in a tuple and passed to the `CheckedExtrinsic` and
// `UncheckedExtrinsic` types defined in the runtime. Lookup `pub type TxExtension = (...)` in
// `node/runtime` and `node-template` for an example of this.

/// A simple transaction extension that checks for the `set_dummy` call. In that case, it increases
/// the priority and prints some log.
///
/// Additionally, it drops any transaction with an encoded length higher than 200 bytes. No
/// particular reason why, just to demonstrate the power of transaction extensions.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct WatchDummy<T: Config + Send + Sync>(PhantomData<T>);

impl<T: Config + Send + Sync> core::fmt::Debug for WatchDummy<T> {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "WatchDummy")
	}
}

impl<T: Config + Send + Sync> TransactionExtension<<T as frame_system::Config>::RuntimeCall>
	for WatchDummy<T>
where
	<T as frame_system::Config>::RuntimeCall: IsSubType<Call<T>>,
{
	const IDENTIFIER: &'static str = "WatchDummy";
	type Implicit = ();
	type Pre = ();
	type Val = ();

	fn validate(
		&self,
		origin: DispatchOriginOf<<T as frame_system::Config>::RuntimeCall>,
		call: &<T as frame_system::Config>::RuntimeCall,
		_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
		_source: TransactionSource,
	) -> ValidateResult<Self::Val, <T as frame_system::Config>::RuntimeCall> {
		// if the transaction is too big, just drop it.
		if len > 200 {
			return Err(InvalidTransaction::ExhaustsResources.into())
		}

		// check for `set_dummy`
		let validity = match call.is_sub_type() {
			Some(Call::set_dummy { .. }) => {
				sp_runtime::print("set_dummy was received.");

				let valid_tx =
					ValidTransaction { priority: Bounded::max_value(), ..Default::default() };
				valid_tx
			},
			_ => Default::default(),
		};
		Ok((validity, (), origin))
	}
	impl_tx_ext_default!(<T as frame_system::Config>::RuntimeCall; weight prepare);
}
