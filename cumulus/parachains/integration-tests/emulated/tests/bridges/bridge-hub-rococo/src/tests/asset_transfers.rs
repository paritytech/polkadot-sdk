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

fn send_assets_from_asset_hub_rococo_to_asset_hub_westend(assets: Assets, fee_idx: u32) {
	let destination = asset_hub_westend_location();

	// fund the AHR's SA on BHR for paying bridge transport fees
	BridgeHubRococo::fund_para_sovereign(AssetHubRococo::para_id(), 10_000_000_000_000u128);

	// set XCM versions
	AssetHubRococo::force_xcm_version(destination.clone(), XCM_VERSION);
	BridgeHubRococo::force_xcm_version(bridge_hub_westend_location(), XCM_VERSION);

	// send message over bridge
	assert_ok!(send_assets_from_asset_hub_rococo(destination, assets, fee_idx));
	assert_bridge_hub_rococo_message_accepted(true);
	assert_bridge_hub_westend_message_received();
}

fn send_asset_from_penpal_rococo_through_local_asset_hub_to_westend_asset_hub(
	id: Location,
	transfer_amount: u128,
) {
	let destination = asset_hub_westend_location();
	let local_asset_hub: Location = PenpalA::sibling_location_of(AssetHubRococo::para_id());
	let sov_penpal_on_ahr = AssetHubRococo::sovereign_account_id_of(
		AssetHubRococo::sibling_location_of(PenpalA::para_id()),
	);
	let sov_ahw_on_ahr = AssetHubRococo::sovereign_account_of_parachain_on_other_global_consensus(
		Westend,
		AssetHubWestend::para_id(),
	);

	// fund the AHR's SA on BHR for paying bridge transport fees
	BridgeHubRococo::fund_para_sovereign(AssetHubRococo::para_id(), 10_000_000_000_000u128);

	// set XCM versions
	PenpalA::force_xcm_version(local_asset_hub.clone(), XCM_VERSION);
	AssetHubRococo::force_xcm_version(destination.clone(), XCM_VERSION);
	BridgeHubRococo::force_xcm_version(bridge_hub_westend_location(), XCM_VERSION);

	// send message over bridge
	assert_ok!(PenpalA::execute_with(|| {
		let signed_origin = <PenpalA as Chain>::RuntimeOrigin::signed(PenpalASender::get());
		let beneficiary: Location =
			AccountId32Junction { network: None, id: AssetHubWestendReceiver::get().into() }.into();
		let assets: Assets = (id.clone(), transfer_amount).into();
		let fees_id: AssetId = id.into();
		let custom_xcm_on_dest = Xcm::<()>(vec![DepositAsset {
			assets: Wild(AllCounted(assets.len() as u32)),
			beneficiary,
		}]);

		<PenpalA as PenpalAPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
			signed_origin,
			bx!(destination.into()),
			bx!(assets.clone().into()),
			bx!(TransferType::RemoteReserve(local_asset_hub.clone().into())),
			bx!(fees_id.into()),
			bx!(TransferType::RemoteReserve(local_asset_hub.into())),
			bx!(VersionedXcm::from(custom_xcm_on_dest)),
			WeightLimit::Unlimited,
		)
	}));
	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubRococo,
			vec![
				// Amount to reserve transfer is withdrawn from Penpal's sovereign account
				RuntimeEvent::Balances(
					pallet_balances::Event::Burned { who, amount }
				) => {
					who: *who == sov_penpal_on_ahr.clone().into(),
					amount: *amount == transfer_amount,
				},
				// Amount deposited in AHW's sovereign account
				RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. }) => {
					who: *who == sov_ahw_on_ahr.clone().into(),
				},
				RuntimeEvent::XcmpQueue(
					cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }
				) => {},
			]
		);
	});
	assert_bridge_hub_rococo_message_accepted(true);
	assert_bridge_hub_westend_message_received();
}

