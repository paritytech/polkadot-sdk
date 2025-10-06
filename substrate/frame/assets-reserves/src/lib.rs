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

//! # Assets Reserves Pallet
//!
//! - [`Config`]
//! - [`Call`]
//!
//! ## Overview
//!
//! The AssetsReserves pallet provides means of configuring reserve locations for `pallet-assets`.
//!
//! The supported dispatchable functions are documented in the [`Call`] enum.
//!
//! ### Terminology
//!
//! * **Asset reserve(s)**: The reserve location(s) of a given asset in the context of cross-chain
//!   reserve-based transfers.
//!
//! ### Goals
//!
//! The assets-reserves system in Substrate is designed to make the following possible:
//!
//! * Providing means to configure and manage cross-chain reserve locations for assets managed by a
//!   local `pallet-assets` instance.
//!
//! * Assets can be transferred across chains using either a reserve-based, or teleport transfer.
//! * A reserve-based transfer implies that a chain acting as a trusted reserve for the transferred
//!   asset has to be somewhere in the transfer path (origin, hop, or destination).
//! * A teleport implies a direct burn/mint mechanism between the origin and destination chains, and
//!   is only allowed if both origin and destination chains are trusted reserves for the teleported
//!   asset.
//!
//! * This pallet facilitates reserve locations configurations, and thus cross-chain transfer
//!   possibilities, for assets managed by a local `pallet-assets` instance.
//!
//! ## Interface
//!
//! ### Permissioned Functions
//!
//! * `todo`: TODO
//!
//! Please refer to the [`Call`] enum and its associated variants for documentation on each
//! function.
//!
//! ### Assumptions
//!
//! * TODO

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{boxed::Box, vec::Vec};
use frame_support::traits::{fungibles::Inspect, EnsureOriginWithArg};

pub use pallet::*;
pub use weights::WeightInfo;
pub mod weights;

pub trait ProvideAssetReserves<A, R> {
	fn reserves(id: &A) -> Vec<R>;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// The maximum number of configurable reserve locations for one asset class.
	const MAX_RESERVES: u32 = 5;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		/// The runtime event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Identifier for the class of asset.
		type AssetId: Parameter + MaybeSerializeDeserialize + MaxEncodedLen;

		/// Identifier for a reserve location for a class of asset.
		type Reserve: Parameter + MaybeSerializeDeserialize + MaxEncodedLen;

		/// Reserve management is only allowed if the origin attempting it and the asset class are
		/// in this set.
		type ManagerOrigin: EnsureOriginWithArg<
			Self::RuntimeOrigin,
			Self::AssetId,
			Success = Self::AccountId,
		>;

		/// The type that provides `fungibles::Inspect` for assets to verify their validity. Usually
		/// `pallet-assets`.
		type AssetInspect: Inspect<Self::AccountId, AssetId = Self::AssetId>;

		/// The Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// Helper type for benchmarks.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: crate::AssetKindFactory<Self::AssetId>;
	}

	/// Maps an asset to a list of its configured reserve locations.
	#[pallet::storage]
	pub type ReserveLocations<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Blake2_128Concat,
		T::AssetId,
		BoundedVec<T::Reserve, ConstU32<MAX_RESERVES>>,
		ValueQuery,
	>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		/// Genesis assets and their reserves
		pub reserves: Vec<(T::AssetId, Vec<T::Reserve>)>,
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> BuildGenesisConfig for GenesisConfig<T, I> {
		fn build(&self) {
			for (id, reserves) in &self.reserves {
				assert!(!ReserveLocations::<T, I>::contains_key(id), "Asset id already in use");
				let reserves =
					BoundedVec::<T::Reserve, ConstU32<MAX_RESERVES>>::try_from(reserves.clone())
						.expect("too many reserves");
				ReserveLocations::<T, I>::insert(id, reserves);
			}
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		// Reserve locations updated for `asset_id`.
		AssetReservesUpdated { asset_id: T::AssetId, reserves: Vec<T::Reserve> },
		// Reserve locations removed for `asset_id`.
		AssetReservesRemoved { asset_id: T::AssetId },
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The given asset ID is unknown.
		UnknownAssetId,
		/// Tried setting too many reserves.
		TooManyReserves,
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Update the reserves for the given asset.
		///
		/// ## Complexity
		/// - O(1)
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::update())]
		pub fn update(
			origin: OriginFor<T>,
			id: Box<T::AssetId>,
			reserves: Vec<T::Reserve>,
		) -> DispatchResult {
			T::ManagerOrigin::ensure_origin(origin, &id)?;
			ensure!(T::AssetInspect::asset_exists(*id.clone()), Error::<T, I>::UnknownAssetId);
			if reserves.is_empty() {
				ReserveLocations::<T, I>::remove(id.as_ref());
				Self::deposit_event(Event::AssetReservesRemoved { asset_id: *id });
			} else {
				let bounded_reserves =
					reserves.clone().try_into().map_err(|_| Error::<T, I>::TooManyReserves)?;
				ReserveLocations::<T, I>::set(id.as_ref(), bounded_reserves);
				Self::deposit_event(Event::AssetReservesUpdated { asset_id: *id, reserves });
			}
			Ok(())
		}

		/// Remove reserves information for destroyed asset classes.
		///
		/// ## Complexity
		/// - O(1)
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::remove())]
		pub fn touch(origin: OriginFor<T>, id: Box<T::AssetId>) -> DispatchResult {
			ensure_signed(origin)?;
			if !T::AssetInspect::asset_exists(*id.clone()) {
				ReserveLocations::<T, I>::remove(id.as_ref());
				Self::deposit_event(Event::AssetReservesRemoved { asset_id: *id });
			}
			Ok(())
		}
	}

	impl<T: Config<I>, I: 'static> ProvideAssetReserves<T::AssetId, T::Reserve> for Pallet<T, I> {
		/// Provide the configured reserves for asset `id`.
		fn reserves(id: &T::AssetId) -> Vec<T::Reserve> {
			ReserveLocations::<T, I>::get(id).into_inner()
		}
	}
}
