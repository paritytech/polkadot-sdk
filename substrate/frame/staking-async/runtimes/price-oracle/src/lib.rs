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

//! Price-Oracle System
//!
//! Pallets:
//!
//! - Oracle: the pallet through which validators submit their price bumps. This pallet implements a
//!   `OneSessionHandler`, allowing it to receive updated about the local session pallet. This local
//!   session pallet is controlled by the next component (`Rc-client`), and pretty much mimics the
//!   relay chain validators.
//! 	- Of course, relay validators need to use their stash key once in the price-oracle parachain
//!    to:
//! 		- Set a proxy for future use
//! 		- Associate a session key with their stash key.
//! - Rc-client: pallet that receives XCMs indicating new validator sets from the RC. It also acts
//!   as two components for the local session pallet:
//!   - `ShouldEndSession`: It immediately signals the session pallet that it should end the
//!     previous session once it receives the validator set via XCM.
//!   - `SessionManager`: Once session realizes it has to rotate the session, it will call into its
//!     `SessionManager`, which is also implemented by rc-client, to which it gives the new
//!     validator keys.
//!
//! In short, the flow is as follows:
//!
//! 1. block N: `relay_new_validator_set` is received, validators are kept as `ToPlan(v)`.
//! 2. Block N+1: `should_end_session` returns `true`.
//! 3. Block N+1: Session calls its `SessionManager`, `v` is returned in `plan_new_session`
//! 4. Block N+1: `ToPlan(v)` updated to `Planned`.
//! 5. Block N+2: `should_end_session` still returns `true`, forcing tht local session to trigger a
//!    new session again.
//! 6. Block N+2: Session again calls `SessionManager`, nothing is returned in `plan_new_session`,
//!    and session pallet will enact the `v` previously received.
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
//!
//! Notes/TODOs:
//! we might want to still retain a periodic session as well, allowing validators to swap keys in
//! case of emergency.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod oracle {
	pub use pallet::*;

	#[frame_support::pallet]
	pub mod pallet {
		extern crate alloc;
		use alloc::vec::Vec;
		use frame_support::{
			dispatch::DispatchResult, pallet_prelude::*, traits::OneSessionHandler, Parameter,
		};
		use frame_system::{
			offchain::{
				AppCrypto, CreateBare, CreateSignedTransaction, SendSignedTransaction, Signer,
			},
			pallet_prelude::*,
		};
		use sp_runtime::{traits::Member, RuntimeAppPublic};

		#[pallet::config]
		pub trait Config:
			frame_system::Config + CreateSignedTransaction<Call<Self>> + CreateBare<Call<Self>>
		{
			/// The key type for the session key we use to sign [`Call::bump_price`].
			type AuthorityId: AppCrypto<Self::Public, Self::Signature>
				+ RuntimeAppPublic
				+ Parameter
				+ Member;

			/// Every `PriceUpdateInterval` blocks, the offchain worker will submit a price update
			/// transaction.
			type PriceUpdateInterval: Get<BlockNumberFor<Self>>;
		}

		#[pallet::event]
		#[pallet::generate_deposit(pub(super) fn deposit_event)]
		pub enum Event<T: Config> {
			/// A new set of validators was announced.
			NewValidatorsAnnounced { count: u32 },
		}

		/// Current best known authorities.
		#[pallet::storage]
		#[pallet::unbounded] // TODO
		pub type Authorities<T: Config> = StorageValue<_, Vec<T::AuthorityId>, ValueQuery>;

		impl<T: Config> sp_runtime::BoundToRuntimeAppPublic for Pallet<T> {
			type Public = T::AuthorityId;
		}

		impl<T: Config> OneSessionHandler<T::AccountId> for Pallet<T> {
			type Key = T::AuthorityId;

			fn on_genesis_session<'a, I: 'a>(validators: I)
			where
				I: Iterator<Item = (&'a T::AccountId, T::AuthorityId)>,
			{
				let authorities = validators.map(|(_, k)| k).collect::<Vec<_>>();
				Authorities::<T>::put(authorities);
			}

			fn on_new_session<'a, I: 'a>(changed: bool, validators: I, _queued_validators: I)
			where
				I: Iterator<Item = (&'a T::AccountId, T::AuthorityId)>,
			{
				// instant changes
				if changed {
					let authorities = validators.map(|(_, k)| k).collect::<Vec<_>>();
					let count = authorities.len() as u32;
					Authorities::<T>::put(authorities);
					Self::deposit_event(Event::<T>::NewValidatorsAnnounced { count });
				}
			}

			fn on_disabled(_: u32) {
				todo!();
			}
		}

		#[pallet::pallet]
		pub struct Pallet<T>(_);

		#[pallet::hooks]
		impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
			fn offchain_worker(block_number: BlockNumberFor<T>) {
				if block_number % T::PriceUpdateInterval::get() != Zero::zero() {
					return;
				}

				use scale_info::prelude::vec::Vec;
				log::info!(target: "runtime", "Offchain worker starting...");
				let keystore_accounts =
					Signer::<T, T::AuthorityId>::keystore_accounts().collect::<Vec<_>>();
				for account in keystore_accounts.iter() {
					log::info!(target: "runtime", "Account: {:?} / {:?} / {:?}", account.id, account.public, account.index);
				}

				let call = Call::<T>::bump_price { bump: Bump::Up };
				let signer = Signer::<T, T::AuthorityId>::all_accounts();
				if !signer.can_sign() {
					log::error!(target: "runtime", "cannot sign!");
					return;
				}

				let res =
					signer.send_single_signed_transaction(keystore_accounts.first().unwrap(), call);
				log::info!(target: "runtime", "submitted, result is {:?}", res);
			}
		}

		#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo)]
		pub enum Bump {
			Up,
			Down,
		}

		#[pallet::call]
		impl<T: Config> Pallet<T> {
			#[pallet::call_index(0)]
			#[pallet::weight(0)]
			pub fn bump_price(origin: OriginFor<T>, _bump: Bump) -> DispatchResult {
				ensure_signed(origin).and_then(|who| {
					log::info!(target: "runtime", "bump_price: who is {:?}", who);
					// TODO: not efficient to read all to check if person is part of. Need a
					// btreeSet
					Authorities::<T>::get()
						.into_iter()
						.find(|a| a.encode() == who.encode()) // TODO: bit too hacky, can improve
						.ok_or(sp_runtime::traits::BadOrigin)
				})?;

				Ok(())
			}
		}
	}
}

