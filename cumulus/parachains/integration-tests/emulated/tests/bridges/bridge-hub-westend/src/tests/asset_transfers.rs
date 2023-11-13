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

fn create_wnd_foreign_asset_on_asset_hub_rococo() -> MultiLocation {
	let sudo_origin = <AssetHubRococo as Chain>::RuntimeOrigin::root();
	let alice: AccountId = AssetHubRococo::account_id_of(ALICE);
	let wnd_at_asset_hub_rococo =
		MultiLocation { parents: 2, interior: X1(GlobalConsensus(NetworkId::Westend)) };

	AssetHubRococo::execute_with(|| {
		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::ForeignAssets::force_create(
			sudo_origin,
			wnd_at_asset_hub_rococo,
			alice.clone().into(),
			true,
			ASSET_MIN_BALANCE,
		));
		assert!(<AssetHubRococo as AssetHubRococoPallet>::ForeignAssets::asset_exists(
			wnd_at_asset_hub_rococo
		));
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubRococo,
			vec![
				RuntimeEvent::ForeignAssets(pallet_assets::Event::ForceCreated {
					asset_id,
					..
				}) => { asset_id: *asset_id == wnd_at_asset_hub_rococo, },
			]
		);
	});
	wnd_at_asset_hub_rococo
}

#[test]
fn send_wnds_from_asset_hub_westend_to_asset_hub_rococo() {
	let wnd_at_asset_hub_rococo = create_wnd_foreign_asset_on_asset_hub_rococo();

	let sender_balance_before =
		<AssetHubWestend as Chain>::account_data_of(AssetHubWestendSender::get()).free;
	let receiver_rocs_before = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(wnd_at_asset_hub_rococo, &AssetHubRococoReceiver::get())
	});

	let signed_origin =
		<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get().into());
	let asset_hub_rococo_para_id = AssetHubRococo::para_id().into();
	let destination = MultiLocation {
		parents: 2,
		interior: X2(GlobalConsensus(NetworkId::Rococo), Parachain(asset_hub_rococo_para_id)),
	};
	let beneficiary_id = AssetHubRococoReceiver::get();
	let beneficiary: MultiLocation =
		AccountId32Junction { network: None, id: beneficiary_id.into() }.into();
	let amount_to_send = ASSET_HUB_WESTEND_ED * 10;
	let assets: MultiAssets = (Parent, amount_to_send).into();
	let fee_asset_item = 0;

	// fund the AHW's SA on BHW for paying bridge transport fees
	let ahw_as_seen_by_bhw = BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id());
	let sov_ahw_on_bhw = BridgeHubWestend::sovereign_account_id_of(ahw_as_seen_by_bhw);
	BridgeHubWestend::fund_accounts(vec![(sov_ahw_on_bhw.into(), 10_000_000_000_000)]);

	AssetHubWestend::execute_with(|| {
		assert_ok!(
			<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::limited_reserve_transfer_assets(
				signed_origin,
				bx!(destination.into()),
				bx!(beneficiary.into()),
				bx!(assets.into()),
				fee_asset_item,
				WeightLimit::Unlimited,
			)
		);
	});

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			BridgeHubWestend,
			vec![
				// pay for bridge fees
				RuntimeEvent::Balances(pallet_balances::Event::Withdraw { .. }) => {},
				// message exported
				RuntimeEvent::BridgeRococoMessages(
					pallet_bridge_messages::Event::MessageAccepted { .. }
				) => {},
				// message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	});
	BridgeHubRococo::execute_with(|| {
		type RuntimeEvent = <BridgeHubRococo as Chain>::RuntimeEvent;
		assert_expected_events!(
			BridgeHubRococo,
			vec![
				// message dispatched successfully
				RuntimeEvent::XcmpQueue(
					cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }
				) => {},
			]
		);
	});
	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubRococo,
			vec![
				// issue ROCs on AHW
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued {
					asset_id,
					owner,
					..
				}) => {
					asset_id: *asset_id == wnd_at_asset_hub_rococo,
					owner: *owner == AssetHubRococoReceiver::get(),
				},
				// message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	});

	let sender_balance_after =
		<AssetHubWestend as Chain>::account_data_of(AssetHubWestendSender::get()).free;
	let receiver_rocs_after = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(wnd_at_asset_hub_rococo, &AssetHubRococoReceiver::get())
	});

	// Sender's balance is reduced
	assert!(sender_balance_before > sender_balance_after);
	// Receiver's balance is increased
	assert!(receiver_rocs_after > receiver_rocs_before);
}
