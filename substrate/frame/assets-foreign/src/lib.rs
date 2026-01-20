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

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

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

pub fn insert_asset_mapping<T: crate::pallet::Config>(
	asset_index: u32,
	asset_id: &T::ForeignAssetId,
) {
	crate::pallet::AssetIndexToForeignAssetId::<T>::insert(asset_index, asset_id.clone());
	crate::pallet::ForeignAssetIdToAssetIndex::<T>::insert(asset_id, asset_index);
}

pub fn remove_asset_mapping<T: crate::pallet::Config>(asset_id: &T::ForeignAssetId) {
	if let Some(asset_index) = crate::pallet::ForeignAssetIdToAssetIndex::<T>::get(&asset_id) {
		crate::pallet::AssetIndexToForeignAssetId::<T>::remove(asset_index);
		crate::pallet::ForeignAssetIdToAssetIndex::<T>::remove(asset_id);
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{pallet_prelude::*, Blake2_128Concat};

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type ForeignAssetId: Member
			+ Parameter
			+ Clone
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen
			+ ToAssetIndex;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Mapping an asset index (which is used internally by the pallet) to an `ForeignAssetId`.
	#[pallet::storage]
	pub type AssetIndexToForeignAssetId<T: Config> =
		StorageMap<_, Blake2_128Concat, u32, T::ForeignAssetId, OptionQuery>;

	/// Mapping an `ForeignAssetId` to an asset index (which is used internally by the pallet).
	#[pallet::storage]
	pub type ForeignAssetIdToAssetIndex<T: Config> =
		StorageMap<_, Blake2_128Concat, T::ForeignAssetId, u32, OptionQuery>;

	impl<T: Config> Pallet<T> {
		pub fn asset_id_of(asset_index: u32) -> Option<T::ForeignAssetId> {
			AssetIndexToForeignAssetId::<T>::get(asset_index)
		}
		pub fn asset_index_of(asset_id: &T::ForeignAssetId) -> Option<u32> {
			ForeignAssetIdToAssetIndex::<T>::get(asset_id)
		}
	}
}
