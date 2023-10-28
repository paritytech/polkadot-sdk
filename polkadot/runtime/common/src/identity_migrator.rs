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

//! This pallet is designed to go into a source chain and destination chain to migrate data. The
//! design motivations are:
//!
//! - Call some function on the source chain that executes some migration (clearing state,
//!   forwarding an XCM program).
//! - Call some function (probably from an XCM program) on the destination chain.
//! - Avoid cluttering the source pallet with new dispatchables that are unrelated to its
//!   functionality and only used for migration.
//!
//! After the migration is complete, the pallet may be removed from both chains' runtimes.

pub use pallet::*;
use pallet_identity::{self, WeightInfo};
use sp_core::Get;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		dispatch::DispatchResultWithPostInfo, pallet_prelude::Pays, traits::EnsureOrigin,
	};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_identity::Config {
		/// The origin that can reap identities. Expected to be `EnsureSigned<AccountId>` on the
		/// source chain such that anyone can all this function.
		type Reaper: EnsureOrigin<Self::RuntimeOrigin>;

		/// Weight information for the extrinsics in the pallet.
		type WeightInfo: pallet_identity::WeightInfo;
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Reap the Identity Info of `who` from the Relay Chain, unreserving any deposits held and
		/// removing storage items associated with `who`.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::reap_identity(
				T::MaxRegistrars::get(),
				T::MaxSubAccounts::get()
		))]
		pub fn reap_identity(
			origin: OriginFor<T>,
			who: T::AccountId,
		) -> DispatchResultWithPostInfo {
			T::Reaper::ensure_origin(origin)?;
			pallet_identity::Pallet::<T>::reap_identity(&who)
		}

		/// Update the deposit of `who`. Meant to be called by the system with an XCM `Transact`
		/// Instruction.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::poke_deposit())]
		pub fn poke_deposit(origin: OriginFor<T>, who: T::AccountId) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			pallet_identity::Pallet::<T>::poke_deposit(&who)?;
			Ok(Pays::No.into())
		}
	}
}
