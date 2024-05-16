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

fn send_asset_from_asset_hub_rococo_to_asset_hub_westend(id: Location, amount: u128) {
	let destination = asset_hub_westend_location();

	// fund the AHR's SA on BHR for paying bridge transport fees
	BridgeHubRococo::fund_para_sovereign(AssetHubRococo::para_id(), 10_000_000_000_000u128);

	// set XCM versions
	AssetHubRococo::force_xcm_version(destination.clone(), XCM_VERSION);
	BridgeHubRococo::force_xcm_version(bridge_hub_westend_location(), XCM_VERSION);

	// send message over bridge
	assert_ok!(send_asset_from_asset_hub_rococo(destination, (id, amount)));
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
fn send_rocs_from_asset_hub_rococo_to_asset_hub_westend() {
	let roc_at_asset_hub_rococo: v3::Location = v3::Parent.into();
	let roc_at_asset_hub_westend =
		v3::Location::new(2, [v3::Junction::GlobalConsensus(v3::NetworkId::Rococo)]);
	let owner: AccountId = AssetHubWestend::account_id_of(ALICE);
	AssetHubWestend::force_create_foreign_asset(
		roc_at_asset_hub_westend,
		owner,
		true,
		ASSET_MIN_BALANCE,
		vec![],
	);
	let sov_ahw_on_ahr = AssetHubRococo::sovereign_account_of_parachain_on_other_global_consensus(
		Westend,
		AssetHubWestend::para_id(),
	);

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		// setup a pool to pay xcm fees with `roc_at_asset_hub_westend` tokens
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			roc_at_asset_hub_westend.into(),
			AssetHubWestendSender::get().into(),
			3_000_000_000_000,
		));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::create_pool(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			Box::new(xcm::v3::Parent.into()),
			Box::new(roc_at_asset_hub_westend),
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { .. }) => {},
			]
		);

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::add_liquidity(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			Box::new(xcm::v3::Parent.into()),
			Box::new(roc_at_asset_hub_westend),
			1_000_000_000_000,
			2_000_000_000_000,
			1,
			1,
			AssetHubWestendSender::get().into()
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {..}) => {},
			]
		);
	});

	let rocs_in_reserve_on_ahr_before =
		<AssetHubRococo as Chain>::account_data_of(sov_ahw_on_ahr.clone()).free;
	let sender_rocs_before =
		<AssetHubRococo as Chain>::account_data_of(AssetHubRococoSender::get()).free;
	let receiver_rocs_before = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(roc_at_asset_hub_westend, &AssetHubWestendReceiver::get())
	});

	let amount = ASSET_HUB_ROCOCO_ED * 1_000_000;
	send_asset_from_asset_hub_rococo_to_asset_hub_westend(
		roc_at_asset_hub_rococo.try_into().unwrap(),
		amount,
	);
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![
				// issue ROCs on AHW
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
					asset_id: *asset_id == roc_at_asset_hub_rococo,
					owner: *owner == AssetHubWestendReceiver::get(),
				},
				// message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	});

	let sender_rocs_after =
		<AssetHubRococo as Chain>::account_data_of(AssetHubRococoSender::get()).free;
	let receiver_rocs_after = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(roc_at_asset_hub_westend, &AssetHubWestendReceiver::get())
	});
	let rocs_in_reserve_on_ahr_after =
		<AssetHubRococo as Chain>::account_data_of(sov_ahw_on_ahr.clone()).free;

	// Sender's balance is reduced
	assert!(sender_rocs_before > sender_rocs_after);
	// Receiver's balance is increased
	assert!(receiver_rocs_after > receiver_rocs_before);
	// Reserve balance is increased by sent amount
	assert_eq!(rocs_in_reserve_on_ahr_after, rocs_in_reserve_on_ahr_before + amount);
}

