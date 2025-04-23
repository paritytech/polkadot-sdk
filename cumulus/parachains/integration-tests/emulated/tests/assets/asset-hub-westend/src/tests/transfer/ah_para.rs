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
use sp_core::{crypto::get_public_from_string_or_panic, sr25519};

fn ah_to_para_transfer_assets(t: SystemParaToParaTest) -> DispatchResult {
	let fee_idx = t.args.fee_asset_item as usize;
	let fee: Asset = t.args.assets.inner().get(fee_idx).cloned().unwrap();
	let custom_xcm_on_dest = Xcm::<()>(vec![DepositAsset {
		assets: Wild(AllCounted(t.args.assets.len() as u32)),
		beneficiary: t.args.beneficiary,
	}]);
	<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.assets.into()),
		bx!(TransferType::LocalReserve),
		bx!(fee.id.into()),
		bx!(TransferType::LocalReserve),
		bx!(VersionedXcm::from(custom_xcm_on_dest)),
		t.args.weight_limit,
	)
}

fn para_to_ah_transfer_assets(t: ParaToSystemParaTest) -> DispatchResult {
	let fee_idx = t.args.fee_asset_item as usize;
	let fee: Asset = t.args.assets.inner().get(fee_idx).cloned().unwrap();
	let custom_xcm_on_dest = Xcm::<()>(vec![DepositAsset {
		assets: Wild(AllCounted(t.args.assets.len() as u32)),
		beneficiary: t.args.beneficiary,
	}]);
	<PenpalA as PenpalAPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.assets.into()),
		bx!(TransferType::DestinationReserve),
		bx!(fee.id.into()),
		bx!(TransferType::DestinationReserve),
		bx!(VersionedXcm::from(custom_xcm_on_dest)),
		t.args.weight_limit,
	)
}

fn para_to_asset_hub_teleport_foreign_assets(t: ParaToSystemParaTest) -> DispatchResult {
	let fee_idx = t.args.fee_asset_item as usize;
	let fee: Asset = t.args.assets.inner().get(fee_idx).cloned().unwrap();
	let custom_xcm_on_dest = Xcm::<()>(vec![DepositAsset {
		assets: Wild(AllCounted(t.args.assets.len() as u32)),
		beneficiary: t.args.beneficiary,
	}]);
	<PenpalA as PenpalAPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.assets.into()),
		bx!(TransferType::Teleport),
		bx!(fee.id.into()),
		bx!(TransferType::DestinationReserve),
		bx!(VersionedXcm::from(custom_xcm_on_dest)),
		t.args.weight_limit,
	)
}

fn asset_hub_to_para_teleport_foreign_assets(t: SystemParaToParaTest) -> DispatchResult {
	let fee_idx = t.args.fee_asset_item as usize;
	let fee: Asset = t.args.assets.inner().get(fee_idx).cloned().unwrap();
	let custom_xcm_on_dest = Xcm::<()>(vec![DepositAsset {
		assets: Wild(AllCounted(t.args.assets.len() as u32)),
		beneficiary: t.args.beneficiary,
	}]);
	<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.assets.into()),
		bx!(TransferType::Teleport),
		bx!(fee.id.into()),
		bx!(TransferType::LocalReserve),
		bx!(VersionedXcm::from(custom_xcm_on_dest)),
		t.args.weight_limit,
	)
}

fn system_para_to_para_sender_assertions(t: SystemParaToParaTest) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
	AssetHubWestend::assert_xcm_pallet_attempted_complete(None);

	let sov_acc_of_dest = AssetHubWestend::sovereign_account_id_of(t.args.dest.clone());
	for asset in t.args.assets.into_inner().into_iter() {
		let expected_id = asset.id.0.clone().try_into().unwrap();
		let asset_amount = if let Fungible(a) = asset.fun { Some(a) } else { None }.unwrap();
		if asset.id == AssetId(Location::new(1, [])) {
			assert_expected_events!(
				AssetHubWestend,
				vec![
					// Amount of native asset is transferred to Parachain's Sovereign account
					RuntimeEvent::Balances(
						pallet_balances::Event::Transfer { from, to, amount }
					) => {
						from: *from == t.sender.account_id,
						to: *to == sov_acc_of_dest,
						amount: *amount == asset_amount,
					},
				]
			);
		} else if matches!(
			asset.id.0.unpack(),
			(0, [PalletInstance(ASSETS_PALLET_ID), GeneralIndex(_)])
		) {
			assert_expected_events!(
				AssetHubWestend,
				vec![
					// Amount of trust-backed asset is transferred to Parachain's Sovereign account
					RuntimeEvent::Assets(
						pallet_assets::Event::Transferred { from, to, amount, .. },
					) => {
						from: *from == t.sender.account_id,
						to: *to == sov_acc_of_dest,
						amount: *amount == asset_amount,
					},
				]
			);
		} else {
			assert_expected_events!(
				AssetHubWestend,
				vec![
					// Amount of foreign asset is transferred to Parachain's Sovereign account
					RuntimeEvent::ForeignAssets(
						pallet_assets::Event::Transferred { asset_id, from, to, amount },
					) => {
						asset_id: *asset_id == expected_id,
						from: *from == t.sender.account_id,
						to: *to == sov_acc_of_dest,
						amount: *amount == asset_amount,
					},
				]
			);
		}
	}
	assert_expected_events!(
		AssetHubWestend,
		vec![
			// Delivery fees are paid
			RuntimeEvent::PolkadotXcm(pallet_xcm::Event::FeesPaid { .. }) => {},
		]
	);
	AssetHubWestend::assert_xcm_pallet_sent();
}

pub fn system_para_to_para_receiver_assertions(t: SystemParaToParaTest) {
	type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;

	PenpalA::assert_xcmp_queue_success(None);
	for asset in t.args.assets.into_inner().into_iter() {
		let expected_id = asset.id.0.try_into().unwrap();
		assert_expected_events!(
			PenpalA,
			vec![
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
					asset_id: *asset_id == expected_id,
					owner: *owner == t.receiver.account_id,
				},
			]
		);
	}
}

fn system_para_to_para_reserve_transfer_assets(t: SystemParaToParaTest) -> DispatchResult {
	<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::limited_reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn para_to_system_para_reserve_transfer_assets(t: ParaToSystemParaTest) -> DispatchResult {
	<PenpalA as PenpalAPallet>::PolkadotXcm::limited_reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn penpal_to_ah_foreign_assets_sender_assertions(t: ParaToSystemParaTest) {
	type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
	let system_para_native_asset_location = RelayLocation::get();
	let expected_asset_id = t.args.asset_id.unwrap();
	let (_, expected_asset_amount) =
		non_fee_asset(&t.args.assets, t.args.fee_asset_item as usize).unwrap();

	PenpalA::assert_xcm_pallet_attempted_complete(None);
	assert_expected_events!(
		PenpalA,
		vec![
			RuntimeEvent::ForeignAssets(
				pallet_assets::Event::Burned { asset_id, owner, .. }
			) => {
				asset_id: *asset_id == system_para_native_asset_location,
				owner: *owner == t.sender.account_id,
			},
			RuntimeEvent::Assets(pallet_assets::Event::Burned { asset_id, owner, balance }) => {
				asset_id: *asset_id == expected_asset_id,
				owner: *owner == t.sender.account_id,
				balance: *balance == expected_asset_amount,
			},
		]
	);
}

fn penpal_to_ah_foreign_assets_receiver_assertions(t: ParaToSystemParaTest) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
	let sov_penpal_on_ahr = AssetHubWestend::sovereign_account_id_of(
		AssetHubWestend::sibling_location_of(PenpalA::para_id()),
	);
	let (_, expected_foreign_asset_amount) =
		non_fee_asset(&t.args.assets, t.args.fee_asset_item as usize).unwrap();
	let (_, fee_asset_amount) = fee_asset(&t.args.assets, t.args.fee_asset_item as usize).unwrap();

	AssetHubWestend::assert_xcmp_queue_success(None);

	assert_expected_events!(
		AssetHubWestend,
		vec![
			// native asset reserve transfer for paying fees, withdrawn from Penpal's sov account
			RuntimeEvent::Balances(
				pallet_balances::Event::Burned { who, amount }
			) => {
				who: *who == sov_penpal_on_ahr.clone().into(),
				amount: *amount >= fee_asset_amount / 2,
			},
			RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. }) => {
				who: *who == t.receiver.account_id,
			},
			RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, amount }) => {
				asset_id: *asset_id == PenpalATeleportableAssetLocation::get(),
				owner: *owner == t.receiver.account_id,
				amount: *amount == expected_foreign_asset_amount,
			},
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { .. }) => {},
		]
	);
}

