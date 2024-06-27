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
use crate::tests::*;

fn send_assets_from_asset_hub_westend_to_asset_hub_rococo(assets: Assets, fee_idx: u32) {
	let destination = asset_hub_rococo_location();

	// fund the AHW's SA on BHW for paying bridge transport fees
	BridgeHubWestend::fund_para_sovereign(AssetHubWestend::para_id(), 10_000_000_000_000u128);

	// set XCM versions
	AssetHubWestend::force_xcm_version(destination.clone(), XCM_VERSION);
	BridgeHubWestend::force_xcm_version(bridge_hub_rococo_location(), XCM_VERSION);

	// send message over bridge
	assert_ok!(send_assets_from_asset_hub_westend(destination, assets, fee_idx));
	assert_bridge_hub_westend_message_accepted(true);
	assert_bridge_hub_rococo_message_received();
}

fn send_asset_from_penpal_westend_through_local_asset_hub_to_rococo_asset_hub(
	id: Location,
	transfer_amount: u128,
) {
	let destination = asset_hub_rococo_location();
	let local_asset_hub: Location = PenpalB::sibling_location_of(AssetHubWestend::para_id());
	let sov_penpal_on_ahw = AssetHubWestend::sovereign_account_id_of(
		AssetHubWestend::sibling_location_of(PenpalB::para_id()),
	);
	let sov_ahr_on_ahw = AssetHubWestend::sovereign_account_of_parachain_on_other_global_consensus(
		Rococo,
		AssetHubRococo::para_id(),
	);

	// fund the AHW's SA on BHW for paying bridge transport fees
	BridgeHubWestend::fund_para_sovereign(AssetHubWestend::para_id(), 10_000_000_000_000u128);

	// set XCM versions
	PenpalB::force_xcm_version(local_asset_hub.clone(), XCM_VERSION);
	AssetHubWestend::force_xcm_version(destination.clone(), XCM_VERSION);
	BridgeHubWestend::force_xcm_version(bridge_hub_rococo_location(), XCM_VERSION);

	// send message over bridge
	assert_ok!(PenpalB::execute_with(|| {
		let signed_origin = <PenpalB as Chain>::RuntimeOrigin::signed(PenpalBSender::get());
		let beneficiary: Location =
			AccountId32Junction { network: None, id: AssetHubRococoReceiver::get().into() }.into();
		let assets: Assets = (id.clone(), transfer_amount).into();
		let fees_id: AssetId = id.into();
		let custom_xcm_on_dest = Xcm::<()>(vec![DepositAsset {
			assets: Wild(AllCounted(assets.len() as u32)),
			beneficiary,
		}]);

		<PenpalB as PenpalBPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
			signed_origin,
			bx!(destination.into()),
			bx!(assets.into()),
			bx!(TransferType::RemoteReserve(local_asset_hub.clone().into())),
			bx!(fees_id.into()),
			bx!(TransferType::RemoteReserve(local_asset_hub.into())),
			bx!(VersionedXcm::from(custom_xcm_on_dest)),
			WeightLimit::Unlimited,
		)
	}));
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![
				// Amount to reserve transfer is withdrawn from Penpal's sovereign account
				RuntimeEvent::Balances(
					pallet_balances::Event::Burned { who, amount }
				) => {
					who: *who == sov_penpal_on_ahw.clone().into(),
					amount: *amount == transfer_amount,
				},
				// Amount deposited in AHR's sovereign account
				RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. }) => {
					who: *who == sov_ahr_on_ahw.clone().into(),
				},
				RuntimeEvent::XcmpQueue(
					cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }
				) => {},
			]
		);
	});
	assert_bridge_hub_westend_message_accepted(true);
	assert_bridge_hub_rococo_message_received();
}

