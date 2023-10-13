// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Various implementations of `ContainsPair<MultiAsset, MultiLocation>` or
//! `Contains<(MultiLocation, Vec<MultiAsset>)>`.

use frame_support::traits::{Contains, ContainsPair, Get};
use sp_std::{marker::PhantomData, vec::Vec};
use xcm::latest::{AssetId::Concrete, MultiAsset, MultiAssetFilter, MultiLocation, WildMultiAsset};

/// Accepts an asset iff it is a native asset.
pub struct NativeAsset;
impl ContainsPair<MultiAsset, MultiLocation> for NativeAsset {
	fn contains(asset: &MultiAsset, origin: &MultiLocation) -> bool {
		log::trace!(target: "xcm::contains", "NativeAsset asset: {:?}, origin: {:?}", asset, origin);
		matches!(asset.id, Concrete(ref id) if id == origin)
	}
}

/// Accepts an asset if it is contained in the given `T`'s `Get` implementation.
pub struct Case<T>(PhantomData<T>);
impl<T: Get<(MultiAssetFilter, MultiLocation)>> ContainsPair<MultiAsset, MultiLocation>
	for Case<T>
{
	fn contains(asset: &MultiAsset, origin: &MultiLocation) -> bool {
		log::trace!(target: "xcm::contains", "Case asset: {:?}, origin: {:?}", asset, origin);
		let (a, o) = T::get();
		a.matches(asset) && &o == origin
	}
}

/// Accepts a tuple `(location, assets)` if the `location` is contained in the `Contains`
/// implementation of the given `Location` and if every asset from `assets` matches at least one of
/// the `MultiAssetFilter` instances provided by the `Get` implementation of `AssetFilters`.
pub struct LocationWithAssetFilters<Location, AssetFilters>(
	sp_std::marker::PhantomData<(Location, AssetFilters)>,
);
impl<Location: Contains<MultiLocation>, AssetFilters: Get<Vec<MultiAssetFilter>>>
	Contains<(MultiLocation, Vec<MultiAsset>)> for LocationWithAssetFilters<Location, AssetFilters>
{
	fn contains((location, assets): &(MultiLocation, Vec<MultiAsset>)) -> bool {
		log::trace!(target: "xcm::contains", "LocationWithAssetFilters location: {:?}, assets: {:?}", location, assets);

		// `location` must match the `Location` filter.
		if !Location::contains(location) {
			return false
		}

		// All `assets` must match at least one of the `AssetFilters`.
		let filters = AssetFilters::get();
		assets.iter().all(|asset| {
			let mut matched = false;
			for filter in &filters {
				if filter.matches(asset) {
					matched = true;
					break
				}
			}
			matched
		})
	}
}

/// Implementation of `Get<Vec<MultiAssetFilter>>` which accepts every asset.
/// (For example, it can be used with `LocationWithAssetFilters`).
pub struct AllAssets;
impl Get<Vec<MultiAssetFilter>> for AllAssets {
	fn get() -> Vec<MultiAssetFilter> {
		sp_std::vec![MultiAssetFilter::Wild(WildMultiAsset::All)]
	}
}