fn ah_to_penpal_foreign_assets_sender_assertions(t: SystemParaToParaTest) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
	AssetHubWestend::assert_xcm_pallet_attempted_complete(None);
	let (expected_foreign_asset_id, expected_foreign_asset_amount) =
		non_fee_asset(&t.args.assets, t.args.fee_asset_item as usize).unwrap();
	assert_expected_events!(
		AssetHubWestend,
		vec![
			// foreign asset is burned locally as part of teleportation
			RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned { asset_id, owner, balance }) => {
				asset_id: *asset_id == expected_foreign_asset_id,
				owner: *owner == t.sender.account_id,
				balance: *balance == expected_foreign_asset_amount,
			},
		]
	);
}

fn ah_to_penpal_foreign_assets_receiver_assertions(t: SystemParaToParaTest) {
	type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
	let expected_asset_id = t.args.asset_id.unwrap();
	let (_, expected_asset_amount) =
		non_fee_asset(&t.args.assets, t.args.fee_asset_item as usize).unwrap();
	let checking_account = <PenpalA as PenpalAPallet>::PolkadotXcm::check_account();
	let system_para_native_asset_location = RelayLocation::get();

	PenpalA::assert_xcmp_queue_success(None);

	assert_expected_events!(
		PenpalA,
		vec![
			// checking account burns local asset as part of incoming teleport
			RuntimeEvent::Assets(pallet_assets::Event::Burned { asset_id, owner, balance }) => {
				asset_id: *asset_id == expected_asset_id,
				owner: *owner == checking_account,
				balance: *balance == expected_asset_amount,
			},
			// local asset is teleported into account of receiver
			RuntimeEvent::Assets(pallet_assets::Event::Issued { asset_id, owner, amount }) => {
				asset_id: *asset_id == expected_asset_id,
				owner: *owner == t.receiver.account_id,
				amount: *amount == expected_asset_amount,
			},
			// native asset for fee is deposited to receiver
			RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
				asset_id: *asset_id == system_para_native_asset_location,
				owner: *owner == t.receiver.account_id,
			},
		]
	);
}

fn para_to_system_para_transfer_assets(t: ParaToSystemParaTest) -> DispatchResult {
	<PenpalA as PenpalAPallet>::PolkadotXcm::transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn system_para_to_para_transfer_assets(t: SystemParaToParaTest) -> DispatchResult {
	<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

/// Bidirectional teleports of local Penpal assets to Asset Hub as foreign assets while paying
/// fees using (reserve transferred) native asset.
fn do_bidirectional_teleport_foreign_assets_between_para_and_asset_hub_using_xt(
	para_to_ah_dispatchable: fn(ParaToSystemParaTest) -> DispatchResult,
	ah_to_para_dispatchable: fn(SystemParaToParaTest) -> DispatchResult,
) {
	// Init values for Parachain
	let fee_amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;
	let asset_location_on_penpal = PenpalLocalTeleportableToAssetHub::get();
	let asset_id_on_penpal = match asset_location_on_penpal.last() {
		Some(Junction::GeneralIndex(id)) => *id as u32,
		_ => unreachable!(),
	};
	let asset_amount_to_send = ASSET_HUB_WESTEND_ED * 1000;
	let asset_owner = PenpalAssetOwner::get();
	let system_para_native_asset_location = RelayLocation::get();
	let sender = PenpalASender::get();
	let penpal_check_account = <PenpalA as PenpalAPallet>::PolkadotXcm::check_account();
	let ah_as_seen_by_penpal = PenpalA::sibling_location_of(AssetHubWestend::para_id());
	let penpal_assets: Assets = vec![
		(Parent, fee_amount_to_send).into(),
		(asset_location_on_penpal.clone(), asset_amount_to_send).into(),
	]
	.into();
	let fee_asset_index = penpal_assets
		.inner()
		.iter()
		.position(|r| r == &(Parent, fee_amount_to_send).into())
		.unwrap() as u32;

	// fund Parachain's sender account
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(asset_owner.clone()),
		system_para_native_asset_location.clone(),
		sender.clone(),
		fee_amount_to_send * 2,
	);
	// No need to create the asset (only mint) as it exists in genesis.
	PenpalA::mint_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(asset_owner.clone()),
		asset_id_on_penpal,
		sender.clone(),
		asset_amount_to_send * 2,
	);
	// fund Parachain's check account to be able to teleport
	PenpalA::fund_accounts(vec![(
		penpal_check_account.clone().into(),
		ASSET_HUB_WESTEND_ED * 1000,
	)]);

	// prefund SA of Penpal on AssetHub with enough native tokens to pay for fees
	let penpal_as_seen_by_ah = AssetHubWestend::sibling_location_of(PenpalA::para_id());
	let sov_penpal_on_ah = AssetHubWestend::sovereign_account_id_of(penpal_as_seen_by_ah);
	AssetHubWestend::fund_accounts(vec![(
		sov_penpal_on_ah.clone().into(),
		ASSET_HUB_WESTEND_ED * 100_000_000_000,
	)]);

	// Init values for System Parachain
	let foreign_asset_at_asset_hub =
		Location::new(1, [Junction::Parachain(PenpalA::para_id().into())])
			.appended_with(asset_location_on_penpal)
			.unwrap();
	let penpal_to_ah_beneficiary_id = AssetHubWestendReceiver::get();

	// Penpal to AH test args
	let penpal_to_ah_test_args = TestContext {
		sender: PenpalASender::get(),
		receiver: AssetHubWestendReceiver::get(),
		args: TestArgs::new_para(
			ah_as_seen_by_penpal,
			penpal_to_ah_beneficiary_id,
			asset_amount_to_send,
			penpal_assets,
			Some(asset_id_on_penpal),
			fee_asset_index,
		),
	};
	let mut penpal_to_ah = ParaToSystemParaTest::new(penpal_to_ah_test_args);
	let penpal_sender_balance_before = foreign_balance_on!(
		PenpalA,
		system_para_native_asset_location.clone(),
		&PenpalASender::get()
	);

	let ah_receiver_balance_before = penpal_to_ah.receiver.balance;

	let penpal_sender_assets_before = PenpalA::execute_with(|| {
		type Assets = <PenpalA as PenpalAPallet>::Assets;
		<Assets as Inspect<_>>::balance(asset_id_on_penpal, &PenpalASender::get())
	});
	let ah_receiver_assets_before = foreign_balance_on!(
		AssetHubWestend,
		foreign_asset_at_asset_hub.clone(),
		&AssetHubWestendReceiver::get()
	);

	penpal_to_ah.set_assertion::<PenpalA>(penpal_to_ah_foreign_assets_sender_assertions);
	penpal_to_ah.set_assertion::<AssetHubWestend>(penpal_to_ah_foreign_assets_receiver_assertions);
	penpal_to_ah.set_dispatchable::<PenpalA>(para_to_ah_dispatchable);
	penpal_to_ah.assert();

	let penpal_sender_balance_after = foreign_balance_on!(
		PenpalA,
		system_para_native_asset_location.clone(),
		&PenpalASender::get()
	);

	let ah_receiver_balance_after = penpal_to_ah.receiver.balance;

	let penpal_sender_assets_after = PenpalA::execute_with(|| {
		type Assets = <PenpalA as PenpalAPallet>::Assets;
		<Assets as Inspect<_>>::balance(asset_id_on_penpal, &PenpalASender::get())
	});
	let ah_receiver_assets_after = foreign_balance_on!(
		AssetHubWestend,
		foreign_asset_at_asset_hub.clone(),
		&AssetHubWestendReceiver::get()
	);

	// Sender's balance is reduced
	assert!(penpal_sender_balance_after < penpal_sender_balance_before);
	// Receiver's balance is increased
	assert!(ah_receiver_balance_after > ah_receiver_balance_before);
	// Receiver's balance increased by `amount_to_send - delivery_fees - bought_execution`;
	// `delivery_fees` might be paid from transfer or JIT, also `bought_execution` is unknown but
	// should be non-zero
	assert!(ah_receiver_balance_after < ah_receiver_balance_before + fee_amount_to_send);

	// Sender's balance is reduced by exact amount
	assert_eq!(penpal_sender_assets_before - asset_amount_to_send, penpal_sender_assets_after);
	// Receiver's balance is increased by exact amount
	assert_eq!(ah_receiver_assets_after, ah_receiver_assets_before + asset_amount_to_send);

	///////////////////////////////////////////////////////////////////////
	// Now test transferring foreign assets back from AssetHub to Penpal //
	///////////////////////////////////////////////////////////////////////

	// Move funds on AH from AHReceiver to AHSender
	AssetHubWestend::execute_with(|| {
		type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		assert_ok!(ForeignAssets::transfer(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendReceiver::get()),
			foreign_asset_at_asset_hub.clone().try_into().unwrap(),
			AssetHubWestendSender::get().into(),
			asset_amount_to_send,
		));
	});

	// Only send back half the amount.
	let asset_amount_to_send = asset_amount_to_send / 2;
	let fee_amount_to_send = fee_amount_to_send / 2;

	let ah_to_penpal_beneficiary_id = PenpalAReceiver::get();
	let penpal_as_seen_by_ah = AssetHubWestend::sibling_location_of(PenpalA::para_id());
	let ah_assets: Assets = vec![
		(Parent, fee_amount_to_send).into(),
		(foreign_asset_at_asset_hub.clone(), asset_amount_to_send).into(),
	]
	.into();
	let fee_asset_index = ah_assets
		.inner()
		.iter()
		.position(|r| r == &(Parent, fee_amount_to_send).into())
		.unwrap() as u32;

	// AH to Penpal test args
	let ah_to_penpal_test_args = TestContext {
		sender: AssetHubWestendSender::get(),
		receiver: PenpalAReceiver::get(),
		args: TestArgs::new_para(
			penpal_as_seen_by_ah,
			ah_to_penpal_beneficiary_id,
			asset_amount_to_send,
			ah_assets,
			Some(asset_id_on_penpal),
			fee_asset_index,
		),
	};
	let mut ah_to_penpal = SystemParaToParaTest::new(ah_to_penpal_test_args);

	let ah_sender_balance_before = ah_to_penpal.sender.balance;
	let penpal_receiver_balance_before = foreign_balance_on!(
		PenpalA,
		system_para_native_asset_location.clone(),
		&PenpalAReceiver::get()
	);

	let ah_sender_assets_before = foreign_balance_on!(
		AssetHubWestend,
		foreign_asset_at_asset_hub.clone(),
		&AssetHubWestendSender::get()
	);
	let penpal_receiver_assets_before = PenpalA::execute_with(|| {
		type Assets = <PenpalA as PenpalAPallet>::Assets;
		<Assets as Inspect<_>>::balance(asset_id_on_penpal, &PenpalAReceiver::get())
	});

	ah_to_penpal.set_assertion::<AssetHubWestend>(ah_to_penpal_foreign_assets_sender_assertions);
	ah_to_penpal.set_assertion::<PenpalA>(ah_to_penpal_foreign_assets_receiver_assertions);
	ah_to_penpal.set_dispatchable::<AssetHubWestend>(ah_to_para_dispatchable);
	ah_to_penpal.assert();

	let ah_sender_balance_after = ah_to_penpal.sender.balance;
	let penpal_receiver_balance_after =
		foreign_balance_on!(PenpalA, system_para_native_asset_location, &PenpalAReceiver::get());

	let ah_sender_assets_after = foreign_balance_on!(
		AssetHubWestend,
		foreign_asset_at_asset_hub.clone(),
		&AssetHubWestendSender::get()
	);
	let penpal_receiver_assets_after = PenpalA::execute_with(|| {
		type Assets = <PenpalA as PenpalAPallet>::Assets;
		<Assets as Inspect<_>>::balance(asset_id_on_penpal, &PenpalAReceiver::get())
	});

	// Sender's balance is reduced
	assert!(ah_sender_balance_after < ah_sender_balance_before);
	// Receiver's balance is increased
	assert!(penpal_receiver_balance_after > penpal_receiver_balance_before);
	// Receiver's balance increased by `amount_to_send - delivery_fees - bought_execution`;
	// `delivery_fees` might be paid from transfer or JIT, also `bought_execution` is unknown but
	// should be non-zero
	assert!(penpal_receiver_balance_after < penpal_receiver_balance_before + fee_amount_to_send);

	// Sender's balance is reduced by exact amount
	assert_eq!(ah_sender_assets_before - asset_amount_to_send, ah_sender_assets_after);
	// Receiver's balance is increased by exact amount
	assert_eq!(penpal_receiver_assets_after, penpal_receiver_assets_before + asset_amount_to_send);
}

