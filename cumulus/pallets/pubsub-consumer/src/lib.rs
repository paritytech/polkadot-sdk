// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

#![cfg_attr(not(feature = "std"), no_std)]

//! Test consumer for pubsub subscriptions.

extern crate alloc;

use alloc::vec::Vec;
use cumulus_primitives_core::ParaId;
use frame_support::{pallet_prelude::*, BoundedVec};
use frame_system::pallet_prelude::*;

pub use pallet::*;

pub struct TestSubscriptionHandler<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> cumulus_pallet_subscriber::SubscriptionHandler for TestSubscriptionHandler<T> {
	fn subscriptions() -> Vec<(ParaId, Vec<Vec<u8>>)> {
		alloc::vec![(ParaId::from(1000), alloc::vec![alloc::vec![0x12, 0x34]])]
	}

	fn on_data_updated(publisher: ParaId, key: Vec<u8>, value: Vec<u8>) {
		let bounded_key: BoundedVec<u8, ConstU32<256>> =
			key.clone().try_into().unwrap_or_default();
		let bounded_value: BoundedVec<u8, ConstU32<1024>> =
			value.clone().try_into().unwrap_or_default();

		<ReceivedData<T>>::insert(&publisher, &bounded_key, &bounded_value);

		Pallet::<T>::deposit_event(Event::DataReceived {
			publisher,
			key: bounded_key,
			value: bounded_value,
		});
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {}

	#[pallet::storage]
	pub type ReceivedData<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ParaId,
		Blake2_128Concat,
		BoundedVec<u8, ConstU32<256>>,
		BoundedVec<u8, ConstU32<1024>>,
		OptionQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		DataReceived {
			publisher: ParaId,
			key: BoundedVec<u8, ConstU32<256>>,
			value: BoundedVec<u8, ConstU32<1024>>,
		},
	}

	impl<T: Config> Pallet<T> {
		pub fn get_data(publisher: ParaId, key: &[u8]) -> Option<Vec<u8>> {
			let bounded_key: BoundedVec<u8, ConstU32<256>> = key.to_vec().try_into().ok()?;
			ReceivedData::<T>::get(publisher, bounded_key).map(|v| v.into_inner())
		}
	}
}
