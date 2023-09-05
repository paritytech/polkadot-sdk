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

use crate::*;

#[test]
fn example() {
	// Init tests variables
	// XcmPallet send arguments
	let sudo_origin = <Rococo as Chain>::RuntimeOrigin::root();
	let destination = Rococo::child_location_of(BridgeHubRococo::para_id()).into();
	let weight_limit = WeightLimit::Unlimited;
	let check_origin = None;

	let remote_xcm = Xcm(vec![ClearOrigin]);

	let xcm = VersionedXcm::from(Xcm(vec![
		UnpaidExecution { weight_limit, check_origin },
		ExportMessage {
			network: WococoId,
			destination: X1(Parachain(AssetHubWococo::para_id().into())),
			xcm: remote_xcm,
		},
	]));

	//Rococo Global Consensus
	// Send XCM message from Relay Chain to Bridge Hub source Parachain
	Rococo::execute_with(|| {
		assert_ok!(<Rococo as RococoPallet>::XcmPallet::send(
			sudo_origin,
			bx!(destination),
			bx!(xcm),
		));

		type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			Rococo,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});
	// Receive XCM message in Bridge Hub source Parachain
	BridgeHubRococo::execute_with(|| {
		type RuntimeEvent = <BridgeHubRococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			BridgeHubRococo,
			vec![
				RuntimeEvent::DmpQueue(cumulus_pallet_dmp_queue::Event::ExecutedDownward {
					outcome: Outcome::Complete(_),
					..
				}) => {},
				RuntimeEvent::BridgeWococoMessages(pallet_bridge_messages::Event::MessageAccepted {
					lane_id: LaneId([0, 0, 0, 1]),
					nonce: 1,
				}) => {},
			]
		);
	});

	// Wococo GLobal Consensus
	// Receive XCM message in Bridge Hub target Parachain
	BridgeHubWococo::execute_with(|| {
		type RuntimeEvent = <BridgeHubWococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			BridgeHubWococo,
			vec![
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},
			]
		);
	});
	// Receive embeded XCM message within `ExportMessage` in Parachain destination
	AssetHubWococo::execute_with(|| {
		type RuntimeEvent = <AssetHubWococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			AssetHubWococo,
			vec![
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::Fail { .. }) => {},
			]
		);
	});
}
