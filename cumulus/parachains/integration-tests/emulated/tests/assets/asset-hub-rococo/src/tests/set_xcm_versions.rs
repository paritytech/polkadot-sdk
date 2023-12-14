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
fn relay_sets_system_para_xcm_supported_version() {
	// Init tests variables
	let sudo_origin = <Rococo as Chain>::RuntimeOrigin::root();
	let system_para_destination: Location = Rococo::child_location_of(AssetHubRococo::para_id());

	// Relay Chain sets supported version for Asset Parachain
	Rococo::execute_with(|| {
		assert_ok!(<Rococo as RococoPallet>::XcmPallet::force_xcm_version(
			sudo_origin,
			bx!(system_para_destination.clone()),
			XCM_V3
		));

		type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			Rococo,
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
	let parent_location = AssetHubRococo::parent_location();
	let force_xcm_version_call =
		<AssetHubRococo as Chain>::RuntimeCall::PolkadotXcm(pallet_xcm::Call::<
			<AssetHubRococo as Chain>::Runtime,
		>::force_xcm_version {
			location: bx!(parent_location.clone()),
			version: XCM_V3,
		})
		.encode()
		.into();

	// System Parachain sets supported version for Relay Chain through it
	Rococo::send_unpaid_transact_to_parachain_as_root(
		AssetHubRococo::para_id(),
		force_xcm_version_call,
	);

	// System Parachain receive the XCM message
	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;

		AssetHubRococo::assert_dmp_queue_complete(Some(Weight::from_parts(1_019_210_000, 200_000)));

		assert_expected_events!(
			AssetHubRococo,
			vec![
				RuntimeEvent::PolkadotXcm(pallet_xcm::Event::SupportedVersionChanged {
					location,
					version: XCM_V3
				}) => { location: *location == parent_location, },
			]
		);
	});
}
