// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT-0

// Permission is hereby granted, free of charge, to any person obtaining a copy of
// this software and associated documentation files (the "Software"), to deal in
// the Software without restriction, including without limitation the rights to
// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
// of the Software, and to permit persons to whom the Software is furnished to do
// so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! # Assets Freezer Pallet
//!
//! A pallet capable of freezing fungibles from `pallet-assets`. This is an extension of
//! `pallet-assets`, wrapping [`fungibles::Inspect`](`Inspect`).
//! It implements both
//! [`fungibles::freeze::Inspect`](InspectFreeze) and
//! [`fungibles::freeze::Mutate`](MutateFreeze). The complexity
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
//! - An implementation of [`fungibles::freeze::Inspect`](InspectFreeze) and
//!   [`fungibles::freeze::Mutate`](MutateFreeze), allowing other pallets to manage freezes for the
//!   `pallet-assets` assets.

#![cfg_attr(not(feature = "std"), no_std)]

use frame::{
	prelude::*,
	traits::{
		fungibles::{Inspect, InspectFreeze, MutateFreeze},
		tokens::{
			DepositConsequence, Fortitude, IdAmount, Preservation, Provenance, WithdrawConsequence,
		},
	},
};

pub use pallet::*;

#[cfg(feature = "try-runtime")]
use frame::try_runtime::TryRuntimeError;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

mod impls;

#[frame::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config(with_default)]
	pub trait Config<I: 'static = ()>: frame_system::Config + pallet_assets::Config<I> {
		/// The overarching freeze reason.
		#[pallet::no_default_bounds]
		type RuntimeFreezeReason: Parameter + Member + MaxEncodedLen + Copy + VariantCount;

		/// The overarching event type.
		#[pallet::no_default_bounds]
		#[allow(deprecated)]
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
	pub type Freezes<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
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
	pub type FrozenBalances<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
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
		fn try_state(_: BlockNumberFor<T>) -> Result<(), TryRuntimeError> {
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

	#[cfg(feature = "try-runtime")]
	fn do_try_state() -> Result<(), TryRuntimeError> {
		for (asset, who, _) in FrozenBalances::<T, I>::iter() {
			let max_frozen_amount =
				Freezes::<T, I>::get(asset.clone(), who.clone()).iter().map(|l| l.amount).max();

			ensure!(
				FrozenBalances::<T, I>::get(asset, who) == max_frozen_amount,
				"The `FrozenAmount` is not equal to the maximum amount in `Freezes` for (`asset`, `who`)"
			);
		}

		Ok(())
	}
}
