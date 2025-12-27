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

//! # Assets Holder Pallet
//!
//! A pallet capable of holding fungibles from `pallet-assets`. This is an extension of
//! `pallet-assets`, wrapping [`fungibles::Inspect`](`frame_support::traits::fungibles::Inspect`).
//! It implements both
//! [`fungibles::hold::Inspect`](frame_support::traits::fungibles::hold::Inspect),
//! [`fungibles::hold::Mutate`](frame_support::traits::fungibles::hold::Mutate), and especially
//! [`fungibles::hold::Unbalanced`](frame_support::traits::fungibles::hold::Unbalanced). The
//! complexity of the operations is `O(1)`.
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
//! - Pallet hooks allowing [`pallet-assets`] to know the balance on hold for an account on a given
//!   asset (see [`pallet_assets::BalanceOnHold`]).
//! - An implementation of
//!   [`fungibles::hold::Inspect`](frame_support::traits::fungibles::hold::Inspect),
//!   [`fungibles::hold::Mutate`](frame_support::traits::fungibles::hold::Mutate) and
//!   [`fungibles::hold::Unbalanced`](frame_support::traits::fungibles::hold::Unbalanced), allowing
//!   other pallets to manage holds for the `pallet-assets` assets.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	pallet_prelude::*,
	traits::{tokens::IdAmount, VariantCount, VariantCountOf},
	BoundedVec,
};
use frame_system::pallet_prelude::BlockNumberFor;

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

mod impl_fungibles;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config(with_default)]
	pub trait Config<I: 'static = ()>:
		frame_system::Config + pallet_assets::Config<I, Holder = Pallet<Self, I>>
	{
		/// The overarching freeze reason.
		#[pallet::no_default_bounds]
		type RuntimeHoldReason: Parameter + Member + MaxEncodedLen + Copy + VariantCount;

		/// The overarching event type.
		#[pallet::no_default_bounds]
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// Number of holds on an account would exceed the count of `RuntimeHoldReason`.
		TooManyHolds,
	}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// `who`s balance on hold was increased by `amount`.
		Held {
			who: T::AccountId,
			asset_id: T::AssetId,
			reason: T::RuntimeHoldReason,
			amount: T::Balance,
		},
		/// `who`s balance on hold was decreased by `amount`.
		Released {
			who: T::AccountId,
			asset_id: T::AssetId,
			reason: T::RuntimeHoldReason,
			amount: T::Balance,
		},
		/// `who`s balance on hold was burned by `amount`.
		Burned {
			who: T::AccountId,
			asset_id: T::AssetId,
			reason: T::RuntimeHoldReason,
			amount: T::Balance,
		},
	}

	/// A map that stores holds applied on an account for a given AssetId.
	#[pallet::storage]
	pub(super) type Holds<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AssetId,
		Blake2_128Concat,
		T::AccountId,
		BoundedVec<
			IdAmount<T::RuntimeHoldReason, T::Balance>,
			VariantCountOf<T::RuntimeHoldReason>,
		>,
		ValueQuery,
	>;

	/// A map that stores the current total balance on hold for every account on a given AssetId.
	#[pallet::storage]
	pub(super) type BalancesOnHold<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AssetId,
		Blake2_128Concat,
		T::AccountId,
		T::Balance,
	>;

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		#[cfg(feature = "try-runtime")]
		fn try_state(_: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	#[cfg(any(test, feature = "try-runtime"))]
	fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		use sp_runtime::{
			traits::{CheckedAdd, Zero},
			ArithmeticError,
		};

		for (asset, who, balance_on_hold) in BalancesOnHold::<T, I>::iter() {
			ensure!(balance_on_hold != Zero::zero(), "zero on hold must not be in state");

			let mut amount_from_holds: T::Balance = Zero::zero();
			for l in Holds::<T, I>::get(asset.clone(), who.clone()).iter() {
				ensure!(l.amount != Zero::zero(), "zero amount is invalid");
				amount_from_holds =
					amount_from_holds.checked_add(&l.amount).ok_or(ArithmeticError::Overflow)?;
			}

			frame_support::ensure!(
				balance_on_hold == amount_from_holds,
				"The `BalancesOnHold` amount is not equal to the sum of `Holds` for (`asset`, `who`)"
			);
		}

		Ok(())
	}
}