// ===========================================================================
// ======= Transfer - Native + Bridged Assets - AssetHub->Parachain ==========
// ===========================================================================
/// Transfers of native asset plus bridged asset from AssetHub to some Parachain
/// while paying fees using native asset.
#[test]
fn transfer_foreign_assets_from_asset_hub_to_para() {
	let destination = AssetHubWestend::sibling_location_of(PenpalA::para_id());
	let sender = AssetHubWestendSender::get();
	let native_amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;
	let native_asset_location = RelayLocation::get();
	let receiver = PenpalAReceiver::get();
	let assets_owner = PenpalAssetOwner::get();
	// Foreign asset used: bridged ROC
	let foreign_amount_to_send = ASSET_HUB_WESTEND_ED * 10_000_000;
	let roc_at_westend_parachains =
		Location::new(2, [Junction::GlobalConsensus(NetworkId::ByGenesis(ROCOCO_GENESIS_HASH))]);

	// Configure destination chain to trust AH as reserve of ROC
	PenpalA::execute_with(|| {
		assert_ok!(<PenpalA as Chain>::System::set_storage(
			<PenpalA as Chain>::RuntimeOrigin::root(),
			vec![(
				PenpalCustomizableAssetFromSystemAssetHub::key().to_vec(),
				Location::new(2, [GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH))]).encode(),
			)],
		));
	});
	PenpalA::force_create_foreign_asset(
		roc_at_westend_parachains.clone(),
		assets_owner.clone(),
		false,
		ASSET_MIN_BALANCE,
		vec![],
	);
	AssetHubWestend::force_create_foreign_asset(
		roc_at_westend_parachains.clone().try_into().unwrap(),
		assets_owner.clone(),
		false,
		ASSET_MIN_BALANCE,
		vec![],
	);
	AssetHubWestend::mint_foreign_asset(
		<AssetHubWestend as Chain>::RuntimeOrigin::signed(assets_owner),
		roc_at_westend_parachains.clone().try_into().unwrap(),
		sender.clone(),
		foreign_amount_to_send * 2,
	);

	// Assets to send
	let assets: Vec<Asset> = vec![
		(Parent, native_amount_to_send).into(),
		(roc_at_westend_parachains.clone(), foreign_amount_to_send).into(),
	];
	let fee_asset_id = AssetId(Parent.into());
	let fee_asset_item = assets.iter().position(|a| a.id == fee_asset_id).unwrap() as u32;

	// Init Test
	let test_args = TestContext {
		sender: sender.clone(),
		receiver: receiver.clone(),
		args: TestArgs::new_para(
			destination.clone(),
			receiver.clone(),
			native_amount_to_send,
			assets.into(),
			None,
			fee_asset_item,
		),
	};
	let mut test = SystemParaToParaTest::new(test_args);

	// Query initial balances
	let sender_balance_before = test.sender.balance;
	let sender_rocs_before = AssetHubWestend::execute_with(|| {
		type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(
			roc_at_westend_parachains.clone().try_into().unwrap(),
			&sender,
		)
	});
	let receiver_assets_before = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(native_asset_location.clone(), &receiver)
	});
	let receiver_rocs_before = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(roc_at_westend_parachains.clone(), &receiver)
	});

	// Set assertions and dispatchables
	test.set_assertion::<AssetHubWestend>(system_para_to_para_sender_assertions);
	test.set_assertion::<PenpalA>(system_para_to_para_receiver_assertions);
	test.set_dispatchable::<AssetHubWestend>(ah_to_para_transfer_assets);
	test.assert();

	// Query final balances
	let sender_balance_after = test.sender.balance;
	let sender_rocs_after = AssetHubWestend::execute_with(|| {
		type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(
			roc_at_westend_parachains.clone().try_into().unwrap(),
			&sender,
		)
	});
	let receiver_assets_after = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(native_asset_location, &receiver)
	});
	let receiver_rocs_after = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(roc_at_westend_parachains, &receiver)
	});

	// Sender's balance is reduced by amount sent plus delivery fees
	assert!(sender_balance_after < sender_balance_before - native_amount_to_send);
	// Sender's balance is reduced by foreign amount sent
	assert_eq!(sender_rocs_after, sender_rocs_before - foreign_amount_to_send);
	// Receiver's assets is increased
	assert!(receiver_assets_after > receiver_assets_before);
	// Receiver's assets increased by `amount_to_send - delivery_fees - bought_execution`;
	// `delivery_fees` might be paid from transfer or JIT, also `bought_execution` is unknown but
	// should be non-zero
	assert!(receiver_assets_after < receiver_assets_before + native_amount_to_send);
	// Receiver's balance is increased by foreign amount sent
	assert_eq!(receiver_rocs_after, receiver_rocs_before + foreign_amount_to_send);
}

