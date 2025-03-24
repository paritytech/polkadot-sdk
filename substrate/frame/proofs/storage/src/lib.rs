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

//! # Proofs storage Pallet
//!
//! Generic key-value storage for proofs-related stuff.
//!
//! FAIL-CI - TBD.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame_support::pallet_prelude::Weight;
use frame_support::traits::EnsureOriginWithArg;

pub use pallet::*;
pub mod weights;
pub use weights::WeightInfo;

/// Runtime hook for when a new data are submitted.
pub trait OnNewData<Key, Value> {
	fn on_new_data(key: &Key, value: &Value);
}

#[impl_trait_for_tuples::impl_for_tuples(10)]
impl<Key, Value> OnNewData<Key, Value> for Tuple {
	fn on_new_data(key: &Key, value: &Value) {
		for_tuples!( #( Tuple::on_new_data(key, value); )* );
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// The Weight information for this pallet.
		type WeightInfo: WeightInfo;

		/// The origin that is allowed to mutate data.
		type SubmitOrigin: EnsureOriginWithArg<Self::RuntimeOrigin, Self::Key>;

		/// Generic key.
		type Key: Parameter + MaxEncodedLen;
		/// Generic value.
		type Value: Parameter + MaxEncodedLen;

		/// Callback triggered when new data are submitted.
		type OnNewData: OnNewData<Self::Key, Self::Value>;
	}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::event]
	#[pallet::generate_deposit(pub fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		Submitted { key: T::Key },
	}

	#[pallet::storage]
	pub type Data<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, T::Key, T::Value, OptionQuery>;

	#[pallet::call(weight(<T as Config<I>>::WeightInfo))]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// TODO: FAIL-CI - documentation
		#[pallet::call_index(0)]
		pub fn submit(origin: OriginFor<T>, key: T::Key, value: T::Value) -> DispatchResult {
			let _ = T::SubmitOrigin::ensure_origin(origin, &key);
			T::OnNewData::on_new_data(&key, &value);
			// TODO: FAIL-CI: add timeout/validity to the stored data (with some configurable trait) and remove `on_idle`.
			Data::<T, I>::insert(key.clone(), value);
			Self::deposit_event(Event::Submitted { key });
			Ok(())
		}
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Returns stored data for key.
		pub fn get_data(key: &T::Key) -> Option<T::Value> {
			Data::<T, I>::get(key)
		}
	}
}
