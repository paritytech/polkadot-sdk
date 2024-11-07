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
fn relay_sets_system_para_xcm_supported_version() {
	// Init tests variables
	let sudo_origin = <Westend as Chain>::RuntimeOrigin::root();
	let system_para_destination: Location = Westend::child_location_of(AssetHubWestend::para_id());

	// Relay Chain sets supported version for Asset Parachain
	Westend::execute_with(|| {
		assert_ok!(<Westend as WestendPallet>::XcmPallet::force_xcm_version(
			sudo_origin,
			bx!(system_para_destination.clone()),
			XCM_V3
		));

		type RuntimeEvent = <Westend as Chain>::RuntimeEvent;

		assert_expected_events!(
			Westend,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::SupportedVersionChanged {
					location,
					version: XCM_V3
				}) => { location: *location == system_para_destination, },
			]
		);
	});
}

#[test]
fn system_para_sets_relay_xcm_supported_version() {
	// Init test variables
	let parent_location = AssetHubWestend::parent_location();
	let force_xcm_version_call =
		<AssetHubWestend as Chain>::RuntimeCall::PolkadotXcm(pallet_xcm::Call::<
			<AssetHubWestend as Chain>::Runtime,
		>::force_xcm_version {
			location: bx!(parent_location.clone()),
			version: XCM_V3,
		})
		.encode()
		.into();

	// System Parachain sets supported version for Relay Chain through it
	Westend::send_unpaid_transact_to_parachain_as_root(
		AssetHubWestend::para_id(),
		force_xcm_version_call,
	);

	// System Parachain receive the XCM message
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		AssetHubWestend::assert_dmp_queue_complete(Some(Weight::from_parts(115_688_000, 0)));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::PolkadotXcm(pallet_xcm::Event::SupportedVersionChanged {
					location,
					version: XCM_V3
				}) => { location: *location == parent_location, },
			]
		);
	});
}
