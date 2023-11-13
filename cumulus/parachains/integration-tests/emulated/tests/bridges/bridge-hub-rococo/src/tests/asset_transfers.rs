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

fn create_roc_foreign_asset_on_asset_hub_westend() -> MultiLocation {
	let sudo_origin = <AssetHubWestend as Chain>::RuntimeOrigin::root();
	let alice: AccountId = AssetHubWestend::account_id_of(ALICE);
	let roc_at_asset_hub_westend =
		MultiLocation { parents: 2, interior: X1(GlobalConsensus(NetworkId::Rococo)) };

	AssetHubWestend::execute_with(|| {
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::force_create(
			sudo_origin,
			roc_at_asset_hub_westend,
			alice.clone().into(),
			true,
			ASSET_MIN_BALANCE,
		));
		assert!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::asset_exists(
			roc_at_asset_hub_westend
		));
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::ForeignAssets(pallet_assets::Event::ForceCreated {
					asset_id,
					..
				}) => { asset_id: *asset_id == roc_at_asset_hub_westend, },
			]
		);
	});
	roc_at_asset_hub_westend
}

#[test]
fn send_rocs_from_asset_hub_rococo_to_asset_hub_westend() {
	let roc_at_asset_hub_westend = create_roc_foreign_asset_on_asset_hub_westend();

	let sender_balance_before =
		<AssetHubRococo as Chain>::account_data_of(AssetHubRococoSender::get()).free;
	let receiver_rocs_before = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(roc_at_asset_hub_westend, &AssetHubWestendReceiver::get())
	});

	let signed_origin =
		<AssetHubRococo as Chain>::RuntimeOrigin::signed(AssetHubRococoSender::get().into());
	let asset_hub_westend_para_id = AssetHubWestend::para_id().into();
	let destination = MultiLocation {
		parents: 2,
		interior: X2(GlobalConsensus(NetworkId::Westend), Parachain(asset_hub_westend_para_id)),
	};
	let beneficiary_id = AssetHubWestendReceiver::get();
	let beneficiary: MultiLocation =
		AccountId32Junction { network: None, id: beneficiary_id.into() }.into();
	let amount_to_send = ASSET_HUB_ROCOCO_ED * 10;
	let assets: MultiAssets = (Parent, amount_to_send).into();
	let fee_asset_item = 0;

	// fund the AHR's SA on BHR for paying bridge transport fees
	let ahr_as_seen_by_bhr = BridgeHubRococo::sibling_location_of(AssetHubRococo::para_id());
	let sov_ahr_on_bhr = BridgeHubRococo::sovereign_account_id_of(ahr_as_seen_by_bhr);
	BridgeHubRococo::fund_accounts(vec![(sov_ahr_on_bhr.into(), 10_000_000_000_000)]);

	AssetHubRococo::execute_with(|| {
		assert_ok!(
			<AssetHubRococo as AssetHubRococoPallet>::PolkadotXcm::limited_reserve_transfer_assets(
				signed_origin,
				bx!(destination.into()),
				bx!(beneficiary.into()),
				bx!(assets.into()),
				fee_asset_item,
				WeightLimit::Unlimited,
			)
		);
	});

	BridgeHubRococo::execute_with(|| {
		type RuntimeEvent = <BridgeHubRococo as Chain>::RuntimeEvent;
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
	});
	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			BridgeHubWestend,
			vec![
				// message dispatched successfully
				RuntimeEvent::XcmpQueue(
					cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }
				) => {},
			]
		);
	});
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![
				// issue ROCs on AHW
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued {
					asset_id,
					owner,
					..
				}) => {
					asset_id: *asset_id == roc_at_asset_hub_westend,
					owner: *owner == AssetHubWestendReceiver::get(),
				},
				// message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	});

	let sender_balance_after =
		<AssetHubRococo as Chain>::account_data_of(AssetHubRococoSender::get()).free;
	let receiver_rocs_after = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(roc_at_asset_hub_westend, &AssetHubWestendReceiver::get())
	});

	// Sender's balance is reduced
	assert!(sender_balance_before > sender_balance_after);
	// Receiver's balance is increased
	assert!(receiver_rocs_after > receiver_rocs_before);
}
