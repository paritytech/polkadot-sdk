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

pub struct StartsWith<T>(sp_std::marker::PhantomData<T>);
impl<LocationValue: Get<Location>> Contains<Location> for StartsWith<LocationValue> {
	fn contains(t: &Location) -> bool {
		t.starts_with(&LocationValue::get())
	}
}

pub struct Equals<T>(sp_std::marker::PhantomData<T>);
impl<LocationValue: Get<Location>> Contains<Location> for Equals<LocationValue> {
	fn contains(t: &Location) -> bool {
		t == &LocationValue::get()
	}
}

pub struct StartsWithExplicitGlobalConsensus<T>(sp_std::marker::PhantomData<T>);
impl<Network: Get<NetworkId>> Contains<Location> for StartsWithExplicitGlobalConsensus<Network> {
	fn contains(t: &Location) -> bool {
		matches!(t.interior.global_consensus(), Ok(requested_network) if requested_network.eq(&Network::get()))
	}
}
use xcm_builder::ensure_is_remote;

frame_support::parameter_types! {
	pub LocalLocationPattern: Location = Location::new(0, Here);
	pub ParentLocation: Location = Location::parent();
}

/// Accepts an asset if it is from the origin.
pub struct IsForeignConcreteAsset<IsForeign>(sp_std::marker::PhantomData<IsForeign>);
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
	sp_std::marker::PhantomData<(SelfParaId, L)>,
);
impl<SelfParaId: Get<ParaId>, L: TryFrom<Location> + TryInto<Location> + Clone> ContainsPair<L, L>
	for FromSiblingParachain<SelfParaId, L>
{
	fn contains(a: &L, b: &L) -> bool {
		let a: Location = if let Ok(location) = (*a).clone().try_into() {
			location
		} else {
			return false;
		};
		let b: Location = if let Ok(location) = (*b).clone().try_into() {
			location
		} else {
			return false;
		};

		// `a` needs to be from `b` at least
		if !a.starts_with(&b) {
			return false;
		}

		// here we check if sibling
		match a.unpack() {
			(1, interior) =>
				matches!(interior.first(), Some(Parachain(sibling_para_id)) if sibling_para_id.ne(&u32::from(SelfParaId::get()))),
			_ => false,
		}
	}
}

/// Adapter verifies if it is allowed to receive `Asset` from `Location`.
///
/// Note: `Location` has to be from a different global consensus.
pub struct IsTrustedBridgedReserveLocationForConcreteAsset<UniversalLocation, Reserves>(
	sp_std::marker::PhantomData<(UniversalLocation, Reserves)>,
);
impl<UniversalLocation: Get<InteriorLocation>, Reserves: ContainsPair<Asset, Location>>
	ContainsPair<Asset, Location>
	for IsTrustedBridgedReserveLocationForConcreteAsset<UniversalLocation, Reserves>
{
	fn contains(asset: &Asset, origin: &Location) -> bool {
		let universal_source = UniversalLocation::get();
		log::trace!(
			target: "xcm::contains",
			"IsTrustedBridgedReserveLocationForConcreteAsset asset: {:?}, origin: {:?}, universal_source: {:?}",
			asset, origin, universal_source
		);

		// check remote origin
		let _ = match ensure_is_remote(universal_source.clone(), origin.clone()) {
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

		// check asset according to the configured reserve locations
		Reserves::contains(asset, origin)
	}
}
