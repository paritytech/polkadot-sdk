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
//! A pallet capable of freezing fungibles from `pallet-assets`. This is an extension of
//! `pallet-assets`, wrapping [`fungibles::Inspect`](`frame_support::traits::fungibles::Inspect`).
//! It implements both
//! [`fungibles::freeze::Inspect`](frame_support::traits::fungibles::freeze::Inspect) and
//! [`fungibles::freeze::Mutate`](frame_support::traits::fungibles::freeze::Mutate). The complexity
//! of the operations is `O(n)`. where `n` is the variant count of `RuntimeFreezeReason`.
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
//! - Pallet hooks allowing [`pallet-assets`] to know the frozen balance for an account on a given
//!   asset (see [`pallet_assets::FrozenBalance`]).
//! - An implementation of
//!   [`fungibles::freeze::Inspect`](frame_support::traits::fungibles::freeze::Inspect) and
//!   [`fungibles::freeze::Mutate`](frame_support::traits::fungibles::freeze::Mutate), allowing
//!   other pallets to manage freezes for the `pallet-assets` assets.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	pallet_prelude::*,
	traits::{tokens::IdAmount, VariantCount, VariantCountOf},
	BoundedVec,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::{
	traits::{Saturating, Zero},
	BoundedSlice,
};

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

mod impls;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config(with_default)]
	pub trait Config<I: 'static = ()>: frame_system::Config + pallet_assets::Config<I> {
		/// The overarching freeze reason.
		#[pallet::no_default_bounds]
		type RuntimeFreezeReason: Parameter + Member + MaxEncodedLen + Copy + VariantCount;

		/// The overarching event type.
		#[pallet::no_default_bounds]
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// Number of freezes on an account would exceed `MaxFreezes`.
		TooManyFreezes,
	}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		// `who`s frozen balance was increased by `amount`.
		Frozen { who: T::AccountId, asset_id: T::AssetId, amount: T::Balance },
		// `who`s frozen balance was decreased by `amount`.
		Thawed { who: T::AccountId, asset_id: T::AssetId, amount: T::Balance },
	}

	/// A map that stores freezes applied on an account for a given AssetId.
	#[pallet::storage]
	pub(super) type Freezes<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AssetId,
		Blake2_128Concat,
		T::AccountId,
		BoundedVec<
			IdAmount<T::RuntimeFreezeReason, T::Balance>,
			VariantCountOf<T::RuntimeFreezeReason>,
		>,
		ValueQuery,
	>;

	/// A map that stores the current total frozen balance for every account on a given AssetId.
	#[pallet::storage]
	pub(super) type FrozenBalances<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
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
	fn update_freezes(
		asset: T::AssetId,
		who: &T::AccountId,
		freezes: BoundedSlice<
			IdAmount<T::RuntimeFreezeReason, T::Balance>,
			VariantCountOf<T::RuntimeFreezeReason>,
		>,
	) -> DispatchResult {
		let prev_frozen = FrozenBalances::<T, I>::get(asset.clone(), who).unwrap_or_default();
		let after_frozen = freezes.into_iter().map(|f| f.amount).max().unwrap_or_else(Zero::zero);
		FrozenBalances::<T, I>::set(asset.clone(), who, Some(after_frozen));
		if freezes.is_empty() {
			Freezes::<T, I>::remove(asset.clone(), who);
			FrozenBalances::<T, I>::remove(asset.clone(), who);
		} else {
			Freezes::<T, I>::insert(asset.clone(), who, freezes);
		}
		if prev_frozen > after_frozen {
			let amount = prev_frozen.saturating_sub(after_frozen);
			Self::deposit_event(Event::Thawed { asset_id: asset, who: who.clone(), amount });
		} else if after_frozen > prev_frozen {
			let amount = after_frozen.saturating_sub(prev_frozen);
			Self::deposit_event(Event::Frozen { asset_id: asset, who: who.clone(), amount });
		}
		Ok(())
	}

	#[cfg(any(test, feature = "try-runtime"))]
	fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		for (asset, who, _) in FrozenBalances::<T, I>::iter() {
			let max_frozen_amount =
				Freezes::<T, I>::get(asset.clone(), who.clone()).iter().map(|l| l.amount).max();

			frame_support::ensure!(
				FrozenBalances::<T, I>::get(asset, who) == max_frozen_amount,
				"The `FrozenAmount` is not equal to the maximum amount in `Freezes` for (`asset`, `who`)"
			);
		}

		Ok(())
	}
}
