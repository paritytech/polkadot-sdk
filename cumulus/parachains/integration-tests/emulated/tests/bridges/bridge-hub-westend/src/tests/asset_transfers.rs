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

fn send_asset_from_asset_hub_westend_to_asset_hub_rococo(id: MultiLocation, amount: u128) {
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
	let assets: MultiAssets = (id, amount).into();
	let fee_asset_item = 0;

	// fund the AHW's SA on BHW for paying bridge transport fees
	let ahw_as_seen_by_bhw = BridgeHubWestend::sibling_location_of(AssetHubWestend::para_id());
	let sov_ahw_on_bhw = BridgeHubWestend::sovereign_account_id_of(ahw_as_seen_by_bhw);
	BridgeHubWestend::fund_accounts(vec![(sov_ahw_on_bhw.into(), 10_000_000_000_000u128)]);

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
}

#[test]
fn send_wnds_from_asset_hub_westend_to_asset_hub_rococo() {
	let wnd_at_asset_hub_westend: MultiLocation = Parent.into();
	let wnd_at_asset_hub_rococo =
		MultiLocation { parents: 2, interior: X1(GlobalConsensus(NetworkId::Westend)) };
	let owner: AccountId = AssetHubRococo::account_id_of(ALICE);
	AssetHubRococo::force_create_foreign_asset(
		wnd_at_asset_hub_rococo,
		owner,
		true,
		ASSET_MIN_BALANCE,
		vec![],
	);

	let sender_wnds_before =
		<AssetHubWestend as Chain>::account_data_of(AssetHubWestendSender::get()).free;
	let receiver_wnds_before = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(wnd_at_asset_hub_rococo, &AssetHubRococoReceiver::get())
	});

	let amount = ASSET_HUB_WESTEND_ED * 1_000;
	send_asset_from_asset_hub_westend_to_asset_hub_rococo(wnd_at_asset_hub_westend, amount);
	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubRococo,
			vec![
				// issue WNDs on AHR
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
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

	let sender_wnds_after =
		<AssetHubWestend as Chain>::account_data_of(AssetHubWestendSender::get()).free;
	let receiver_wnds_after = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(wnd_at_asset_hub_rococo, &AssetHubRococoReceiver::get())
	});

	// Sender's balance is reduced
	assert!(sender_wnds_before > sender_wnds_after);
	// Receiver's balance is increased
	assert!(receiver_wnds_after > receiver_wnds_before);
}

#[test]
fn send_rocs_from_asset_hub_westend_to_asset_hub_rococo() {
	let prefund_amount = 10_000_000_000_000u128;
	let roc_at_asset_hub_westend =
		MultiLocation { parents: 2, interior: X1(GlobalConsensus(NetworkId::Rococo)) };
	let owner: AccountId = AssetHubWestend::account_id_of(ALICE);
	AssetHubWestend::force_create_foreign_asset(
		roc_at_asset_hub_westend,
		owner,
		true,
		ASSET_MIN_BALANCE,
		vec![(AssetHubWestendSender::get(), prefund_amount)],
	);

	// fund the AHW's SA on AHR with the ROC tokens held in reserve
	let ahw_as_seen_by_ahr = MultiLocation {
		parents: 2,
		interior: X2(
			GlobalConsensus(NetworkId::Westend),
			Parachain(AssetHubWestend::para_id().into()),
		),
	};
	let sov_ahw_on_ahr = AssetHubRococo::execute_with(|| {
		AssetHubRococo::sovereign_account_id_of(ahw_as_seen_by_ahr)
	});
	AssetHubRococo::fund_accounts(vec![(sov_ahw_on_ahr.clone(), prefund_amount)]);

	let sender_rocs_before = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(roc_at_asset_hub_westend, &AssetHubWestendSender::get())
	});
	assert_eq!(sender_rocs_before, prefund_amount);
	let receiver_rocs_before =
		<AssetHubRococo as Chain>::account_data_of(AssetHubRococoReceiver::get()).free;

	let amount_to_send = ASSET_HUB_ROCOCO_ED * 1_000;
	send_asset_from_asset_hub_westend_to_asset_hub_rococo(roc_at_asset_hub_westend, amount_to_send);
	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubRococo,
			vec![
				// ROC is withdrawn from AHW's SA on AHR
				RuntimeEvent::Balances(
					pallet_balances::Event::Withdraw { who, amount }
				) => {
					who: *who == sov_ahw_on_ahr,
					amount: *amount == amount_to_send,
				},
				// ROCs deposited to beneficiary
				RuntimeEvent::Balances(pallet_balances::Event::Deposit { who, .. }) => {
					who: *who == AssetHubRococoReceiver::get(),
				},
				// message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	});

	let sender_rocs_after = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(roc_at_asset_hub_westend, &AssetHubWestendSender::get())
	});
	let receiver_rocs_after =
		<AssetHubRococo as Chain>::account_data_of(AssetHubRococoReceiver::get()).free;

	// Sender's balance is reduced
	assert!(sender_rocs_before > sender_rocs_after);
	// Receiver's balance is increased
	assert!(receiver_rocs_after > receiver_rocs_before);
}
