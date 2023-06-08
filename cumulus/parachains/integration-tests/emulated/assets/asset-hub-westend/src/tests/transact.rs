// Copyright Parity Technologies (UK) Ltd.
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
fn transact_sudo_from_relay_to_assets_para() {
	// Init tests variables
	// Call to be executed in Assets Parachain
	const ASSET_ID: u32 = 1;

	let call = <AssetHubWestend as Para>::RuntimeCall::Assets(pallet_assets::Call::<
		<AssetHubWestend as Para>::Runtime,
		Instance1,
	>::force_create {
		id: ASSET_ID.into(),
		is_sufficient: true,
		min_balance: 1000,
		owner: AssetHubWestendSender::get().into(),
	})
	.encode()
	.into();

	// XcmPallet send arguments
	let sudo_origin = <Westend as Relay>::RuntimeOrigin::root();
	let assets_para_destination: VersionedMultiLocation =
		Westend::child_location_of(AssetHubWestend::para_id()).into();

	let weight_limit = WeightLimit::Unlimited;
	let require_weight_at_most = Weight::from_parts(1000000000, 200000);
	let origin_kind = OriginKind::Superuser;
	let check_origin = None;

	let xcm = VersionedXcm::from(Xcm(vec![
		UnpaidExecution { weight_limit, check_origin },
		Transact { require_weight_at_most, origin_kind, call },
	]));

	// Send XCM message from Relay Chain
	Westend::execute_with(|| {
		assert_ok!(<Westend as WestendPallet>::XcmPallet::send(
			sudo_origin,
			bx!(assets_para_destination),
			bx!(xcm),
		));

		type RuntimeEvent = <Westend as Relay>::RuntimeEvent;

		assert_expected_events!(
			Westend,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	// Receive XCM message in Assets Parachain
	AssetHubWestend::execute_with(|| {
		assert!(<AssetHubWestend as AssetHubWestendPallet>::Assets::asset_exists(ASSET_ID));
	});
}
