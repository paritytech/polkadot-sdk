// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
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

pub struct PeopleWestendCountWeigher;
impl AssetFilterCountWeigher for PeopleWestendCountWeigher {
	fn max_assets() -> u64 {
		MAX_ASSETS
	}

	fn max_assets_into_holding() -> u64 {
		MaxAssetsIntoHolding::get() as u64
	}
}

pub struct PeopleWestendXcmWeightConfig;
impl<Call> CountBasedXcmWeightConfig<Call> for PeopleWestendXcmWeightConfig {
	type GenericWeights = XcmBenchWeight<Runtime>;
	type FungibleWeights = XcmBenchWeight<Runtime>;
	type FilterCountWeigher = PeopleWestendCountWeigher;
	type AssetsListWeigher = UniformAssetsWeigher;
}

pub type PeopleWestendXcmWeight<Call> = AutoCountBasedXcmWeight<Call, PeopleWestendXcmWeightConfig>;