#[test]
fn send_wnds_from_asset_hub_westend_to_asset_hub_rococo() {
	let amount = ASSET_HUB_WESTEND_ED * 1_000;
	let sender = AssetHubWestendSender::get();
	let receiver = AssetHubRococoReceiver::get();
	let wnd_at_asset_hub_westend = wnd_at_ah_westend();
	let bridged_wnd_at_asset_hub_rococo = bridged_wnd_at_ah_rococo().try_into().unwrap();
	create_foreign_on_ah_rococo(bridged_wnd_at_asset_hub_rococo, true);

	set_up_pool_with_roc_on_ah_rococo(bridged_wnd_at_asset_hub_rococo);

	////////////////////////////////////////////////////////////
	// Let's first send over just some WNDs as a simple example
	////////////////////////////////////////////////////////////
	let sov_ahr_on_ahw = AssetHubWestend::sovereign_account_of_parachain_on_other_global_consensus(
		Rococo,
		AssetHubRococo::para_id(),
	);
	let wnds_in_reserve_on_ahw_before =
		<AssetHubWestend as Chain>::account_data_of(sov_ahr_on_ahw.clone()).free;
	let sender_wnds_before = <AssetHubWestend as Chain>::account_data_of(sender.clone()).free;
	let receiver_wnds_before =
		foreign_balance_on_ah_rococo(bridged_wnd_at_asset_hub_rococo, &receiver);

	// send WNDs, use them for fees
	let assets: Assets = (wnd_at_asset_hub_westend, amount).into();
	let fee_index = 0;
	send_assets_from_asset_hub_westend_to_asset_hub_rococo(assets, fee_index);

	// verify expected events on final destination
	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubRococo,
			vec![
				// issue WNDs on AHR
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
					asset_id: *asset_id == bridged_wnd_at_asset_hub_rococo,
					owner: *owner == receiver,
				},
				// message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	});

	let sender_wnds_after = <AssetHubWestend as Chain>::account_data_of(sender).free;
	let receiver_wnds_after =
		foreign_balance_on_ah_rococo(bridged_wnd_at_asset_hub_rococo, &receiver);
	let wnds_in_reserve_on_ahw_after =
		<AssetHubWestend as Chain>::account_data_of(sov_ahr_on_ahw).free;

	// Sender's balance is reduced
	assert!(sender_wnds_before > sender_wnds_after);
	// Receiver's balance is increased
	assert!(receiver_wnds_after > receiver_wnds_before);
	// Reserve balance is increased by sent amount
	assert_eq!(wnds_in_reserve_on_ahw_after, wnds_in_reserve_on_ahw_before + amount);
}

#[test]
fn send_rocs_from_asset_hub_westend_to_asset_hub_rococo() {
	let prefund_amount = 10_000_000_000_000u128;
	let sender = AssetHubWestendSender::get();
	let receiver = AssetHubRococoReceiver::get();
	let bridged_roc_at_asset_hub_westend = bridged_roc_at_ah_westend();
	let bridged_roc_at_asset_hub_westend_v3 =
		bridged_roc_at_asset_hub_westend.clone().try_into().unwrap();
	AssetHubWestend::force_create_foreign_asset(
		bridged_roc_at_asset_hub_westend_v3,
		AssetHubWestend::account_id_of(ALICE),
		true,
		ASSET_MIN_BALANCE,
		vec![(sender.clone(), prefund_amount)],
	);

	// fund the AHW's SA on AHR with the ROC tokens held in reserve
	let sov_ahw_on_ahr = AssetHubRococo::sovereign_account_of_parachain_on_other_global_consensus(
		Westend,
		AssetHubWestend::para_id(),
	);
	AssetHubRococo::fund_accounts(vec![(sov_ahw_on_ahr.clone(), prefund_amount)]);

	let rocs_in_reserve_on_ahr_before =
		<AssetHubRococo as Chain>::account_data_of(sov_ahw_on_ahr.clone()).free;
	assert_eq!(rocs_in_reserve_on_ahr_before, prefund_amount);

	let sender_rocs_before =
		foreign_balance_on_ah_westend(bridged_roc_at_asset_hub_westend_v3, &sender);
	assert_eq!(sender_rocs_before, prefund_amount);
	let receiver_rocs_before = <AssetHubRococo as Chain>::account_data_of(receiver.clone()).free;

	let amount_to_send = ASSET_HUB_ROCOCO_ED * 1_000;
	send_assets_from_asset_hub_westend_to_asset_hub_rococo(
		(bridged_roc_at_asset_hub_westend, amount_to_send).into(),
		0,
	);
	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubRococo,
			vec![
				// ROC is withdrawn from AHW's SA on AHR
				RuntimeEvent::Balances(
					pallet_balances::Event::Burned { who, amount }
				) => {
					who: *who == sov_ahw_on_ahr,
					amount: *amount == amount_to_send,
				},
				// ROCs deposited to beneficiary
				RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. }) => {
					who: *who == receiver,
				},
				// message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	});

	let sender_rocs_after =
		foreign_balance_on_ah_westend(bridged_roc_at_asset_hub_westend_v3, &sender);
	let receiver_rocs_after = <AssetHubRococo as Chain>::account_data_of(receiver).free;
	let rocs_in_reserve_on_ahr_after =
		<AssetHubRococo as Chain>::account_data_of(sov_ahw_on_ahr.clone()).free;

	// Sender's balance is reduced
	assert!(sender_rocs_before > sender_rocs_after);
	// Receiver's balance is increased
	assert!(receiver_rocs_after > receiver_rocs_before);
	// Reserve balance is reduced by sent amount
	assert_eq!(rocs_in_reserve_on_ahr_after, rocs_in_reserve_on_ahr_before - amount_to_send);
}

