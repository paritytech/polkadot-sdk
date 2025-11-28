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
//! Components:
//!
//! - Oracle: the pallet through which validators submit their price bumps.
//! - Rc-client: pallet that receives XCM messages, indicating new validator sets, from the RC. It
//!   also acts as two components for the local session pallet:
//!   - `ShouldEndSession`: It immediately signals the session pallet that it should end the
//!     previous session. TODO: we might want to still retain a periodic session as well, allowing
//!     validators to swap keys in case of emergency.
//!   - `SessionManager`: Once session realizes it has to rotate the session, it will call into its
//!     `SessionManager`, which is also implemented by rc-client, to which it gives the new
//!     validator keys.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod oracle {
	pub use pallet::*;

	#[frame_support::pallet]
	pub mod pallet {
		extern crate alloc;
		use alloc::vec::Vec;
		use frame_support::{
			dispatch::DispatchResult,
			pallet_prelude::*,
			traits::{EnsureOrigin, OneSessionHandler},
			Parameter,
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
			type AuthorityId: AppCrypto<Self::Public, Self::Signature>
				+ RuntimeAppPublic
				+ Parameter
				+ Member;

			type RelayChainOrigin: EnsureOrigin<Self::RuntimeOrigin>;
		}

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
					Authorities::<T>::put(validators.map(|(_, k)| k).collect::<Vec<_>>());
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
			fn offchain_worker(_block_number: BlockNumberFor<T>) {
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
					// TODO: not efficient to read all to check if person is part of. Need a
					// btreeSet
					log::info!(target: "runtime", "bump_price: who is {:?}", who);
					// TODO
					// Authorities::<T>::get().into_iter().find(|a| a == &who).ok_or(BadOrigin)
					Ok(())
				})?;

				Ok(())
			}

			#[pallet::call_index(1)]
			#[pallet::weight(0)]
			pub fn relay_session_change(
				origin: OriginFor<T>,
				validators: Vec<T::AccountId>,
			) -> DispatchResult {
				T::RelayChainOrigin::ensure_origin_or_root(origin)?;
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
					Self::None => (Self::None, None),
					Self::ToPlan(to_plan) => (Self::Planned, Some(to_plan)),
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
				T::RelayChainOrigin::ensure_origin_or_root(origin)?;
				ValidatorSetStorage::<T>::put(ValidatorSet::ToPlan(validators));
				Ok(())
			}
		}

		impl<T: Config> pallet_session::ShouldEndSession<BlockNumberFor<T>> for Pallet<T> {
			fn should_end_session(now: BlockNumberFor<T>) -> bool {
				ValidatorSetStorage::<T>::get().should_end_session()
			}
		}

		impl<T: Config> pallet_session::SessionManager<T::AccountId> for Pallet<T> {
			fn new_session(new_index: u32) -> Option<Vec<T::AccountId>> {
				let (next, ret) = ValidatorSetStorage::<T>::get().new_session();
				ValidatorSetStorage::<T>::put(next);
				ret
			}
			fn end_session(end_index: u32) {
				// nada
			}
			fn start_session(start_index: u32) {
				// nada
			}
		}
	}
}
