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

use crate::{create_pool_with_wnd_on, foreign_balance_on, imports::*};

fn para_to_para_assethub_hop_assertions(t: ParaToParaThroughAHTest) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
	let sov_penpal_a_on_ah = AssetHubWestend::sovereign_account_id_of(
		AssetHubWestend::sibling_location_of(PenpalA::para_id()),
	);
	let sov_penpal_b_on_ah = AssetHubWestend::sovereign_account_id_of(
		AssetHubWestend::sibling_location_of(PenpalB::para_id()),
	);

	assert_expected_events!(
		AssetHubWestend,
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
	let asset_hub_location: Location = PenpalA::sibling_location_of(AssetHubWestend::para_id());
	let custom_xcm_on_dest = Xcm::<()>(vec![DepositAsset {
		assets: Wild(AllCounted(t.args.assets.len() as u32)),
		beneficiary: t.args.beneficiary,
	}]);
	<PenpalA as PenpalAPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.assets.into()),
		bx!(TransferType::RemoteReserve(asset_hub_location.clone().into())),
		bx!(fee.id.into()),
		bx!(TransferType::RemoteReserve(asset_hub_location.into())),
		bx!(VersionedXcm::from(custom_xcm_on_dest)),
		t.args.weight_limit,
	)
}

fn para_to_para_asset_hub_hop_assertions(t: ParaToParaThroughAHTest) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
	let sov_penpal_a_on_ah = AssetHubWestend::sovereign_account_id_of(
		AssetHubWestend::sibling_location_of(PenpalA::para_id()),
	);

	let (_, asset_amount) = fee_asset(&t.args.assets, t.args.fee_asset_item as usize).unwrap();

	assert_expected_events!(
		AssetHubWestend,
		vec![
			// Withdrawn from sender parachain SA
			RuntimeEvent::Assets(
				pallet_assets::Event::Burned { owner, balance, .. }
			) => {
				owner: *owner == sov_penpal_a_on_ah,
				balance: *balance == asset_amount,
			},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn para_to_para_through_asset_hub_limited_reserve_transfer_assets(
	t: ParaToParaThroughAHTest,
) -> DispatchResult {
	<PenpalA as PenpalAPallet>::PolkadotXcm::limited_reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

pub fn para_to_para_through_hop_sender_assertions<Hop: Clone>(t: Test<PenpalA, PenpalB, Hop>) {
	type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
	PenpalA::assert_xcm_pallet_attempted_complete(None);

	for asset in t.args.assets.into_inner() {
		let expected_id = asset.id.0.clone().try_into().unwrap();
		let amount = if let Fungible(a) = asset.fun { Some(a) } else { None }.unwrap();
		assert_expected_events!(
			PenpalA,
			vec![
				// Amount to reserve transfer is transferred to Parachain's Sovereign account
				RuntimeEvent::ForeignAssets(
					pallet_assets::Event::Burned { asset_id, owner, balance },
				) => {
					asset_id: *asset_id == expected_id,
					owner: *owner == t.sender.account_id,
					balance: *balance == amount,
				},
			]
		);
	}
}

pub fn para_to_para_through_hop_receiver_assertions<Hop: Clone>(t: Test<PenpalA, PenpalB, Hop>) {
	type RuntimeEvent = <PenpalB as Chain>::RuntimeEvent;

	PenpalB::assert_xcmp_queue_success(None);
	for asset in t.args.assets.into_inner().into_iter() {
		let expected_id = asset.id.0.try_into().unwrap();
		assert_expected_events!(
			PenpalB,
			vec![
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
					asset_id: *asset_id == expected_id,
					owner: *owner == t.receiver.account_id,
				},
			]
		);
	}
}

// ==============================================================================
// ===== Transfer - Native + Bridged Assets - Parachain->AssetHub->Parachain ====
// ==============================================================================
/// Transfers of native asset plus bridged asset from Parachain to Parachain
/// (through AssetHub reserve) with fees paid using native asset.
#[test]
fn transfer_foreign_assets_from_para_to_para_through_asset_hub() {
	// Init values for Parachain Origin
	let destination = PenpalA::sibling_location_of(PenpalB::para_id());
	let sender = PenpalASender::get();
	let wnd_to_send: Balance = WESTEND_ED * 10000;
	let assets_owner = PenpalAssetOwner::get();
	let wnd_location = RelayLocation::get();
	let sender_as_seen_by_ah = AssetHubWestend::sibling_location_of(PenpalA::para_id());
	let sov_of_sender_on_ah = AssetHubWestend::sovereign_account_id_of(sender_as_seen_by_ah);
	let receiver_as_seen_by_ah = AssetHubWestend::sibling_location_of(PenpalB::para_id());
	let sov_of_receiver_on_ah = AssetHubWestend::sovereign_account_id_of(receiver_as_seen_by_ah);
	let roc_to_send = ASSET_HUB_WESTEND_ED * 10_000_000;

	// Configure source and destination chains to trust AH as reserve of ROC
	PenpalA::execute_with(|| {
		assert_ok!(<PenpalA as Chain>::System::set_storage(
			<PenpalA as Chain>::RuntimeOrigin::root(),
			vec![(
				PenpalCustomizableAssetFromSystemAssetHub::key().to_vec(),
				Location::new(2, [GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH))]).encode(),
			)],
		));
	});
	PenpalB::execute_with(|| {
		assert_ok!(<PenpalB as Chain>::System::set_storage(
			<PenpalB as Chain>::RuntimeOrigin::root(),
			vec![(
				PenpalCustomizableAssetFromSystemAssetHub::key().to_vec(),
				Location::new(2, [GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH))]).encode(),
			)],
		));
	});

	// Register ROC as foreign asset and transfer it around the Westend ecosystem
	let roc_at_westend_parachains =
		Location::new(2, [Junction::GlobalConsensus(NetworkId::ByGenesis(ROCOCO_GENESIS_HASH))]);
	AssetHubWestend::force_create_foreign_asset(
		roc_at_westend_parachains.clone().try_into().unwrap(),
		assets_owner.clone(),
		false,
		ASSET_MIN_BALANCE,
		vec![],
	);
	PenpalA::force_create_foreign_asset(
		roc_at_westend_parachains.clone(),
		assets_owner.clone(),
		false,
		ASSET_MIN_BALANCE,
		vec![],
	);
	PenpalB::force_create_foreign_asset(
		roc_at_westend_parachains.clone(),
		assets_owner.clone(),
		false,
		ASSET_MIN_BALANCE,
		vec![],
	);

	// fund Parachain's sender account
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(assets_owner.clone()),
		wnd_location.clone(),
		sender.clone(),
		wnd_to_send * 2,
	);
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(assets_owner.clone()),
		roc_at_westend_parachains.clone(),
		sender.clone(),
		roc_to_send * 2,
	);
	// fund the Parachain Origin's SA on Asset Hub with the assets held in reserve
	AssetHubWestend::fund_accounts(vec![(sov_of_sender_on_ah.clone().into(), wnd_to_send * 2)]);
	AssetHubWestend::mint_foreign_asset(
		<AssetHubWestend as Chain>::RuntimeOrigin::signed(assets_owner),
		roc_at_westend_parachains.clone().try_into().unwrap(),
		sov_of_sender_on_ah.clone(),
		roc_to_send * 2,
	);

	// Init values for Parachain Destination
	let receiver = PenpalBReceiver::get();

	// Assets to send
	let assets: Vec<Asset> = vec![
		(wnd_location.clone(), wnd_to_send).into(),
		(roc_at_westend_parachains.clone(), roc_to_send).into(),
	];
	let fee_asset_id: AssetId = wnd_location.clone().into();
	let fee_asset_item = assets.iter().position(|a| a.id == fee_asset_id).unwrap() as u32;

	// Init Test
	let test_args = TestContext {
		sender: sender.clone(),
		receiver: receiver.clone(),
		args: TestArgs::new_para(
			destination,
			receiver.clone(),
			wnd_to_send,
			assets.into(),
			None,
			fee_asset_item,
		),
	};
	let mut test = ParaToParaThroughAHTest::new(test_args);

	// Query initial balances
	let sender_wnds_before = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(wnd_location.clone(), &sender)
	});
	let sender_rocs_before = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(roc_at_westend_parachains.clone(), &sender)
	});
	let wnds_in_sender_reserve_on_ah_before =
		<AssetHubWestend as Chain>::account_data_of(sov_of_sender_on_ah.clone()).free;
	let rocs_in_sender_reserve_on_ah_before = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(
			roc_at_westend_parachains.clone().try_into().unwrap(),
			&sov_of_sender_on_ah,
		)
	});
	let wnds_in_receiver_reserve_on_ah_before =
		<AssetHubWestend as Chain>::account_data_of(sov_of_receiver_on_ah.clone()).free;
	let rocs_in_receiver_reserve_on_ah_before = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(
			roc_at_westend_parachains.clone().try_into().unwrap(),
			&sov_of_receiver_on_ah,
		)
	});
	let receiver_wnds_before = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(wnd_location.clone(), &receiver)
	});
	let receiver_rocs_before = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(roc_at_westend_parachains.clone(), &receiver)
	});

	// Set assertions and dispatchables
	test.set_assertion::<PenpalA>(para_to_para_through_hop_sender_assertions);
	test.set_assertion::<AssetHubWestend>(para_to_para_assethub_hop_assertions);
	test.set_assertion::<PenpalB>(para_to_para_through_hop_receiver_assertions);
	test.set_dispatchable::<PenpalA>(para_to_para_transfer_assets_through_ah);
	test.assert();

	// Query final balances
	let sender_wnds_after = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(wnd_location.clone(), &sender)
	});
	let sender_rocs_after = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(roc_at_westend_parachains.clone(), &sender)
	});
	let rocs_in_sender_reserve_on_ah_after = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(
			roc_at_westend_parachains.clone().try_into().unwrap(),
			&sov_of_sender_on_ah,
		)
	});
	let wnds_in_sender_reserve_on_ah_after =
		<AssetHubWestend as Chain>::account_data_of(sov_of_sender_on_ah).free;
	let rocs_in_receiver_reserve_on_ah_after = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(
			roc_at_westend_parachains.clone().try_into().unwrap(),
			&sov_of_receiver_on_ah,
		)
	});
	let wnds_in_receiver_reserve_on_ah_after =
		<AssetHubWestend as Chain>::account_data_of(sov_of_receiver_on_ah).free;
	let receiver_wnds_after = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(wnd_location, &receiver)
	});
	let receiver_rocs_after = PenpalB::execute_with(|| {
		type ForeignAssets = <PenpalB as PenpalBPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(roc_at_westend_parachains, &receiver)
	});

	// Sender's balance is reduced by amount sent.
	assert!(sender_wnds_after < sender_wnds_before - wnd_to_send);
	assert_eq!(sender_rocs_after, sender_rocs_before - roc_to_send);
	// Sovereign accounts on reserve are changed accordingly.
	assert_eq!(
		wnds_in_sender_reserve_on_ah_after,
		wnds_in_sender_reserve_on_ah_before - wnd_to_send
	);
	assert_eq!(
		rocs_in_sender_reserve_on_ah_after,
		rocs_in_sender_reserve_on_ah_before - roc_to_send
	);
	assert!(wnds_in_receiver_reserve_on_ah_after > wnds_in_receiver_reserve_on_ah_before);
	assert_eq!(
		rocs_in_receiver_reserve_on_ah_after,
		rocs_in_receiver_reserve_on_ah_before + roc_to_send
	);
	// Receiver's balance is increased by amount sent minus delivery fees.
	assert!(receiver_wnds_after > receiver_wnds_before);
	assert_eq!(receiver_rocs_after, receiver_rocs_before + roc_to_send);
}

