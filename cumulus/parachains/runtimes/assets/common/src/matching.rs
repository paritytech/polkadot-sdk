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
use xcm::prelude::*;

use xcm_builder::ensure_is_remote;

frame_support::parameter_types! {
	pub LocalLocationPattern: Location = Location::new(0, Here);
	pub ParentLocation: Location = Location::parent();
}

/// Accepts an asset if it is from the origin.
pub struct IsForeignConcreteAsset<IsForeign>(core::marker::PhantomData<IsForeign>);
impl<IsForeign: ContainsPair<Location, Location>> ContainsPair<Asset, Location>
	for IsForeignConcreteAsset<IsForeign>
{
	fn contains(asset: &Asset, origin: &Location) -> bool {
		log::trace!(target: "xcm::contains", "IsForeignConcreteAsset asset: {:?}, origin: {:?}", asset, origin);
		matches!(asset.id, AssetId(ref id) if IsForeign::contains(id, origin))
	}
}

/// Checks if `a` is from sibling location `b`. Checks that `Location-a` starts with
/// `Location-b`, and that the `ParaId` of `b` is not equal to `a`.
pub struct FromSiblingParachain<SelfParaId, L = Location>(
	core::marker::PhantomData<(SelfParaId, L)>,
);
impl<SelfParaId: Get<ParaId>, L: TryFrom<Location> + TryInto<Location> + Clone> ContainsPair<L, L>
	for FromSiblingParachain<SelfParaId, L>
{
	fn contains(a: &L, b: &L) -> bool {
		// We convert locations to latest
		let a = match ((*a).clone().try_into(), (*b).clone().try_into()) {
			(Ok(a), Ok(b)) if a.starts_with(&b) => a, // `a` needs to be from `b` at least
			_ => return false,
		};

		// here we check if sibling
		match a.unpack() {
			(1, interior) =>
				matches!(interior.first(), Some(Parachain(sibling_para_id)) if sibling_para_id.ne(&u32::from(SelfParaId::get()))),
			_ => false,
		}
	}
}

/// Checks if `a` is from the expected global consensus network. Checks that `Location-a`
/// starts with `Location-b`, and that network is a foreign consensus system.
pub struct FromNetwork<UniversalLocation, ExpectedNetworkId, L = Location>(
	core::marker::PhantomData<(UniversalLocation, ExpectedNetworkId, L)>,
);
impl<
		UniversalLocation: Get<InteriorLocation>,
		ExpectedNetworkId: Get<NetworkId>,
		L: TryFrom<Location> + TryInto<Location> + Clone,
	> ContainsPair<L, L> for FromNetwork<UniversalLocation, ExpectedNetworkId, L>
{
	fn contains(a: &L, b: &L) -> bool {
		// We convert locations to latest
		let a = match ((*a).clone().try_into(), (*b).clone().try_into()) {
			(Ok(a), Ok(b)) if a.starts_with(&b) => a, // `a` needs to be from `b` at least
			_ => return false,
		};

		let universal_source = UniversalLocation::get();

		// ensure that `a` is remote and from the expected network
		match ensure_is_remote(universal_source.clone(), a.clone()) {
			Ok((network_id, _)) => network_id == ExpectedNetworkId::get(),
			Err(e) => {
				log::trace!(
					target: "xcm::contains",
					"FromNetwork origin: {:?} is not remote to the universal_source: {:?} {:?}",
					a, universal_source, e
				);
				false
			},
		}
	}
}

