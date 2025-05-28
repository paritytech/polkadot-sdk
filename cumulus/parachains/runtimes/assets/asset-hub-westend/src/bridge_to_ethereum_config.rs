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

#[cfg(not(feature = "runtime-benchmarks"))]
use crate::xcm_config::XcmRouter;
use crate::{
	weights, xcm_config,
	xcm_config::{
		AssetTransactors, LocationToAccountId, TrustBackedAssetsPalletLocation, UniversalLocation,
		XcmConfig,
	},
	AccountId, AssetConversion, Assets, ForeignAssets, Runtime, RuntimeEvent,
};
use assets_common::{matching::FromSiblingParachain, AssetIdForTrustBackedAssetsConvert};
#[cfg(feature = "runtime-benchmarks")]
use benchmark_helpers::DoNothingRouter;
use frame_support::{parameter_types, traits::EitherOf};
use frame_system::EnsureRootWithSuccess;
use parachains_common::AssetIdForTrustBackedAssets;
use snowbridge_runtime_common::{ForeignAssetOwner, LocalAssetOwner};
use testnet_parachains_constants::westend::snowbridge::{EthereumNetwork, FRONTEND_PALLET_INDEX};
use xcm::prelude::{Asset, InteriorLocation, Location, PalletInstance, Parachain};
use xcm_executor::XcmExecutor;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmark_helpers {
	use crate::{
		xcm_config::LocationToAccountId, AccountId, AssetConversion, Balances, ForeignAssets,
		RuntimeOrigin,
	};
	use alloc::boxed::Box;
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

	impl snowbridge_pallet_system_frontend::BenchmarkHelper<RuntimeOrigin, AccountId> for () {
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

		fn setup_pools(caller: AccountId, asset: Location) {
			// Prefund the caller's account with DOT
			Balances::force_set_balance(RuntimeOrigin::root(), caller.into(), 10_000_000_000_000)
				.unwrap();

			let asset_owner = LocationToAccountId::convert_location(&asset).unwrap();
			ForeignAssets::force_create(
				RuntimeOrigin::root(),
				asset.clone(),
				asset_owner.clone().into(),
				true,
				1,
			)
			.unwrap();

			let signed_owner = RuntimeOrigin::signed(asset_owner.clone());

			// Prefund the asset owner's account with DOT and Ether to create the pools
			ForeignAssets::mint(
				signed_owner.clone(),
				asset.clone().into(),
				asset_owner.clone().into(),
				10_000_000_000_000,
			)
			.unwrap();
			Balances::force_set_balance(
				RuntimeOrigin::root(),
				asset_owner.clone().into(),
				10_000_000_000_000,
			)
			.unwrap();

			// Create the pool so the swap will succeed
			let native_asset: Location = Parent.into();
			AssetConversion::create_pool(
				signed_owner.clone(),
				Box::new(native_asset.clone()),
				Box::new(asset.clone()),
			)
			.unwrap();
			AssetConversion::add_liquidity(
				signed_owner,
				Box::new(native_asset),
				Box::new(asset),
				1_000_000_000_000,
				2_000_000_000_000,
				0,
				0,
				asset_owner.into(),
			)
			.unwrap();
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
	type Swap = AssetConversion;
	type BackendWeightInfo = weights::snowbridge_pallet_system_backend::WeightInfo<Runtime>;
}