// ===============================================================
// ====== Transfer - Native Asset - Relay->AssetHub->Penpal ======
// ===============================================================
/// Transfers of native asset Relay to Penpal (using AssetHub reserve). Parachains want to avoid
/// managing SAs on all system chains, thus want all their DOT-in-reserve to be held in their
/// Sovereign Account on Asset Hub.
#[test]
fn transfer_native_asset_from_relay_to_penpal_through_asset_hub() {
	// Init values for Relay
	let destination = Westend::child_location_of(PenpalA::para_id());
	let sender = WestendSender::get();
	let amount_to_send: Balance = WESTEND_ED * 1000;

	// Init values for Parachain
	let relay_native_asset_location = RelayLocation::get();
	let receiver = PenpalAReceiver::get();

	// Init Test
	let test_args = TestContext {
		sender,
		receiver: receiver.clone(),
		args: TestArgs::new_relay(destination.clone(), receiver.clone(), amount_to_send),
	};
	let mut test = RelayToParaThroughAHTest::new(test_args);

	let sov_penpal_on_ah = AssetHubWestend::sovereign_account_id_of(
		AssetHubWestend::sibling_location_of(PenpalA::para_id()),
	);
	// Query initial balances
	let sender_balance_before = test.sender.balance;
	let sov_penpal_on_ah_before = AssetHubWestend::execute_with(|| {
		<AssetHubWestend as AssetHubWestendPallet>::Balances::free_balance(sov_penpal_on_ah.clone())
	});
	let receiver_assets_before = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(relay_native_asset_location.clone(), &receiver)
	});

	fn relay_assertions(t: RelayToParaThroughAHTest) {
		type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
		Westend::assert_xcm_pallet_attempted_complete(None);
		assert_expected_events!(
			Westend,
			vec![
				// Amount to teleport is withdrawn from Sender
				RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount }) => {
					who: *who == t.sender.account_id,
					amount: *amount == t.args.amount,
				},
				// Amount to teleport is deposited in Relay's `CheckAccount`
				RuntimeEvent::Balances(pallet_balances::Event::Minted { who, amount }) => {
					who: *who == <Westend as WestendPallet>::XcmPallet::check_account(),
					amount:  *amount == t.args.amount,
				},
			]
		);
	}
	fn asset_hub_assertions(_: RelayToParaThroughAHTest) {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		let sov_penpal_on_ah = AssetHubWestend::sovereign_account_id_of(
			AssetHubWestend::sibling_location_of(PenpalA::para_id()),
		);
		assert_expected_events!(
			AssetHubWestend,
			vec![
				// Deposited to receiver parachain SA
				RuntimeEvent::Balances(
					pallet_balances::Event::Minted { who, .. }
				) => {
					who: *who == sov_penpal_on_ah,
				},
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: true, .. }
				) => {},
			]
		);
	}
	fn penpal_assertions(t: RelayToParaThroughAHTest) {
		type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
		assert_expected_events!(
			PenpalA,
			vec![
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
					asset_id: *asset_id == Location::new(1, Here),
					owner: *owner == t.receiver.account_id,
				},
			]
		);
	}
	fn transfer_assets_dispatchable(t: RelayToParaThroughAHTest) -> DispatchResult {
		let fee_idx = t.args.fee_asset_item as usize;
		let fee: Asset = t.args.assets.inner().get(fee_idx).cloned().unwrap();
		let asset_hub_location = Westend::child_location_of(AssetHubWestend::para_id());
		let context = WestendUniversalLocation::get();

		// reanchor fees to the view of destination (Penpal)
		let mut remote_fees = fee.clone().reanchored(&t.args.dest, &context).unwrap();
		if let Fungible(ref mut amount) = remote_fees.fun {
			// we already spent some fees along the way, just use half of what we started with
			*amount = *amount / 2;
		}
		let xcm_on_final_dest = Xcm::<()>(vec![
			BuyExecution { fees: remote_fees, weight_limit: t.args.weight_limit.clone() },
			DepositAsset {
				assets: Wild(AllCounted(t.args.assets.len() as u32)),
				beneficiary: t.args.beneficiary,
			},
		]);

		// reanchor final dest (Penpal) to the view of hop (Asset Hub)
		let mut dest = t.args.dest.clone();
		dest.reanchor(&asset_hub_location, &context).unwrap();
		// on Asset Hub, forward assets to Penpal
		let xcm_on_hop = Xcm::<()>(vec![DepositReserveAsset {
			assets: Wild(AllCounted(t.args.assets.len() as u32)),
			dest,
			xcm: xcm_on_final_dest,
		}]);

		Dmp::make_parachain_reachable(AssetHubWestend::para_id());

		// First leg is a teleport, from there a local-reserve-transfer to final dest
		<Westend as WestendPallet>::XcmPallet::transfer_assets_using_type_and_then(
			t.signed_origin,
			bx!(asset_hub_location.into()),
			bx!(t.args.assets.into()),
			bx!(TransferType::Teleport),
			bx!(fee.id.into()),
			bx!(TransferType::Teleport),
			bx!(VersionedXcm::from(xcm_on_hop)),
			t.args.weight_limit,
		)
	}

	// Set assertions and dispatchables
	test.set_assertion::<Westend>(relay_assertions);
	test.set_assertion::<AssetHubWestend>(asset_hub_assertions);
	test.set_assertion::<PenpalA>(penpal_assertions);
	test.set_dispatchable::<Westend>(transfer_assets_dispatchable);
	test.assert();

	// Query final balances
	let sender_balance_after = test.sender.balance;
	let sov_penpal_on_ah_after = AssetHubWestend::execute_with(|| {
		<AssetHubWestend as AssetHubWestendPallet>::Balances::free_balance(sov_penpal_on_ah)
	});
	let receiver_assets_after = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(relay_native_asset_location, &receiver)
	});

	// Sender's balance is reduced by amount sent plus delivery fees
	assert!(sender_balance_after < sender_balance_before - amount_to_send);
	// SA on AH balance is increased
	assert!(sov_penpal_on_ah_after > sov_penpal_on_ah_before);
	// Receiver's asset balance is increased
	assert!(receiver_assets_after > receiver_assets_before);
	// Receiver's asset balance increased by `amount_to_send - delivery_fees - bought_execution`;
	// `delivery_fees` might be paid from transfer or JIT, also `bought_execution` is unknown but
	// should be non-zero
	assert!(receiver_assets_after < receiver_assets_before + amount_to_send);
}