#[test]
/// Test transfer of ROC, USDT and wETH from AssetHub Rococo to AssetHub Westend.
///
/// This mix of assets should cover the whole range:
/// - native assets: ROC,
/// - trust-based assets: USDT (exists only on Rococo, Westend gets it from Rococo over bridge),
/// - foreign asset / bridged asset (other bridge / Snowfork): wETH (bridged from Ethereum to Rococo
///   over Snowbridge, then bridged over to Westend through this bridge).
fn send_roc_usdt_and_weth_from_asset_hub_rococo_to_asset_hub_westend() {
	let amount = ASSET_HUB_ROCOCO_ED * 1_000_000;
	let sender = AssetHubRococoSender::get();
	let receiver = AssetHubWestendReceiver::get();
	let roc_at_asset_hub_rococo: v3::Location = roc_at_ah_rococo().try_into().unwrap();
	let bridged_roc_at_asset_hub_westend = bridged_roc_at_ah_westend().try_into().unwrap();

	create_foreign_on_ah_westend(bridged_roc_at_asset_hub_westend, true);
	set_up_pool_with_wnd_on_ah_westend(bridged_roc_at_asset_hub_westend);

	////////////////////////////////////////////////////////////
	// Let's first send over just some ROCs as a simple example
	////////////////////////////////////////////////////////////
	let sov_ahw_on_ahr = AssetHubRococo::sovereign_account_of_parachain_on_other_global_consensus(
		Westend,
		AssetHubWestend::para_id(),
	);
	let rocs_in_reserve_on_ahr_before =
		<AssetHubRococo as Chain>::account_data_of(sov_ahw_on_ahr.clone()).free;
	let sender_rocs_before = <AssetHubRococo as Chain>::account_data_of(sender.clone()).free;
	let receiver_rocs_before =
		foreign_balance_on_ah_westend(bridged_roc_at_asset_hub_westend, &receiver);

	// send ROCs, use them for fees
	let assets: Assets = (Location::try_from(roc_at_asset_hub_rococo).unwrap(), amount).into();
	let fee_index = 0;
	send_assets_from_asset_hub_rococo_to_asset_hub_westend(assets, fee_index);

	// verify expected events on final destination
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![
				// issue ROCs on AHW
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
					asset_id: *asset_id == roc_at_asset_hub_rococo,
					owner: owner == &receiver,
				},
				// message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	});

	let sender_rocs_after = <AssetHubRococo as Chain>::account_data_of(sender.clone()).free;
	let receiver_rocs_after =
		foreign_balance_on_ah_westend(bridged_roc_at_asset_hub_westend, &receiver);
	let rocs_in_reserve_on_ahr_after =
		<AssetHubRococo as Chain>::account_data_of(sov_ahw_on_ahr.clone()).free;

	// Sender's ROC balance is reduced
	assert!(sender_rocs_before > sender_rocs_after);
	// Receiver's ROC balance is increased
	assert!(receiver_rocs_after > receiver_rocs_before);
	// Reserve ROC balance is increased by sent amount
	assert_eq!(rocs_in_reserve_on_ahr_after, rocs_in_reserve_on_ahr_before + amount);

	/////////////////////////////////////////////////////////////
	// Now let's send over USDTs + wETH (and pay fees with USDT)
	/////////////////////////////////////////////////////////////

	let usdt_at_asset_hub_rococo = usdt_at_ah_rococo();
	let bridged_usdt_at_asset_hub_westend = bridged_usdt_at_ah_westend().try_into().unwrap();
	// wETH has same relative location on both Rococo and Westend AssetHubs
	let bridged_weth_at_ah = weth_at_asset_hubs().try_into().unwrap();

	// mint USDT in sender's account (USDT already created in genesis)
	AssetHubRococo::mint_asset(
		<AssetHubRococo as Chain>::RuntimeOrigin::signed(AssetHubRococoAssetOwner::get()),
		USDT_ID,
		sender.clone(),
		amount * 2,
	);
	// create wETH at src and dest and prefund sender's account
	create_foreign_on_ah_rococo(bridged_weth_at_ah, true, vec![(sender.clone(), amount * 2)]);
	create_foreign_on_ah_westend(bridged_weth_at_ah, true);
	create_foreign_on_ah_westend(bridged_usdt_at_asset_hub_westend, true);
	set_up_pool_with_wnd_on_ah_westend(bridged_usdt_at_asset_hub_westend);

	let receiver_usdts_before =
		foreign_balance_on_ah_westend(bridged_usdt_at_asset_hub_westend, &receiver);
	let receiver_weth_before = foreign_balance_on_ah_westend(bridged_weth_at_ah, &receiver);

	// send USDTs and wETHs
	let assets: Assets = vec![
		(usdt_at_asset_hub_rococo.clone(), amount).into(),
		(Location::try_from(bridged_weth_at_ah).unwrap(), amount).into(),
	]
	.into();
	// use USDT for fees
	let fee: AssetId = usdt_at_asset_hub_rococo.into();

	// use the more involved transfer extrinsic
	let custom_xcm_on_dest = Xcm::<()>(vec![DepositAsset {
		assets: Wild(AllCounted(assets.len() as u32)),
		beneficiary: AccountId32Junction { network: None, id: receiver.clone().into() }.into(),
	}]);
	assert_ok!(AssetHubRococo::execute_with(|| {
		<AssetHubRococo as AssetHubRococoPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
			<AssetHubRococo as Chain>::RuntimeOrigin::signed(sender.into()),
			bx!(asset_hub_westend_location().into()),
			bx!(assets.into()),
			bx!(TransferType::LocalReserve),
			bx!(fee.into()),
			bx!(TransferType::LocalReserve),
			bx!(VersionedXcm::from(custom_xcm_on_dest)),
			WeightLimit::Unlimited,
		)
	}));
	// verify hops (also advances the message through the hops)
	assert_bridge_hub_rococo_message_accepted(true);
	assert_bridge_hub_westend_message_received();
	AssetHubWestend::execute_with(|| {
		AssetHubWestend::assert_xcmp_queue_success(None);
	});

	let receiver_usdts_after =
		foreign_balance_on_ah_westend(bridged_usdt_at_asset_hub_westend, &receiver);
	let receiver_weth_after = foreign_balance_on_ah_westend(bridged_weth_at_ah, &receiver);

	// Receiver's USDT balance is increased by almost `amount` (minus fees)
	assert!(receiver_usdts_after > receiver_usdts_before);
	assert!(receiver_usdts_after < receiver_usdts_before + amount);
	// Receiver's wETH balance is increased by sent amount
	assert_eq!(receiver_weth_after, receiver_weth_before + amount);
}

