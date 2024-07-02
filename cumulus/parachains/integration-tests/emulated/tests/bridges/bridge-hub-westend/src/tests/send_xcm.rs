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

use crate::tests::*;

#[test]
fn send_xcm_from_westend_relay_to_rococo_asset_hub_should_fail_on_not_applicable() {
	// Init tests variables
	// XcmPallet send arguments
	let sudo_origin = <Westend as Chain>::RuntimeOrigin::root();
	let destination = Westend::child_location_of(BridgeHubWestend::para_id()).into();
	let weight_limit = WeightLimit::Unlimited;
	let check_origin = None;

	let remote_xcm = Xcm(vec![ClearOrigin]);

	let xcm = VersionedXcm::from(Xcm(vec![
		UnpaidExecution { weight_limit, check_origin },
		ExportMessage {
			network: RococoId,
			destination: [Parachain(AssetHubRococo::para_id().into())].into(),
			xcm: remote_xcm,
		},
	]));

	// Westend Global Consensus
	// Send XCM message from Relay Chain to Bridge Hub source Parachain
	Westend::execute_with(|| {
		assert_ok!(<Westend as WestendPallet>::XcmPallet::send(
			sudo_origin,
			bx!(destination),
			bx!(xcm),
		));

		type RuntimeEvent = <Westend as Chain>::RuntimeEvent;

		assert_expected_events!(
			Westend,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});
	// Receive XCM message in Bridge Hub source Parachain, it should fail, because we don't have
	// opened bridge/lane.
	assert_bridge_hub_westend_message_accepted(false);
}

#[test]
fn send_xcm_through_opened_lane_with_different_xcm_version_on_hops_works() {
	// Initially set only default version on all runtimes
	let newer_xcm_version = xcm::prelude::XCM_VERSION;
	let older_xcm_version = newer_xcm_version - 1;

	AssetHubRococo::force_default_xcm_version(Some(older_xcm_version));
	BridgeHubRococo::force_default_xcm_version(Some(older_xcm_version));
	BridgeHubWestend::force_default_xcm_version(Some(older_xcm_version));
	AssetHubWestend::force_default_xcm_version(Some(older_xcm_version));

	// prepare data
	let destination = asset_hub_rococo_location();
	let native_token = Location::parent();
	let amount = ASSET_HUB_WESTEND_ED * 1_000;

	// fund the AHR's SA on BHR for paying bridge transport fees
	BridgeHubWestend::fund_para_sovereign(AssetHubWestend::para_id(), 10_000_000_000_000u128);
	// fund sender
	AssetHubWestend::fund_accounts(vec![(AssetHubWestendSender::get().into(), amount * 10)]);

	// send XCM from AssetHubWestend - fails - destination version not known
	assert_err!(
		send_assets_from_asset_hub_westend(
			destination.clone(),
			(native_token.clone(), amount).into(),
			0
		),
		DispatchError::Module(sp_runtime::ModuleError {
			index: 31,
			error: [1, 0, 0, 0],
			message: Some("SendFailure")
		})
	);

	// set destination version
	AssetHubWestend::force_xcm_version(destination.clone(), newer_xcm_version);

	// set version with `ExportMessage` for BridgeHubWestend
	AssetHubWestend::force_xcm_version(
		ParentThen(Parachain(BridgeHubWestend::para_id().into()).into()).into(),
		newer_xcm_version,
	);
	// send XCM from AssetHubWestend - ok
	assert_ok!(send_assets_from_asset_hub_westend(
		destination.clone(),
		(native_token.clone(), amount).into(),
		0
	));

	// `ExportMessage` on local BridgeHub - fails - remote BridgeHub version not known
	assert_bridge_hub_westend_message_accepted(false);

	// set version for remote BridgeHub on BridgeHubWestend
	BridgeHubWestend::force_xcm_version(bridge_hub_rococo_location(), newer_xcm_version);
	// set version for AssetHubRococo on BridgeHubRococo
	BridgeHubRococo::force_xcm_version(
		ParentThen(Parachain(AssetHubRococo::para_id().into()).into()).into(),
		newer_xcm_version,
	);

	// send XCM from AssetHubWestend - ok
	assert_ok!(send_assets_from_asset_hub_westend(
		destination.clone(),
		(native_token.clone(), amount).into(),
		0
	));
	assert_bridge_hub_westend_message_accepted(true);
	assert_bridge_hub_rococo_message_received();
	// message delivered and processed at destination
	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubRococo,
			vec![
				// message processed with failure, but for this scenario it is ok, important is that was delivered
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: false, .. }
				) => {},
			]
		);
	});
}
