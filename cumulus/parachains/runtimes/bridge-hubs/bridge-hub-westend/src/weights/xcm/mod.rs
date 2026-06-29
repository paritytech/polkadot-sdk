// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

use crate::{xcm_config::MaxAssetsIntoHolding, Runtime};
use ::pallet_xcm_benchmarks::xcm_weights::{
	AssetFilterCountWeigher, AutoCountBasedXcmWeight, CountBasedXcmWeightConfig,
	UniformAssetsWeigher,
};
use ::pallet_xcm_benchmarks::{
	impl_xcm_fungible_weight_info_provider, impl_xcm_generic_weight_info_provider,
};
use pallet_xcm_benchmarks::WeightInfo as XcmBenchWeight;

impl_xcm_generic_weight_info_provider!(XcmBenchWeight<Runtime>);
impl_xcm_fungible_weight_info_provider!(XcmBenchWeight<Runtime>);

const MAX_ASSETS: u64 = 100;

pub struct BridgeHubWestendCountWeigher;
impl AssetFilterCountWeigher for BridgeHubWestendCountWeigher {
	fn max_assets() -> u64 {
		MAX_ASSETS
	}

	fn max_assets_into_holding() -> u64 {
		MaxAssetsIntoHolding::get() as u64
	}
}

pub struct BridgeHubWestendXcmWeightConfig;
impl<Call> CountBasedXcmWeightConfig<Call> for BridgeHubWestendXcmWeightConfig {
	type GenericWeights = XcmBenchWeight<Runtime>;
	type FungibleWeights = XcmBenchWeight<Runtime>;
	type FilterCountWeigher = BridgeHubWestendCountWeigher;
	type AssetsListWeigher = UniformAssetsWeigher;

	fn export_message(_: &NetworkId, _: &Junctions, inner: &Xcm<()>) -> Weight {
		let inner_encoded_len = inner.encode().len() as u32;
		Self::GenericWeights::export_message(inner_encoded_len)
	}
}

pub type BridgeHubWestendXcmWeight<Call> =
	AutoCountBasedXcmWeight<Call, BridgeHubWestendXcmWeightConfig>;