#[test]
/// Send bridged WNDs "back" from AssetHub Rococo to AssetHub Westend.
fn send_back_wnds_from_asset_hub_rococo_to_asset_hub_westend() {
	let prefund_amount = 10_000_000_000_000u128;
	let sender = AssetHubRococoSender::get();
	let receiver = AssetHubWestendReceiver::get();
	let wnd_at_asset_hub_rococo = bridged_wnd_at_ah_rococo();
	let wnd_at_asset_hub_rococo_v3 = wnd_at_asset_hub_rococo.clone().try_into().unwrap();
	let prefund_accounts = vec![(sender.clone(), prefund_amount)];
	create_foreign_on_ah_rococo(wnd_at_asset_hub_rococo_v3, true, prefund_accounts);

	// fund the AHR's SA on AHW with the WND tokens held in reserve
	let sov_ahr_on_ahw = AssetHubWestend::sovereign_account_of_parachain_on_other_global_consensus(
		Rococo,
		AssetHubRococo::para_id(),
	);
	AssetHubWestend::fund_accounts(vec![(sov_ahr_on_ahw.clone(), prefund_amount)]);

	let wnds_in_reserve_on_ahw_before =
		<AssetHubWestend as Chain>::account_data_of(sov_ahr_on_ahw.clone()).free;
	assert_eq!(wnds_in_reserve_on_ahw_before, prefund_amount);

	let sender_wnds_before = foreign_balance_on_ah_rococo(wnd_at_asset_hub_rococo_v3, &sender);
	assert_eq!(sender_wnds_before, prefund_amount);
	let receiver_wnds_before = <AssetHubWestend as Chain>::account_data_of(receiver.clone()).free;

	let amount_to_send = ASSET_HUB_WESTEND_ED * 1_000;
	send_assets_from_asset_hub_rococo_to_asset_hub_westend(
		(wnd_at_asset_hub_rococo, amount_to_send).into(),
		0,
	);

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![
				// WND is withdrawn from AHR's SA on AHW
				RuntimeEvent::Balances(
					pallet_balances::Event::Burned { who, amount }
				) => {
					who: *who == sov_ahr_on_ahw,
					amount: *amount == amount_to_send,
				},
				// WNDs deposited to beneficiary
				RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. }) => {
					who: who == &receiver,
				},
				// message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	});

	let sender_wnds_after = foreign_balance_on_ah_rococo(wnd_at_asset_hub_rococo_v3, &sender);
	let receiver_wnds_after = <AssetHubWestend as Chain>::account_data_of(receiver).free;
	let wnds_in_reserve_on_ahw_after =
		<AssetHubWestend as Chain>::account_data_of(sov_ahr_on_ahw).free;

	// Sender's balance is reduced
	assert!(sender_wnds_before > sender_wnds_after);
	// Receiver's balance is increased
	assert!(receiver_wnds_after > receiver_wnds_before);
	// Reserve balance is reduced by sent amount
	assert_eq!(wnds_in_reserve_on_ahw_after, wnds_in_reserve_on_ahw_before - amount_to_send);
}

