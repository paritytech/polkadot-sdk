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
use xcm::latest::AssetTransferFilter;

fn send_assets_over_bridge<F: FnOnce()>(send_fn: F) {
	// fund the AHW's SA on BHW for paying bridge transport fees
	BridgeHubWestend::fund_para_sovereign(AssetHubWestend::para_id(), 10_000_000_000_000u128);

	// set XCM versions
	let local_asset_hub = PenpalB::sibling_location_of(AssetHubWestend::para_id());
	PenpalB::force_xcm_version(local_asset_hub.clone(), XCM_VERSION);
	AssetHubWestend::force_xcm_version(asset_hub_rococo_location(), XCM_VERSION);
	BridgeHubWestend::force_xcm_version(bridge_hub_rococo_location(), XCM_VERSION);

	// open bridge
	open_bridge_between_asset_hub_rococo_and_asset_hub_westend();

	// send message over bridge
	send_fn();

	// process and verify intermediary hops
	assert_bridge_hub_westend_message_accepted(true);
	assert_bridge_hub_rococo_message_received();
}

fn set_up_wnds_for_penpal_westend_through_ahw_to_ahr(
	sender: &AccountId,
	amount: u128,
) -> (Location, v5::Location) {
	let wnd_at_westend_parachains = wnd_at_ah_westend();
	let wnd_at_asset_hub_rococo = bridged_wnd_at_ah_rococo();
	create_foreign_on_ah_rococo(wnd_at_asset_hub_rococo.clone(), true);

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
	(wnd_at_westend_parachains, wnd_at_asset_hub_rococo)
}

