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

mod asset_transfers;
mod send_xcm;
mod snowbridge;
mod teleport;

pub(crate) fn asset_hub_westend_location() -> Location {
	Location::new(
		2,
		[GlobalConsensus(NetworkId::Westend), Parachain(AssetHubWestend::para_id().into())],
	)
}

pub(crate) fn bridge_hub_westend_location() -> Location {
	Location::new(
		2,
		[GlobalConsensus(NetworkId::Westend), Parachain(BridgeHubWestend::para_id().into())],
	)
}

pub(crate) fn send_asset_from_asset_hub_rococo(
	destination: Location,
	(id, amount): (Location, u128),
) -> DispatchResult {
	let signed_origin =
		<AssetHubRococo as Chain>::RuntimeOrigin::signed(AssetHubRococoSender::get().into());

	let beneficiary: Location =
		AccountId32Junction { network: None, id: AssetHubWestendReceiver::get().into() }.into();

	let assets: Assets = (id, amount).into();
	let fee_asset_item = 0;

	AssetHubRococo::execute_with(|| {
		<AssetHubRococo as AssetHubRococoPallet>::PolkadotXcm::limited_reserve_transfer_assets(
			signed_origin,
			bx!(destination.into()),
			bx!(beneficiary.into()),
			bx!(assets.into()),
			fee_asset_item,
			WeightLimit::Unlimited,
		)
	})
}

pub(crate) fn assert_bridge_hub_rococo_message_accepted(expected_processed: bool) {
	BridgeHubRococo::execute_with(|| {
		type RuntimeEvent = <BridgeHubRococo as Chain>::RuntimeEvent;

		if expected_processed {
			assert_expected_events!(
				BridgeHubRococo,
				vec![
					// pay for bridge fees
					RuntimeEvent::Balances(pallet_balances::Event::Withdraw { .. }) => {},
					// message exported
					RuntimeEvent::BridgeWestendMessages(
						pallet_bridge_messages::Event::MessageAccepted { .. }
					) => {},
					// message processed successfully
					RuntimeEvent::MessageQueue(
						pallet_message_queue::Event::Processed { success: true, .. }
					) => {},
				]
			);
		} else {
			assert_expected_events!(
				BridgeHubRococo,
				vec![
					RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {
						success: false,
						..
					}) => {},
				]
			);
		}
	});
}

pub(crate) fn assert_bridge_hub_westend_message_received() {
	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			BridgeHubWestend,
			vec![
				// message sent to destination
				RuntimeEvent::XcmpQueue(
					cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }
				) => {},
			]
		);
	})
}
