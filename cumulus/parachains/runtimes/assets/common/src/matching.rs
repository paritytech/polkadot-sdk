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

use cumulus_primitives_core::ParaId;
use frame_support::{
	pallet_prelude::Get,
	traits::{Contains, ContainsPair},
};
use parachains_common::xcm_config::{LocationFilter, MatchesLocation};
use xcm::{
	latest::prelude::{MultiAsset, MultiLocation},
	prelude::*,
};
use xcm_builder::{ensure_is_remote, ExporterFor};

pub struct StartsWith<T>(sp_std::marker::PhantomData<T>);
impl<Location: Get<MultiLocation>> Contains<MultiLocation> for StartsWith<Location> {
	fn contains(t: &MultiLocation) -> bool {
		t.starts_with(&Location::get())
	}
}

pub struct Equals<T>(sp_std::marker::PhantomData<T>);
impl<Location: Get<MultiLocation>> Contains<MultiLocation> for Equals<Location> {
	fn contains(t: &MultiLocation) -> bool {
		t == &Location::get()
	}
}

pub struct StartsWithExplicitGlobalConsensus<T>(sp_std::marker::PhantomData<T>);
impl<Network: Get<NetworkId>> Contains<MultiLocation>
	for StartsWithExplicitGlobalConsensus<Network>
{
	fn contains(t: &MultiLocation) -> bool {
		matches!(t.interior.global_consensus(), Ok(requested_network) if requested_network.eq(&Network::get()))
	}
}

frame_support::parameter_types! {
	pub LocalMultiLocationPattern: MultiLocation = MultiLocation::new(0, Here);
	pub ParentLocation: MultiLocation = MultiLocation::parent();
}

/// Accepts an asset if it is from the origin.
pub struct IsForeignConcreteAsset<IsForeign>(sp_std::marker::PhantomData<IsForeign>);
impl<IsForeign: ContainsPair<MultiLocation, MultiLocation>> ContainsPair<MultiAsset, MultiLocation>
	for IsForeignConcreteAsset<IsForeign>
{
	fn contains(asset: &MultiAsset, origin: &MultiLocation) -> bool {
		log::trace!(target: "xcm::contains", "IsForeignConcreteAsset asset: {:?}, origin: {:?}", asset, origin);
		matches!(asset.id, Concrete(ref id) if IsForeign::contains(id, origin))
	}
}

/// Checks if `a` is from sibling location `b`. Checks that `MultiLocation-a` starts with
/// `MultiLocation-b`, and that the `ParaId` of `b` is not equal to `a`.
pub struct FromSiblingParachain<SelfParaId>(sp_std::marker::PhantomData<SelfParaId>);
impl<SelfParaId: Get<ParaId>> ContainsPair<MultiLocation, MultiLocation>
	for FromSiblingParachain<SelfParaId>
{
	fn contains(&a: &MultiLocation, b: &MultiLocation) -> bool {
		// `a` needs to be from `b` at least
		if !a.starts_with(b) {
			return false
		}

		// here we check if sibling
		match a {
			MultiLocation { parents: 1, interior } =>
				matches!(interior.first(), Some(Parachain(sibling_para_id)) if sibling_para_id.ne(&u32::from(SelfParaId::get()))),
			_ => false,
		}
	}
}

/// Adapter verifies if it is allowed to receive `MultiAsset` from `MultiLocation`.
///
/// Note: `MultiLocation` has to be from different global consensus.
pub struct IsTrustedBridgedReserveLocationForConcreteAsset<UniversalLocation, Reserves>(
	sp_std::marker::PhantomData<(UniversalLocation, Reserves)>,
);
impl<
		UniversalLocation: Get<InteriorMultiLocation>,
		Reserves: Get<sp_std::vec::Vec<FilteredLocation>>,
	> ContainsPair<MultiAsset, MultiLocation>
	for IsTrustedBridgedReserveLocationForConcreteAsset<UniversalLocation, Reserves>
{
	fn contains(asset: &MultiAsset, origin: &MultiLocation) -> bool {
		let universal_source = UniversalLocation::get();
		log::trace!(
			target: "xcm::contains",
			"IsTrustedBridgedReserveLocationForConcreteAsset asset: {:?}, origin: {:?}, universal_source: {:?}",
			asset, origin, universal_source
		);

		// check remote origin
		let _ = match ensure_is_remote(universal_source, *origin) {
			Ok(devolved) => devolved,
			Err(_) => {
				log::trace!(
					target: "xcm::contains",
					"IsTrustedBridgedReserveLocationForConcreteAsset origin: {:?} is not remote to the universal_source: {:?}",
					origin, universal_source
				);
				return false
			},
		};

		// check asset location
		let asset_location = match &asset.id {
			Concrete(location) => location,
			_ => return false,
		};

		// check asset according to the configured reserve locations
		for (reserve_location, asset_filter) in Reserves::get() {
			if origin.eq(&reserve_location) && asset_filter.matches(asset_location) {
				return true
			}
		}

		false
	}
}

