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

//! Foreign asset ID mapping pallet.
//!
//! This module provides storage and utilities for mapping foreign asset IDs (e.g.,
//! `xcm::v5::Location`) to u32 indices that can be used to derive precompile addresses.

use core::marker::PhantomData;
use pallet_assets::AssetsCallback;

pub use pallet::*;

/// Trait to convert various types to u32 asset index used for deriving precompile addresses.
pub trait ToAssetIndex {
	fn to_asset_index(&self) -> u32;
}

/// Implemented for trust-backed assets and pool assets.
impl ToAssetIndex for u32 {
	fn to_asset_index(&self) -> u32 {
		*self
	}
}

/// Implemented for trust-backed assets and pool assets with u128 IDs.
impl ToAssetIndex for u128 {
	fn to_asset_index(&self) -> u32 {
		use codec::Encode;
		let h = sp_core::hashing::blake2_256(&self.encode());
		u32::from_le_bytes([h[0], h[1], h[2], h[3]])
	}
}

/// Implemented for foreign assets using XCM locations.
impl ToAssetIndex for xcm::v5::Location {
	fn to_asset_index(&self) -> u32 {
		use codec::Encode;
		let h = sp_core::hashing::blake2_256(&self.encode());
		u32::from_le_bytes([h[0], h[1], h[2], h[3]])
	}
}

/// Insert a bidirectional mapping between asset index and foreign asset ID.
pub fn insert_asset_mapping<T: pallet::Config>(asset_index: u32, asset_id: &T::ForeignAssetId) {
	pallet::AssetIndexToForeignAssetId::<T>::insert(asset_index, asset_id.clone());
	pallet::ForeignAssetIdToAssetIndex::<T>::insert(asset_id, asset_index);
}

/// Remove a bidirectional mapping for the given foreign asset ID.
pub fn remove_asset_mapping<T: pallet::Config>(asset_id: &T::ForeignAssetId) {
	if let Some(asset_index) = pallet::ForeignAssetIdToAssetIndex::<T>::get(&asset_id) {
		pallet::AssetIndexToForeignAssetId::<T>::remove(asset_index);
		pallet::ForeignAssetIdToAssetIndex::<T>::remove(asset_id);
	}
}

/// An [`AssetsCallback`] implementation that maintains the foreign asset ID mapping.
///
/// This callback should be used in the `CallbackHandle` configuration of pallet-assets
/// for foreign assets instances to automatically maintain the mapping when assets are
/// created or destroyed.
pub struct ForeignAssetIdCallback<T, I = ()>(PhantomData<(T, I)>);

impl<T, I: 'static> AssetsCallback<T::AssetId, T::AccountId> for ForeignAssetIdCallback<T, I>
where
	T: pallet_assets::Config<I> + pallet::Config<ForeignAssetId = T::AssetId>,
	T::AssetId: ToAssetIndex,
{
	fn created(id: &T::AssetId, _owner: &T::AccountId) -> Result<(), ()> {
		insert_asset_mapping::<T>(id.to_asset_index(), id);
		Ok(())
	}

	fn destroyed(id: &T::AssetId) -> Result<(), ()> {
		remove_asset_mapping::<T>(id);
		Ok(())
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{pallet_prelude::*, Blake2_128Concat};

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The foreign asset ID type. This must match the `AssetId` type used by the
		/// `pallet_assets` instance for foreign assets.
		type ForeignAssetId: Member
			+ Parameter
			+ Clone
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen
			+ ToAssetIndex;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Mapping an asset index (derived from the precompile address) to a `ForeignAssetId`.
	#[pallet::storage]
	pub type AssetIndexToForeignAssetId<T: Config> =
		StorageMap<_, Blake2_128Concat, u32, T::ForeignAssetId, OptionQuery>;

	/// Mapping a `ForeignAssetId` to an asset index (used for deriving precompile addresses).
	#[pallet::storage]
	pub type ForeignAssetIdToAssetIndex<T: Config> =
		StorageMap<_, Blake2_128Concat, T::ForeignAssetId, u32, OptionQuery>;

	impl<T: Config> Pallet<T> {
		/// Get the foreign asset ID for a given asset index.
		pub fn asset_id_of(asset_index: u32) -> Option<T::ForeignAssetId> {
			AssetIndexToForeignAssetId::<T>::get(asset_index)
		}

		/// Get the asset index for a given foreign asset ID.
		pub fn asset_index_of(asset_id: &T::ForeignAssetId) -> Option<u32> {
			ForeignAssetIdToAssetIndex::<T>::get(asset_id)
		}
	}
}