fn para_to_system_para_receiver_assertions(t: ParaToSystemParaTest) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
	AssetHubWestend::assert_xcmp_queue_success(None);

	let sov_acc_of_penpal = AssetHubWestend::sovereign_account_id_of(Location::new(
		1,
		Parachain(PenpalA::para_id().into()),
	));
	for (idx, asset) in t.args.assets.into_inner().into_iter().enumerate() {
		let expected_id = asset.id.0.clone().try_into().unwrap();
		let asset_amount = if let Fungible(a) = asset.fun { Some(a) } else { None }.unwrap();
		if idx == t.args.fee_asset_item as usize {
			assert_expected_events!(
				AssetHubWestend,
				vec![
					// Amount of native is withdrawn from Parachain's Sovereign account
					RuntimeEvent::Balances(
						pallet_balances::Event::Burned { who, amount }
					) => {
						who: *who == sov_acc_of_penpal.clone().into(),
						amount: *amount == asset_amount,
					},
					RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. }) => {
						who: *who == t.receiver.account_id,
					},
				]
			);
		} else {
			assert_expected_events!(
				AssetHubWestend,
				vec![
					// Amount of foreign asset is transferred from Parachain's Sovereign account
					// to Receiver's account
					RuntimeEvent::ForeignAssets(
						pallet_assets::Event::Burned { asset_id, owner, balance },
					) => {
						asset_id: *asset_id == expected_id,
						owner: *owner == sov_acc_of_penpal,
						balance: *balance == asset_amount,
					},
					RuntimeEvent::ForeignAssets(
						pallet_assets::Event::Issued { asset_id, owner, amount },
					) => {
						asset_id: *asset_id == expected_id,
						owner: *owner == t.receiver.account_id,
						amount: *amount == asset_amount,
					},
				]
			);
		}
	}
	assert_expected_events!(
		AssetHubWestend,
		vec![
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

/// Reserve Transfers of native asset from Parachain to System Parachain should work
// ===========================================================================
// ======= Transfer - Native + Bridged Assets - Parachain->AssetHub ==========
// ===========================================================================
/// Transfers of native asset plus bridged asset from some Parachain to AssetHub
/// while paying fees using native asset.
#[test]
fn transfer_foreign_assets_from_para_to_asset_hub() {
	// Init values for Parachain
	let destination = PenpalA::sibling_location_of(AssetHubWestend::para_id());
	let sender = PenpalASender::get();
	let native_amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 10000;
	let native_asset_location = RelayLocation::get();
	let assets_owner = PenpalAssetOwner::get();

	// Foreign asset used: bridged ROC
	let foreign_amount_to_send = ASSET_HUB_WESTEND_ED * 10_000_000;
	let roc_at_westend_parachains =
		Location::new(2, [Junction::GlobalConsensus(NetworkId::ByGenesis(ROCOCO_GENESIS_HASH))]);

	// Configure destination chain to trust AH as reserve of ROC
	PenpalA::execute_with(|| {
		assert_ok!(<PenpalA as Chain>::System::set_storage(
			<PenpalA as Chain>::RuntimeOrigin::root(),
			vec![(
				PenpalCustomizableAssetFromSystemAssetHub::key().to_vec(),
				Location::new(2, [GlobalConsensus(ByGenesis(ROCOCO_GENESIS_HASH))]).encode(),
			)],
		));
	});
	PenpalA::force_create_foreign_asset(
		roc_at_westend_parachains.clone(),
		assets_owner.clone(),
		false,
		ASSET_MIN_BALANCE,
		vec![],
	);
	AssetHubWestend::force_create_foreign_asset(
		roc_at_westend_parachains.clone().try_into().unwrap(),
		assets_owner.clone(),
		false,
		ASSET_MIN_BALANCE,
		vec![],
	);

	// fund Parachain's sender account
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(assets_owner.clone()),
		native_asset_location.clone(),
		sender.clone(),
		native_amount_to_send * 2,
	);
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(assets_owner.clone()),
		roc_at_westend_parachains.clone(),
		sender.clone(),
		foreign_amount_to_send * 2,
	);

	// Init values for System Parachain
	let receiver = AssetHubWestendReceiver::get();
	let penpal_location_as_seen_by_ahr = AssetHubWestend::sibling_location_of(PenpalA::para_id());
	let sov_penpal_on_ahr =
		AssetHubWestend::sovereign_account_id_of(penpal_location_as_seen_by_ahr);

	// fund Parachain's SA on AssetHub with the assets held in reserve
	AssetHubWestend::fund_accounts(vec![(
		sov_penpal_on_ahr.clone().into(),
		native_amount_to_send * 2,
	)]);
	AssetHubWestend::mint_foreign_asset(
		<AssetHubWestend as Chain>::RuntimeOrigin::signed(assets_owner),
		roc_at_westend_parachains.clone().try_into().unwrap(),
		sov_penpal_on_ahr,
		foreign_amount_to_send * 2,
	);

	// Assets to send
	let assets: Vec<Asset> = vec![
		(Parent, native_amount_to_send).into(),
		(roc_at_westend_parachains.clone(), foreign_amount_to_send).into(),
	];
	let fee_asset_id = AssetId(Parent.into());
	let fee_asset_item = assets.iter().position(|a| a.id == fee_asset_id).unwrap() as u32;

	// Init Test
	let test_args = TestContext {
		sender: sender.clone(),
		receiver: receiver.clone(),
		args: TestArgs::new_para(
			destination.clone(),
			receiver.clone(),
			native_amount_to_send,
			assets.into(),
			None,
			fee_asset_item,
		),
	};
	let mut test = ParaToSystemParaTest::new(test_args);

	// Query initial balances
	let sender_native_before = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(native_asset_location.clone(), &sender)
	});
	let sender_rocs_before = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(roc_at_westend_parachains.clone(), &sender)
	});
	let receiver_native_before = test.receiver.balance;
	let receiver_rocs_before = AssetHubWestend::execute_with(|| {
		type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(
			roc_at_westend_parachains.clone().try_into().unwrap(),
			&receiver,
		)
	});

	// Set assertions and dispatchables
	test.set_assertion::<PenpalA>(para_to_system_para_sender_assertions);
	test.set_assertion::<AssetHubWestend>(para_to_system_para_receiver_assertions);
	test.set_dispatchable::<PenpalA>(para_to_ah_transfer_assets);
	test.assert();

	// Query final balances
	let sender_native_after = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(native_asset_location, &sender)
	});
	let sender_rocs_after = PenpalA::execute_with(|| {
		type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(roc_at_westend_parachains.clone(), &sender)
	});
	let receiver_native_after = test.receiver.balance;
	let receiver_rocs_after = AssetHubWestend::execute_with(|| {
		type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(
			roc_at_westend_parachains.try_into().unwrap(),
			&receiver,
		)
	});

	// Sender's balance is reduced by amount sent plus delivery fees
	assert!(sender_native_after < sender_native_before - native_amount_to_send);
	// Sender's balance is reduced by foreign amount sent
	assert_eq!(sender_rocs_after, sender_rocs_before - foreign_amount_to_send);
	// Receiver's balance is increased
	assert!(receiver_native_after > receiver_native_before);
	// Receiver's balance increased by `amount_to_send - delivery_fees - bought_execution`;
	// `delivery_fees` might be paid from transfer or JIT, also `bought_execution` is unknown but
	// should be non-zero
	assert!(receiver_native_after < receiver_native_before + native_amount_to_send);
	// Receiver's balance is increased by foreign amount sent
	assert_eq!(receiver_rocs_after, receiver_rocs_before + foreign_amount_to_send);
}

