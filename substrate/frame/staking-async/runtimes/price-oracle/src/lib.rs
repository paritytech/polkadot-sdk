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

//! Price-Oracle pallet
//!
//! TODO

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	extern crate alloc;
	use alloc::vec::Vec;
	use codec::{Codec, EncodeLike};
	use frame_support::{
		dispatch::DispatchResult,
		pallet_prelude::*,
		traits::{EnsureOrigin, OneSessionHandler},
		Parameter,
	};
	use frame_system::{
		offchain::{AppCrypto, CreateBare, CreateSignedTransaction, SendSignedTransaction, Signer},
		pallet_prelude::*,
	};
	use sp_runtime::{
		traits::{BadOrigin, Member, SaturatedConversion},
		RuntimeAppPublic,
	};

	#[pallet::config]
	pub trait Config:
		frame_system::Config + CreateSignedTransaction<Call<Self>> + CreateBare<Call<Self>>
	{
		type AuthorityId: AppCrypto<Self::Public, Self::Signature>
			+ RuntimeAppPublic
			+ Parameter
			+ Member;
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
				// TODO: not efficient to read all to check if person is part of. Need a btreeSet
				log::info!(target: "runtime", "bump_price: who is {:?}", who);
				// TODO
				// Authorities::<T>::get().into_iter().find(|a| a == &who).ok_or(BadOrigin)
				Ok(())
			})?;

			Ok(())
		}
	}
}
