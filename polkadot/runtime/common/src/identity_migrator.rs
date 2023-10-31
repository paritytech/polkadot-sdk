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

use frame_support::{dispatch::DispatchResult, traits::Currency};
pub use pallet::*;
use pallet_identity::{self, WeightInfo};
use sp_core::Get;

type BalanceOf<T> = <<T as pallet_identity::Config>::Currency as Currency<
	<T as frame_system::Config>::AccountId,
>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		dispatch::{DispatchResultWithPostInfo, PostDispatchInfo},
		pallet_prelude::*,
		traits::EnsureOrigin,
	};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_identity::Config {
		/// Overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The origin that can reap identities. Expected to be `EnsureSigned<AccountId>` on the
		/// source chain such that anyone can all this function.
		type Reaper: EnsureOrigin<Self::RuntimeOrigin>;

		/// A handler for what to do when an identity is reaped.
		type ReapIdentityHandler: OnReapIdentity<Self::AccountId>;

		/// Weight information for the extrinsics in the pallet.
		type WeightInfo: pallet_identity::WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The identity and all sub accounts were reaped for `who`.
		IdentityReaped { who: T::AccountId },
		/// The deposits held for `who` were updated. `identity` is the new deposit held for
		/// identity info, and `subs` is the new deposit held for the sub-accounts.
		DepositUpdated { who: T::AccountId, identity: BalanceOf<T>, subs: BalanceOf<T> },
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
			let (registrars, fields, subs) = pallet_identity::Pallet::<T>::reap_identity(&who)?;
			T::ReapIdentityHandler::on_reap_identity(&who, fields, subs)?;
			Self::deposit_event(Event::IdentityReaped { who });
			let post = PostDispatchInfo {
				actual_weight: Some(<T as pallet::Config>::WeightInfo::reap_identity(
					registrars, subs,
				)),
				pays_fee: Pays::No,
			};
			Ok(post)
		}

		/// Update the deposit of `who`. Meant to be called by the system with an XCM `Transact`
		/// Instruction.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::poke_deposit())]
		pub fn poke_deposit(origin: OriginFor<T>, who: T::AccountId) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			let (id_deposit, subs_deposit) = pallet_identity::Pallet::<T>::poke_deposit(&who)?;
			Self::deposit_event(Event::DepositUpdated {
				who,
				identity: id_deposit,
				subs: subs_deposit,
			});
			Ok(Pays::No.into())
		}
	}
}

/// Trait to handle reaping identity from state.
pub trait OnReapIdentity<AccountId> {
	/// What to do when an identity is reaped. For example, the implementation could send an XCM
	/// program to another chain. Concretely, a type implementing this trait in the Polkadot
	/// runtime would teleport enough DOT to the People Chain to cover the Identity deposit there.
	///
	/// This could also directly include `Transact { poke_deposit(..), ..}`.
	///
	/// Inputs
	/// - `who`: Whose identity was reaped.
	/// - `fields`: The number of `additional_fields` they had.
	/// - `subs`: The number of sub-accounts they had.
	fn on_reap_identity(who: &AccountId, fields: u32, subs: u32) -> DispatchResult;
}

impl<AccountId> OnReapIdentity<AccountId> for () {
	fn on_reap_identity(_who: &AccountId, _fields: u32, _subs: u32) -> DispatchResult {
		Ok(())
	}
}