#[test]
fn send_wnds_from_asset_hub_rococo_to_asset_hub_westend() {
	let prefund_amount = 10_000_000_000_000u128;
	let wnd_at_asset_hub_rococo =
		v3::Location::new(2, [v3::Junction::GlobalConsensus(v3::NetworkId::Westend)]);
	let owner: AccountId = AssetHubRococo::account_id_of(ALICE);
	AssetHubRococo::force_create_foreign_asset(
		wnd_at_asset_hub_rococo,
		owner,
		true,
		ASSET_MIN_BALANCE,
		vec![(AssetHubRococoSender::get(), prefund_amount)],
	);

	// fund the AHR's SA on AHW with the WND tokens held in reserve
	let sov_ahr_on_ahw = AssetHubWestend::sovereign_account_of_parachain_on_other_global_consensus(
		Rococo,
		AssetHubRococo::para_id(),
	);
	AssetHubWestend::fund_accounts(vec![(sov_ahr_on_ahw.clone(), prefund_amount)]);

	let wnds_in_reserve_on_ahw_before =
		<AssetHubWestend as Chain>::account_data_of(sov_ahr_on_ahw.clone()).free;
	assert_eq!(wnds_in_reserve_on_ahw_before, prefund_amount);
	let sender_wnds_before = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(wnd_at_asset_hub_rococo, &AssetHubRococoSender::get())
	});
	assert_eq!(sender_wnds_before, prefund_amount);
	let receiver_wnds_before =
		<AssetHubWestend as Chain>::account_data_of(AssetHubWestendReceiver::get()).free;

	let amount_to_send = ASSET_HUB_WESTEND_ED * 1_000;
	send_asset_from_asset_hub_rococo_to_asset_hub_westend(
		Location::try_from(wnd_at_asset_hub_rococo).unwrap(),
		amount_to_send,
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
					who: *who == AssetHubWestendReceiver::get(),
				},
				// message processed successfully
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	});

	let sender_wnds_after = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(wnd_at_asset_hub_rococo, &AssetHubRococoSender::get())
	});
	let receiver_wnds_after =
		<AssetHubWestend as Chain>::account_data_of(AssetHubWestendReceiver::get()).free;
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
	let roc_at_rococo_parachains: Location = Parent.into();
	let roc_at_asset_hub_westend = Location::new(2, [Junction::GlobalConsensus(NetworkId::Rococo)]);
	let owner: AccountId = AssetHubWestend::account_id_of(ALICE);
	AssetHubWestend::force_create_foreign_asset(
		roc_at_asset_hub_westend.clone().try_into().unwrap(),
		owner,
		true,
		ASSET_MIN_BALANCE,
		vec![],
	);
	let sov_ahw_on_ahr = AssetHubRococo::sovereign_account_of_parachain_on_other_global_consensus(
		Westend,
		AssetHubWestend::para_id(),
	);

	let amount = ASSET_HUB_ROCOCO_ED * 10_000_000;
	let penpal_location = AssetHubRococo::sibling_location_of(PenpalA::para_id());
	let sov_penpal_on_ahr = AssetHubRococo::sovereign_account_id_of(penpal_location);
	// fund Penpal's sovereign account on AssetHub
	AssetHubRococo::fund_accounts(vec![(sov_penpal_on_ahr.into(), amount * 2)]);
	// fund Penpal's sender account
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(PenpalAssetOwner::get()),
		roc_at_rococo_parachains.clone(),
		PenpalASender::get(),
		amount * 2,
	);

	let rocs_in_reserve_on_ahr_before =
		<AssetHubRococo as Chain>::account_data_of(sov_ahw_on_ahr.clone()).free;
	let sender_rocs_before = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(
			roc_at_rococo_parachains.clone(),
			&PenpalASender::get(),
		)
	});
	let receiver_rocs_before = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(
			roc_at_asset_hub_westend.clone().try_into().unwrap(),
			&AssetHubWestendReceiver::get(),
		)
	});
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
					owner: *owner == AssetHubWestendReceiver::get(),
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
		<ForeignAssets as Inspect<_>>::balance(roc_at_rococo_parachains, &PenpalASender::get())
	});
	let receiver_rocs_after = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(
			roc_at_asset_hub_westend.try_into().unwrap(),
			&AssetHubWestendReceiver::get(),
		)
	});
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