#[test]
fn send_wnds_from_penpal_westend_through_asset_hub_westend_to_asset_hub_rococo() {
	let wnd_at_westend_parachains = wnd_at_ah_westend();
	let wnd_at_asset_hub_rococo = bridged_wnd_at_ah_rococo().try_into().unwrap();
	create_foreign_on_ah_rococo(wnd_at_asset_hub_rococo, true);

	let amount = ASSET_HUB_WESTEND_ED * 10_000_000;
	let sender = PenpalBSender::get();
	let receiver = AssetHubRococoReceiver::get();
	let penpal_location = AssetHubWestend::sibling_location_of(PenpalB::para_id());
	let sov_penpal_on_ahw = AssetHubWestend::sovereign_account_id_of(penpal_location);
	// fund Penpal's sovereign account on AssetHub
	AssetHubWestend::fund_accounts(vec![(sov_penpal_on_ahw.into(), amount * 2)]);
	// fund Penpal's sender account
	PenpalB::mint_foreign_asset(
		<PenpalB as Chain>::RuntimeOrigin::signed(PenpalAssetOwner::get()),
		wnd_at_westend_parachains.clone(),
		sender.clone(),
		amount * 2,
	);

	let sov_ahr_on_ahw = AssetHubWestend::sovereign_account_of_parachain_on_other_global_consensus(
		Rococo,
		AssetHubRococo::para_id(),
	);
	let wnds_in_reserve_on_ahw_before =
		<AssetHubWestend as Chain>::account_data_of(sov_ahr_on_ahw.clone()).free;
	let sender_wnds_before = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(wnd_at_westend_parachains.clone(), &sender)
	});
	let receiver_wnds_before = foreign_balance_on_ah_rococo(wnd_at_asset_hub_rococo, &receiver);

	// Send WNDs over bridge
	send_asset_from_penpal_westend_through_local_asset_hub_to_rococo_asset_hub(
		wnd_at_westend_parachains.clone(),
		amount,
	);

	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubRococo,
			vec![
				// issue WNDs on AHR
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
					asset_id: *asset_id == wnd_at_westend_parachains.clone().try_into().unwrap(),
					owner: owner == &receiver,
				},
				// message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	});

	let sender_wnds_after = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(wnd_at_westend_parachains, &sender)
	});
	let receiver_wnds_after = foreign_balance_on_ah_rococo(wnd_at_asset_hub_rococo, &receiver);
	let wnds_in_reserve_on_ahw_after =
		<AssetHubWestend as Chain>::account_data_of(sov_ahr_on_ahw.clone()).free;

	// Sender's balance is reduced
	assert!(sender_wnds_after < sender_wnds_before);
	// Receiver's balance is increased
	assert!(receiver_wnds_after > receiver_wnds_before);
	// Reserve balance is increased by sent amount (less fess)
	assert!(wnds_in_reserve_on_ahw_after > wnds_in_reserve_on_ahw_before);
	assert!(wnds_in_reserve_on_ahw_after <= wnds_in_reserve_on_ahw_before + amount);
}