pub mod rc_client {
	pub use pallet::*;

	#[frame_support::pallet]
	pub mod pallet {
		use frame_support::pallet_prelude::*;
		use frame_system::pallet_prelude::{BlockNumberFor, *};
		extern crate alloc;
		use alloc::vec::Vec;

		#[pallet::config]
		pub trait Config: frame_system::Config {
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
			fn should_end_session(&self) -> bool {
				matches!(self, ValidatorSet::ToPlan(_) | ValidatorSet::Planned)
			}

			fn new_session(self) -> (Self, Option<Vec<AccountId>>) {
				match self {
					Self::None => {
						debug_assert!(false, "we should never instruct session to trigger a new session if we have no validator set to plan");
						(Self::None, None)
					},
					// We have something to be planned, return it, and set our next stage to
					// `planned`.
					Self::ToPlan(to_plan) => (Self::Planned, Some(to_plan)),
					// We just planned something, don't plan return anything new to be planned,
					// just let session enact what was previously planned. Set our next stage to
					// `None`.
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
			#[pallet::call_index(0)]
			#[pallet::weight(0)]
			pub fn relay_new_validator_set(
				origin: OriginFor<T>,
				validators: Vec<T::AccountId>,
			) -> DispatchResult {
				log::info!(target: "runtime::price-oracle", "relay_new_validator_set: validators: {:?}", validators);
				T::RelayChainOrigin::ensure_origin_or_root(origin)?;
				ValidatorSetStorage::<T>::put(ValidatorSet::ToPlan(validators));
				Ok(())
			}
		}

		impl<T: Config> pallet_session::ShouldEndSession<BlockNumberFor<T>> for Pallet<T> {
			fn should_end_session(_now: BlockNumberFor<T>) -> bool {
				log::info!(target: "runtime::price-oracle", "should_end_session: {:?}", ValidatorSetStorage::<T>::get().should_end_session());
				ValidatorSetStorage::<T>::get().should_end_session()
			}
		}

		impl<T: Config> pallet_session::SessionManager<T::AccountId> for Pallet<T> {
			fn new_session(new_index: u32) -> Option<Vec<T::AccountId>> {
				log::info!(target: "runtime::price-oracle", "new_session: {:?}", new_index);
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
}