/// Disallow all assets the are either not `Concrete`, or not explicitly allowed by
/// `LocationAssetFilters`, iff `dest` matches any location in `LocationAssetFilters`.
///
/// Returns `false` regardless of `assets`, if `dest` does not match any location in
/// `LocationAssetFilters`. Otherwise, returns `true` if asset is either not `Concrete` or is not
/// explicitly allowed by `LocationAssetFilters`, otherwise returns `false`.
pub struct DisallowConcreteAssetUnless<LocationAssetFilters>(
	sp_std::marker::PhantomData<LocationAssetFilters>,
);
impl<LocationAssetFilters: Get<sp_std::vec::Vec<FilteredLocation>>>
	Contains<(MultiLocation, sp_std::vec::Vec<MultiAsset>)>
	for DisallowConcreteAssetUnless<LocationAssetFilters>
{
	fn contains((dest, assets): &(MultiLocation, sp_std::vec::Vec<MultiAsset>)) -> bool {
		for (allowed_dest, asset_filter) in LocationAssetFilters::get().iter() {
			// we only disallow `assets` on explicitly configured destinations
			if !allowed_dest.eq(dest) {
				continue
			}

			// check all assets
			for asset in assets {
				let asset_location = match &asset.id {
					Concrete(location) => location,
					_ => return true,
				};

				if !asset_filter.matches(asset_location) {
					// if asset does not match filter, disallow it
					return true
				}
			}
		}

		// if we got here, allow it
		false
	}
}

/// Adapter for `Contains<(MultiLocation, sp_std::vec::Vec<MultiAsset>)>` which returns `true`
/// iff `Exporters` contains exporter for **remote** `MultiLocation` _and_
///`assets` also pass`Filter`, otherwise returns `false`.
///
/// Note: Assumes that `Exporters` do not depend on `XCM program` and works for `Xcm::default()`.
pub struct ExcludeOnlyForRemoteDestination<UniversalLocation, Exporters, Exclude>(
	sp_std::marker::PhantomData<(UniversalLocation, Exporters, Exclude)>,
);
impl<UniversalLocation, Exporters, Exclude> Contains<(MultiLocation, sp_std::vec::Vec<MultiAsset>)>
	for ExcludeOnlyForRemoteDestination<UniversalLocation, Exporters, Exclude>
where
	UniversalLocation: Get<InteriorMultiLocation>,
	Exporters: ExporterFor,
	Exclude: Contains<(MultiLocation, sp_std::vec::Vec<MultiAsset>)>,
{
	fn contains(dest_and_assets: &(MultiLocation, sp_std::vec::Vec<MultiAsset>)) -> bool {
		let universal_source = UniversalLocation::get();
		log::trace!(
			target: "xcm::contains",
			"CheckOnlyForRemoteDestination dest: {:?}, assets: {:?}, universal_source: {:?}",
			dest_and_assets.0, dest_and_assets.1, universal_source
		);

		// check if it is remote destination
		match ensure_is_remote(universal_source, dest_and_assets.0) {
			Ok((remote_network, remote_destination)) => {
				if Exporters::exporter_for(&remote_network, &remote_destination, &Xcm::default())
					.is_some()
				{
					// destination is remote, and has configured exporter, now check filter
					Exclude::contains(dest_and_assets)
				} else {
					log::trace!(
						target: "xcm::contains",
						"CheckOnlyForRemoteDestination no exporter for dest: {:?}",
						dest_and_assets.0
					);
					// no exporter means that we exclude by default
					true
				}
			},
			Err(_) => {
				log::trace!(
					target: "xcm::contains",
					"CheckOnlyForRemoteDestination dest: {:?} is not remote to the universal_source: {:?}",
					dest_and_assets.0, universal_source
				);
				// not a remote destination, do not exclude
				false
			},
		}
	}
}

/// Location as `MultiLocation` with `AssetFilter`.
pub type FilteredLocation = (MultiLocation, AssetFilter);

/// Simple asset location filter.
#[derive(Debug)]
pub enum AssetFilter {
	ByMultiLocation(LocationFilter<MultiLocation>),
}

impl MatchesLocation<MultiLocation> for AssetFilter {
	fn matches(&self, asset_location: &MultiLocation) -> bool {
		match self {
			AssetFilter::ByMultiLocation(by_location) => by_location.matches(asset_location),
		}
	}
}
