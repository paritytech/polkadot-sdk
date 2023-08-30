// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use crate::*;

#[test]
fn relay_sets_system_para_xcm_supported_version() {
	// Init tests variables
	let sudo_origin = <Westend as Chain>::RuntimeOrigin::root();
	let system_para_destination: MultiLocation =
		Westend::child_location_of(AssetHubWestend::para_id());

	// Relay Chain sets supported version for Asset Parachain
	Westend::execute_with(|| {
		assert_ok!(<Westend as WestendPallet>::XcmPallet::force_xcm_version(
			sudo_origin,
			bx!(system_para_destination),
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
	let sudo_origin = <Westend as Chain>::RuntimeOrigin::root();
	let parent_location = AssetHubWestend::parent_location();
	let system_para_destination: VersionedMultiLocation =
		Westend::child_location_of(AssetHubWestend::para_id()).into();
	let call = <AssetHubWestend as Chain>::RuntimeCall::PolkadotXcm(pallet_xcm::Call::<
		<AssetHubWestend as Chain>::Runtime,
	>::force_xcm_version {
		location: bx!(parent_location),
		version: XCM_V3,
	})
	.encode()
	.into();
	let origin_kind = OriginKind::Superuser;

	let xcm = xcm_transact_unpaid_execution(call, origin_kind);

	// System Parachain sets supported version for Relay Chain throught it
	Westend::execute_with(|| {
		assert_ok!(<Westend as WestendPallet>::XcmPallet::send(
			sudo_origin,
			bx!(system_para_destination),
			bx!(xcm),
		));

		Westend::assert_xcm_pallet_sent();
	});

	// System Parachain receive the XCM message
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		AssetHubWestend::assert_dmp_queue_complete(Some(Weight::from_parts(
			1_019_210_000,
			200_000,
		)));

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