fn send_assets_from_penpal_westend_through_westend_ah_to_rococo_ah(
	destination: Location,
	assets: (Assets, TransferType),
	fees: (AssetId, TransferType),
	custom_xcm_on_dest: Xcm<()>,
) {
	send_assets_over_bridge(|| {
		let sov_penpal_on_ahw = AssetHubWestend::sovereign_account_id_of(
			AssetHubWestend::sibling_location_of(PenpalB::para_id()),
		);
		let sov_ahr_on_ahw =
			AssetHubWestend::sovereign_account_of_parachain_on_other_global_consensus(
				Rococo,
				AssetHubRococo::para_id(),
			);
		// send message over bridge
		assert_ok!(PenpalB::execute_with(|| {
			let signed_origin = <PenpalB as Chain>::RuntimeOrigin::signed(PenpalBSender::get());
			<PenpalB as PenpalBPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
				signed_origin,
				bx!(destination.into()),
				bx!(assets.0.into()),
				bx!(assets.1),
				bx!(fees.0.into()),
				bx!(fees.1),
				bx!(VersionedXcm::from(custom_xcm_on_dest)),
				WeightLimit::Unlimited,
			)
		}));
		// verify intermediary AH Westend hop
		AssetHubWestend::execute_with(|| {
			type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
			assert_expected_events!(
				AssetHubWestend,
				vec![
					// Amount to reserve transfer is withdrawn from Penpal's sovereign account
					RuntimeEvent::Balances(
						pallet_balances::Event::Burned { who, .. }
					) => {
						who: *who == sov_penpal_on_ahw.clone().into(),
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
	});
}

#[test]
/// Test transfer of WND, USDT and wETH from AssetHub Westend to AssetHub Rococo.
///
/// This mix of assets should cover the whole range:
/// - native assets: WND,
/// - trust-based assets: USDT (exists only on Westend, Rococo gets it from Westend over bridge),
/// - foreign asset / bridged asset (other bridge / Snowfork): wETH (bridged from Ethereum to
///   Westend over Snowbridge, then bridged over to Rococo through this bridge).
fn send_wnds_usdt_and_weth_from_asset_hub_westend_to_asset_hub_rococo() {
	let amount = ASSET_HUB_WESTEND_ED * 1_000;
	let sender = AssetHubWestendSender::get();
	let receiver = AssetHubRococoReceiver::get();
	let wnd_at_asset_hub_westend = wnd_at_ah_westend();
	let bridged_wnd_at_asset_hub_rococo = bridged_wnd_at_ah_rococo();

	create_foreign_on_ah_rococo(bridged_wnd_at_asset_hub_rococo.clone(), true);
	set_up_pool_with_roc_on_ah_rococo(bridged_wnd_at_asset_hub_rococo.clone(), true);

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
		foreign_balance_on_ah_rococo(bridged_wnd_at_asset_hub_rococo.clone(), &receiver);

	// send WNDs, use them for fees
	send_assets_over_bridge(|| {
		let destination = asset_hub_rococo_location();
		let assets: Assets = (wnd_at_asset_hub_westend, amount).into();
		let fee_idx = 0;
		assert_ok!(send_assets_from_asset_hub_westend(destination, assets, fee_idx));
	});

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

	let sender_wnds_after = <AssetHubWestend as Chain>::account_data_of(sender.clone()).free;
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

	/////////////////////////////////////////////////////////////
	// Now let's send over USDTs + wETH (and pay fees with USDT)
	/////////////////////////////////////////////////////////////
	let usdt_at_asset_hub_westend = usdt_at_ah_westend();
	let bridged_usdt_at_asset_hub_rococo = bridged_usdt_at_ah_rococo();
	// wETH has same relative location on both Westend and Rococo AssetHubs
	let bridged_weth_at_ah = weth_at_asset_hubs();

	// mint USDT in sender's account (USDT already created in genesis)
	AssetHubWestend::mint_asset(
		<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendAssetOwner::get()),
		USDT_ID,
		sender.clone(),
		amount * 2,
	);
	// create wETH at src and dest and prefund sender's account
	create_foreign_on_ah_westend(
		bridged_weth_at_ah.clone(),
		true,
		vec![(sender.clone(), amount * 2)],
	);
	create_foreign_on_ah_rococo(bridged_weth_at_ah.clone(), true);
	create_foreign_on_ah_rococo(bridged_usdt_at_asset_hub_rococo.clone(), true);
	set_up_pool_with_roc_on_ah_rococo(bridged_usdt_at_asset_hub_rococo.clone(), true);

	let receiver_usdts_before =
		foreign_balance_on_ah_rococo(bridged_usdt_at_asset_hub_rococo.clone(), &receiver);
	let receiver_weth_before = foreign_balance_on_ah_rococo(bridged_weth_at_ah.clone(), &receiver);

	// send USDTs and wETHs
	let assets: Assets = vec![
		(usdt_at_asset_hub_westend.clone(), amount).into(),
		(Location::try_from(bridged_weth_at_ah.clone()).unwrap(), amount).into(),
	]
	.into();
	// use USDT for fees
	let fee: AssetId = usdt_at_asset_hub_westend.into();

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
			bx!(TransferType::LocalReserve),
			bx!(fee.into()),
			bx!(TransferType::LocalReserve),
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

	let receiver_usdts_after =
		foreign_balance_on_ah_rococo(bridged_usdt_at_asset_hub_rococo, &receiver);
	let receiver_weth_after = foreign_balance_on_ah_rococo(bridged_weth_at_ah, &receiver);

	// Receiver's USDT balance is increased by almost `amount` (minus fees)
	assert!(receiver_usdts_after > receiver_usdts_before);
	assert!(receiver_usdts_after < receiver_usdts_before + amount);
	// Receiver's wETH balance is increased by sent amount
	assert_eq!(receiver_weth_after, receiver_weth_before + amount);
}

#[test]
/// Send bridged ROCs "back" from AssetHub Westend to AssetHub Rococo.
fn send_back_rocs_from_asset_hub_westend_to_asset_hub_rococo() {
	let prefund_amount = 10_000_000_000_000u128;
	let amount_to_send = ASSET_HUB_ROCOCO_ED * 1_000;
	let sender = AssetHubWestendSender::get();
	let receiver = AssetHubRococoReceiver::get();
	let bridged_roc_at_asset_hub_westend = bridged_roc_at_ah_westend();
	let prefund_accounts = vec![(sender.clone(), prefund_amount)];
	create_foreign_on_ah_westend(bridged_roc_at_asset_hub_westend.clone(), true, prefund_accounts);

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
		foreign_balance_on_ah_westend(bridged_roc_at_asset_hub_westend.clone(), &sender);
	assert_eq!(sender_rocs_before, prefund_amount);
	let receiver_rocs_before = <AssetHubRococo as Chain>::account_data_of(receiver.clone()).free;

	// send back ROCs, use them for fees
	send_assets_over_bridge(|| {
		let destination = asset_hub_rococo_location();
		let assets: Assets = (bridged_roc_at_asset_hub_westend.clone(), amount_to_send).into();
		let fee_idx = 0;
		assert_ok!(send_assets_from_asset_hub_westend(destination, assets, fee_idx));
	});

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
		foreign_balance_on_ah_westend(bridged_roc_at_asset_hub_westend, &sender);
	let receiver_rocs_after = <AssetHubRococo as Chain>::account_data_of(receiver.clone()).free;
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
	let amount = ASSET_HUB_WESTEND_ED * 10_000_000;
	let sender = PenpalBSender::get();
	let receiver = AssetHubRococoReceiver::get();
	let local_asset_hub = PenpalB::sibling_location_of(AssetHubWestend::para_id());
	let (wnd_at_westend_parachains, wnd_at_asset_hub_rococo) =
		set_up_wnds_for_penpal_westend_through_ahw_to_ahr(&sender, amount);

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
	let receiver_wnds_before =
		foreign_balance_on_ah_rococo(wnd_at_asset_hub_rococo.clone(), &receiver);

	// Send WNDs over bridge
	{
		let destination = asset_hub_rococo_location();
		let assets: Assets = (wnd_at_westend_parachains.clone(), amount).into();
		let asset_transfer_type = TransferType::RemoteReserve(local_asset_hub.clone().into());
		let fees_id: AssetId = wnd_at_westend_parachains.clone().into();
		let fees_transfer_type = TransferType::RemoteReserve(local_asset_hub.into());
		let beneficiary: Location =
			AccountId32Junction { network: None, id: receiver.clone().into() }.into();
		let custom_xcm_on_dest = Xcm::<()>(vec![DepositAsset {
			assets: Wild(AllCounted(assets.len() as u32)),
			beneficiary,
		}]);
		send_assets_from_penpal_westend_through_westend_ah_to_rococo_ah(
			destination,
			(assets, asset_transfer_type),
			(fees_id, fees_transfer_type),
			custom_xcm_on_dest,
		);
	}

	// process AHR incoming message and check events
	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubRococo,
			vec![
				// issue WNDs on AHR
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
					asset_id: *asset_id == wnd_at_westend_parachains.clone(),
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

#[test]
fn send_back_rocs_from_penpal_westend_through_asset_hub_westend_to_asset_hub_rococo() {
	let roc_at_westend_parachains = bridged_roc_at_ah_westend();
	let amount = ASSET_HUB_WESTEND_ED * 10_000_000;
	let sender = PenpalBSender::get();
	let receiver = AssetHubRococoReceiver::get();

	// set up WNDs for transfer
	let (wnd_at_westend_parachains, _) =
		set_up_wnds_for_penpal_westend_through_ahw_to_ahr(&sender, amount);

	// set up ROCs for transfer
	let penpal_location = AssetHubWestend::sibling_location_of(PenpalB::para_id());
	let sov_penpal_on_ahr = AssetHubWestend::sovereign_account_id_of(penpal_location);
	let prefund_accounts = vec![(sov_penpal_on_ahr, amount * 2)];
	create_foreign_on_ah_westend(roc_at_westend_parachains.clone(), true, prefund_accounts);
	let asset_owner: AccountId = AssetHubWestend::account_id_of(ALICE);
	PenpalB::force_create_foreign_asset(
		roc_at_westend_parachains.clone(),
		asset_owner.clone(),
		true,
		ASSET_MIN_BALANCE,
		vec![(sender.clone(), amount * 2)],
	);
	// Configure source Penpal chain to trust local AH as reserve of bridged ROC
	PenpalB::execute_with(|| {
		assert_ok!(<PenpalB as Chain>::System::set_storage(
			<PenpalB as Chain>::RuntimeOrigin::root(),
			vec![(
				PenpalCustomizableAssetFromSystemAssetHub::key().to_vec(),
				roc_at_westend_parachains.encode(),
			)],
		));
	});

	// fund the AHW's SA on AHR with the ROC tokens held in reserve
	let sov_ahw_on_ahr = AssetHubRococo::sovereign_account_of_parachain_on_other_global_consensus(
		Westend,
		AssetHubWestend::para_id(),
	);
	AssetHubRococo::fund_accounts(vec![(sov_ahw_on_ahr.clone(), amount * 2)]);

	// balances before
	let sender_rocs_before = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(roc_at_westend_parachains.clone().into(), &sender)
	});
	let receiver_rocs_before = <AssetHubRococo as Chain>::account_data_of(receiver.clone()).free;

	// send ROCs over the bridge, WNDs only used to pay fees on local AH, pay with ROC on remote AH
	{
		let final_destination = asset_hub_rococo_location();
		let intermediary_hop = PenpalB::sibling_location_of(AssetHubWestend::para_id());
		let context = PenpalB::execute_with(|| PenpalUniversalLocation::get());

		// what happens at final destination
		let beneficiary = AccountId32Junction { network: None, id: receiver.clone().into() }.into();
		// use ROC as fees on the final destination (AHW)
		let remote_fees: Asset = (roc_at_westend_parachains.clone(), amount).into();
		let remote_fees = remote_fees.reanchored(&final_destination, &context).unwrap();
		// buy execution using ROCs, then deposit all remaining ROCs
		let xcm_on_final_dest = Xcm::<()>(vec![
			BuyExecution { fees: remote_fees, weight_limit: WeightLimit::Unlimited },
			DepositAsset { assets: Wild(AllCounted(1)), beneficiary },
		]);

		// what happens at intermediary hop
		// reanchor final dest (Asset Hub Rococo) to the view of hop (Asset Hub Westend)
		let mut final_destination = final_destination.clone();
		final_destination.reanchor(&intermediary_hop, &context).unwrap();
		// reanchor ROCs to the view of hop (Asset Hub Westend)
		let asset: Asset = (roc_at_westend_parachains.clone(), amount).into();
		let asset = asset.reanchored(&intermediary_hop, &context).unwrap();
		// on Asset Hub Westend, forward a request to withdraw ROCs from reserve on Asset Hub Rococo
		let xcm_on_hop = Xcm::<()>(vec![InitiateReserveWithdraw {
			assets: Definite(asset.into()), // ROCs
			reserve: final_destination,     // AHR
			xcm: xcm_on_final_dest,         // XCM to execute on AHR
		}]);
		// assets to send from Penpal and how they reach the intermediary hop
		let assets: Assets = vec![
			(roc_at_westend_parachains.clone(), amount).into(),
			(wnd_at_westend_parachains.clone(), amount).into(),
		]
		.into();
		let asset_transfer_type = TransferType::DestinationReserve;
		let fees_id: AssetId = wnd_at_westend_parachains.into();
		let fees_transfer_type = TransferType::DestinationReserve;

		// initiate the transfer
		send_assets_from_penpal_westend_through_westend_ah_to_rococo_ah(
			intermediary_hop,
			(assets, asset_transfer_type),
			(fees_id, fees_transfer_type),
			xcm_on_hop,
		);
	}

	// process AHR incoming message and check events
	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubRococo,
			vec![
				// issue WNDs on AHR
				RuntimeEvent::Balances(pallet_balances::Event::Issued { .. }) => {},
				// message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	});

	let sender_rocs_after = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(roc_at_westend_parachains.into(), &sender)
	});
	let receiver_rocs_after = <AssetHubRococo as Chain>::account_data_of(receiver).free;

	// Sender's balance is reduced by sent "amount"
	assert_eq!(sender_rocs_after, sender_rocs_before - amount);
	// Receiver's balance is increased by no more than "amount"
	assert!(receiver_rocs_after > receiver_rocs_before);
	assert!(receiver_rocs_after <= receiver_rocs_before + amount);
}

#[test]
fn dry_run_transfer_to_rococo_sends_xcm_to_bridge_hub() {
	test_dry_run_transfer_across_pk_bridge!(
		AssetHubWestend,
		BridgeHubWestend,
		asset_hub_rococo_location()
	);
}

fn do_send_pens_and_wnds_from_penpal_westend_via_ahw_to_asset_hub_rococo(
	wnds: (Location, u128),
	pens: (Location, u128),
) {
	let (wnds_id, wnds_amount) = wnds;
	let (pens_id, pens_amount) = pens;
	send_assets_over_bridge(|| {
		let sov_penpal_on_ahw = AssetHubWestend::sovereign_account_id_of(
			AssetHubWestend::sibling_location_of(PenpalB::para_id()),
		);
		let sov_ahr_on_ahw =
			AssetHubWestend::sovereign_account_of_parachain_on_other_global_consensus(
				Rococo,
				AssetHubRococo::para_id(),
			);
		// send message over bridge
		assert_ok!(PenpalB::execute_with(|| {
			let destination = asset_hub_rococo_location();
			let local_asset_hub = PenpalB::sibling_location_of(AssetHubWestend::para_id());
			let signed_origin = <PenpalB as Chain>::RuntimeOrigin::signed(PenpalBSender::get());
			let beneficiary: Location =
				AccountId32Junction { network: None, id: AssetHubRococoReceiver::get().into() }
					.into();
			let wnds: Asset = (wnds_id.clone(), wnds_amount).into();
			let pens: Asset = (pens_id, pens_amount).into();
			let assets: Assets = vec![wnds.clone(), pens.clone()].into();

			// TODO: dry-run to get exact fees, for now just some static value 100_000_000_000
			let penpal_fees_amount = 100_000_000_000;
			// use 100_000_000_000 WNDs in fees on AHW
			// (exec fees: 3_593_000_000, transpo fees: 69_021_561_290 = 72_614_561_290)
			// TODO: make this exact once we have bridge dry-running
			let ahw_fee_amount = 100_000_000_000;

			// XCM to be executed at dest (Rococo Asset Hub)
			let xcm_on_dest = Xcm(vec![
				// since this is the last hop, we don't need to further use any assets previously
				// reserved for fees (there are no further hops to cover transport fees for); we
				// RefundSurplus to get back any unspent fees
				RefundSurplus,
				// deposit everything to final beneficiary
				DepositAsset { assets: Wild(All), beneficiary: beneficiary.clone() },
			]);

			// XCM to be executed at (intermediary) Westend Asset Hub
			let context = PenpalUniversalLocation::get();
			let reanchored_dest =
				destination.clone().reanchored(&local_asset_hub, &context).unwrap();
			let reanchored_pens = pens.clone().reanchored(&local_asset_hub, &context).unwrap();
			let mut onward_wnds = wnds.clone().reanchored(&local_asset_hub, &context).unwrap();
			onward_wnds.fun = Fungible(wnds_amount - ahw_fee_amount - penpal_fees_amount);
			let xcm_on_ahw = Xcm(vec![
				// both WNDs and PENs are local-reserve transferred to Rococo Asset Hub
				// initially, all WNDs are reserved for fees on destination, but at the end of the
				// program we RefundSurplus to get back any unspent and deposit them to final
				// beneficiary
				InitiateTransfer {
					destination: reanchored_dest,
					remote_fees: Some(AssetTransferFilter::ReserveDeposit(onward_wnds.into())),
					assets: vec![AssetTransferFilter::ReserveDeposit(reanchored_pens.into())],
					remote_xcm: xcm_on_dest,
				},
			]);

			let penpal_fees = (wnds.id.clone(), Fungible(penpal_fees_amount));
			let ahw_fees: Asset = (wnds.id.clone(), Fungible(ahw_fee_amount)).into();
			let ahw_non_fees_wnds: Asset =
				(wnds.id.clone(), Fungible(wnds_amount - ahw_fee_amount - penpal_fees_amount))
					.into();
			// XCM to be executed locally
			let xcm = Xcm::<()>(vec![
				// Withdraw both WNDs and PENs from origin account
				WithdrawAsset(assets.into()),
				PayFees { asset: penpal_fees.into() },
				// Execute the transfers while paying remote fees with WNDs
				InitiateTransfer {
					destination: local_asset_hub,
					// WNDs for fees are reserve-withdrawn at AHW and reserved for fees
					remote_fees: Some(AssetTransferFilter::ReserveWithdraw(ahw_fees.into())),
					// PENs are teleported to AHW, rest of non-fee WNDs are reserve-withdrawn at AHW
					assets: vec![
						AssetTransferFilter::Teleport(pens.into()),
						AssetTransferFilter::ReserveWithdraw(ahw_non_fees_wnds.into()),
					],
					remote_xcm: xcm_on_ahw,
				},
			]);

			<PenpalB as PenpalBPallet>::PolkadotXcm::execute(
				signed_origin,
				bx!(xcm::VersionedXcm::V5(xcm.into())),
				Weight::MAX,
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
						amount: *amount == wnds_amount,
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
	});
}

/// Transfer "PEN"s plus "WND"s from PenpalWestend to AssetHubWestend, over bridge to
/// AssetHubRococo. PENs need to be teleported to AHW, while WNDs reserve-withdrawn, then both
/// reserve transferred further to AHR. (transfer 2 different assets with different transfer types
/// across 3 different chains)
#[test]
fn send_pens_and_wnds_from_penpal_westend_via_ahw_to_ahr() {
	let penpal_check_account = <PenpalB as PenpalBPallet>::PolkadotXcm::check_account();
	let owner: AccountId = AssetHubRococo::account_id_of(ALICE);
	let sender = PenpalBSender::get();
	let amount = ASSET_HUB_WESTEND_ED * 10_000_000;

	let (wnd_at_westend_parachains, wnd_at_rococo_parachains) =
		set_up_wnds_for_penpal_westend_through_ahw_to_ahr(&sender, amount);

	let pens_location_on_penpal =
		Location::try_from(PenpalLocalTeleportableToAssetHub::get()).unwrap();
	let pens_id_on_penpal = match pens_location_on_penpal.last() {
		Some(Junction::GeneralIndex(id)) => *id as u32,
		_ => unreachable!(),
	};

	let penpal_parachain_junction = Junction::Parachain(PenpalB::para_id().into());
	let pens_at_ahw = Location::new(
		1,
		pens_location_on_penpal
			.interior()
			.clone()
			.pushed_front_with(penpal_parachain_junction)
			.unwrap(),
	);
	let pens_at_rococo_parachains = Location::new(
		2,
		pens_at_ahw
			.interior()
			.clone()
			.pushed_front_with(Junction::GlobalConsensus(NetworkId::Westend))
			.unwrap(),
	);
	let wnds_to_send = amount;
	let pens_to_send = amount;

	// ---------- Set up Penpal Westend ----------
	// Fund Penpal's sender account. No need to create the asset (only mint), it exists in genesis.
	PenpalB::mint_asset(
		<PenpalB as Chain>::RuntimeOrigin::signed(owner.clone()),
		pens_id_on_penpal,
		sender.clone(),
		pens_to_send * 2,
	);
	// fund Penpal's check account to be able to teleport
	PenpalB::fund_accounts(vec![(penpal_check_account.clone().into(), pens_to_send * 2)]);

	// ---------- Set up Asset Hub Rococo ----------
	// create PEN at AHR
	AssetHubRococo::force_create_foreign_asset(
		pens_at_rococo_parachains.clone(),
		owner.clone(),
		false,
		ASSET_MIN_BALANCE,
		vec![],
	);

	// account balances before
	let sender_wnds_before = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(
			wnd_at_westend_parachains.clone().into(),
			&PenpalBSender::get(),
		)
	});
	let sender_pens_before = PenpalB::execute_with(|| {
		type Assets = <PenpalB as PenpalBPallet>::Assets;
		<Assets as Inspect<_>>::balance(pens_id_on_penpal, &PenpalBSender::get())
	});
	let sov_ahr_on_ahw = AssetHubWestend::sovereign_account_of_parachain_on_other_global_consensus(
		Rococo,
		AssetHubRococo::para_id(),
	);
	let wnds_in_reserve_on_ahw_before =
		<AssetHubWestend as Chain>::account_data_of(sov_ahr_on_ahw.clone()).free;
	let pens_in_reserve_on_ahw_before = AssetHubWestend::execute_with(|| {
		type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(pens_at_ahw.clone(), &sov_ahr_on_ahw)
	});
	let receiver_wnds_before = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(
			wnd_at_rococo_parachains.clone(),
			&AssetHubRococoReceiver::get(),
		)
	});
	let receiver_pens_before = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(
			pens_at_rococo_parachains.clone(),
			&AssetHubRococoReceiver::get(),
		)
	});

	// transfer assets
	do_send_pens_and_wnds_from_penpal_westend_via_ahw_to_asset_hub_rococo(
		(wnd_at_westend_parachains.clone(), wnds_to_send),
		(pens_location_on_penpal.try_into().unwrap(), pens_to_send),
	);

	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubRococo,
			vec![
				// issue WNDs on AHR
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
					asset_id: *asset_id == wnd_at_westend_parachains.clone().try_into().unwrap(),
					owner: *owner == AssetHubRococoReceiver::get(),
				},
				// message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	});

	// account balances after
	let sender_wnds_after = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(
			wnd_at_westend_parachains.into(),
			&PenpalBSender::get(),
		)
	});
	let sender_pens_after = PenpalB::execute_with(|| {
		type Assets = <PenpalB as PenpalBPallet>::Assets;
		<Assets as Inspect<_>>::balance(pens_id_on_penpal, &PenpalBSender::get())
	});
	let wnds_in_reserve_on_ahw_after =
		<AssetHubWestend as Chain>::account_data_of(sov_ahr_on_ahw.clone()).free;
	let pens_in_reserve_on_ahw_after = AssetHubWestend::execute_with(|| {
		type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(pens_at_ahw, &sov_ahr_on_ahw)
	});
	let receiver_wnds_after = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(
			wnd_at_rococo_parachains.clone(),
			&AssetHubRococoReceiver::get(),
		)
	});
	let receiver_pens_after = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(pens_at_rococo_parachains, &AssetHubRococoReceiver::get())
	});

	// Sender's balance is reduced
	assert!(sender_wnds_after < sender_wnds_before);
	// Receiver's balance is increased
	assert!(receiver_wnds_after > receiver_wnds_before);
	// Reserve balance is increased by sent amount (less fess)
	assert!(wnds_in_reserve_on_ahw_after > wnds_in_reserve_on_ahw_before);
	assert!(wnds_in_reserve_on_ahw_after <= wnds_in_reserve_on_ahw_before + wnds_to_send);

	// Sender's balance is reduced by sent amount
	assert_eq!(sender_pens_after, sender_pens_before - pens_to_send);
	// Reserve balance is increased by sent amount
	assert_eq!(pens_in_reserve_on_ahw_after, pens_in_reserve_on_ahw_before + pens_to_send);
	// Receiver's balance is increased by sent amount
	assert_eq!(receiver_pens_after, receiver_pens_before + pens_to_send);
}
