// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! > Made with *Substrate*, for *Polkadot*.
//!
//! [![github]](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/frame/sudo)
//! [![polkadot]](https://polkadot.network)
//!
//! [github]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
//! [polkadot]: https://img.shields.io/badge/polkadot-E6007A?style=for-the-badge&logo=polkadot&logoColor=white
//!
//! # Sudo Pallet
//!
//! A pallet to provide a way to execute privileged runtime calls using a specified sudo ("superuser
//! do") account.
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Overview
//!
//! In Substrate blockchains, pallets may contain dispatchable calls that can only be called at
//! the system level of the chain (i.e. dispatchables that require a `Root` origin).
//! Setting a privileged account, called the _sudo key_, allows you to make such calls as an
//! extrinisic.
//!
//! Here's an example of a privileged function in another pallet:
//!
//! ```
//! #[frame_support::pallet]
//! pub mod pallet {
//! 	use super::*;
//! 	use frame_support::pallet_prelude::*;
//! 	use frame_system::pallet_prelude::*;
//!
//! 	#[pallet::pallet]
//! 	pub struct Pallet<T>(_);
//!
//! 	#[pallet::config]
//! 	pub trait Config: frame_system::Config {}
//!
//! 	#[pallet::call]
//! 	impl<T: Config> Pallet<T> {
//! 		#[pallet::weight(0)]
//!         pub fn privileged_function(origin: OriginFor<T>) -> DispatchResult {
//!             ensure_root(origin)?;
//!
//!             // do something...
//!
//!             Ok(())
//!         }
//! 	}
//! }
//! ```
//!
//! With the Sudo pallet configured in your chain's runtime you can execute this privileged
//! function by constructing a call using the [`sudo`](Pallet::sudo) dispatchable.
//!
//! To use this pallet in your runtime, a sudo key must be specified in the [`GenesisConfig`] of
//! the pallet. You can change this key at anytime once your chain is live using the
//! [`set_key`](Pallet::set_key) dispatchable, however <strong>only one sudo key can be set at a
//! time</strong>. The pallet also allows you to make a call using
//! [`sudo_unchecked_weight`](Pallet::sudo_unchecked_weight), which allows the sudo account to
//! execute a call with a custom weight.
//!
//! <div class="example-wrap" style="display:inline-block"><pre class="compile_fail"
//! style="white-space:normal;font:inherit;">
//! <strong>Note:</strong> this pallet is not meant to be used inside other pallets. It is only
//! meant to be used by constructing runtime calls from outside the runtime.
//! </pre></div>
//!
//! This pallet also defines a [`SignedExtension`](sp_runtime::traits::SignedExtension) called
//! [`CheckOnlySudoAccount`] to ensure that only signed transactions by the sudo account are
//! accepted by the transaction pool. The intended use of this signed extension is to prevent other
//! accounts from spamming the transaction pool for the initial phase of a chain, during which
//! developers may only want a sudo account to be able to make transactions.
//!
//! Learn more about the `Root` origin in the [`RawOrigin`](frame_system::RawOrigin) type
//! documentation.
//!
//! ### Examples
//!
//! 1. You can make a privileged runtime call using `sudo` with an account that matches the sudo
//!    key.
#![doc = docify::embed!("src/tests.rs", sudo_basics)]
//!
//! 2. Only an existing sudo key can set a new one.
#![doc = docify::embed!("src/tests.rs", set_key_basics)]
//!
//! 3. You can also make non-privileged calls using `sudo_as`.
#![doc = docify::embed!("src/tests.rs", sudo_as_emits_events_correctly)]
//!
//! ## Low Level / Implementation Details
//!
//! This pallet checks that the caller of its dispatchables is a signed account and ensures that the
//! caller matches the sudo key in storage.
//! A caller of this pallet's dispatchables does not pay any fees to dispatch a call. If the account
//! making one of these calls is not the sudo key, the pallet returns a [`Error::RequireSudo`]
//! error.
//!
//! Once an origin is verified, sudo calls use `dispatch_bypass_filter` from the
//! [`UnfilteredDispatchable`](frame_support::traits::UnfilteredDispatchable) trait to allow call
//! execution without enforcing any further origin checks.

#![deny(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

use sp_runtime::{traits::StaticLookup, DispatchResult};
use sp_std::prelude::*;

use frame_support::{dispatch::GetDispatchInfo, traits::UnfilteredDispatchable};

mod extension;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;
pub use weights::WeightInfo;

pub use extension::CheckOnlySudoAccount;
pub use pallet::*;

type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;

#[frame_support::pallet]
pub mod pallet {
	use super::{DispatchResult, *};
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// Default preludes for [`Config`].
	pub mod config_preludes {
		use super::*;
		use frame_support::derive_impl;

		/// Default prelude sensible to be used in a testing environment.
		pub struct TestDefaultConfig;

		#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig, no_aggregated_types)]
		impl frame_system::DefaultConfig for TestDefaultConfig {}

