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

//! # Price Oracle (rc) Client.
//!
//! This pallet acts as two components for the local session pallet:
//!
//! * `ShouldEndSession`: It immediately signals the session pallet that it should end the previous
//!   session once it receives the validator set via XCM.
//! * `SessionManager`: Once session realizes it has to rotate the session, it will call into its
//!   `SessionManager`, which is also implemented by this pallet, to which it gives the new
//!   validator keys.
//!
//! In short, the flow is as follows:
//!
//! 1. block N: `relay_new_validator_set` is received, validators are kept as `ToPlan(v)`.
//! 2. Block N+1: `should_end_session` in this pallet returns `true` to `pallet-session`.
//! 3. Block N+1: `pallet-session` calls its `SessionManager`, `v` is returned in `new_session`
//! 4. Block N+1: `ToPlan(v)` updated to `Planned`.
//! 5. Block N+2: `should_end_session` still returns `true`, forcing the the local `pallet-session`
//!    to trigger a new session again, activating the new validators set.
//! 6. Block N+2: `pallet-session`again calls `SessionManager`, nothing is returned in
//!    `new_session`, and session pallet will enact the `v` previously received.
//!
//! This design hinges on the fact that the session pallet always does 3 calls at the same time when
//! interacting with the `SessionManager`:
//!
//! * `end_session(n)`
//! * `start_session(n+1)`
//! * `new_session(n+2)`
//!
//! Every time `new_session` receives some validator set as return value, it is only enacted on the
//! next session rotation.

#[cfg(test)]
pub mod mock;
#[cfg(test)]
pub mod test;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::{BlockNumberFor, *};
	extern crate alloc;
	use alloc::vec::Vec;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The origin representing the relay chain.
		type RelayChainOrigin: EnsureOrigin<Self::RuntimeOrigin>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[derive(
		Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, Default,
	)]
	pub enum ValidatorSet<AccountId> {
		/// We don't have a validator set yet.
		#[default]
		None,
		/// We have a validator set, but we have not given it to the session pallet to be
		/// planned yet.
		ToPlan(Vec<AccountId>),
		/// A validator set was just given to the session pallet to be planned.
		///
		/// We should immediately signal the session pallet to trigger a new session, and
		/// activate it.
		Planned,
	}

	impl<AccountId> ValidatorSet<AccountId> {
		/// Should we signal the local session pallet to end the session or not.
		fn should_end_session(&self) -> bool {
			matches!(self, ValidatorSet::ToPlan(_) | ValidatorSet::Planned)
		}

		/// What should we return to the session pallet as our role of `SessionManager`.
		///
		/// Returns `(our_next_state, return_value_to_session)`
		fn new_session(self) -> (Self, Option<Vec<AccountId>>) {
			match self {
				Self::None => {
					debug_assert!(false, "we should never instruct session to trigger a new session if we have no validator set to plan");
					(Self::None, None)
				},
				// We have something `ToPlan`, return it, and set our next stage to `Planned`.
				Self::ToPlan(to_plan) => (Self::Planned, Some(to_plan)),
				// We just planned something, don't return anything new to session, just let session
				// enact what was previously planned. Set our next stage to `None`.
				Self::Planned => (Self::None, None),
			}
		}
	}

	#[pallet::storage]
	#[pallet::unbounded]
	pub type ValidatorSetStorage<T: Config> =
		StorageValue<_, ValidatorSet<T::AccountId>, ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Activate the given `validators` in the local session pallet.
		///
		/// Dispatch origin of this call must be [`Config::RelayChainOrigin`].
		#[pallet::call_index(0)]
		#[pallet::weight({T::DbWeight::get().reads_writes(1, 1)})]
		pub fn relay_new_validator_set(
			origin: OriginFor<T>,
			validators: Vec<T::AccountId>,
		) -> DispatchResult {
			client_log!(
				debug,
				"[client] relay_new_validator_set: validators: {:?}",
				validators.len()
			);
			T::RelayChainOrigin::ensure_origin_or_root(origin)?;
			ValidatorSetStorage::<T>::put(ValidatorSet::ToPlan(validators));
			Ok(())
		}
	}

	impl<T: Config> pallet_session::ShouldEndSession<BlockNumberFor<T>> for Pallet<T> {
		fn should_end_session(_now: BlockNumberFor<T>) -> bool {
			ValidatorSetStorage::<T>::get().should_end_session()
		}
	}

	impl<T: Config> pallet_session::SessionManager<T::AccountId> for Pallet<T> {
		fn new_session(_: u32) -> Option<Vec<T::AccountId>> {
			let (next, ret) = ValidatorSetStorage::<T>::get().new_session();
			ValidatorSetStorage::<T>::put(next);
			ret
		}
		fn end_session(_end_index: u32) {
			// nada
		}
		fn start_session(_start_index: u32) {
			// nada
		}
	}
}
