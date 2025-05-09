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
				pallet_xcm_bridge::Event::BridgeOpened { lane_id, .. },
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
				pallet_xcm_bridge::Event::BridgeOpened { lane_id, .. },
			) = event
			{
				Some(*lane_id)
			} else {
				None
			}
		})
	});
	assert!(rococo_bridge_opened_lane_id.is_some(), "Rococo BridgeOpened event not found");

	// check the same lane ID is generated
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
				pallet_xcm_bridge::Event::BridgePruned { lane_id, .. },
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
				pallet_xcm_bridge::Event::BridgePruned { lane_id, .. },
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

#[test]
fn para_to_para_open_close_bridge_works() {
	let penpal_a_para_sovereign_account = AssetHubWestend::sovereign_account_id_of(
		AssetHubWestend::sibling_location_of(PenpalA::para_id()),
	);
	let fee_amount = ASSET_HUB_WESTEND_ED * 1000;
	let system_asset = (Parent, fee_amount);

	let call = bp_asset_hub_westend::Call::XcmOverAssetHubRococo(
		bp_xcm_bridge::XcmBridgeCall::open_bridge {
			bridge_destination_universal_location: Box::new(PenpalBUniversalLocation::get().into()),
			maybe_notify: None,
		},
	).encode();

	// wrap the call as paid execution 
	let xcm = xcm_transact_paid_execution(
		call.into(),
		OriginKind::Xcm,
		system_asset.into(),
		penpal_a_para_sovereign_account,
	);
	// send XCM from PenpalA to the AssetHubWestend
	let system_para_destination = PenpalA::sibling_location_of(AssetHubWestend::para_id());
	PenpalA::execute_with(|| {
		let root_origin = <PenpalA as Chain>::RuntimeOrigin::root();
		assert_ok!(<PenpalA as PenpalAPallet>::PolkadotXcm::send(
			root_origin,
			bx!(system_para_destination.into()),
			bx!(xcm),
		));

		PenpalA::assert_xcm_pallet_sent();
	});

	let penpal_a_bridge_opened_lane_id = AssetHubWestend::execute_with(|| {
		// check BridgeOpened event on AssetHubWestend
		let events = AssetHubWestend::events();
		type RuntimeEventWestend = <AssetHubWestend as Chain>::RuntimeEvent;
		AssetHubWestend::assert_xcmp_queue_success(None);
		events.iter().find_map(|event| {
			if let RuntimeEventWestend::XcmOverAssetHubRococo(
				pallet_xcm_bridge::Event::BridgeOpened { lane_id, .. },
			) = event
			{
				Some(*lane_id)
			} else {
				None
			}
		})
	});

	assert!(penpal_a_bridge_opened_lane_id.is_some(), "PenpalA BridgeOpened event not found");
}
