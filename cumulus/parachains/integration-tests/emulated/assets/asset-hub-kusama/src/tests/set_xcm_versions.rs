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
	let sudo_origin = <Kusama as Chain>::RuntimeOrigin::root();
	let system_para_destination: MultiLocation =
		Kusama::child_location_of(AssetHubKusama::para_id());

	// Relay Chain sets supported version for Asset Parachain
	Kusama::execute_with(|| {
		assert_ok!(<Kusama as KusamaPallet>::XcmPallet::force_xcm_version(
			sudo_origin,
			bx!(system_para_destination),
			XCM_V3
		));

		type RuntimeEvent = <Kusama as Chain>::RuntimeEvent;

		assert_expected_events!(
			Kusama,
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
	let parent_location = AssetHubKusama::parent_location();

	// System Parachain sets supported version for Relay Chain through it
	Kusama::send_transact_to_parachain(
		OriginKind::Superuser,
		AssetHubKusama::para_id(),
		<AssetHubKusama as Chain>::RuntimeCall::PolkadotXcm(pallet_xcm::Call::<
			<AssetHubKusama as Chain>::Runtime,
		>::force_xcm_version {
			location: bx!(parent_location),
			version: XCM_V3,
		})
		.encode()
		.into(),
	);

	// System Parachain receive the XCM message
	AssetHubKusama::execute_with(|| {
		type RuntimeEvent = <AssetHubKusama as Chain>::RuntimeEvent;

		AssetHubKusama::assert_dmp_queue_complete(Some(Weight::from_parts(1_019_210_000, 200_000)));

		assert_expected_events!(
			AssetHubKusama,
			vec![
				RuntimeEvent::PolkadotXcm(pallet_xcm::Event::SupportedVersionChanged {
					location,
					version: XCM_V3
				}) => { location: *location == parent_location, },
			]
		);
	});
}
