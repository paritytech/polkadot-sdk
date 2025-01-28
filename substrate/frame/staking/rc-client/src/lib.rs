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

pub trait ElectionResultHandler<ValidatorId> {
	fn handle_election_result(result: Vec<ValidatorId>);
}

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_staking::{Exposure, SessionIndex};

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);
	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	impl<T, ValidatorId> ElectionResultHandler<ValidatorId> for Pallet<T> {
		fn handle_election_result(result: Vec<ValidatorId>) {
			//send `new_validators` XCM to session/ah_client
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		// #[pallet::weight(T::WeightInfo::end_session())] // TODO
		pub fn end_session(origin: OriginFor<T>, end_index: SessionIndex) -> DispatchResult {
			// call staking pallet
			todo!()
		}

		#[pallet::call_index(1)]
		// #[pallet::weight(T::WeightInfo::end_session())] // TODO
		pub fn start_session(origin: OriginFor<T>, start_index: SessionIndex) -> DispatchResult {
			// call start_session from rc-client
			todo!()
		}
	}
}