fn para_to_system_para_sender_assertions(t: ParaToSystemParaTest) {
	type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
	PenpalA::assert_xcm_pallet_attempted_complete(None);
	for asset in t.args.assets.into_inner().into_iter() {
		let expected_id = asset.id.0;
		let asset_amount = if let Fungible(a) = asset.fun { Some(a) } else { None }.unwrap();
		assert_expected_events!(
			PenpalA,
			vec![
				RuntimeEvent::ForeignAssets(
					pallet_assets::Event::Burned { asset_id, owner, balance }
				) => {
					asset_id: *asset_id == expected_id,
					owner: *owner == t.sender.account_id,
					balance: *balance == asset_amount,
				},
			]
		);
	}
}

fn system_para_to_para_assets_sender_assertions(t: SystemParaToParaTest) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
	AssetHubWestend::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(
		864_610_000,
		8799,
	)));
	assert_expected_events!(
		AssetHubWestend,
		vec![
			// Amount to reserve transfer is transferred to Parachain's Sovereign account
			RuntimeEvent::Assets(
				pallet_assets::Event::Transferred { asset_id, from, to, amount }
			) => {
				asset_id: *asset_id == RESERVABLE_ASSET_ID,
				from: *from == t.sender.account_id,
				to: *to == AssetHubWestend::sovereign_account_id_of(
					t.args.dest.clone()
				),
				amount: *amount == t.args.amount,
			},
			// Native asset to pay for fees is transferred to Parachain's Sovereign account
			RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. }) => {
				who: *who == TreasuryAccount::get(),
			},
			// Delivery fees are paid
			RuntimeEvent::PolkadotXcm(
				pallet_xcm::Event::FeesPaid { .. }
			) => {},
		]
	);
}

fn para_to_system_para_assets_sender_assertions(t: ParaToSystemParaTest) {
	type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
	let system_para_native_asset_location = RelayLocation::get();
	let reservable_asset_location = PenpalLocalReservableFromAssetHub::get();
	PenpalA::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(2_000_000_000, 140000)));
	assert_expected_events!(
		PenpalA,
		vec![
			// Fees amount to reserve transfer is burned from Parachains's sender account
			RuntimeEvent::ForeignAssets(
				pallet_assets::Event::Burned { asset_id, owner, .. }
			) => {
				asset_id: *asset_id == system_para_native_asset_location,
				owner: *owner == t.sender.account_id,
			},
			// Amount to reserve transfer is burned from Parachains's sender account
			RuntimeEvent::ForeignAssets(
				pallet_assets::Event::Burned { asset_id, owner, balance }
			) => {
				asset_id: *asset_id == reservable_asset_location,
				owner: *owner == t.sender.account_id,
				balance: *balance == t.args.amount,
			},
			// Delivery fees are paid
			RuntimeEvent::PolkadotXcm(
				pallet_xcm::Event::FeesPaid { .. }
			) => {},
		]
	);
}

fn system_para_to_para_assets_receiver_assertions(t: SystemParaToParaTest) {
	type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
	let system_para_asset_location = PenpalLocalReservableFromAssetHub::get();
	PenpalA::assert_xcmp_queue_success(None);
	assert_expected_events!(
		PenpalA,
		vec![
			RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
				asset_id: *asset_id == RelayLocation::get(),
				owner: *owner == t.receiver.account_id,
			},
			RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, amount }) => {
				asset_id: *asset_id == system_para_asset_location,
				owner: *owner == t.receiver.account_id,
				amount: *amount == t.args.amount,
			},
		]
	);
}

fn para_to_system_para_assets_receiver_assertions(t: ParaToSystemParaTest) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
	let sov_penpal_on_ahr = AssetHubWestend::sovereign_account_id_of(
		AssetHubWestend::sibling_location_of(PenpalA::para_id()),
	);
	AssetHubWestend::assert_xcmp_queue_success(None);
	assert_expected_events!(
		AssetHubWestend,
		vec![
			// Amount to reserve transfer is burned from Parachain's Sovereign account
			RuntimeEvent::Assets(pallet_assets::Event::Burned { asset_id, owner, balance }) => {
				asset_id: *asset_id == RESERVABLE_ASSET_ID,
				owner: *owner == sov_penpal_on_ahr,
				balance: *balance == t.args.amount,
			},
			// Fee amount is burned from Parachain's Sovereign account
			RuntimeEvent::Balances(pallet_balances::Event::Burned { who, .. }) => {
				who: *who == sov_penpal_on_ahr,
			},
			// Amount to reserve transfer is issued for beneficiary
			RuntimeEvent::Assets(pallet_assets::Event::Issued { asset_id, owner, amount }) => {
				asset_id: *asset_id == RESERVABLE_ASSET_ID,
				owner: *owner == t.receiver.account_id,
				amount: *amount == t.args.amount,
			},
			// Remaining fee amount is minted for for beneficiary
			RuntimeEvent::Balances(pallet_balances::Event::Minted { who, .. }) => {
				who: *who == t.receiver.account_id,
			},
		]
	);
}

// ==============================================================================================
// ==== Bidirectional Transfer - Native + Teleportable Foreign Assets - Parachain<->AssetHub ====
// ==============================================================================================
/// Transfers of native asset plus teleportable foreign asset from Parachain to AssetHub and back
/// with fees paid using native asset.
#[test]
fn bidirectional_teleport_foreign_asset_between_para_and_asset_hub_using_explicit_transfer_types() {
	do_bidirectional_teleport_foreign_assets_between_para_and_asset_hub_using_xt(
		para_to_asset_hub_teleport_foreign_assets,
		asset_hub_to_para_teleport_foreign_assets,
	);
}

// ==============================================================================================
// ==== Bidirectional Transfer - Native + Teleportable Foreign Assets - Parachain<->AssetHub ====
// ==============================================================================================
/// Transfers of native asset plus teleportable foreign asset from Parachain to AssetHub and back
/// with fees paid using native asset.
#[test]
fn bidirectional_transfer_multiple_assets_between_penpal_and_asset_hub() {
	fn execute_xcm_penpal_to_asset_hub(t: ParaToSystemParaTest) -> DispatchResult {
		let all_assets = t.args.assets.clone().into_inner();
		let mut assets = all_assets.clone();
		let mut fees = assets.remove(t.args.fee_asset_item as usize);
		// TODO(https://github.com/paritytech/polkadot-sdk/issues/6197): dry-run to get exact fees.
		// For now just use half the fees locally, half on dest
		if let Fungible(fees_amount) = fees.fun {
			fees.fun = Fungible(fees_amount / 2);
		}
		// xcm to be executed at dest
		let xcm_on_dest = Xcm(vec![
			// since this is the last hop, we don't need to further use any assets previously
			// reserved for fees (there are no further hops to cover delivery fees for); we
			// RefundSurplus to get back any unspent fees
			RefundSurplus,
			DepositAsset { assets: Wild(All), beneficiary: t.args.beneficiary },
		]);
		let xcm = Xcm::<()>(vec![
			WithdrawAsset(all_assets.into()),
			PayFees { asset: fees.clone() },
			InitiateTransfer {
				destination: t.args.dest,
				remote_fees: Some(AssetTransferFilter::ReserveWithdraw(fees.into())),
				preserve_origin: false,
				assets: BoundedVec::truncate_from(vec![AssetTransferFilter::Teleport(
					assets.into(),
				)]),
				remote_xcm: xcm_on_dest,
			},
		]);
		<PenpalA as PenpalAPallet>::PolkadotXcm::execute(
			t.signed_origin,
			bx!(xcm::VersionedXcm::from(xcm.into())),
			Weight::MAX,
		)
		.unwrap();
		Ok(())
	}
	fn execute_xcm_asset_hub_to_penpal(t: SystemParaToParaTest) -> DispatchResult {
		let all_assets = t.args.assets.clone().into_inner();
		let mut assets = all_assets.clone();
		let mut fees = assets.remove(t.args.fee_asset_item as usize);
		// TODO(https://github.com/paritytech/polkadot-sdk/issues/6197): dry-run to get exact fees.
		// For now just use half the fees locally, half on dest
		if let Fungible(fees_amount) = fees.fun {
			fees.fun = Fungible(fees_amount / 2);
		}
		// xcm to be executed at dest
		let xcm_on_dest = Xcm(vec![
			// since this is the last hop, we don't need to further use any assets previously
			// reserved for fees (there are no further hops to cover delivery fees for); we
			// RefundSurplus to get back any unspent fees
			RefundSurplus,
			DepositAsset { assets: Wild(All), beneficiary: t.args.beneficiary },
		]);
		let xcm = Xcm::<()>(vec![
			WithdrawAsset(all_assets.into()),
			PayFees { asset: fees.clone() },
			InitiateTransfer {
				destination: t.args.dest,
				remote_fees: Some(AssetTransferFilter::ReserveDeposit(fees.into())),
				preserve_origin: false,
				assets: BoundedVec::truncate_from(vec![AssetTransferFilter::Teleport(
					assets.into(),
				)]),
				remote_xcm: xcm_on_dest,
			},
		]);
		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::execute(
			t.signed_origin,
			bx!(xcm::VersionedXcm::from(xcm.into())),
			Weight::MAX,
		)
		.unwrap();
		Ok(())
	}
	do_bidirectional_teleport_foreign_assets_between_para_and_asset_hub_using_xt(
		execute_xcm_penpal_to_asset_hub,
		execute_xcm_asset_hub_to_penpal,
	);
}

