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

//! # Assets Freezer Pallet
//!
//! A pallet capable of freezing fungibles from `pallet-assets`.
//!
//! > Made with *Substrate*, for *Polkadot*.
//!
//! [![github]](https://github.com/paritytech/polkadot-sdk/tree/master/substrate/frame/examples/basic)
//! [![polkadot]](https://polkadot.network)
//!
//! [polkadot]: https://img.shields.io/badge/polkadot-E6007A?style=for-the-badge&logo=polkadot&logoColor=white
//! [github]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Overview
//!
//! This pallet provides the following functionality:
//!
//! - Pallet hooks that implement custom logic to let `pallet-assets` know whether an balance is
//!   frozen for an account on a given asset (see: [`pallet_assets::types::FrozenBalance`]).
//! - An implementation of fungibles [freezer mutation API]
//!   [`frame_supoprt:traits::tokens::fungibles::MutateFreeze`].
//! - Support for force freezing and thawing assets, given a Freezer ID
//!   (see [`pallet_assets_freezer::Config::FreezerId`]).

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use core::marker::PhantomData;
use frame_support::{
	dispatch::{ClassifyDispatch, DispatchClass, DispatchResult, Pays, PaysFee, WeighData},
	traits::IsSubType,
	weights::Weight,
};
use frame_system::ensure_signed;
use log::info;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{Bounded, DispatchInfoOf, SaturatedConversion, Saturating, SignedExtension},
	transaction_validity::{
		InvalidTransaction, TransactionValidity, TransactionValidityError, ValidTransaction,
	},
};

// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;

#[cfg(test)]
mod tests;

type AssetIdOf<T, I> = <T as pallet_assets::Config<I>>::AssetId;
type AssetBalanceOf<T, I> = <T as pallet_assets::Config<I>>::Balance;
type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{pallet_prelude::*, storage::bounded_vec::BoundedVec, Blake2_128Concat};
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config<I: 'static = ()>:
		pallet_balances::Config + frame_system::Config + pallet_assets::Config<I>
	{
		/// The overarching freeze reason.
		type RuntimeFreezeReason: VariantCount;

		/// The overarching origin to allow freezing/thawing calls
		type FreezeOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Self::AccountId>;

		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		#[pallet::constant]
		type MaxFreezes: Get<u32>;
	}

	// Simple declaration of the `Pallet` type. It is placeholder we use to implement traits and
	// method.
	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::call(weight(<T as Config>::WeightInfo))]
	impl<T: Config<I>, I: 'static = ()> Pallet<T, I> {
		// .
		#[pallet::call_index(0)]
		pub fn freeze(origin: OriginFor<T>, increase_by: T::Balance) -> DispatchResult {
			let _ = T::FreezeOrigin::ensure_origin(origin)?;

			todo!("Implement call");

			Ok(())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		// A reducible balance has been increased due to a freeze action.
		AssetBalanceFrozen {
			who: AccountIdOf<T>,
			asset_id: AssetIdOf<T>,
			balance: AssetBalanceOf<T>,
		},
		// A reducible balance has been reduced due to a thaw action.
		AssetBalanceThawed {
			who: AccountIdOf<T>,
			asset_id: AssetIdOf<T>,
			balance: AssetBalanceOf<T>,
		},
	}

	/// A map that stores all the current freezes applied on an account for a given AssetId.
	#[pallet::storage]
	pub(super) type Freezes<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		AccountIdOf<T>,
		Blake2_128Concat,
		AssetIdOf<T, I>,
		BoundedVec<AssetBalanceOf<T, I>, T::MaxFreezes>,
	>;

	/// A map that stores the current reducible balance for every account on a given AssetId.
	#[pallet::storage]
	pub(super) type FrozenBalances<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		AccountIdOf<T>,
		Blake2_128Concat,
		AssetIdOf<T, I>,
		AssetBalanceOf<T, I>,
	>;
}

// TODO: implement [`pallet_asset::types::FrozenBalances`] and [`frame_support::traits::tokens::fungibles::freezes::Inspect`] for `Pallet<T, I>`