/// Accept an asset if it is native to `AssetsAllowedNetworks` and it is coming from
/// `OriginLocation`.
pub struct RemoteAssetFromLocation<AssetsAllowedNetworks, OriginLocation>(
	core::marker::PhantomData<(AssetsAllowedNetworks, OriginLocation)>,
);
impl<
		L: TryInto<Location> + Clone,
		AssetsAllowedNetworks: Contains<Location>,
		OriginLocation: Get<Location>,
	> ContainsPair<L, L> for RemoteAssetFromLocation<AssetsAllowedNetworks, OriginLocation>
{
	fn contains(asset: &L, origin: &L) -> bool {
		let Ok(asset) = asset.clone().try_into() else {
			return false;
		};
		let Ok(origin) = origin.clone().try_into() else {
			return false;
		};
		let expected_origin = OriginLocation::get();
		// ensure `origin` is expected `OriginLocation`
		if !expected_origin.eq(&origin) {
			log::trace!(
				target: "xcm::contains",
				"RemoteAssetFromLocation asset: {asset:?}, origin: {origin:?} is not from expected {expected_origin:?}"
			);
			return false;
		} else {
			log::trace!(
				target: "xcm::contains",
				"RemoteAssetFromLocation asset: {asset:?}, origin: {origin:?}",
			);
		}

		// ensure `asset` is from remote consensus listed in `AssetsAllowedNetworks`
		AssetsAllowedNetworks::contains(&asset)
	}
}
impl<AssetsAllowedNetworks: Contains<Location>, OriginLocation: Get<Location>>
	ContainsPair<Asset, Location> for RemoteAssetFromLocation<AssetsAllowedNetworks, OriginLocation>
{
	fn contains(asset: &Asset, origin: &Location) -> bool {
		<Self as ContainsPair<Location, Location>>::contains(&asset.id.0, origin)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::parameter_types;

	parameter_types! {
		pub UniversalLocation: InteriorLocation = [GlobalConsensus(Rococo), Parachain(1000)].into();
		pub ExpectedNetworkId: NetworkId = Wococo;
	}

	#[test]
	fn from_network_contains_works() {
		// asset and origin from foreign consensus works
		let asset: Location = (
			Parent,
			Parent,
			GlobalConsensus(Wococo),
			Parachain(1000),
			PalletInstance(1),
			GeneralIndex(1),
		)
			.into();
		let origin: Location = (Parent, Parent, GlobalConsensus(Wococo), Parachain(1000)).into();
		assert!(FromNetwork::<UniversalLocation, ExpectedNetworkId>::contains(&asset, &origin));

		// asset and origin from local consensus fails
		let asset: Location = (
			Parent,
			Parent,
			GlobalConsensus(Rococo),
			Parachain(1000),
			PalletInstance(1),
			GeneralIndex(1),
		)
			.into();
		let origin: Location = (Parent, Parent, GlobalConsensus(Rococo), Parachain(1000)).into();
		assert!(!FromNetwork::<UniversalLocation, ExpectedNetworkId>::contains(&asset, &origin));

		// asset and origin from here fails
		let asset: Location = (PalletInstance(1), GeneralIndex(1)).into();
		let origin: Location = Here.into();
		assert!(!FromNetwork::<UniversalLocation, ExpectedNetworkId>::contains(&asset, &origin));

		// asset from different consensus fails
		let asset: Location = (
			Parent,
			Parent,
			GlobalConsensus(Polkadot),
			Parachain(1000),
			PalletInstance(1),
			GeneralIndex(1),
		)
			.into();
		let origin: Location = (Parent, Parent, GlobalConsensus(Wococo), Parachain(1000)).into();
		assert!(!FromNetwork::<UniversalLocation, ExpectedNetworkId>::contains(&asset, &origin));

		// origin from different consensus fails
		let asset: Location = (
			Parent,
			Parent,
			GlobalConsensus(Wococo),
			Parachain(1000),
			PalletInstance(1),
			GeneralIndex(1),
		)
			.into();
		let origin: Location = (Parent, Parent, GlobalConsensus(Polkadot), Parachain(1000)).into();
		assert!(!FromNetwork::<UniversalLocation, ExpectedNetworkId>::contains(&asset, &origin));

		// asset and origin from unexpected consensus fails
		let asset: Location = (
			Parent,
			Parent,
			GlobalConsensus(Polkadot),
			Parachain(1000),
			PalletInstance(1),
			GeneralIndex(1),
		)
			.into();
		let origin: Location = (Parent, Parent, GlobalConsensus(Polkadot), Parachain(1000)).into();
		assert!(!FromNetwork::<UniversalLocation, ExpectedNetworkId>::contains(&asset, &origin));
	}
}
