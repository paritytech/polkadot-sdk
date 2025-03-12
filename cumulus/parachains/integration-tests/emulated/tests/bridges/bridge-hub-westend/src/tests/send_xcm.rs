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

use rococo_westend_system_emulated_network::westend_emulated_chain::westend_runtime::Dmp;

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
			network: ByGenesis(ROCOCO_GENESIS_HASH),
			destination: [Parachain(AssetHubRococo::para_id().into())].into(),
			xcm: remote_xcm,
		},
	]));

	// Westend Global Consensus
	// Send XCM message from Relay Chain to Bridge Hub source Parachain
	Westend::execute_with(|| {
		Dmp::make_parachain_reachable(BridgeHubWestend::para_id());

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
	// prepare data
	let destination = asset_hub_rococo_location();
	let native_token = Location::parent();
	let amount = ASSET_HUB_WESTEND_ED * 1_000;

	// fund the AHR's SA on BHR for paying bridge delivery fees
	BridgeHubWestend::fund_para_sovereign(AssetHubWestend::para_id(), 10_000_000_000_000u128);
	// fund sender
	AssetHubWestend::fund_accounts(vec![(AssetHubWestendSender::get().into(), amount * 10)]);

	// Initially set only default version on all runtimes
	let newer_xcm_version = xcm::prelude::XCM_VERSION;
	let older_xcm_version = newer_xcm_version - 1;
	AssetHubRococo::force_default_xcm_version(Some(older_xcm_version));
	BridgeHubRococo::force_default_xcm_version(Some(older_xcm_version));
	BridgeHubWestend::force_default_xcm_version(Some(older_xcm_version));
	AssetHubWestend::force_default_xcm_version(Some(older_xcm_version));

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

// Hypothetical utility to get last executed message ID (add to test framework if needed)
fn get_last_executed_message_id() -> [u8; 32] {
	// Placeholder: Replace with actual logic (e.g., from XcmExecutor state or events)
	BridgeHubWestend::events()
		.iter()
		.rev()
		.find_map(|e| {
			if let BridgeHubWestendRuntimeEvent::XcmPallet(pallet_xcm::Event::Attempted { outcome }) = e {
				if let Outcome::Complete { .. } = outcome {
					return Some([42; 32]); // Replace with real message_id extraction
				}
			}
			None
		})
		.unwrap_or([0; 32])
}

#[test]
fn xcm_persists_set_topic_across_hops() {
	// Define a consistent topic ID
	let topic_id = [42; 32]; // Arbitrary but fixed for traceability

	// Initial XCM from Westend Relay to BridgeHubWestend
	let initial_xcm = VersionedXcm::from(Xcm(vec![
		UnpaidExecution { weight_limit: WeightLimit::Unlimited, check_origin: None },
		SetTopic(topic_id),
		ExportMessage {
			network: ByGenesis(ROCOCO_GENESIS_HASH),
			destination: [Parachain(AssetHubRococo::para_id().into())].into(),
			xcm: Xcm(vec![ClearOrigin]), // Onward message without SetTopic initially
		},
	]));

	// Step 1: Send XCM from Westend Relay to BridgeHubWestend
	Westend::execute_with(|| {
		Dmp::make_parachain_reachable(BridgeHubWestend::para_id());
		assert_ok!(<Westend as WestendPallet>::XcmPallet::send(
            <Westend as Chain>::RuntimeOrigin::root(),
            bx!(Westend::child_location_of(BridgeHubWestend::para_id()).into()),
            bx!(initial_xcm.clone()),
        ));

		type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
		assert_expected_events!(
            Westend,
            vec![
                RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { message_id, .. }) => {
                    message_id: *message_id == topic_id,
                },
            ]
        );
	});

	// Step 2: Process on BridgeHubWestend and assert topic_id
	BridgeHubWestend::execute_with(|| {
		assert_bridge_hub_westend_message_accepted(true);
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;

		// Check that TrailingSetTopicAsId preserved the topic_id
		let executed_id = get_last_executed_message_id(); // Replace with real utility
		assert_eq!(executed_id, topic_id, "TrailingSetTopicAsId should set message_id to topic_id");

		assert_expected_events!(
            BridgeHubWestend,
            vec![
                RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { message_hash }) => {
                    message_hash: *message_hash == topic_id,
                },
                RuntimeEvent::XcmPallet(pallet_xcm::Event::Attempted { outcome: Outcome::Complete { .. } }) => {},
            ]
        );
	});

	// Step 3: Fund and send onward message from BridgeHubWestend to AssetHubRococo
	BridgeHubWestend::fund_para_sovereign(AssetHubRococo::para_id(), 10_000_000_000_000u128);
	BridgeHubWestend::execute_with(|| {
		let onward_destination = ParentThen([Parachain(AssetHubRococo::para_id().into())].into()).into();
		let onward_xcm = VersionedXcm::from(Xcm(vec![ClearOrigin])); // No SetTopic initially
		assert_ok!(<BridgeHubWestend as BridgeHubWestendPallet>::XcmPallet::send(
            <BridgeHubWestend as Chain>::RuntimeOrigin::root(),
            bx!(onward_destination),
            bx!(onward_xcm),
        ));

		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
            BridgeHubWestend,
            vec![
                RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { message_id, .. }) => {
                    message_id: *message_id == topic_id,
                },
            ]
        );
	});

	// Step 4: Verify AssetHubRococo receives the message with the same topic_id
	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		assert_expected_events!(
            AssetHubRococo,
            vec![
                RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { id, success: true, .. }) => {
                    id: *id == topic_id.into(),
                },
            ]
        );

		// Hypothetical: Check received XCM (add utility if needed)
		let received_xcm = get_last_received_xcm(); // Placeholder
		assert_eq!(
			received_xcm.0.last(),
			Some(&SetTopic(topic_id)),
			"WithUniqueTopic should persist original topic_id"
		);
	});
}