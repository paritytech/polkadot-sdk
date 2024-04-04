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

use super::reserve_transfer::{
	para_to_para_through_hop_receiver_assertions, para_to_para_through_hop_sender_assertions,
};
use crate::imports::*;

fn para_to_para_assethub_hop_assertions(t: ParaToParaThroughAHTest) {
	type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
	let sov_penpal_a_on_ah = AssetHubRococo::sovereign_account_id_of(
		AssetHubRococo::sibling_location_of(PenpalA::para_id()),
	);
	let sov_penpal_b_on_ah = AssetHubRococo::sovereign_account_id_of(
		AssetHubRococo::sibling_location_of(PenpalB::para_id()),
	);

	assert_expected_events!(
		AssetHubRococo,
		vec![
			// Withdrawn from sender parachain SA
			RuntimeEvent::Balances(
				pallet_balances::Event::Burned { who, amount }
			) => {
				who: *who == sov_penpal_a_on_ah,
				amount: *amount == t.args.amount,
			},
			// Deposited to receiver parachain SA
			RuntimeEvent::Balances(
				pallet_balances::Event::Minted { who, .. }
			) => {
				who: *who == sov_penpal_b_on_ah,
			},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn para_to_para_transfer_assets_through_ah(t: ParaToParaThroughAHTest) -> DispatchResult {
	let fee_idx = t.args.fee_asset_item as usize;
	let fee: Asset = t.args.assets.inner().get(fee_idx).cloned().unwrap();
	let asset_hub_location: Location = PenpalA::sibling_location_of(AssetHubRococo::para_id());
	<PenpalA as PenpalAPallet>::PolkadotXcm::transfer_assets_using_reserve(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		bx!(TransferType::RemoteReserve(asset_hub_location.clone().into())),
		bx!(fee.into()),
		bx!(TransferType::RemoteReserve(asset_hub_location.into())),
		t.args.weight_limit,
	)
}

// ======================================================================================
// ===== Reserve Transfers - Native + Bridged Asset - Parachain<>AssetHub<>Parachain ====
// ======================================================================================
/// Reserve Transfers of native asset plus bridged asset from Parachain to Parachain
/// (through AssetHub reserve) should work - fees are paid using native asset
#[test]
fn reserve_transfer_native_asset_from_para_to_para_through_asset_hub() {
	// Init values for Parachain Origin
	let destination = PenpalA::sibling_location_of(PenpalB::para_id());
	let sender = PenpalASender::get();
	let roc_to_send: Balance = ROCOCO_ED * 10000;
	let penpal_roc_owner = PenpalAssetOwner::get();
	let roc_location = v3::Location::try_from(RelayLocation::get()).unwrap();
	let roc_location_latest: Location = roc_location.try_into().unwrap();
	let sender_as_seen_by_ah = AssetHubRococo::sibling_location_of(PenpalA::para_id());
	let sov_of_sender_on_ah = AssetHubRococo::sovereign_account_id_of(sender_as_seen_by_ah);
	let receiver_as_seen_by_ah = AssetHubRococo::sibling_location_of(PenpalB::para_id());
	let sov_of_receiver_on_ah = AssetHubRococo::sovereign_account_id_of(receiver_as_seen_by_ah);
	let wnd_to_send = ASSET_HUB_ROCOCO_ED * 10_000_000;

	// Configure destination chain to trust AH as reserve of WND
	PenpalB::execute_with(|| {
		assert_ok!(<PenpalB as Chain>::System::set_storage(
			<PenpalB as Chain>::RuntimeOrigin::root(),
			vec![(
				penpal_runtime::xcm_config::CustomizableAssetFromSystemAssetHub::key().to_vec(),
				Location::new(2, [GlobalConsensus(Westend)]).encode(),
			)],
		));
	});

	// Register WND as foreign asset and transfer it around the Rococo ecosystem
	let wnd_at_rococo_parachains =
		v3::Location::new(2, [v3::Junction::GlobalConsensus(v3::NetworkId::Westend)]);
	let wnd_at_rococo_parachains_latest: Location = wnd_at_rococo_parachains.try_into().unwrap();
	let owner = AssetHubRococo::account_id_of(emulated_integration_tests_common::accounts::ALICE);
	AssetHubRococo::force_create_foreign_asset(
		wnd_at_rococo_parachains,
		owner.clone(),
		false,
		ASSET_MIN_BALANCE,
		vec![],
	);
	PenpalA::force_create_foreign_asset(
		wnd_at_rococo_parachains,
		owner.clone(),
		false,
		ASSET_MIN_BALANCE,
		vec![],
	);
	PenpalB::force_create_foreign_asset(
		wnd_at_rococo_parachains,
		owner.clone(),
		false,
		ASSET_MIN_BALANCE,
		vec![],
	);

	// fund Parachain's sender account
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(penpal_roc_owner),
		roc_location,
		sender.clone(),
		roc_to_send * 2,
	);
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(owner.clone()),
		wnd_at_rococo_parachains,
		sender.clone(),
		wnd_to_send * 2,
	);
	// fund the Parachain Origin's SA on Asset Hub with the assets held in reserve
	AssetHubRococo::fund_accounts(vec![(sov_of_sender_on_ah.clone().into(), roc_to_send * 2)]);
	AssetHubRococo::mint_foreign_asset(
		<AssetHubRococo as Chain>::RuntimeOrigin::signed(owner.clone()),
		wnd_at_rococo_parachains,
		sov_of_sender_on_ah.clone(),
		wnd_to_send * 2,
	);

	// Init values for Parachain Destination
	let receiver = PenpalBReceiver::get();

	// Assets to send
	let assets: Vec<Asset> = vec![
		(roc_location_latest.clone(), roc_to_send).into(),
		(wnd_at_rococo_parachains_latest, wnd_to_send).into(),
	];
	let fee_asset_id: AssetId = roc_location_latest.into();
	let fee_asset_item = assets.iter().position(|a| a.id == fee_asset_id).unwrap() as u32;

	// Init Test
	let test_args = TestContext {
		sender: sender.clone(),
		receiver: receiver.clone(),
		args: TestArgs::new_para(
			destination,
			receiver.clone(),
			roc_to_send,
			assets.into(),
			None,
			fee_asset_item,
		),
	};
	let mut test = ParaToParaThroughAHTest::new(test_args);

	// Query initial balances
	let sender_rocs_before = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(roc_location, &sender)
	});
	let sender_wnds_before = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(wnd_at_rococo_parachains, &sender)
	});
	let rocs_in_sender_reserve_on_ahr_before =
		<AssetHubRococo as Chain>::account_data_of(sov_of_sender_on_ah.clone()).free;
	let wnds_in_sender_reserve_on_ahr_before = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(wnd_at_rococo_parachains, &sov_of_sender_on_ah)
	});
	let rocs_in_receiver_reserve_on_ahr_before =
		<AssetHubRococo as Chain>::account_data_of(sov_of_receiver_on_ah.clone()).free;
	let wnds_in_receiver_reserve_on_ahr_before = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(wnd_at_rococo_parachains, &sov_of_receiver_on_ah)
	});
	let receiver_rocs_before = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(roc_location, &receiver)
	});
	let receiver_wnds_before = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(wnd_at_rococo_parachains, &receiver)
	});

	// Set assertions and dispatchables
	test.set_assertion::<PenpalA>(para_to_para_through_hop_sender_assertions);
	test.set_assertion::<AssetHubRococo>(para_to_para_assethub_hop_assertions);
	test.set_assertion::<PenpalB>(para_to_para_through_hop_receiver_assertions);
	test.set_dispatchable::<PenpalA>(para_to_para_transfer_assets_through_ah);
	test.assert();

	// Query final balances
	let sender_rocs_after = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(roc_location, &sender)
	});
	let sender_wnds_after = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(wnd_at_rococo_parachains, &sender)
	});
	let wnds_in_sender_reserve_on_ahr_after = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(wnd_at_rococo_parachains, &sov_of_sender_on_ah)
	});
	let rocs_in_sender_reserve_on_ahr_after =
		<AssetHubRococo as Chain>::account_data_of(sov_of_sender_on_ah).free;
	let wnds_in_receiver_reserve_on_ahr_after = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(wnd_at_rococo_parachains, &sov_of_receiver_on_ah)
	});
	let rocs_in_receiver_reserve_on_ahr_after =
		<AssetHubRococo as Chain>::account_data_of(sov_of_receiver_on_ah).free;
	let receiver_rocs_after = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(roc_location, &receiver)
	});
	let receiver_wnds_after = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(wnd_at_rococo_parachains, &receiver)
	});

	// Sender's balance is reduced by amount sent plus delivery fees
	assert!(sender_rocs_after < sender_rocs_before - roc_to_send);
	assert_eq!(sender_wnds_after, sender_wnds_before - wnd_to_send);
	// Sovereign accounts on reserve are changed accordingly
	assert_eq!(
		rocs_in_sender_reserve_on_ahr_after,
		rocs_in_sender_reserve_on_ahr_before - roc_to_send
	);
	assert_eq!(
		wnds_in_sender_reserve_on_ahr_after,
		wnds_in_sender_reserve_on_ahr_before - wnd_to_send
	);
	assert!(rocs_in_receiver_reserve_on_ahr_after > rocs_in_receiver_reserve_on_ahr_before);
	assert_eq!(
		wnds_in_receiver_reserve_on_ahr_after,
		wnds_in_receiver_reserve_on_ahr_before + wnd_to_send
	);
	// Receiver's balance is increased
	assert!(receiver_rocs_after > receiver_rocs_before);
	assert_eq!(receiver_wnds_after, receiver_wnds_before + wnd_to_send);
}
