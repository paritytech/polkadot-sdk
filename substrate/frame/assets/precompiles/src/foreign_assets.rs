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

use core::marker::PhantomData;
use frame_support::LOG_TARGET;
use pallet_assets::AssetsCallback;

pub use pallet::*;

pub struct ForeignAssetId<T, I = ()>(PhantomData<(T, I)>);
impl<T: Config, I> AssetsCallback<T::AssetId, T::AccountId> for ForeignAssetId<T, I>
where
	T: Config<ForeignAssetId = T::AssetId> + pallet_assets::Config<I>,
	I: 'static,
{
	fn created(id: &T::AssetId, _: &T::AccountId) -> Result<(), ()> {
		Pallet::<T>::insert_asset_mapping(id).map(|_| ())
	}

	fn destroyed(id: &T::AssetId) -> Result<(), ()> {
		Pallet::<T>::remove_asset_mapping(id);
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
		type ForeignAssetId: Member + Parameter + Clone + MaybeSerializeDeserialize + MaxEncodedLen;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// The next available asset index for foreign assets.
	/// This is incremented each time a new foreign asset mapping is created.
	#[pallet::storage]
	pub type NextAssetIndex<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Mapping an asset index (derived from the precompile address) to a `ForeignAssetId`.
	#[pallet::storage]
	pub type AssetIndexToForeignAssetId<T: Config> =
		StorageMap<_, Identity, u32, T::ForeignAssetId, OptionQuery>;

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

		/// Get the next available asset index without incrementing it.
		pub fn next_asset_index() -> u32 {
			NextAssetIndex::<T>::get()
		}

		/// Insert a new asset mapping, allocating a sequential index.
		/// Returns the allocated asset index on success.
		pub fn insert_asset_mapping(asset_id: &T::ForeignAssetId) -> Result<u32, ()> {
			if ForeignAssetIdToAssetIndex::<T>::contains_key(asset_id) {
				log::error!(target: LOG_TARGET, "Asset id {:?} already mapped", asset_id);
				return Err(());
			}

			let asset_index = NextAssetIndex::<T>::get();
			let next_index = asset_index.checked_add(1).ok_or_else(|| {
				log::error!(target: LOG_TARGET, "Asset index overflow");
				()
			})?;

			AssetIndexToForeignAssetId::<T>::insert(asset_index, asset_id.clone());
			ForeignAssetIdToAssetIndex::<T>::insert(asset_id, asset_index);
			NextAssetIndex::<T>::put(next_index);

			log::debug!(target: LOG_TARGET, "Mapped asset {:?} to index {:?}", asset_id, asset_index);
			Ok(asset_index)
		}

		pub fn remove_asset_mapping(asset_id: &T::ForeignAssetId) {
			if let Some(asset_index) = ForeignAssetIdToAssetIndex::<T>::get(&asset_id) {
				AssetIndexToForeignAssetId::<T>::remove(asset_index);
				ForeignAssetIdToAssetIndex::<T>::remove(asset_id);
			}
		}
	}
}
