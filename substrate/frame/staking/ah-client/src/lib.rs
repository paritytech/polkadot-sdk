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

//! TODO

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

/// The balance type of this pallet.
pub type BalanceOf<T> = <T as Config>::CurrencyBalance;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use crate::BalanceOf;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use pallet_session::historical;
	use sp_staking::{Exposure, SessionIndex};

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);
	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	// todo:
	// Storage item for new_validator_set:
	// `Option<Vec<(T::AccountId, Exposure<T::AccountId, BalanceOf<T>>)>>`

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Just the `Currency::Balance` type; we have this item to allow us to constrain it to
		/// `From<u64>`.
		type CurrencyBalance: sp_runtime::traits::AtLeast32BitUnsigned
			+ codec::FullCodec
			+ Copy
			+ MaybeSerializeDeserialize
			+ core::fmt::Debug
			+ Default
			+ From<u64>
			+ TypeInfo
			+ Send
			+ Sync
			+ MaxEncodedLen;
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		// #[pallet::weight(T::WeightInfo::new_validators())] // TODO
		pub fn new_validators(
			origin: OriginFor<T>,
			new_validator_set: Vec<(T::AccountId, Exposure<T::AccountId, BalanceOf<T>>)>,
		) -> DispatchResult {
			// TODO: origin?

			// TODO: save validators in `new_validator_set` storage item

			Ok(())
		}
	}

	impl<T: Config> historical::SessionManager<T::AccountId, Exposure<T::AccountId, BalanceOf<T>>>
		for Pallet<T>
	{
		fn new_session(
			new_index: sp_staking::SessionIndex,
		) -> Option<Vec<(T::AccountId, Exposure<T::AccountId, BalanceOf<T>>)>> {
			// todo: `take()` what's in `new_validator_set` and return it
			None
		}

		fn new_session_genesis(
			new_index: SessionIndex,
		) -> Option<Vec<(T::AccountId, Exposure<T::AccountId, BalanceOf<T>>)>> {
			// todo: `take()` what's in `new_validator_set` and return it
			// todo: Make sure that pallet_session handles this correctly
			None
		}

		fn start_session(start_index: SessionIndex) {
			<Self as pallet_session::SessionManager<_>>::start_session(start_index)
		}

		fn end_session(end_index: SessionIndex) {
			<Self as pallet_session::SessionManager<_>>::end_session(end_index)
		}
	}

	impl<T: Config> pallet_session::SessionManager<T::AccountId> for Pallet<T> {
		fn new_session(_: u32) -> std::option::Option<Vec<<T as frame_system::Config>::AccountId>> {
			// Doesn't do anything because all the logic is handled in `historical::SessionManager`
			// implementation
			defensive!("new_session should not be called");
			None
		}

		fn end_session(_: u32) {
			// call end_session from rc-client and pass era points info
			todo!()
		}

		fn start_session(_: u32) {
			// call start_session from rc-client and pass active valdiator set somehow(tm)
			todo!()
		}
	}
}
