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
use asset_hub_rococo_runtime::{
	xcm_config::{GovernanceLocation, RelayTreasuryLocation},
	TreasurerBodyId,
};
use codec::Encode;
use emulated_integration_tests_common::ASSET_HUB_ROCOCO_ID;
use frame_support::{assert_ok, sp_runtime::traits::Dispatchable};
use pallet_xcm::{EnsureXcm, IsVoiceOfBody};

#[test]
fn treasury_creates_asset_reward_pool() {
	AssetHubRococo::execute_with(|| {
		type AssetHubRococoRuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		type AssetHubRococoRuntimeCall = <AssetHubRococo as Chain>::RuntimeCall;
		type AssetHubRococoRuntimeOrigin = <AssetHubRococo as Chain>::RuntimeOrigin;
		type AssetHubRococoRuntime = <AssetHubRococo as Chain>::Runtime;
		type RococoRuntimeCall = <Rococo as Chain>::RuntimeCall;
		type RococoRuntime = <Rococo as Chain>::Runtime;
		type RococoRuntimeOrigin = <Rococo as Chain>::RuntimeOrigin;

		let staked_asset_id = bx!(xcm::v3::Junction::PalletInstance(1).into());
		let reward_asset_id = bx!(xcm::v3::Junction::PalletInstance(2).into());
		let reward_rate_per_block = 1_000_000_000;
		let expiry_block = 1_000_000_000;
		let admin = None;

		let create_pool_call =
			RococoRuntimeCall::XcmPallet(pallet_xcm::Call::<RococoRuntime>::send {
				dest: bx!(VersionedLocation::V4(
					xcm::v4::Junction::Parachain(ASSET_HUB_ROCOCO_ID).into()
				)),
				message: bx!(VersionedXcm::V4(Xcm(vec![
					UnpaidExecution { weight_limit: Unlimited, check_origin: None },
					Transact {
						origin_kind: OriginKind::Xcm,
						require_weight_at_most: Weight::from_parts(5_000_000_000, 500_000),
						call: AssetHubRococoRuntimeCall::AssetRewards(
							pallet_asset_rewards::Call::<AssetHubRococoRuntime>::create_pool {
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

		let treasury_origin: RococoRuntimeOrigin =
			rococo_runtime::governance::pallet_custom_origins::Origin::Treasurer.into();

		assert_ok!(create_pool_call.dispatch(treasury_origin));

		// 	assert_expected_events!(
		// 		CollectivesPolkadot,
		// 		vec![
		// 			RuntimeEvent::PolkadotXcm(pallet_xcm::Event::Sent { .. }) => {},
		// 		]
		// 	);
		// });
		//
		// Polkadot::execute_with(|| {
		// 	type RuntimeEvent = <Polkadot as Chain>::RuntimeEvent;
		//
		// 	assert_expected_events!(
		// 		Polkadot,
		// 		vec![
		// 			RuntimeEvent::Whitelist(pallet_whitelist::Event::CallWhitelisted { .. }) => {},
		// 			RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true, .. })
		// => {}, 		]
		// 	);
	});
}
