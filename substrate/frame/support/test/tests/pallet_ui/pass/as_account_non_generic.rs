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

// Verify a non-generic enum origin with `#[pallet::as_account]` compiles.
// The closure accesses `Self` (= `Pallet<T>`) to convert concrete fields to AccountId.

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(core::marker::PhantomData<T>);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::storage]
	pub type CouncilAccounts<T: Config> = StorageMap<_, Twox64Concat, u32, T::AccountId>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn account_for_council(id: &u32) -> Option<T::AccountId> {
			CouncilAccounts::<T>::get(id)
		}
	}

	#[pallet::origin]
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	pub enum Origin {
		#[pallet::as_account(|id| Self::account_for_council(id))]
		Council(u32),
		Admin,
	}
}

fn main() {}
