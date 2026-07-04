// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

mod pallet_xcm_benchmarks;

use crate::Runtime;
use ::pallet_xcm_benchmarks::{
	impl_xcm_fungible_weight_info_provider, impl_xcm_generic_weight_info_provider,
	xcm_weights::{
		AssetMatcher, AssetTypes, AssetWeigher, AutoXcmWeight, AutoXcmWeightConfig,
		MatchedAssetWeigher,
	},
};
use frame_support::weights::Weight;
use xcm::latest::prelude::*;

use pallet_xcm_benchmarks::WeightInfo as XcmBenchWeight;

impl_xcm_generic_weight_info_provider!(XcmBenchWeight<Runtime>);
impl_xcm_fungible_weight_info_provider!(XcmBenchWeight<Runtime>);

pub struct WestendAssetMatcher;
impl AssetMatcher for WestendAssetMatcher {
	fn classify(asset: &Asset) -> AssetTypes {
		match asset {
			Asset { id: AssetId(Location { parents: 0, interior: Here }), .. } => {
				AssetTypes::Balances
			},
			_ => AssetTypes::Unknown,
		}
	}

	fn max_assets() -> u64 {
		MAX_ASSETS
	}
}

// Westend only knows about one asset, the balances pallet.
const MAX_ASSETS: u64 = 1;

pub struct WestendXcmWeightConfig;
impl<Call> AutoXcmWeightConfig<Call> for WestendXcmWeightConfig {
	type GenericWeights = XcmBenchWeight<Runtime>;
	type FungibleWeights = XcmBenchWeight<Runtime>;
	type AssetWeigher = MatchedAssetWeigher<WestendAssetMatcher>;

	fn universal_origin() -> Weight {
		Weight::MAX
	}
}

pub type WestendXcmWeight<RuntimeCall> = AutoXcmWeight<RuntimeCall, WestendXcmWeightConfig>;

#[test]
fn all_counted_has_a_sane_weight_upper_limit() {
	let assets = AssetFilter::Wild(AllCounted(4294967295));
	let weight = Weight::from_parts(1000, 1000);

	assert_eq!(
		<MatchedAssetWeigher<WestendAssetMatcher> as AssetWeigher>::weigh_asset_filter(
			&assets, weight
		),
		weight * MAX_ASSETS
	);
}
