// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use crate::{
	xcm_config,
	xcm_config::{AssetTransactors, XcmConfig},
	Runtime, RuntimeEvent, RuntimeOrigin,
};
use assets_common::matching::FromSiblingParachain;
use frame_support::{parameter_types, traits::Everything};
use pallet_xcm::{EnsureXcm, Origin as XcmOrigin};
use testnet_parachains_constants::westend::snowbridge::EthereumNetwork;
use xcm::prelude::{Asset, InteriorLocation, Location, PalletInstance, Parachain};
use xcm_executor::XcmExecutor;

#[cfg(not(feature = "runtime-benchmarks"))]
use crate::xcm_config::XcmRouter;
use crate::xcm_config::{LocalOriginToLocation, UniversalLocation};
#[cfg(feature = "runtime-benchmarks")]
use benchmark_helpers::DoNothingRouter;
use frame_support::traits::{
	ContainsPair, EitherOf, EnsureOrigin, EnsureOriginWithArg, OriginTrait,
};
use xcm_builder::EnsureXcmOrigin;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmark_helpers {
	use crate::RuntimeOrigin;
	use codec::Encode;
	use xcm::prelude::*;

	pub struct DoNothingRouter;
	impl SendXcm for DoNothingRouter {
		type Ticket = Xcm<()>;

		fn validate(
			_dest: &mut Option<Location>,
			xcm: &mut Option<Xcm<()>>,
		) -> SendResult<Self::Ticket> {
			Ok((xcm.clone().unwrap(), Assets::new()))
		}
		fn deliver(xcm: Xcm<()>) -> Result<XcmHash, SendError> {
			let hash = xcm.using_encoded(sp_io::hashing::blake2_256);
			Ok(hash)
		}
	}

	impl snowbridge_pallet_system_frontend::BenchmarkHelper<RuntimeOrigin> for () {
		fn make_xcm_origin(location: Location) -> RuntimeOrigin {
			RuntimeOrigin::from(pallet_xcm::Origin::Xcm(location))
		}
	}
}

parameter_types! {
	pub storage FeeAsset: Location = Location::new(
			2,
			[
				EthereumNetwork::get().into(),
			],
	);
	pub storage DeliveryFee: Asset = (Location::parent(), 80_000_000_000u128).into();
	pub BridgeHubLocation: Location = Location::new(1,[Parachain(westend_runtime_constants::system_parachain::BRIDGE_HUB_ID)]);
	pub SystemFrontendPalletLocation: InteriorLocation = [PalletInstance(80)].into();
}

impl snowbridge_pallet_system_frontend::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = ();
	type CreateAgentOrigin =
		EitherOf<EnsureXcm<Everything>, EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>>;
	type RegisterTokenOrigin = ForeignTokenCreator<
		(
			FromSiblingParachain<parachain_info::Pallet<Runtime>, Location>,
			xcm_config::bridging::to_rococo::RococoAssetFromAssetHubRococo,
		),
		Location,
	>;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type XcmSender = XcmRouter;
	#[cfg(feature = "runtime-benchmarks")]
	type XcmSender = DoNothingRouter;
	type AssetTransactor = AssetTransactors;
	type EthereumLocation = FeeAsset;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type BridgeHubLocation = BridgeHubLocation;
	type UniversalLocation = UniversalLocation;
	type PalletLocation = SystemFrontendPalletLocation;
}

/// `EnsureOriginWithArg` impl for `ForeignTokenCreator` that allows only XCM origins that are
/// locations containing the class location.
pub struct ForeignTokenCreator<IsForeign, L = Location>(core::marker::PhantomData<(IsForeign, L)>);
impl<
		IsForeign: ContainsPair<L, L>,
		RuntimeOrigin: From<XcmOrigin> + OriginTrait + Clone,
		L: TryFrom<Location> + TryInto<Location> + Clone,
	> EnsureOriginWithArg<RuntimeOrigin, L> for ForeignTokenCreator<IsForeign, L>
where
	RuntimeOrigin::PalletsOrigin:
		From<XcmOrigin> + TryInto<XcmOrigin, Error = RuntimeOrigin::PalletsOrigin>,
{
	type Success = Location;

	fn try_origin(
		origin: RuntimeOrigin,
		asset_location: &L,
	) -> Result<Self::Success, RuntimeOrigin> {
		let origin_location = EnsureXcm::<Everything, L>::try_origin(origin.clone())?;
		if !IsForeign::contains(asset_location, &origin_location) {
			return Err(origin)
		}
		let latest_location: Location =
			origin_location.clone().try_into().map_err(|_| origin.clone())?;
		Ok(latest_location)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin(a: &L) -> Result<RuntimeOrigin, ()> {
		let latest_location: Location = (*a).clone().try_into().map_err(|_| ())?;
		Ok(pallet_xcm::Origin::Xcm(latest_location).into())
	}
}