// =========================================================================
// ======= Reserve Transfers - Native Asset - AssetHub<>Parachain ==========
// =========================================================================
/// Reserve Transfers of native asset from Asset Hub to Parachain should work
#[test]
fn reserve_transfer_native_asset_from_asset_hub_to_para() {
	// Init values for Asset Hub
	let destination = AssetHubWestend::sibling_location_of(PenpalA::para_id());
	let sender = AssetHubWestendSender::get();
	let amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 2000;
	let assets: Assets = (Parent, amount_to_send).into();

	// Init values for Parachain
	let system_para_native_asset_location = RelayLocation::get();
	let receiver = PenpalAReceiver::get();

	// Init Test
	let test_args = TestContext {
		sender,
		receiver: receiver.clone(),
		args: TestArgs::new_para(
			destination.clone(),
			receiver.clone(),
			amount_to_send,
			assets.clone(),
			None,
			0,
		),
	};
	let mut test = SystemParaToParaTest::new(test_args);

	// Query initial balances
	let sender_balance_before = test.sender.balance;
	let receiver_assets_before =
		foreign_balance_on!(PenpalA, system_para_native_asset_location.clone(), &receiver);

	// Set assertions and dispatchables
	test.set_assertion::<AssetHubWestend>(system_para_to_para_sender_assertions);
	test.set_assertion::<PenpalA>(system_para_to_para_receiver_assertions);
	test.set_dispatchable::<AssetHubWestend>(system_para_to_para_reserve_transfer_assets);
	test.assert();

	// Query final balances
	let sender_balance_after = test.sender.balance;
	let receiver_assets_after =
		foreign_balance_on!(PenpalA, system_para_native_asset_location, &receiver);

	// Sender's balance is reduced by amount sent plus delivery fees
	assert!(sender_balance_after < sender_balance_before - amount_to_send);
	// Receiver's assets is increased
	assert!(receiver_assets_after > receiver_assets_before);
	// Receiver's assets increased by `amount_to_send - delivery_fees - bought_execution`;
	// `delivery_fees` might be paid from transfer or JIT, also `bought_execution` is unknown but
	// should be non-zero
	assert!(receiver_assets_after < receiver_assets_before + amount_to_send);
}

/// Reserve Transfers of native asset from Parachain to Asset Hub should work
#[test]
fn reserve_transfer_native_asset_from_para_to_asset_hub() {
	// Init values for Parachain
	let destination = PenpalA::sibling_location_of(AssetHubWestend::para_id());
	let sender = PenpalASender::get();
	let amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;
	let assets: Assets = (Parent, amount_to_send).into();
	let system_para_native_asset_location = RelayLocation::get();
	let asset_owner = PenpalAssetOwner::get();

	// fund Parachain's sender account
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(asset_owner),
		system_para_native_asset_location.clone(),
		sender.clone(),
		amount_to_send * 2,
	);

	// Init values for Asset Hub
	let receiver = AssetHubWestendReceiver::get();
	let penpal_location_as_seen_by_ahr = AssetHubWestend::sibling_location_of(PenpalA::para_id());
	let sov_penpal_on_ahr =
		AssetHubWestend::sovereign_account_id_of(penpal_location_as_seen_by_ahr);

	// fund Parachain's SA on Asset Hub with the native tokens held in reserve
	AssetHubWestend::fund_accounts(vec![(sov_penpal_on_ahr.into(), amount_to_send * 2)]);

	// Init Test
	let test_args = TestContext {
		sender: sender.clone(),
		receiver: receiver.clone(),
		args: TestArgs::new_para(
			destination.clone(),
			receiver.clone(),
			amount_to_send,
			assets.clone(),
			None,
			0,
		),
	};
	let mut test = ParaToSystemParaTest::new(test_args);

	// Query initial balances
	let sender_assets_before =
		foreign_balance_on!(PenpalA, system_para_native_asset_location.clone(), &sender);
	let receiver_balance_before = test.receiver.balance;

	// Set assertions and dispatchables
	test.set_assertion::<PenpalA>(para_to_system_para_sender_assertions);
	test.set_assertion::<AssetHubWestend>(para_to_system_para_receiver_assertions);
	test.set_dispatchable::<PenpalA>(para_to_system_para_reserve_transfer_assets);
	test.assert();

	// Query final balances
	let sender_assets_after =
		foreign_balance_on!(PenpalA, system_para_native_asset_location, &sender);
	let receiver_balance_after = test.receiver.balance;

	// Sender's balance is reduced by amount sent plus delivery fees
	assert!(sender_assets_after < sender_assets_before - amount_to_send);
	// Receiver's balance is increased
	assert!(receiver_balance_after > receiver_balance_before);
	// Receiver's balance increased by `amount_to_send - delivery_fees - bought_execution`;
	// `delivery_fees` might be paid from transfer or JIT, also `bought_execution` is unknown but
	// should be non-zero
	assert!(receiver_balance_after < receiver_balance_before + amount_to_send);
}

