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
use frame_support::{pallet_prelude::Get, traits::ContainsPair};
use xcm::{
	latest::prelude::{MultiAsset, MultiLocation},
	prelude::*,
};
use xcm_builder::ensure_is_remote;

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
/// Note: `MultiLocation` has to be from a different global consensus.
pub struct IsTrustedBridgedReserveLocationForConcreteAsset<UniversalLocation, Reserves>(
	sp_std::marker::PhantomData<(UniversalLocation, Reserves)>,
);
impl<
		UniversalLocation: Get<InteriorMultiLocation>,
		Reserves: ContainsPair<MultiAsset, MultiLocation>,
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

		// check asset according to the configured reserve locations
		Reserves::contains(asset, origin)
	}
}
