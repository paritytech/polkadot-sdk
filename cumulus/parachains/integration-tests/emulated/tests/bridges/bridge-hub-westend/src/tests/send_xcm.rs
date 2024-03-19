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
	AssetHubRococo::force_default_xcm_version(Some(xcm::v2::prelude::XCM_VERSION));
	BridgeHubRococo::force_default_xcm_version(Some(xcm::v2::prelude::XCM_VERSION));
	BridgeHubWestend::force_default_xcm_version(Some(xcm::v2::prelude::XCM_VERSION));
	AssetHubWestend::force_default_xcm_version(Some(xcm::v2::prelude::XCM_VERSION));

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
		send_asset_from_asset_hub_westend(destination.clone(), (native_token.clone(), amount)),
		DispatchError::Module(sp_runtime::ModuleError {
			index: 31,
			error: [1, 0, 0, 0],
			message: Some("SendFailure")
		})
	);

	// set destination version
	AssetHubWestend::force_xcm_version(destination.clone(), xcm::v3::prelude::XCM_VERSION);

	// TODO: remove this block, when removing `xcm:v2`
	{
		// send XCM from AssetHubRococo - fails - AssetHubRococo is set to the default/safe `2`
		// version, which does not have the `ExportMessage` instruction. If the default `2` is
		// changed to `3`, then this assert can go away"
		assert_err!(
			send_asset_from_asset_hub_westend(destination.clone(), (native_token.clone(), amount)),
			DispatchError::Module(sp_runtime::ModuleError {
				index: 31,
				error: [1, 0, 0, 0],
				message: Some("SendFailure")
			})
		);

		// set exact version for BridgeHubWestend to `2` without `ExportMessage` instruction
		AssetHubWestend::force_xcm_version(
			ParentThen(Parachain(BridgeHubWestend::para_id().into()).into()).into(),
			xcm::v2::prelude::XCM_VERSION,
		);
		// send XCM from AssetHubWestend - fails - `ExportMessage` is not in `2`
		assert_err!(
			send_asset_from_asset_hub_westend(destination.clone(), (native_token.clone(), amount)),
			DispatchError::Module(sp_runtime::ModuleError {
				index: 31,
				error: [1, 0, 0, 0],
				message: Some("SendFailure")
			})
		);
	}

	// set version with `ExportMessage` for BridgeHubWestend
	AssetHubWestend::force_xcm_version(
		ParentThen(Parachain(BridgeHubWestend::para_id().into()).into()).into(),
		xcm::v3::prelude::XCM_VERSION,
	);
	// send XCM from AssetHubWestend - ok
	assert_ok!(send_asset_from_asset_hub_westend(
		destination.clone(),
		(native_token.clone(), amount)
	));

	// `ExportMessage` on local BridgeHub - fails - remote BridgeHub version not known
	assert_bridge_hub_westend_message_accepted(false);

	// set version for remote BridgeHub on BridgeHubWestend
	BridgeHubWestend::force_xcm_version(
		bridge_hub_rococo_location(),
		xcm::v3::prelude::XCM_VERSION,
	);
	// set version for AssetHubRococo on BridgeHubRococo
	BridgeHubRococo::force_xcm_version(
		ParentThen(Parachain(AssetHubRococo::para_id().into()).into()).into(),
		xcm::v3::prelude::XCM_VERSION,
	);

	// send XCM from AssetHubWestend - ok
	assert_ok!(send_asset_from_asset_hub_westend(
		destination.clone(),
		(native_token.clone(), amount)
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

	// TODO: remove this block, when removing `xcm:v2`
	{
		// set `2` version for remote BridgeHub on BridgeHubRococo, which does not have
		// `UniversalOrigin` and `DescendOrigin`
		BridgeHubWestend::force_xcm_version(
			bridge_hub_rococo_location(),
			xcm::v2::prelude::XCM_VERSION,
		);

		// send XCM from AssetHubWestend - ok
		assert_ok!(send_asset_from_asset_hub_westend(destination, (native_token, amount)));
		// message is not accepted on the local BridgeHub (`DestinationUnsupported`) because we
		// cannot add `UniversalOrigin` and `DescendOrigin`
		assert_bridge_hub_westend_message_accepted(false);
	}
}