#[test]
fn send_rocs_from_penpal_rococo_through_asset_hub_rococo_to_asset_hub_westend() {
	let roc_at_rococo_parachains = roc_at_ah_rococo();
	let roc_at_asset_hub_westend = bridged_roc_at_ah_westend().try_into().unwrap();
	create_foreign_on_ah_westend(roc_at_asset_hub_westend, true);

	let amount = ASSET_HUB_ROCOCO_ED * 10_000_000;
	let sender = PenpalASender::get();
	let receiver = AssetHubWestendReceiver::get();
	let penpal_location = AssetHubRococo::sibling_location_of(PenpalA::para_id());
	let sov_penpal_on_ahr = AssetHubRococo::sovereign_account_id_of(penpal_location);
	// fund Penpal's sovereign account on AssetHub
	AssetHubRococo::fund_accounts(vec![(sov_penpal_on_ahr.into(), amount * 2)]);
	// fund Penpal's sender account
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(PenpalAssetOwner::get()),
		roc_at_rococo_parachains.clone(),
		sender.clone(),
		amount * 2,
	);

	let sov_ahw_on_ahr = AssetHubRococo::sovereign_account_of_parachain_on_other_global_consensus(
		Westend,
		AssetHubWestend::para_id(),
	);
	let rocs_in_reserve_on_ahr_before =
		<AssetHubRococo as Chain>::account_data_of(sov_ahw_on_ahr.clone()).free;
	let sender_rocs_before = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(roc_at_rococo_parachains.clone(), &sender)
	});
	let receiver_rocs_before = foreign_balance_on_ah_westend(roc_at_asset_hub_westend, &receiver);

	// Send ROCs over bridge
	send_asset_from_penpal_rococo_through_local_asset_hub_to_westend_asset_hub(
		roc_at_rococo_parachains.clone(),
		amount,
	);

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![
				// issue ROCs on AHW
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
					asset_id: *asset_id == roc_at_rococo_parachains.clone().try_into().unwrap(),
					owner: owner == &receiver,
				},
				// message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	});

	let sender_rocs_after = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(roc_at_rococo_parachains, &sender)
	});
	let receiver_rocs_after = foreign_balance_on_ah_westend(roc_at_asset_hub_westend, &receiver);
	let rocs_in_reserve_on_ahr_after =
		<AssetHubRococo as Chain>::account_data_of(sov_ahw_on_ahr.clone()).free;

	// Sender's balance is reduced
	assert!(sender_rocs_after < sender_rocs_before);
	// Receiver's balance is increased
	assert!(receiver_rocs_after > receiver_rocs_before);
	// Reserve balance is increased by sent amount (less fess)
	assert!(rocs_in_reserve_on_ahr_after > rocs_in_reserve_on_ahr_before);
	assert!(rocs_in_reserve_on_ahr_after <= rocs_in_reserve_on_ahr_before + amount);
}
