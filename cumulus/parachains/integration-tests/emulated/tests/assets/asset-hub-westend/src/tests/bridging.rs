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

#[test]
fn ah_to_ah_open_close_bridge_works() {
	// open bridges
	let westend_bridge_opened_lane_id = AssetHubWestend::execute_with(|| {
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::XcmOverAssetHubRococo::open_bridge(
			AssetHubWestendRuntimeOrigin::root(),
			Box::new(AssetHubRococoUniversalLocation::get().into()),
			None,
		));
		let events = AssetHubWestend::events();
		type RuntimeEventWestend = <AssetHubWestend as Chain>::RuntimeEvent;
		events.iter().find_map(|event| {
			if let RuntimeEventWestend::XcmOverAssetHubRococo(
				pallet_xcm_bridge::Event::BridgeOpened { lane_id,.. },
			) = event
			{
				Some(*lane_id)
			} else {
				None
			}
		})
	});
	assert!(westend_bridge_opened_lane_id.is_some(), "Westend BridgeOpened event not found");

	let rococo_bridge_opened_lane_id = AssetHubRococo::execute_with(|| {
		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::XcmOverAssetHubWestend::open_bridge(
			AssetHubRococoRuntimeOrigin::root(),
			Box::new(AssetHubWestendUniversalLocation::get().into()),
			None,
		));
		let events = AssetHubRococo::events();
		type RuntimeEventRococo = <AssetHubRococo as Chain>::RuntimeEvent;
		events.iter().find_map(|event| {
			if let RuntimeEventRococo::XcmOverAssetHubWestend(
				pallet_xcm_bridge::Event::BridgeOpened { lane_id,.. },
			) = event
			{
				Some(*lane_id)
			} else {
				None
			}
		})
	});
	assert!(rococo_bridge_opened_lane_id.is_some(), "Rococo BridgeOpened event not found");

	assert_eq!(westend_bridge_opened_lane_id, rococo_bridge_opened_lane_id);

	// close bridges  
	let westend_bridge_pruned_lane_id = AssetHubWestend::execute_with(|| {
		assert_ok!(
			<AssetHubWestend as AssetHubWestendPallet>::XcmOverAssetHubRococo::close_bridge(
				AssetHubWestendRuntimeOrigin::root(),
				Box::new(AssetHubRococoUniversalLocation::get().into()),
				1,
			)
		);
		let events = AssetHubWestend::events();
		type RuntimeEventWestend = <AssetHubWestend as Chain>::RuntimeEvent;
		events.iter().find_map(|event| {
			if let RuntimeEventWestend::XcmOverAssetHubRococo(
				pallet_xcm_bridge::Event::BridgePruned { lane_id,.. },
			) = event
			{
				Some(*lane_id)
			} else {
				None
			}
		})
	});
	assert!(westend_bridge_pruned_lane_id.is_some(), "Westend BridgePruned event not found");

	let rococo_bridge_pruned_lane_id = AssetHubRococo::execute_with(|| {
		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::XcmOverAssetHubWestend::close_bridge(
			AssetHubRococoRuntimeOrigin::root(),
			Box::new(AssetHubWestendUniversalLocation::get().into()),
			1,
		));
		let events = AssetHubRococo::events();
		type RuntimeEventRococo = <AssetHubRococo as Chain>::RuntimeEvent;
		events.iter().find_map(|event| {
			if let RuntimeEventRococo::XcmOverAssetHubWestend(
				pallet_xcm_bridge::Event::BridgePruned { lane_id,.. },
			) = event
			{
				Some(*lane_id)
			} else {
				None
			}
		})
	});
	assert!(rococo_bridge_pruned_lane_id.is_some(), "Rococo BridgePruned event not found");
}
