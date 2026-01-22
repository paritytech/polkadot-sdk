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

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

use core::marker::PhantomData;
use frame_support::LOG_TARGET;
use pallet_assets::AssetsCallback;

pub use pallet::*;

pub struct ForeignAssetId<T, I = ()>(PhantomData<(T, I)>);
impl<T: Config, I> AssetsCallback<T::AssetId, T::AccountId> for ForeignAssetId<T, I>
where
	T::AssetId: ToAssetIndex,
	T: pallet::Config<ForeignAssetId = T::AssetId> + pallet_assets::Config<I>,
	I: 'static,
{
	fn created(id: &T::AssetId, _: &T::AccountId) -> Result<(), ()> {
		pallet::Pallet::<T>::insert_asset_mapping(id.to_asset_index(), id)
	}

	fn destroyed(id: &T::AssetId) -> Result<(), ()> {
		pallet::Pallet::<T>::remove_asset_mapping(id);
		Ok(())
	}
}

/// Trait to convert various types to u32 asset index used internally by the pallet.
pub trait ToAssetIndex {
	fn to_asset_index(&self) -> u32;
}

/// Implemented for trust-backed assets and pool assets.
impl ToAssetIndex for u32 {
	fn to_asset_index(&self) -> u32 {
		*self
	}
}

/// Implemented for trust-backed assets and pool assets.
impl ToAssetIndex for u128 {
	fn to_asset_index(&self) -> u32 {
		use codec::Encode;
		let h = sp_core::hashing::blake2_256(&self.encode());
		u32::from_le_bytes([h[0], h[1], h[2], h[3]])
	}
}

/// Implemented for foreign assets.
impl ToAssetIndex for xcm::v5::Location {
	fn to_asset_index(&self) -> u32 {
		use codec::Encode;
		let h = sp_core::hashing::blake2_256(&self.encode());
		u32::from_le_bytes([h[0], h[1], h[2], h[3]])
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

		pub fn insert_asset_mapping(
			asset_index: u32,
			asset_id: &T::ForeignAssetId,
		) -> Result<(), ()> {
			if AssetIndexToForeignAssetId::<T>::contains_key(asset_index) {
				log::debug!(target: LOG_TARGET, "Asset index {:?} already mapped", asset_index);
				return Err(());
			}
			if ForeignAssetIdToAssetIndex::<T>::contains_key(asset_id) {
				log::debug!(target: LOG_TARGET, "Asset id {:?} already mapped", asset_id);
				return Err(());
			}
			AssetIndexToForeignAssetId::<T>::insert(asset_index, asset_id.clone());
			ForeignAssetIdToAssetIndex::<T>::insert(asset_id, asset_index);
			Ok(())
		}

		pub fn remove_asset_mapping(asset_id: &T::ForeignAssetId) {
			if let Some(asset_index) = ForeignAssetIdToAssetIndex::<T>::get(&asset_id) {
				AssetIndexToForeignAssetId::<T>::remove(asset_index);
				ForeignAssetIdToAssetIndex::<T>::remove(asset_id);
			}
		}
	}
}