// ===============================================================
// ===== Transfer - Native Asset - Penpal->AssetHub->Relay =======
// ===============================================================
/// Transfers of native asset Penpal to Relay (using AssetHub reserve). Parachains want to avoid
/// managing SAs on all system chains, thus want all their DOT-in-reserve to be held in their
/// Sovereign Account on Asset Hub.
#[test]
fn transfer_native_asset_from_penpal_to_relay_through_asset_hub() {
	// Init values for Penpal
	let destination = RelayLocation::get();
	let sender = PenpalASender::get();
	let amount_to_send: Balance = WESTEND_ED * 100;

	// Init values for Penpal
	let relay_native_asset_location = RelayLocation::get();
	let receiver = WestendReceiver::get();

	// Init Test
	let test_args = TestContext {
		sender: sender.clone(),
		receiver: receiver.clone(),
		args: TestArgs::new_para(
			destination.clone(),
			receiver.clone(),
			amount_to_send,
			(Parent, amount_to_send).into(),
			None,
			0,
		),
	};
	let mut test = PenpalToRelayThroughAHTest::new(test_args);

	let sov_penpal_on_ah = AssetHubWestend::sovereign_account_id_of(
		AssetHubWestend::sibling_location_of(PenpalA::para_id()),
	);
	// fund Penpal's sender account
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(PenpalAssetOwner::get()),
		relay_native_asset_location.clone(),
		sender.clone(),
		amount_to_send * 2,
	);
	// fund Penpal's SA on AssetHub with the assets held in reserve
	AssetHubWestend::fund_accounts(vec![(sov_penpal_on_ah.clone().into(), amount_to_send * 2)]);

	// prefund Relay checking account so we accept teleport "back" from AssetHub
	let check_account =
		Westend::execute_with(|| <Westend as WestendPallet>::XcmPallet::check_account());
	Westend::fund_accounts(vec![(check_account, amount_to_send)]);

	// Query initial balances
	let sender_balance_before = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(relay_native_asset_location.clone(), &sender)
	});
	let sov_penpal_on_ah_before = AssetHubWestend::execute_with(|| {
		<AssetHubWestend as AssetHubWestendPallet>::Balances::free_balance(sov_penpal_on_ah.clone())
	});
	let receiver_balance_before = Westend::execute_with(|| {
		<Westend as WestendPallet>::Balances::free_balance(receiver.clone())
	});

	fn transfer_assets_dispatchable(t: PenpalToRelayThroughAHTest) -> DispatchResult {
		let fee_idx = t.args.fee_asset_item as usize;
		let fee: Asset = t.args.assets.inner().get(fee_idx).cloned().unwrap();
		let asset_hub_location = PenpalA::sibling_location_of(AssetHubWestend::para_id());
		let context = PenpalUniversalLocation::get();

		// reanchor fees to the view of destination (Westend Relay)
		let mut remote_fees = fee.clone().reanchored(&t.args.dest, &context).unwrap();
		if let Fungible(ref mut amount) = remote_fees.fun {
			// we already spent some fees along the way, just use half of what we started with
			*amount = *amount / 2;
		}
		let xcm_on_final_dest = Xcm::<()>(vec![
			BuyExecution { fees: remote_fees, weight_limit: t.args.weight_limit.clone() },
			DepositAsset {
				assets: Wild(AllCounted(t.args.assets.len() as u32)),
				beneficiary: t.args.beneficiary,
			},
		]);

		// reanchor final dest (Westend Relay) to the view of hop (Asset Hub)
		let mut dest = t.args.dest.clone();
		dest.reanchor(&asset_hub_location, &context).unwrap();
		// on Asset Hub
		let xcm_on_hop = Xcm::<()>(vec![InitiateTeleport {
			assets: Wild(AllCounted(t.args.assets.len() as u32)),
			dest,
			xcm: xcm_on_final_dest,
		}]);

		// First leg is a reserve-withdraw, from there a teleport to final dest
		<PenpalA as PenpalAPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
			t.signed_origin,
			bx!(asset_hub_location.into()),
			bx!(t.args.assets.into()),
			bx!(TransferType::DestinationReserve),
			bx!(fee.id.into()),
			bx!(TransferType::DestinationReserve),
			bx!(VersionedXcm::from(xcm_on_hop)),
			t.args.weight_limit,
		)
	}
	test.set_dispatchable::<PenpalA>(transfer_assets_dispatchable);
	test.assert();

	// Query final balances
	let sender_balance_after = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(relay_native_asset_location.clone(), &sender)
	});
	let sov_penpal_on_ah_after = AssetHubWestend::execute_with(|| {
		<AssetHubWestend as AssetHubWestendPallet>::Balances::free_balance(sov_penpal_on_ah.clone())
	});
	let receiver_balance_after = Westend::execute_with(|| {
		<Westend as WestendPallet>::Balances::free_balance(receiver.clone())
	});

	// Sender's asset balance is reduced by amount sent plus delivery fees
	assert!(sender_balance_after < sender_balance_before - amount_to_send);
	// SA on AH balance is decreased by `amount_to_send`
	assert_eq!(sov_penpal_on_ah_after, sov_penpal_on_ah_before - amount_to_send);
	// Receiver's balance is increased
	assert!(receiver_balance_after > receiver_balance_before);
	// Receiver's balance increased by `amount_to_send - delivery_fees - bought_execution`;
	// `delivery_fees` might be paid from transfer or JIT, also `bought_execution` is unknown but
	// should be non-zero
	assert!(receiver_balance_after < receiver_balance_before + amount_to_send);
}

