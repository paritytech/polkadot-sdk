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

//! DDay custom origins.

#[frame_support::pallet]
pub mod pallet_origins {
	use frame_support::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[derive(
		PartialEq,
		Eq,
		Clone,
		MaxEncodedLen,
		Encode,
		Decode,
		DecodeWithMemTracking,
		TypeInfo,
		RuntimeDebug,
	)]
	#[pallet::origin]
	pub enum Origin {
		/// Origin aggregated through weighted votes of AssetHub accounts.
		/// Aka the "voice" of AssetHub accounts with balance.
		AssetHubAccounts,
	}

	/// Ensures that `RuntimeOrigin
	pub struct EnsureDDayOrigin;
	impl<O: OriginTrait + From<Origin>> EnsureOrigin<O> for EnsureDDayOrigin
	where
		for<'a> &'a O::PalletsOrigin: TryInto<&'a Origin>,
	{
		type Success = ();
		fn try_origin(o: O) -> Result<Self::Success, O> {
			match o.caller().try_into() {
				Ok(Origin::AssetHubAccounts) => return Ok(()),
				_ => (),
			}

			Err(o)
		}
		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin() -> Result<O, ()> {
			Ok(O::from(Origin::AssetHubAccounts))
		}
	}
}