// =========================================================================
// ======= Reserve Transfers - Non-system Asset - AssetHub<>Parachain ======
// =========================================================================
/// Reserve Transfers of a local asset and native asset from Asset Hub to Parachain should
/// work
#[test]
fn reserve_transfer_multiple_assets_from_asset_hub_to_para() {
	// Init values for Asset Hub
	let destination = AssetHubWestend::sibling_location_of(PenpalA::para_id());
	let sov_penpal_on_ahr = AssetHubWestend::sovereign_account_id_of(destination.clone());
	let sender = AssetHubWestendSender::get();
	let fee_amount_to_send = ASSET_HUB_WESTEND_ED * 100;
	let asset_amount_to_send = ASSET_HUB_WESTEND_ED * 100;
	let asset_owner = AssetHubWestendAssetOwner::get();
	let asset_owner_signer = <AssetHubWestend as Chain>::RuntimeOrigin::signed(asset_owner.clone());
	let assets: Assets = vec![
		(Parent, fee_amount_to_send).into(),
		(
			[PalletInstance(ASSETS_PALLET_ID), GeneralIndex(RESERVABLE_ASSET_ID.into())],
			asset_amount_to_send,
		)
			.into(),
	]
	.into();
	let fee_asset_index = assets
		.inner()
		.iter()
		.position(|r| r == &(Parent, fee_amount_to_send).into())
		.unwrap() as u32;
	AssetHubWestend::mint_asset(
		asset_owner_signer,
		RESERVABLE_ASSET_ID,
		asset_owner,
		asset_amount_to_send * 2,
	);

	// Create SA-of-Penpal-on-AHR with ED.
	AssetHubWestend::fund_accounts(vec![(sov_penpal_on_ahr.into(), ASSET_HUB_WESTEND_ED)]);

	// Init values for Parachain
	let receiver = PenpalAReceiver::get();
	let system_para_native_asset_location = RelayLocation::get();
	let system_para_foreign_asset_location = PenpalLocalReservableFromAssetHub::get();

	// Init Test
	let para_test_args = TestContext {
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
	let mut test = SystemParaToParaTest::new(para_test_args);

	// Query initial balances
	let sender_balance_before = test.sender.balance;
	let sender_assets_before = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
		<Assets as Inspect<_>>::balance(RESERVABLE_ASSET_ID, &sender)
	});
	let receiver_system_native_assets_before =
		foreign_balance_on!(PenpalA, system_para_native_asset_location.clone(), &receiver);
	let receiver_foreign_assets_before =
		foreign_balance_on!(PenpalA, system_para_foreign_asset_location.clone(), &receiver);

	// Set assertions and dispatchables
	test.set_assertion::<AssetHubWestend>(system_para_to_para_assets_sender_assertions);
	test.set_assertion::<PenpalA>(system_para_to_para_assets_receiver_assertions);
	test.set_dispatchable::<AssetHubWestend>(system_para_to_para_reserve_transfer_assets);
	test.assert();

	// Query final balances
	let sender_balance_after = test.sender.balance;
	let sender_assets_after = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
		<Assets as Inspect<_>>::balance(RESERVABLE_ASSET_ID, &sender)
	});
	let receiver_system_native_assets_after =
		foreign_balance_on!(PenpalA, system_para_native_asset_location, &receiver);
	let receiver_foreign_assets_after =
		foreign_balance_on!(PenpalA, system_para_foreign_asset_location.clone(), &receiver);
	// Sender's balance is reduced
	assert!(sender_balance_after < sender_balance_before);
	// Receiver's foreign asset balance is increased
	assert!(receiver_foreign_assets_after > receiver_foreign_assets_before);
	// Receiver's system asset balance increased by `amount_to_send - delivery_fees -
	// bought_execution`; `delivery_fees` might be paid from transfer or JIT, also
	// `bought_execution` is unknown but should be non-zero
	assert!(
		receiver_system_native_assets_after <
			receiver_system_native_assets_before + fee_amount_to_send
	);

	// Sender's asset balance is reduced by exact amount
	assert_eq!(sender_assets_before - asset_amount_to_send, sender_assets_after);
	// Receiver's foreign asset balance is increased by exact amount
	assert_eq!(
		receiver_foreign_assets_after,
		receiver_foreign_assets_before + asset_amount_to_send
	);
}

/// Reserve Transfers of a random asset and native asset from Parachain to Asset Hub should work
/// Receiver is empty account to show deposit works as long as transfer includes enough DOT for ED.
/// Once we have https://github.com/paritytech/polkadot-sdk/issues/5298,
/// we should do equivalent test with USDT instead of DOT.
#[test]
fn reserve_transfer_multiple_assets_from_para_to_asset_hub() {
	// Init values for Parachain
	let destination = PenpalA::sibling_location_of(AssetHubWestend::para_id());
	let sender = PenpalASender::get();
	let fee_amount_to_send = ASSET_HUB_WESTEND_ED * 100;
	let asset_amount_to_send = ASSET_HUB_WESTEND_ED * 100;
	let penpal_asset_owner = PenpalAssetOwner::get();
	let penpal_asset_owner_signer = <PenpalA as Chain>::RuntimeOrigin::signed(penpal_asset_owner);
	let asset_location_on_penpal = PenpalLocalReservableFromAssetHub::get();
	let system_asset_location_on_penpal = RelayLocation::get();
	let assets: Assets = vec![
		(Parent, fee_amount_to_send).into(),
		(asset_location_on_penpal.clone(), asset_amount_to_send).into(),
	]
	.into();
	let fee_asset_index = assets
		.inner()
		.iter()
		.position(|r| r == &(Parent, fee_amount_to_send).into())
		.unwrap() as u32;
	// Fund Parachain's sender account with some foreign assets
	PenpalA::mint_foreign_asset(
		penpal_asset_owner_signer.clone(),
		asset_location_on_penpal.clone(),
		sender.clone(),
		asset_amount_to_send * 2,
	);
	// Fund Parachain's sender account with some system assets
	PenpalA::mint_foreign_asset(
		penpal_asset_owner_signer,
		system_asset_location_on_penpal.clone(),
		sender.clone(),
		fee_amount_to_send * 2,
	);

	// Beneficiary is a new (empty) account
	let receiver: sp_runtime::AccountId32 =
		get_public_from_string_or_panic::<sr25519::Public>(DUMMY_EMPTY).into();
	// Init values for Asset Hub
	let penpal_location_as_seen_by_ahr = AssetHubWestend::sibling_location_of(PenpalA::para_id());
	let sov_penpal_on_ahr =
		AssetHubWestend::sovereign_account_id_of(penpal_location_as_seen_by_ahr);
	let ah_asset_owner = AssetHubWestendAssetOwner::get();
	let ah_asset_owner_signer = <AssetHubWestend as Chain>::RuntimeOrigin::signed(ah_asset_owner);

	// Fund SA-of-Penpal-on-AHR to be able to pay for the fees.
	AssetHubWestend::fund_accounts(vec![(
		sov_penpal_on_ahr.clone().into(),
		ASSET_HUB_WESTEND_ED * 1000,
	)]);
	// Fund SA-of-Penpal-on-AHR to be able to pay for the sent amount.
	AssetHubWestend::mint_asset(
		ah_asset_owner_signer,
		RESERVABLE_ASSET_ID,
		sov_penpal_on_ahr,
		asset_amount_to_send * 2,
	);

	// Init Test
	let para_test_args = TestContext {
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
	let mut test = ParaToSystemParaTest::new(para_test_args);

	// Query initial balances
	let sender_system_assets_before =
		foreign_balance_on!(PenpalA, system_asset_location_on_penpal.clone(), &sender);
	let sender_foreign_assets_before =
		foreign_balance_on!(PenpalA, asset_location_on_penpal.clone(), &sender);
	let receiver_balance_before = test.receiver.balance;
	let receiver_assets_before = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
		<Assets as Inspect<_>>::balance(RESERVABLE_ASSET_ID, &receiver)
	});

	// Set assertions and dispatchables
	test.set_assertion::<PenpalA>(para_to_system_para_assets_sender_assertions);
	test.set_assertion::<AssetHubWestend>(para_to_system_para_assets_receiver_assertions);
	test.set_dispatchable::<PenpalA>(para_to_system_para_reserve_transfer_assets);
	test.assert();

	// Query final balances
	let sender_system_assets_after =
		foreign_balance_on!(PenpalA, system_asset_location_on_penpal, &sender);
	let sender_foreign_assets_after =
		foreign_balance_on!(PenpalA, asset_location_on_penpal, &sender);
	let receiver_balance_after = test.receiver.balance;
	let receiver_assets_after = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
		<Assets as Inspect<_>>::balance(RESERVABLE_ASSET_ID, &receiver)
	});
	// Sender's system asset balance is reduced
	assert!(sender_system_assets_after < sender_system_assets_before);
	// Receiver's balance is increased
	assert!(receiver_balance_after > receiver_balance_before);
	// Receiver's balance increased by `amount_to_send - delivery_fees - bought_execution`;
	// `delivery_fees` might be paid from transfer or JIT, also `bought_execution` is unknown but
	// should be non-zero
	assert!(receiver_balance_after < receiver_balance_before + fee_amount_to_send);

	// Sender's asset balance is reduced by exact amount
	assert_eq!(sender_foreign_assets_before - asset_amount_to_send, sender_foreign_assets_after);
	// Receiver's foreign asset balance is increased by exact amount
	assert_eq!(receiver_assets_after, receiver_assets_before + asset_amount_to_send);
}

