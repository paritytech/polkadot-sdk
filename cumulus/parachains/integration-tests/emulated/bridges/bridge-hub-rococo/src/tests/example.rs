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
fn example() {
	// Init tests variables
	// XcmPallet send arguments
	let sudo_origin = <Rococo as Relay>::RuntimeOrigin::root();
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

		type RuntimeEvent = <Rococo as Relay>::RuntimeEvent;

		assert_expected_events!(
			Rococo,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});
	// Receive XCM message in Bridge Hub source Parachain
	BridgeHubRococo::execute_with(|| {
		type RuntimeEvent = <BridgeHubRococo as Para>::RuntimeEvent;

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
		type RuntimeEvent = <BridgeHubWococo as Para>::RuntimeEvent;

		assert_expected_events!(
			BridgeHubWococo,
			vec![
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},
			]
		);
	});
	// Receive embeded XCM message within `ExportMessage` in Parachain destination
	AssetHubWococo::execute_with(|| {
		type RuntimeEvent = <AssetHubWococo as Para>::RuntimeEvent;

		assert_expected_events!(
			AssetHubWococo,
			vec![
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::Fail { .. }) => {},
			]
		);
	});
}