		#[frame_support::register_default_impl(TestDefaultConfig)]
		impl DefaultConfig for TestDefaultConfig {
			type WeightInfo = ();
			#[inject_runtime_type]
			type RuntimeEvent = ();
			#[inject_runtime_type]
			type RuntimeCall = ();
		}
	}
	#[pallet::config(with_default)]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[pallet::no_default_bounds]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// A sudo-able call.
		#[pallet::no_default_bounds]
		type RuntimeCall: Parameter
			+ UnfilteredDispatchable<RuntimeOrigin = Self::RuntimeOrigin>
			+ GetDispatchInfo;

		/// Type representing the weight of this pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Authenticates the sudo key and dispatches a function call with `Root` origin.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(0)]
		#[pallet::weight({
			let dispatch_info = call.get_dispatch_info();
			(
				T::WeightInfo::sudo().saturating_add(dispatch_info.weight),
				dispatch_info.class
			)
		})]
		pub fn sudo(
			origin: OriginFor<T>,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResultWithPostInfo {
			// This is a public call, so we ensure that the origin is some signed account.
			let sender = ensure_signed(origin)?;
			ensure!(Self::key().map_or(false, |k| sender == k), Error::<T>::RequireSudo);

			let res = call.dispatch_bypass_filter(frame_system::RawOrigin::Root.into());
			Self::deposit_event(Event::Sudid { sudo_result: res.map(|_| ()).map_err(|e| e.error) });
			// Sudo user does not pay a fee.
			Ok(Pays::No.into())
		}

		/// Authenticates the sudo key and dispatches a function call with `Root` origin.
		/// This function does not check the weight of the call, and instead allows the
		/// Sudo user to specify the weight of the call.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(1)]
		#[pallet::weight((*weight, call.get_dispatch_info().class))]
		pub fn sudo_unchecked_weight(
			origin: OriginFor<T>,
			call: Box<<T as Config>::RuntimeCall>,
			weight: Weight,
		) -> DispatchResultWithPostInfo {
			// This is a public call, so we ensure that the origin is some signed account.
			let sender = ensure_signed(origin)?;
			let _ = weight; // We don't check the weight witness since it is a root call.
			ensure!(Self::key().map_or(false, |k| sender == k), Error::<T>::RequireSudo);

			let res = call.dispatch_bypass_filter(frame_system::RawOrigin::Root.into());
			Self::deposit_event(Event::Sudid { sudo_result: res.map(|_| ()).map_err(|e| e.error) });
			// Sudo user does not pay a fee.
			Ok(Pays::No.into())
		}

		/// Authenticates the current sudo key and sets the given AccountId (`new`) as the new sudo
		/// key.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::set_key())]
		pub fn set_key(
			origin: OriginFor<T>,
			new: AccountIdLookupOf<T>,
		) -> DispatchResultWithPostInfo {
			// This is a public call, so we ensure that the origin is some signed account.
			let sender = ensure_signed(origin)?;
			ensure!(Self::key().map_or(false, |k| sender == k), Error::<T>::RequireSudo);
			let new = T::Lookup::lookup(new)?;

			Self::deposit_event(Event::KeyChanged { old_sudoer: Key::<T>::get() });
			Key::<T>::put(&new);
			// Sudo user does not pay a fee.
			Ok(Pays::No.into())
		}

		/// Authenticates the sudo key and dispatches a function call with `Signed` origin from
		/// a given account.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// ## Complexity
		/// - O(1).
		#[pallet::call_index(3)]
		#[pallet::weight({
			let dispatch_info = call.get_dispatch_info();
			(
				T::WeightInfo::sudo_as().saturating_add(dispatch_info.weight),
				dispatch_info.class,
			)
		})]
		pub fn sudo_as(
			origin: OriginFor<T>,
			who: AccountIdLookupOf<T>,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResultWithPostInfo {
			// This is a public call, so we ensure that the origin is some signed account.
			let sender = ensure_signed(origin)?;
			ensure!(Self::key().map_or(false, |k| sender == k), Error::<T>::RequireSudo);

			let who = T::Lookup::lookup(who)?;

			let res = call.dispatch_bypass_filter(frame_system::RawOrigin::Signed(who).into());

			Self::deposit_event(Event::SudoAsDone {
				sudo_result: res.map(|_| ()).map_err(|e| e.error),
			});
			// Sudo user does not pay a fee.
			Ok(Pays::No.into())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A sudo call just took place.
		Sudid {
			/// The result of the call made by the sudo user.
			sudo_result: DispatchResult,
		},
		/// The sudo key has been updated.
		KeyChanged {
			/// The old sudo key if one was previously set.
			old_sudoer: Option<T::AccountId>,
		},
		/// A [sudo_as](Pallet::sudo_as) call just took place.
		SudoAsDone {
			/// The result of the call made by the sudo user.
			sudo_result: DispatchResult,
		},
	}

	#[pallet::error]
	/// Error for the Sudo pallet
	pub enum Error<T> {
		/// Sender must be the Sudo account
		RequireSudo,
	}

	/// The `AccountId` of the sudo key.
	#[pallet::storage]
	#[pallet::getter(fn key)]
	pub(super) type Key<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// The `AccountId` of the sudo key.
		pub key: Option<T::AccountId>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			if let Some(ref key) = self.key {
				Key::<T>::put(key);
			}
		}
	}
}
