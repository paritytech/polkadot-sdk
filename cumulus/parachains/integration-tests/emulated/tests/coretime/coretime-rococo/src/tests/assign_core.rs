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
use rococo_wococo_system_emulated_network::{
	coretime_rococo_emulated_chain::CoretimeRococo, coretime_wococo_emulated_chain::CoretimeWococo,
};

#[test]
fn example() {
	// Init tests vars
	// XcmPallet send args
	let sudo_origin = <Rococo as Chain>::RuntimeOrigin::root();
	let destination = Rococo::child_location_of(CoretimeWococo::para_id()).into();
	let weight_limit = Unlimited;
	let check_origin = None;

	let remove_xcm = Xcm(vec![ClearOrigin]);

	let xcm = VersionedXcm::from(Xcm(vec![
		UnpaidExecution { weight_limit, check_origin },
		ExportMessage {
			network: WococoId,
			destination: X1(Parachain(CoretimeWococo::para_id().into())),
			xcm: remove_xcm,
		},
	]));

	//Rococo Global Consensus
	// Send XCM message from Relay Chain to Coretime source Parachain
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

	//Receive XCM message in Coretime source Parachain
	CoretimeRococo::execute_with(|| {
		type RuntimeEvent = <CoretimeRococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			CoretimeRococo,
			vec![
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {
					success: true,
					..
				}) => {},
			]
		);
	});

	//Wococo Global Conesnsus
	//Receive XCM message in Coretime target Parachain
	CoretimeRococo::execute_with(|| {
		type RuntimeEvent = <BridgeHubWococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			CoretimeWococo,
			vec![
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},
			]
		);
	});

	//Receive embedded XCM message within `ExportMessage` in Parachain destination
	CoretimeRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubWococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			CoretimeWococo,
			vec![
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::ProcessingFailed {
					..
				}) => {},
			]
		);
	});
}
