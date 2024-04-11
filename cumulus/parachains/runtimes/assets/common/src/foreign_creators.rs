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

use frame_support::traits::{
	ContainsPair, EnsureOrigin, EnsureOriginWithArg, Everything, OriginTrait,
};
use pallet_xcm::{EnsureXcm, Origin as XcmOrigin};
use xcm::latest::Location;
use xcm_executor::traits::ConvertLocation;

/// `EnsureOriginWithArg` impl for `CreateOrigin` that allows only XCM origins that are locations
/// containing the class location.
pub struct ForeignCreators<IsForeign, AccountOf, AccountId, L = Location>(
	sp_std::marker::PhantomData<(IsForeign, AccountOf, AccountId, L)>,
);
impl<
		IsForeign: ContainsPair<L, L>,
		AccountOf: ConvertLocation<AccountId>,
		AccountId: Clone,
		RuntimeOrigin: From<XcmOrigin> + OriginTrait + Clone,
		L: TryFrom<Location> + TryInto<Location> + Clone,
	> EnsureOriginWithArg<RuntimeOrigin, L> for ForeignCreators<IsForeign, AccountOf, AccountId, L>
where
	RuntimeOrigin::PalletsOrigin:
		From<XcmOrigin> + TryInto<XcmOrigin, Error = RuntimeOrigin::PalletsOrigin>,
{
	type Success = AccountId;

	fn try_origin(
		origin: RuntimeOrigin,
		asset_location: &L,
	) -> sp_std::result::Result<Self::Success, RuntimeOrigin> {
		let origin_location = EnsureXcm::<Everything, L>::try_origin(origin.clone())?;
		if !IsForeign::contains(asset_location, &origin_location) {
			return Err(origin)
		}
		let latest_location: Location =
			origin_location.clone().try_into().map_err(|_| origin.clone())?;
		AccountOf::convert_location(&latest_location).ok_or(origin)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin(a: &L) -> Result<RuntimeOrigin, ()> {
		let latest_location: Location = (*a).clone().try_into().map_err(|_| ())?;
		Ok(pallet_xcm::Origin::Xcm(latest_location).into())
	}
}
