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

use crate::{
	weights, xcm_config,
	xcm_config::{
		AssetTransactors, LocationToAccountId, TrustBackedAssetsPalletLocation, UniversalLocation,
		XcmConfig,
	},
	AccountId, Assets, ForeignAssets, Runtime, RuntimeEvent,
};
use assets_common::{matching::FromSiblingParachain, AssetIdForTrustBackedAssetsConvert};
use frame_support::{parameter_types, traits::EitherOf};
use frame_system::EnsureRootWithSuccess;
use parachains_common::AssetIdForTrustBackedAssets;
use snowbridge_runtime_common::{ForeignAssetOwner, LocalAssetOwner};
use testnet_parachains_constants::westend::snowbridge::{EthereumNetwork, FRONTEND_PALLET_INDEX};
use xcm::prelude::{Asset, InteriorLocation, Location, PalletInstance, Parachain};
use xcm_executor::XcmExecutor;

#[cfg(not(feature = "runtime-benchmarks"))]
use crate::xcm_config::XcmRouter;
#[cfg(feature = "runtime-benchmarks")]
use benchmark_helpers::DoNothingRouter;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmark_helpers {
	use crate::{xcm_config::LocationToAccountId, ForeignAssets, RuntimeOrigin};
	use codec::Encode;
	use xcm::prelude::*;
	use xcm_executor::traits::ConvertLocation;

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

		fn initialize_storage(asset_location: Location, asset_owner: Location) {
			let asset_owner = LocationToAccountId::convert_location(&asset_owner).unwrap();
			ForeignAssets::force_create(
				RuntimeOrigin::root(),
				asset_location,
				asset_owner.into(),
				true,
				1,
			)
			.unwrap()
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
	pub BridgeHubLocation: Location = Location::new(1, [Parachain(westend_runtime_constants::system_parachain::BRIDGE_HUB_ID)]);
	pub SystemFrontendPalletLocation: InteriorLocation = [PalletInstance(FRONTEND_PALLET_INDEX)].into();
	pub const RootLocation: Location = Location::here();
}

impl snowbridge_pallet_system_frontend::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::snowbridge_pallet_system_frontend::WeightInfo<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = ();
	type RegisterTokenOrigin = EitherOf<
		EitherOf<
			LocalAssetOwner<
				AssetIdForTrustBackedAssetsConvert<TrustBackedAssetsPalletLocation, Location>,
				Assets,
				AccountId,
				AssetIdForTrustBackedAssets,
				Location,
			>,
			ForeignAssetOwner<
				(
					FromSiblingParachain<parachain_info::Pallet<Runtime>, Location>,
					xcm_config::bridging::to_rococo::RococoAssetFromAssetHubRococo,
				),
				ForeignAssets,
				AccountId,
				LocationToAccountId,
				Location,
			>,
		>,
		EnsureRootWithSuccess<AccountId, RootLocation>,
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
	type BackendWeightInfo = weights::snowbridge_pallet_system_backend::WeightInfo<Runtime>;
}
