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
	let sudo_origin = <Westend as Chain>::RuntimeOrigin::root();
	let destination = Westend::child_location_of(BridgeHubWestend::para_id()).into();
	let weight_limit = WeightLimit::Unlimited;
	let check_origin = None;

	let remote_xcm = Xcm(vec![ClearOrigin]);

	let xcm = VersionedXcm::from(Xcm(vec![
		UnpaidExecution { weight_limit, check_origin },
		ExportMessage {
			network: RococoId,
			destination: X1(Parachain(AssetHubRococo::para_id().into())),
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
	// Receive XCM message in Bridge Hub source Parachain
	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;

		assert_expected_events!(
			BridgeHubWestend,
			vec![
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {
					success: true,
					..
				}) => {},
				RuntimeEvent::BridgeRococoMessages(pallet_bridge_messages::Event::MessageAccepted {
					lane_id: LaneId([0, 0, 0, 2]),
					nonce: 1,
				}) => {},
			]
		);
	});
}
