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

mod pallet_xcm_benchmarks;

use crate::{
	xcm_config::{ERC20TransferGasLimit, MaxAssetsIntoHolding},
	Runtime,
};
use ::pallet_xcm_benchmarks::{
	impl_xcm_fungible_weight_info_provider, impl_xcm_generic_weight_info_provider,
	xcm_weights::{
		AssetFilterCountWeigher, AssetWeigher, AssetsWeigher, AutoXcmWeight, AutoXcmWeightConfig,
		CountBasedAssetsAndFilterWeigher, XcmGenericWeightInfo,
	},
};
use assets_common::IsLocalAccountKey20;
use frame_support::{traits::Contains, weights::Weight};
use pallet_xcm_benchmarks::WeightInfo as XcmBenchWeight;
use xcm::latest::prelude::*;

impl_xcm_generic_weight_info_provider!(XcmBenchWeight<Runtime>);
impl_xcm_fungible_weight_info_provider!(XcmBenchWeight<Runtime>);

const MAX_ASSETS: u64 = 100;

/// Count-based filter weigher for Asset Hub Westend.
pub struct AssetHubWestendCountWeigher;
impl AssetFilterCountWeigher for AssetHubWestendCountWeigher {
	fn max_assets() -> u64 {
		MAX_ASSETS
	}

	fn max_assets_into_holding() -> u64 {
		MaxAssetsIntoHolding::get() as u64
	}

	fn minimum_asset_count() -> u64 {
		1
	}
}

/// Per-asset weight hook for Asset Hub Westend.
///
/// ERC20 assets routed via Snowbridge are priced in Ethereum gas (a fixed ceiling)
/// rather than Substrate execution weight. All other assets use the provided weight
/// as-is.
pub struct WestendERC20AssetWeigher;
impl AssetsWeigher for WestendERC20AssetWeigher {
	fn weigh_assets(assets: &Assets, weight: Weight) -> Weight {
		assets.inner().iter().fold(Weight::zero(), |acc, asset| {
			let asset_weight = if IsLocalAccountKey20::contains(&asset.id.0) {
				ERC20TransferGasLimit::get()
			} else {
				weight
			};

			acc.saturating_add(asset_weight)
		})
	}
}

pub struct AssetHubWestendXcmWeightConfig;

impl<Call> AutoXcmWeightConfig<Call> for AssetHubWestendXcmWeightConfig {
	type GenericWeights = XcmBenchWeight<Runtime>;
	type FungibleWeights = XcmBenchWeight<Runtime>;
	type AssetWeigher =
		CountBasedAssetsAndFilterWeigher<AssetHubWestendCountWeigher, WestendERC20AssetWeigher>;

	fn exchange_asset(give: &AssetFilter, receive: &Assets, _maximal: &bool) -> Weight {
		let base_weight = <Self::GenericWeights as XcmGenericWeightInfo>::exchange_asset();
		let give_weight =
			<Self::AssetWeigher as AssetWeigher>::weigh_asset_filter(give, base_weight);
		let receive_weight =
			<Self::AssetWeigher as AssetWeigher>::weigh_assets(receive, base_weight);
		give_weight.max(receive_weight)
	}
}

pub type AssetHubWestendXcmWeight<Call> = AutoXcmWeight<Call, AssetHubWestendXcmWeightConfig>;