// ============================================================================
// ==== Reserve Transfers USDT - AssetHub->Parachain - pay fees using pool ====
// ============================================================================
#[test]
fn reserve_transfer_usdt_from_asset_hub_to_para() {
	let usdt_id = 1984u32;
	let penpal_location = AssetHubWestend::sibling_location_of(PenpalA::para_id());
	let penpal_sov_account = AssetHubWestend::sovereign_account_id_of(penpal_location.clone());

	// Create SA-of-Penpal-on-AHW with ED.
	// This ED isn't reflected in any derivative in a PenpalA account.
	AssetHubWestend::fund_accounts(vec![(penpal_sov_account.clone().into(), ASSET_HUB_WESTEND_ED)]);

	let sender = AssetHubWestendSender::get();
	let receiver = PenpalAReceiver::get();
	let asset_amount_to_send = 1_000_000_000_000;

	AssetHubWestend::execute_with(|| {
		use frame_support::traits::tokens::fungibles::Mutate;
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
		assert_ok!(<Assets as Mutate<_>>::mint_into(
			usdt_id.into(),
			&AssetHubWestendSender::get(),
			asset_amount_to_send + 10_000_000_000_000, // Make sure it has enough.
		));
	});

	let usdt_from_asset_hub = PenpalUsdtFromAssetHub::get();
	// Setup the pool between `relay_asset_penpal_pov` and `usdt_from_asset_hub` on PenpalA.
	// So we can swap the custom asset that comes from AssetHubWestend for native asset to pay for
	// fees.
	create_pool_with_wnd_on!(PenpalA, PenpalUsdtFromAssetHub::get(), true, PenpalAssetOwner::get());

	let assets: Assets = vec![(
		[PalletInstance(ASSETS_PALLET_ID), GeneralIndex(usdt_id.into())],
		asset_amount_to_send,
	)
		.into()]
	.into();

	let test_args = TestContext {
		sender: sender.clone(),
		receiver: receiver.clone(),
		args: TestArgs::new_para(
			penpal_location,
			receiver.clone(),
			asset_amount_to_send,
			assets,
			None,
			0,
		),
	};
	let mut test = SystemParaToParaTest::new(test_args);

	let sender_initial_balance = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
		<Assets as Inspect<_>>::balance(usdt_id, &sender)
	});
	let sender_initial_native_balance = AssetHubWestend::execute_with(|| {
		type Balances = <AssetHubWestend as AssetHubWestendPallet>::Balances;
		Balances::free_balance(&sender)
	});
	let receiver_initial_balance =
		foreign_balance_on!(PenpalA, usdt_from_asset_hub.clone(), &receiver);

	test.set_assertion::<AssetHubWestend>(system_para_to_para_sender_assertions);
	test.set_assertion::<PenpalA>(system_para_to_penpal_receiver_assertions);
	test.set_dispatchable::<AssetHubWestend>(system_para_to_para_reserve_transfer_assets);
	test.assert();

	let sender_after_balance = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
		<Assets as Inspect<_>>::balance(usdt_id, &sender)
	});
	let sender_after_native_balance = AssetHubWestend::execute_with(|| {
		type Balances = <AssetHubWestend as AssetHubWestendPallet>::Balances;
		Balances::free_balance(&sender)
	});
	let receiver_after_balance = foreign_balance_on!(PenpalA, usdt_from_asset_hub, &receiver);

	// TODO(https://github.com/paritytech/polkadot-sdk/issues/5160): When we allow payment with different assets locally, this should be the same, since
	// they aren't used for fees.
	assert!(sender_after_native_balance < sender_initial_native_balance);
	// Sender account's balance decreases.
	assert_eq!(sender_after_balance, sender_initial_balance - asset_amount_to_send);
	// Receiver account's balance increases.
	assert!(receiver_after_balance > receiver_initial_balance);
	assert!(receiver_after_balance < receiver_initial_balance + asset_amount_to_send);
}

fn system_para_to_penpal_receiver_assertions(t: SystemParaToParaTest) {
	type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;

	PenpalA::assert_xcmp_queue_success(None);
	for asset in t.args.assets.into_inner().into_iter() {
		let mut expected_id: Location = asset.id.0.try_into().unwrap();
		let relative_id = match expected_id {
			Location { parents: 1, interior: Here } => expected_id,
			_ => {
				expected_id
					.push_front_interior(Parachain(AssetHubWestend::para_id().into()))
					.unwrap();
				Location::new(1, expected_id.interior().clone())
			},
		};

		assert_expected_events!(
			PenpalA,
			vec![
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
					asset_id: *asset_id == relative_id,
					owner: *owner == t.receiver.account_id,
				},
			]
		);
	}
}

/// Reserve Withdraw Native Asset from AssetHub to Parachain fails.
#[test]
fn reserve_withdraw_from_untrusted_reserve_fails() {
	// Init values for Parachain Origin
	let destination = AssetHubWestend::sibling_location_of(PenpalA::para_id());
	let signed_origin =
		<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get().into());
	let roc_to_send: Balance = WESTEND_ED * 10000;
	let roc_location = RelayLocation::get();

	// Assets to send
	let assets: Vec<Asset> = vec![(roc_location.clone(), roc_to_send).into()];
	let fee_id: AssetId = roc_location.into();

	// this should fail
	AssetHubWestend::execute_with(|| {
		let result = <AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
			signed_origin.clone(),
			bx!(destination.clone().into()),
			bx!(assets.clone().into()),
			bx!(TransferType::DestinationReserve),
			bx!(fee_id.into()),
			bx!(TransferType::DestinationReserve),
			bx!(VersionedXcm::from(Xcm::<()>::new())),
			Unlimited,
		);
		assert_err!(
			result,
			DispatchError::Module(sp_runtime::ModuleError {
				index: 31,
				error: [22, 0, 0, 0],
				message: Some("InvalidAssetUnsupportedReserve")
			})
		);
	});

	// this should also fail
	AssetHubWestend::execute_with(|| {
		let xcm: Xcm<asset_hub_westend_runtime::RuntimeCall> = Xcm(vec![
			WithdrawAsset(assets.into()),
			InitiateReserveWithdraw {
				assets: Wild(All),
				reserve: destination,
				xcm: Xcm::<()>::new(),
			},
		]);
		let result = <AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::execute(
			signed_origin,
			bx!(xcm::VersionedXcm::from(xcm)),
			Weight::MAX,
		);
		assert!(result.is_err());
	});
}

/// Bidirectional teleports of local Penpal assets to Asset Hub as foreign assets should work
/// (using native reserve-based transfer for fees)
#[test]
fn bidirectional_teleport_foreign_assets_between_para_and_asset_hub() {
	do_bidirectional_teleport_foreign_assets_between_para_and_asset_hub_using_xt(
		para_to_system_para_transfer_assets,
		system_para_to_para_transfer_assets,
	);
}

/// Teleport Native Asset from AssetHub to Parachain fails.
#[test]
fn teleport_to_untrusted_chain_fails() {
	// Init values for Parachain Origin
	let destination = AssetHubWestend::sibling_location_of(PenpalA::para_id());
	let signed_origin =
		<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get().into());
	let roc_to_send: Balance = WESTEND_ED * 10000;
	let roc_location = RelayLocation::get();

	// Assets to send
	let assets: Vec<Asset> = vec![(roc_location.clone(), roc_to_send).into()];
	let fee_id: AssetId = roc_location.into();

	// this should fail
	AssetHubWestend::execute_with(|| {
		let result = <AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
			signed_origin.clone(),
			bx!(destination.clone().into()),
			bx!(assets.clone().into()),
			bx!(TransferType::Teleport),
			bx!(fee_id.into()),
			bx!(TransferType::Teleport),
			bx!(VersionedXcm::from(Xcm::<()>::new())),
			Unlimited,
		);
		assert_err!(
			result,
			DispatchError::Module(sp_runtime::ModuleError {
				index: 31,
				error: [2, 0, 0, 0],
				message: Some("Filtered")
			})
		);
	});

	// this should also fail
	AssetHubWestend::execute_with(|| {
		let xcm: Xcm<asset_hub_westend_runtime::RuntimeCall> = Xcm(vec![
			WithdrawAsset(assets.into()),
			InitiateTeleport { assets: Wild(All), dest: destination, xcm: Xcm::<()>::new() },
		]);
		let result = <AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::execute(
			signed_origin,
			bx!(xcm::VersionedXcm::from(xcm)),
			Weight::MAX,
		);
		assert!(result.is_err());
	});
}
