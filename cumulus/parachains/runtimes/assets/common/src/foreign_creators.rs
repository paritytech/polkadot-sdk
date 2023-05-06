// Copyright (C) 2023 Parity Technologies (UK) Ltd.
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

use frame_support::traits::{
	ContainsPair, EnsureOrigin, EnsureOriginWithArg, Everything, OriginTrait,
};
use pallet_xcm::{EnsureXcm, Origin as XcmOrigin};
use xcm::latest::MultiLocation;
use xcm_executor::traits::Convert;

// `EnsureOriginWithArg` impl for `CreateOrigin` that allows only XCM origins that are locations
// containing the class location.
pub struct ForeignCreators<IsForeign, AccountOf, AccountId>(
	sp_std::marker::PhantomData<(IsForeign, AccountOf, AccountId)>,
);
impl<
		IsForeign: ContainsPair<MultiLocation, MultiLocation>,
		AccountOf: Convert<MultiLocation, AccountId>,
		AccountId: Clone,
		RuntimeOrigin: From<XcmOrigin> + OriginTrait + Clone,
	> EnsureOriginWithArg<RuntimeOrigin, MultiLocation>
	for ForeignCreators<IsForeign, AccountOf, AccountId>
where
	RuntimeOrigin::PalletsOrigin:
		From<XcmOrigin> + TryInto<XcmOrigin, Error = RuntimeOrigin::PalletsOrigin>,
{
	type Success = AccountId;

	fn try_origin(
		origin: RuntimeOrigin,
		asset_location: &MultiLocation,
	) -> sp_std::result::Result<Self::Success, RuntimeOrigin> {
		let origin_location = EnsureXcm::<Everything>::try_origin(origin.clone())?;
		if !IsForeign::contains(asset_location, &origin_location) {
			return Err(origin)
		}
		AccountOf::convert(origin_location).map_err(|_| origin)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin(a: &MultiLocation) -> Result<RuntimeOrigin, ()> {
		Ok(pallet_xcm::Origin::Xcm(*a).into())
	}
}
