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

//! Various implementations of `ContainsPair<Asset, Location>` or
//! `Contains<(Location, Vec<Asset>)>`.

use frame_support::traits::{Contains, ContainsPair, Get};
use sp_std::{marker::PhantomData, vec::Vec};
use xcm::latest::{Asset, AssetFilter, AssetId, Location, WildAsset};

/// Accepts an asset iff it is a native asset.
pub struct NativeAsset;
impl ContainsPair<Asset, Location> for NativeAsset {
	fn contains(asset: &Asset, origin: &Location) -> bool {
		log::trace!(target: "xcm::contains", "NativeAsset asset: {:?}, origin: {:?}", asset, origin);
		matches!(asset.id, AssetId(ref id) if id == origin)
	}
}

/// Accepts an asset if it is contained in the given `T`'s `Get` implementation.
pub struct Case<T>(PhantomData<T>);
impl<T: Get<(AssetFilter, Location)>> ContainsPair<Asset, Location> for Case<T> {
	fn contains(asset: &Asset, origin: &Location) -> bool {
		log::trace!(target: "xcm::contains", "Case asset: {:?}, origin: {:?}", asset, origin);
		let (a, o) = T::get();
		a.matches(asset) && &o == origin
	}
}

/// Accepts a tuple `(location, assets)` if the `location` is contained in the `Contains`
/// implementation of the given `Location` and if every asset from `assets` matches at least one of
/// the `AssetFilter` instances provided by the `Get` implementation of `AssetFilters`.
pub struct LocationWithAssetFilters<LocationFilter, AssetFilters>(
	sp_std::marker::PhantomData<(LocationFilter, AssetFilters)>,
);
impl<LocationFilter: Contains<Location>, AssetFilters: Get<Vec<AssetFilter>>>
	Contains<(Location, Vec<Asset>)> for LocationWithAssetFilters<LocationFilter, AssetFilters>
{
	fn contains((location, assets): &(Location, Vec<Asset>)) -> bool {
		log::trace!(target: "xcm::contains", "LocationWithAssetFilters location: {:?}, assets: {:?}", location, assets);

		// `location` must match the `Location` filter.
		if !LocationFilter::contains(location) {
			return false
		}

		// All `assets` must match at least one of the `AssetFilters`.
		let filters = AssetFilters::get();
		assets.iter().all(|asset| {
			for filter in &filters {
				if filter.matches(asset) {
					return true
				}
			}
			false
		})
	}
}

/// Implementation of `Get<Vec<AssetFilter>>` which accepts every asset.
/// (For example, it can be used with `LocationWithAssetFilters`).
pub struct AllAssets;
impl Get<Vec<AssetFilter>> for AllAssets {
	fn get() -> Vec<AssetFilter> {
		sp_std::vec![AssetFilter::Wild(WildAsset::All)]
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::traits::Equals;
	use xcm::latest::prelude::*;

	#[test]
	fn location_with_asset_filters_works() {
		frame_support::parameter_types! {
			pub ParaA: Location = Location::new(1, [Parachain(1001)]);
			pub ParaB: Location = Location::new(1, [Parachain(1002)]);
			pub ParaC: Location = Location::new(1, [Parachain(1003)]);

			pub AssetXLocation: Location = Location::new(1, [GeneralIndex(1111)]);
			pub AssetYLocation: Location = Location::new(1, [GeneralIndex(2222)]);
			pub AssetZLocation: Location = Location::new(1, [GeneralIndex(3333)]);

			pub OnlyAssetXOrAssetY: sp_std::vec::Vec<AssetFilter> = sp_std::vec![
				Wild(AllOf { fun: WildFungible, id: AssetId(AssetXLocation::get()) }),
				Wild(AllOf { fun: WildFungible, id: AssetId(AssetYLocation::get()) }),
			];
			pub OnlyAssetZ: sp_std::vec::Vec<AssetFilter> = sp_std::vec![
				Wild(AllOf { fun: WildFungible, id: AssetId(AssetZLocation::get()) })
			];
		}

		let test_data: Vec<(Location, Vec<Asset>, bool)> = vec![
			(ParaA::get(), vec![(AssetXLocation::get(), 1).into()], true),
			(ParaA::get(), vec![(AssetYLocation::get(), 1).into()], true),
			(ParaA::get(), vec![(AssetZLocation::get(), 1).into()], false),
			(
				ParaA::get(),
				vec![(AssetXLocation::get(), 1).into(), (AssetYLocation::get(), 1).into()],
				true,
			),
			(
				ParaA::get(),
				vec![(AssetXLocation::get(), 1).into(), (AssetZLocation::get(), 1).into()],
				false,
			),
			(
				ParaA::get(),
				vec![(AssetYLocation::get(), 1).into(), (AssetZLocation::get(), 1).into()],
				false,
			),
			(
				ParaA::get(),
				vec![
					(AssetXLocation::get(), 1).into(),
					(AssetYLocation::get(), 1).into(),
					(AssetZLocation::get(), 1).into(),
				],
				false,
			),
			(ParaB::get(), vec![(AssetXLocation::get(), 1).into()], false),
			(ParaB::get(), vec![(AssetYLocation::get(), 1).into()], false),
			(ParaB::get(), vec![(AssetZLocation::get(), 1).into()], true),
			(
				ParaB::get(),
				vec![(AssetXLocation::get(), 1).into(), (AssetYLocation::get(), 1).into()],
				false,
			),
			(
				ParaB::get(),
				vec![(AssetXLocation::get(), 1).into(), (AssetZLocation::get(), 1).into()],
				false,
			),
			(
				ParaB::get(),
				vec![(AssetYLocation::get(), 1).into(), (AssetZLocation::get(), 1).into()],
				false,
			),
			(
				ParaB::get(),
				vec![
					(AssetXLocation::get(), 1).into(),
					(AssetYLocation::get(), 1).into(),
					(AssetZLocation::get(), 1).into(),
				],
				false,
			),
			(ParaC::get(), vec![(AssetXLocation::get(), 1).into()], true),
			(ParaC::get(), vec![(AssetYLocation::get(), 1).into()], true),
			(ParaC::get(), vec![(AssetZLocation::get(), 1).into()], true),
			(
				ParaC::get(),
				vec![(AssetXLocation::get(), 1).into(), (AssetYLocation::get(), 1).into()],
				true,
			),
			(
				ParaC::get(),
				vec![(AssetXLocation::get(), 1).into(), (AssetZLocation::get(), 1).into()],
				true,
			),
			(
				ParaC::get(),
				vec![(AssetYLocation::get(), 1).into(), (AssetZLocation::get(), 1).into()],
				true,
			),
			(
				ParaC::get(),
				vec![
					(AssetXLocation::get(), 1).into(),
					(AssetYLocation::get(), 1).into(),
					(AssetZLocation::get(), 1).into(),
				],
				true,
			),
		];

		type Filter = (
			// For ParaA accept only asset X and Y.
			LocationWithAssetFilters<Equals<ParaA>, OnlyAssetXOrAssetY>,
			// For ParaB accept only asset Z.
			LocationWithAssetFilters<Equals<ParaB>, OnlyAssetZ>,
			// For ParaC accept all assets.
			LocationWithAssetFilters<Equals<ParaC>, AllAssets>,
		);

		for (location, assets, expected_result) in test_data {
			assert_eq!(
				Filter::contains(&(location.clone(), assets.clone())),
				expected_result,
				"expected_result: {expected_result} not matched for (location, assets): ({:?}, {:?})!", location, assets,
			)
		}
	}
}
