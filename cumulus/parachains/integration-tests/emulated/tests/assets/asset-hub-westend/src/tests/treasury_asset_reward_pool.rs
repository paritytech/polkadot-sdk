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

use crate::imports::*;
use asset_hub_westend_runtime::xcm_config::WestendLocationV3;
use codec::Encode;
use emulated_integration_tests_common::ASSET_HUB_WESTEND_ID;
use frame_support::{assert_ok, sp_runtime::traits::Dispatchable};

#[test]
fn treasury_creates_asset_reward_pool() {
	Westend::execute_with(|| {
		type AssetHubWestendRuntimeCall = <AssetHubWestend as Chain>::RuntimeCall;
		type AssetHubWestendRuntime = <AssetHubWestend as Chain>::Runtime;
		type WestendRuntimeCall = <Westend as Chain>::RuntimeCall;
		type WestendRuntime = <Westend as Chain>::Runtime;
		type WestendRuntimeEvent = <Westend as Chain>::RuntimeEvent;
		type WestendRuntimeOrigin = <Westend as Chain>::RuntimeOrigin;

		let staked_asset_id = bx!(WestendLocationV3::get());
		let reward_asset_id = bx!(WestendLocationV3::get());

		let reward_rate_per_block = 1_000_000_000;
		let expiry_block = 1_000_000_000;
		let admin = None;

		let create_pool_call =
			WestendRuntimeCall::XcmPallet(pallet_xcm::Call::<WestendRuntime>::send {
				dest: bx!(VersionedLocation::V4(
					xcm::v4::Junction::Parachain(ASSET_HUB_WESTEND_ID).into()
				)),
				message: bx!(VersionedXcm::V4(Xcm(vec![
					UnpaidExecution { weight_limit: Unlimited, check_origin: None },
					Transact {
						origin_kind: OriginKind::Xcm,
						require_weight_at_most: Weight::from_parts(5_000_000_000, 500_000),
						call: AssetHubWestendRuntimeCall::AssetRewards(
							pallet_asset_rewards::Call::<AssetHubWestendRuntime>::create_pool {
								staked_asset_id,
								reward_asset_id,
								reward_rate_per_block,
								expiry_block,
								admin
							}
						)
						.encode()
						.into(),
					}
				]))),
			});

		let treasury_origin: WestendRuntimeOrigin =
			westend_runtime::governance::pallet_custom_origins::Origin::Treasurer.into();
		assert_ok!(create_pool_call.dispatch(treasury_origin));

		assert_expected_events!(
			Westend,
			vec![
				WestendRuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	AssetHubWestend::execute_with(|| {
		type AssetHubWestendRuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![
				AssetHubWestendRuntimeEvent::AssetRewards(
					pallet_asset_rewards::Event::PoolCreated {
						..
					}
				) => {
				},

			]
		);
	});
}
