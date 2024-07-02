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
/// Test transfer of WND from AssetHub Westend to AssetHub Rococo.
fn send_wnds_from_asset_hub_westend_to_asset_hub_rococo() {
	let amount = ASSET_HUB_WESTEND_ED * 1_000;
	let sender = AssetHubWestendSender::get();
	let receiver = AssetHubRococoReceiver::get();
	let wnd_at_asset_hub_westend = wnd_at_ah_westend();
	let bridged_wnd_at_asset_hub_rococo = bridged_wnd_at_ah_rococo().try_into().unwrap();
	create_foreign_on_ah_rococo(bridged_wnd_at_asset_hub_rococo, true);

	set_up_pool_with_roc_on_ah_rococo(bridged_wnd_at_asset_hub_rococo, true);

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
/// Send bridged assets "back" from AssetHub Rococo to AssetHub Westend.
///
/// This mix of assets should cover the whole range:
/// - bridged native assets: ROC,
/// - bridged trust-based assets: USDT (exists only on Rococo, Westend gets it from Rococo over
///   bridge),
/// - bridged foreign asset / double-bridged asset (other bridge / Snowfork): wETH (bridged from
///   Ethereum to Rococo over Snowbridge, then bridged over to Westend through this bridge).
fn send_back_rocs_usdt_and_weth_from_asset_hub_westend_to_asset_hub_rococo() {
	let prefund_amount = 10_000_000_000_000u128;
	let sender = AssetHubWestendSender::get();
	let receiver = AssetHubRococoReceiver::get();
	let bridged_roc_at_asset_hub_westend = bridged_roc_at_ah_westend();
	let bridged_roc_at_asset_hub_westend_v3 =
		bridged_roc_at_asset_hub_westend.clone().try_into().unwrap();
	let prefund_accounts = vec![(sender.clone(), prefund_amount)];
	create_foreign_on_ah_westend(bridged_roc_at_asset_hub_westend_v3, true, prefund_accounts);

	////////////////////////////////////////////////////////////
	// Let's first send back just some ROCs as a simple example
	////////////////////////////////////////////////////////////

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
	let receiver_rocs_after = <AssetHubRococo as Chain>::account_data_of(receiver.clone()).free;
	let rocs_in_reserve_on_ahr_after =
		<AssetHubRococo as Chain>::account_data_of(sov_ahw_on_ahr.clone()).free;

	// Sender's balance is reduced
	assert!(sender_rocs_before > sender_rocs_after);
	// Receiver's balance is increased
	assert!(receiver_rocs_after > receiver_rocs_before);
	// Reserve balance is reduced by sent amount
	assert_eq!(rocs_in_reserve_on_ahr_after, rocs_in_reserve_on_ahr_before - amount_to_send);

	//////////////////////////////////////////////////////////////////
	// Now let's send back over USDTs + wETH (and pay fees with USDT)
	//////////////////////////////////////////////////////////////////

	// wETH has same relative location on both Rococo and Westend AssetHubs
	let bridged_weth_at_ah = weth_at_asset_hubs().try_into().unwrap();
	let bridged_usdt_at_asset_hub_westend = bridged_usdt_at_ah_westend().try_into().unwrap();

	// set up destination chain AH Rococo:
	// create a ROC/USDT pool to be able to pay fees with USDT (USDT created in genesis)
	set_up_pool_with_roc_on_ah_rococo(usdt_at_ah_rococo().try_into().unwrap(), false);
	// create wETH on Rococo (IRL it's already created by Snowbridge)
	create_foreign_on_ah_rococo(bridged_weth_at_ah, true);
	// prefund AHW's sovereign account on AHR to be able to withdraw USDT and wETH from reserves
	let sov_ahw_on_ahr = AssetHubRococo::sovereign_account_of_parachain_on_other_global_consensus(
		Westend,
		AssetHubWestend::para_id(),
	);
	AssetHubRococo::mint_asset(
		<AssetHubRococo as Chain>::RuntimeOrigin::signed(AssetHubRococoAssetOwner::get()),
		USDT_ID,
		sov_ahw_on_ahr.clone(),
		amount_to_send * 2,
	);
	AssetHubRococo::mint_foreign_asset(
		<AssetHubRococo as Chain>::RuntimeOrigin::signed(AssetHubRococo::account_id_of(ALICE)),
		bridged_weth_at_ah,
		sov_ahw_on_ahr,
		amount_to_send * 2,
	);

	// set up source chain AH Westend:
	// create wETH and USDT foreign assets on Westend and prefund sender's account
	let prefund_accounts = vec![(sender.clone(), amount_to_send * 2)];
	create_foreign_on_ah_westend(bridged_weth_at_ah, true, prefund_accounts.clone());
	create_foreign_on_ah_westend(bridged_usdt_at_asset_hub_westend, true, prefund_accounts);

	// check balances before
	let receiver_usdts_before = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::Assets;
		<Assets as Inspect<_>>::balance(USDT_ID, &receiver)
	});
	let receiver_weth_before = foreign_balance_on_ah_rococo(bridged_weth_at_ah, &receiver);

	let usdt_id: AssetId = Location::try_from(bridged_usdt_at_asset_hub_westend).unwrap().into();
	// send USDTs and wETHs
	let assets: Assets = vec![
		(usdt_id.clone(), amount_to_send).into(),
		(Location::try_from(bridged_weth_at_ah).unwrap(), amount_to_send).into(),
	]
	.into();
	// use USDT for fees
	let fee = usdt_id;

	// use the more involved transfer extrinsic
	let custom_xcm_on_dest = Xcm::<()>(vec![DepositAsset {
		assets: Wild(AllCounted(assets.len() as u32)),
		beneficiary: AccountId32Junction { network: None, id: receiver.clone().into() }.into(),
	}]);
	assert_ok!(AssetHubWestend::execute_with(|| {
		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(sender.into()),
			bx!(asset_hub_rococo_location().into()),
			bx!(assets.into()),
			bx!(TransferType::DestinationReserve),
			bx!(fee.into()),
			bx!(TransferType::DestinationReserve),
			bx!(VersionedXcm::from(custom_xcm_on_dest)),
			WeightLimit::Unlimited,
		)
	}));
	// verify hops (also advances the message through the hops)
	assert_bridge_hub_westend_message_accepted(true);
	assert_bridge_hub_rococo_message_received();
	AssetHubRococo::execute_with(|| {
		AssetHubRococo::assert_xcmp_queue_success(None);
	});

	let receiver_usdts_after = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::Assets;
		<Assets as Inspect<_>>::balance(USDT_ID, &receiver)
	});
	let receiver_weth_after = foreign_balance_on_ah_rococo(bridged_weth_at_ah, &receiver);

	// Receiver's USDT balance is increased by almost `amount_to_send` (minus fees)
	assert!(receiver_usdts_after > receiver_usdts_before);
	assert!(receiver_usdts_after < receiver_usdts_before + amount_to_send);
	// Receiver's wETH balance is increased by `amount_to_send`
	assert_eq!(receiver_weth_after, receiver_weth_before + amount_to_send);
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