// ===================================================================================
// == Reserve Transfers USDT - Parachain->AssetHub->Parachain - pay fees using pool ==
// ===================================================================================
//
// Transfer USDT From Penpal A to Penpal B with AssetHub as the reserve, while paying fees using
// USDT by making use of existing USDT pools on AssetHub and destination.
#[test]
fn reserve_transfer_usdt_from_para_to_para_through_asset_hub() {
	let destination = PenpalA::sibling_location_of(PenpalB::para_id());
	let sender = PenpalASender::get();
	let asset_amount_to_send: Balance = WESTEND_ED * 10000;
	let fee_amount_to_send: Balance = WESTEND_ED * 10000;
	let sender_chain_as_seen_by_asset_hub =
		AssetHubWestend::sibling_location_of(PenpalA::para_id());
	let sov_of_sender_on_asset_hub =
		AssetHubWestend::sovereign_account_id_of(sender_chain_as_seen_by_asset_hub);
	let receiver_as_seen_by_asset_hub = AssetHubWestend::sibling_location_of(PenpalB::para_id());
	let sov_of_receiver_on_asset_hub =
		AssetHubWestend::sovereign_account_id_of(receiver_as_seen_by_asset_hub);

	// Create SA-of-Penpal-on-AHW with ED.
	// This ED isn't reflected in any derivative in a PenpalA account.
	AssetHubWestend::fund_accounts(vec![
		(sov_of_sender_on_asset_hub.clone().into(), ASSET_HUB_WESTEND_ED),
		(sov_of_receiver_on_asset_hub.clone().into(), ASSET_HUB_WESTEND_ED),
	]);

	// Give USDT to sov account of sender.
	let usdt_id: u32 = 1984;
	AssetHubWestend::execute_with(|| {
		use frame_support::traits::tokens::fungibles::Mutate;
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
		assert_ok!(<Assets as Mutate<_>>::mint_into(
			usdt_id.into(),
			&sov_of_sender_on_asset_hub.clone().into(),
			asset_amount_to_send + fee_amount_to_send,
		));
	});

	// We create a pool between WND and USDT in AssetHub.
	let usdt = Location::new(
		0,
		[Junction::PalletInstance(ASSETS_PALLET_ID), Junction::GeneralIndex(usdt_id.into())],
	);
	create_pool_with_wnd_on!(AssetHubWestend, usdt, false, AssetHubWestendSender::get());
	// We also need a pool between WND and USDT on PenpalB.
	create_pool_with_wnd_on!(PenpalB, PenpalUsdtFromAssetHub::get(), true, PenpalAssetOwner::get());

	let usdt_from_asset_hub = PenpalUsdtFromAssetHub::get();
	PenpalA::execute_with(|| {
		use frame_support::traits::tokens::fungibles::Mutate;
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		assert_ok!(<ForeignAssets as Mutate<_>>::mint_into(
			usdt_from_asset_hub.clone(),
			&sender,
			asset_amount_to_send + fee_amount_to_send,
		));
	});

	// Prepare assets to transfer.
	let assets: Assets =
		(usdt_from_asset_hub.clone(), asset_amount_to_send + fee_amount_to_send).into();
	// Just to be very specific we're not including anything other than USDT.
	assert_eq!(assets.len(), 1);

	// Give the sender enough Relay tokens to pay for local delivery fees.
	// TODO(https://github.com/paritytech/polkadot-sdk/issues/5160): When we support local delivery fee payment in other assets, we don't need this.
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(PenpalAssetOwner::get()),
		RelayLocation::get(),
		sender.clone(),
		10_000_000_000_000, // Large estimate to make sure it works.
	);

	// Init values for Parachain Destination
	let receiver = PenpalBReceiver::get();

	// Init Test
	let fee_asset_index = 0;
	let test_args = TestContext {
		sender: sender.clone(),
		receiver: receiver.clone(),
		args: TestArgs::new_para(
			destination,
			receiver.clone(),
			asset_amount_to_send,
			assets,
			None,
			fee_asset_index,
		),
	};
	let mut test = ParaToParaThroughAHTest::new(test_args);

	// Query initial balances
	let sender_assets_before = foreign_balance_on!(PenpalA, usdt_from_asset_hub.clone(), &sender);
	let receiver_assets_before =
		foreign_balance_on!(PenpalB, usdt_from_asset_hub.clone(), &receiver);
	test.set_assertion::<PenpalA>(para_to_para_through_hop_sender_assertions);
	test.set_assertion::<AssetHubWestend>(para_to_para_asset_hub_hop_assertions);
	test.set_assertion::<PenpalB>(para_to_para_through_hop_receiver_assertions);
	test.set_dispatchable::<PenpalA>(
		para_to_para_through_asset_hub_limited_reserve_transfer_assets,
	);
	test.assert();

	// Query final balances
	let sender_assets_after = foreign_balance_on!(PenpalA, usdt_from_asset_hub.clone(), &sender);
	let receiver_assets_after = foreign_balance_on!(PenpalB, usdt_from_asset_hub, &receiver);

	// Sender's balance is reduced by amount
	assert!(sender_assets_after < sender_assets_before - asset_amount_to_send);
	// Receiver's balance is increased
	assert!(receiver_assets_after > receiver_assets_before);
}
