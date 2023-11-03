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
use hex_literal::hex;
use integration_tests_common::BridgeHubRococoPallet;
use snowbridge_control;
use snowbridge_router_primitives::inbound::{Command, MessageV1, VersionedMessage};

#[test]
fn create_agent() {
	BridgeHubRococo::fund_accounts(vec![(
		BridgeHubRococo::sovereign_account_id_of(MultiLocation {
			parents: 1,
			interior: X1(Parachain(1000)),
		}),
		5_000_000 * ROCOCO_ED,
	)]);

	let sudo_origin = <Rococo as Chain>::RuntimeOrigin::root();
	let destination = Rococo::child_location_of(BridgeHubRococo::para_id()).into();

	let remote_xcm = VersionedXcm::from(Xcm(vec![
		UnpaidExecution { weight_limit: Unlimited, check_origin: None },
		DescendOrigin(X1(Parachain(1000))),
		Transact {
			require_weight_at_most: 3000000000.into(),
			origin_kind: OriginKind::Xcm,
			call: vec![51, 1].into(),
		},
	]));

	//Rococo Global Consensus
	// Send XCM message from Relay Chain to Bridge Hub source Parachain
	Rococo::execute_with(|| {
		assert_ok!(<Rococo as RococoPallet>::XcmPallet::send(
			sudo_origin,
			bx!(destination),
			bx!(remote_xcm),
		));

		type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			Rococo,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	BridgeHubRococo::execute_with(|| {
		type RuntimeEvent = <BridgeHubRococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			BridgeHubRococo,
			vec![
				RuntimeEvent::DmpQueue(cumulus_pallet_dmp_queue::Event::ExecutedDownward {
					outcome: Outcome::Complete(_),
					..
				}) => {},
				RuntimeEvent::EthereumControl(snowbridge_control::Event::CreateAgent {
					..
				}) => {},
			]
		);
	});
}

#[test]
fn create_channel() {
	let source_location = MultiLocation { parents: 1, interior: X1(Parachain(1000)) };

	BridgeHubRococo::fund_accounts(vec![(
		BridgeHubRococo::sovereign_account_id_of(source_location),
		5_000_000 * ROCOCO_ED,
	)]);

	let sudo_origin = <Rococo as Chain>::RuntimeOrigin::root();
	let destination: VersionedMultiLocation =
		Rococo::child_location_of(BridgeHubRococo::para_id()).into();

	let create_agent_xcm = VersionedXcm::from(Xcm(vec![
		UnpaidExecution { weight_limit: Unlimited, check_origin: None },
		DescendOrigin(X1(Parachain(1000))),
		Transact {
			require_weight_at_most: 3000000000.into(),
			origin_kind: OriginKind::Xcm,
			call: vec![51, 1].into(),
		},
	]));

	let create_channel_xcm = VersionedXcm::from(Xcm(vec![
		UnpaidExecution { weight_limit: Unlimited, check_origin: None },
		DescendOrigin(X1(Parachain(1000))),
		Transact {
			require_weight_at_most: 3000000000.into(),
			origin_kind: OriginKind::Xcm,
			call: vec![51, 2].into(),
		},
	]));

	//Rococo Global Consensus
	// Send XCM message from Relay Chain to Bridge Hub source Parachain
	Rococo::execute_with(|| {
		assert_ok!(<Rococo as RococoPallet>::XcmPallet::send(
			sudo_origin.clone(),
			bx!(destination.clone()),
			bx!(create_agent_xcm),
		));

		assert_ok!(<Rococo as RococoPallet>::XcmPallet::send(
			sudo_origin,
			bx!(destination),
			bx!(create_channel_xcm),
		));

		type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			Rococo,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	BridgeHubRococo::execute_with(|| {
		type RuntimeEvent = <BridgeHubRococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			BridgeHubRococo,
			vec![
				RuntimeEvent::DmpQueue(cumulus_pallet_dmp_queue::Event::ExecutedDownward {
					outcome: Outcome::Complete(_),
					..
				}) => {},
				RuntimeEvent::EthereumControl(snowbridge_control::Event::CreateChannel {
					..
				}) => {},
			]
		);
	});
}

#[test]
fn register_token() {
	BridgeHubRococo::fund_accounts(vec![(
		BridgeHubRococo::sovereign_account_id_of(MultiLocation {
			parents: 1,
			interior: X1(Parachain(1000)),
		}),
		5_000_000 * ROCOCO_ED,
	)]);

	// Fund gateway sovereign in asset hub
	AssetHubRococo::fund_accounts(vec![(
		hex!("c9794dd8013efb2ad83f668845c62b373c16ad33971745731408058e4d0c6ff5").into(),
		5_000_000_000_000 * ROCOCO_ED,
	)]);

	BridgeHubRococo::execute_with(|| {
		type RuntimeEvent = <BridgeHubRococo as Chain>::RuntimeEvent;
		type EthereumInboundQueue =
			<BridgeHubRococo as BridgeHubRococoPallet>::EthereumInboundQueue;
		let message = VersionedMessage::V1(MessageV1 {
			chain_id: 15,
			command: Command::RegisterToken {
				gateway: hex!("EDa338E4dC46038493b885327842fD3E301CaB39").into(),
				token: hex!("87d1f7fdfEe7f651FaBc8bFCB6E086C278b77A7d").into(),
			},
		});
		let xcm = EthereumInboundQueue::do_convert(message).unwrap();
		let _ = EthereumInboundQueue::send_xcm(xcm, 1000.into()).unwrap();

		assert_expected_events!(
			BridgeHubRococo,
			vec![
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},
			]
		);
	});

	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			AssetHubRococo,
			vec![
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Created { .. }) => {},
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::Success { .. }) => {},
			]
		);
	});
}
