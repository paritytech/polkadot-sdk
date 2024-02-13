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
use asset_hub_westend_runtime::xcm_config::XcmConfig as AssetHubWestendXcmConfig;
use emulated_integration_tests_common::xcm_helpers::non_fee_asset;
use westend_runtime::xcm_config::XcmConfig as WestendXcmConfig;
use westend_system_emulated_network::penpal_emulated_chain::LocalTeleportableToAssetHubV3 as PenpalLocalTeleportableToAssetHubV3;

fn relay_origin_assertions(t: RelayToSystemParaTest) {
	type RuntimeEvent = <Westend as Chain>::RuntimeEvent;

	Westend::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(632_207_000, 7_186)));

	assert_expected_events!(
		Westend,
		vec![
			// Amount to teleport is withdrawn from Sender
			RuntimeEvent::Balances(pallet_balances::Event::Withdraw { who, amount }) => {
				who: *who == t.sender.account_id,
				amount: *amount == t.args.amount,
			},
			// Amount to teleport is deposited in Relay's `CheckAccount`
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { who, amount }) => {
				who: *who == <Westend as WestendPallet>::XcmPallet::check_account(),
				amount:  *amount == t.args.amount,
			},
		]
	);
}

fn relay_dest_assertions(t: SystemParaToRelayTest) {
	type RuntimeEvent = <Westend as Chain>::RuntimeEvent;

	Westend::assert_ump_queue_processed(
		true,
		Some(AssetHubWestend::para_id()),
		Some(Weight::from_parts(308_222_000, 7_186)),
	);

	assert_expected_events!(
		Westend,
		vec![
			// Amount is withdrawn from Relay Chain's `CheckAccount`
			RuntimeEvent::Balances(pallet_balances::Event::Withdraw { who, amount }) => {
				who: *who == <Westend as WestendPallet>::XcmPallet::check_account(),
				amount: *amount == t.args.amount,
			},
			// Amount minus fees are deposited in Receiver's account
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { who, .. }) => {
				who: *who == t.receiver.account_id,
			},
		]
	);
}

fn relay_dest_assertions_fail(_t: SystemParaToRelayTest) {
	Westend::assert_ump_queue_processed(
		false,
		Some(AssetHubWestend::para_id()),
		Some(Weight::from_parts(148_705_000, 3_593)),
	);
}

fn para_origin_assertions(t: SystemParaToRelayTest) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

	AssetHubWestend::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(
		533_910_000,
		7167,
	)));

	AssetHubWestend::assert_parachain_system_ump_sent();

	assert_expected_events!(
		AssetHubWestend,
		vec![
			// Amount is withdrawn from Sender's account
			RuntimeEvent::Balances(pallet_balances::Event::Withdraw { who, amount }) => {
				who: *who == t.sender.account_id,
				amount: *amount == t.args.amount,
			},
		]
	);
}

fn para_dest_assertions(t: RelayToSystemParaTest) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

	AssetHubWestend::assert_dmp_queue_complete(Some(Weight::from_parts(164_793_000, 3593)));

	assert_expected_events!(
		AssetHubWestend,
		vec![
			// Amount minus fees are deposited in Receiver's account
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { who, .. }) => {
				who: *who == t.receiver.account_id,
			},
		]
	);
}

fn penpal_to_ah_foreign_assets_sender_assertions(t: ParaToSystemParaTest) {
	type RuntimeEvent = <PenpalB as Chain>::RuntimeEvent;
	PenpalB::assert_xcm_pallet_attempted_complete(None);
	let expected_asset_id = t.args.asset_id.unwrap();
	let (_, expected_asset_amount) =
		non_fee_asset(&t.args.assets, t.args.fee_asset_item as usize).unwrap();
	assert_expected_events!(
		PenpalB,
		vec![
			RuntimeEvent::Balances(
				pallet_balances::Event::Withdraw { who, amount }
			) => {
				who: *who == t.sender.account_id,
				amount: *amount == t.args.amount,
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
		AssetHubWestend::sibling_location_of(PenpalB::para_id()),
	);
	let (expected_foreign_asset_id, expected_foreign_asset_amount) =
		non_fee_asset(&t.args.assets, t.args.fee_asset_item as usize).unwrap();
	let expected_foreign_asset_id_v3: v3::Location = expected_foreign_asset_id.try_into().unwrap();
	assert_expected_events!(
		AssetHubWestend,
		vec![
			// native asset reserve transfer for paying fees, withdrawn from Penpal's sov account
			RuntimeEvent::Balances(
				pallet_balances::Event::Withdraw { who, amount }
			) => {
				who: *who == sov_penpal_on_ahr.clone().into(),
				amount: *amount == t.args.amount,
			},
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { who, .. }) => {
				who: *who == t.receiver.account_id,
			},
			RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, amount }) => {
				asset_id: *asset_id == expected_foreign_asset_id_v3,
				owner: *owner == t.receiver.account_id,
				amount: *amount == expected_foreign_asset_amount,
			},
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { .. }) => {},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn ah_to_penpal_foreign_assets_sender_assertions(t: SystemParaToParaTest) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
	AssetHubWestend::assert_xcm_pallet_attempted_complete(None);
	let (expected_foreign_asset_id, expected_foreign_asset_amount) =
		non_fee_asset(&t.args.assets, t.args.fee_asset_item as usize).unwrap();
	let expected_foreign_asset_id_v3: v3::Location = expected_foreign_asset_id.try_into().unwrap();
	assert_expected_events!(
		AssetHubWestend,
		vec![
			// native asset used for fees is transferred to Parachain's Sovereign account as reserve
			RuntimeEvent::Balances(
				pallet_balances::Event::Transfer { from, to, amount }
			) => {
				from: *from == t.sender.account_id,
				to: *to == AssetHubWestend::sovereign_account_id_of(
					t.args.dest.clone()
				),
				amount: *amount == t.args.amount,
			},
			// foreign asset is burned locally as part of teleportation
			RuntimeEvent::ForeignAssets(pallet_assets::Event::Burned { asset_id, owner, balance }) => {
				asset_id: *asset_id == expected_foreign_asset_id_v3,
				owner: *owner == t.sender.account_id,
				balance: *balance == expected_foreign_asset_amount,
			},
		]
	);
}

fn ah_to_penpal_foreign_assets_receiver_assertions(t: SystemParaToParaTest) {
	type RuntimeEvent = <PenpalB as Chain>::RuntimeEvent;
	let expected_asset_id = t.args.asset_id.unwrap();
	let (_, expected_asset_amount) =
		non_fee_asset(&t.args.assets, t.args.fee_asset_item as usize).unwrap();
	let checking_account = <PenpalB as PenpalBPallet>::PolkadotXcm::check_account();
	assert_expected_events!(
		PenpalB,
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
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { who, .. }) => {
				who: *who == t.receiver.account_id,
			},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn relay_limited_teleport_assets(t: RelayToSystemParaTest) -> DispatchResult {
	<Westend as WestendPallet>::XcmPallet::limited_teleport_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn relay_teleport_assets(t: RelayToSystemParaTest) -> DispatchResult {
	<Westend as WestendPallet>::XcmPallet::teleport_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
	)
}

fn system_para_limited_teleport_assets(t: SystemParaToRelayTest) -> DispatchResult {
	<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::limited_teleport_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn system_para_teleport_assets(t: SystemParaToRelayTest) -> DispatchResult {
	<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::teleport_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
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

fn para_to_system_para_transfer_assets(t: ParaToSystemParaTest) -> DispatchResult {
	<PenpalB as PenpalBPallet>::PolkadotXcm::transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

/// Limited Teleport of native asset from Relay Chain to the System Parachain should work
#[test]
fn limited_teleport_native_assets_from_relay_to_system_para_works() {
	// Init values for Relay Chain
	let amount_to_send: Balance = WESTEND_ED * 1000;
	let dest = Westend::child_location_of(AssetHubWestend::para_id());
	let beneficiary = AssetHubWestendReceiver::get();
	let test_args = TestContext {
		sender: WestendSender::get(),
		receiver: beneficiary.clone(),
		args: TestArgs::new_relay(dest, beneficiary, amount_to_send),
	};

	let mut test = RelayToSystemParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<Westend>(relay_origin_assertions);
	test.set_assertion::<AssetHubWestend>(para_dest_assertions);
	test.set_dispatchable::<Westend>(relay_limited_teleport_assets);
	test.assert();

	let delivery_fees = Westend::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<WestendXcmConfig as xcm_executor::Config>::XcmSender,
		>(test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest)
	});

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	// Receiver's balance is increased
	assert!(receiver_balance_after > receiver_balance_before);
}

/// Limited Teleport of native asset from System Parachain to Relay Chain
/// should work when there is enough balance in Relay Chain's `CheckAccount`
#[test]
fn limited_teleport_native_assets_back_from_system_para_to_relay_works() {
	// Dependency - Relay Chain's `CheckAccount` should have enough balance
	limited_teleport_native_assets_from_relay_to_system_para_works();

	// Init values for Relay Chain
	let amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;
	let destination = AssetHubWestend::parent_location();
	let beneficiary_id = WestendReceiver::get();
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: AssetHubWestendSender::get(),
		receiver: WestendReceiver::get(),
		args: TestArgs::new_para(destination, beneficiary_id, amount_to_send, assets, None, 0),
	};

	let mut test = SystemParaToRelayTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<AssetHubWestend>(para_origin_assertions);
	test.set_assertion::<Westend>(relay_dest_assertions);
	test.set_dispatchable::<AssetHubWestend>(system_para_limited_teleport_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	let delivery_fees = AssetHubWestend::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<AssetHubWestendXcmConfig as xcm_executor::Config>::XcmSender,
		>(test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest)
	});

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	// Receiver's balance is increased
	assert!(receiver_balance_after > receiver_balance_before);
}

/// Limited Teleport of native asset from System Parachain to Relay Chain
/// should't work when there is not enough balance in Relay Chain's `CheckAccount`
#[test]
fn limited_teleport_native_assets_from_system_para_to_relay_fails() {
	// Init values for Relay Chain
	let amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;
	let destination = AssetHubWestend::parent_location().into();
	let beneficiary_id = WestendReceiver::get().into();
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: AssetHubWestendSender::get(),
		receiver: WestendReceiver::get(),
		args: TestArgs::new_para(destination, beneficiary_id, amount_to_send, assets, None, 0),
	};

	let mut test = SystemParaToRelayTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<AssetHubWestend>(para_origin_assertions);
	test.set_assertion::<Westend>(relay_dest_assertions_fail);
	test.set_dispatchable::<AssetHubWestend>(system_para_limited_teleport_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	let delivery_fees = AssetHubWestend::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<AssetHubWestendXcmConfig as xcm_executor::Config>::XcmSender,
		>(test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest)
	});

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	// Receiver's balance does not change
	assert_eq!(receiver_balance_after, receiver_balance_before);
}

/// Teleport of native asset from Relay Chain to the System Parachain should work
#[test]
fn teleport_native_assets_from_relay_to_system_para_works() {
	// Init values for Relay Chain
	let amount_to_send: Balance = WESTEND_ED * 1000;
	let dest = Westend::child_location_of(AssetHubWestend::para_id());
	let beneficiary = AssetHubWestendReceiver::get();
	let test_args = TestContext {
		sender: WestendSender::get(),
		receiver: beneficiary.clone(),
		args: TestArgs::new_relay(dest, beneficiary, amount_to_send),
	};

	let mut test = RelayToSystemParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<Westend>(relay_origin_assertions);
	test.set_assertion::<AssetHubWestend>(para_dest_assertions);
	test.set_dispatchable::<Westend>(relay_teleport_assets);
	test.assert();

	let delivery_fees = Westend::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<WestendXcmConfig as xcm_executor::Config>::XcmSender,
		>(test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest)
	});

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	// Receiver's balance is increased
	assert!(receiver_balance_after > receiver_balance_before);
}

/// Teleport of native asset from System Parachains to the Relay Chain
/// should work when there is enough balance in Relay Chain's `CheckAccount`
#[test]
fn teleport_native_assets_back_from_system_para_to_relay_works() {
	// Dependency - Relay Chain's `CheckAccount` should have enough balance
	teleport_native_assets_from_relay_to_system_para_works();

	// Init values for Relay Chain
	let amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;
	let destination = AssetHubWestend::parent_location();
	let beneficiary_id = WestendReceiver::get();
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: AssetHubWestendSender::get(),
		receiver: WestendReceiver::get(),
		args: TestArgs::new_para(destination, beneficiary_id, amount_to_send, assets, None, 0),
	};

	let mut test = SystemParaToRelayTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<AssetHubWestend>(para_origin_assertions);
	test.set_assertion::<Westend>(relay_dest_assertions);
	test.set_dispatchable::<AssetHubWestend>(system_para_teleport_assets);
	test.assert();

	let delivery_fees = AssetHubWestend::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<AssetHubWestendXcmConfig as xcm_executor::Config>::XcmSender,
		>(test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest)
	});

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	// Receiver's balance is increased
	assert!(receiver_balance_after > receiver_balance_before);
}

/// Teleport of native asset from System Parachain to Relay Chain
/// shouldn't work when there is not enough balance in Relay Chain's `CheckAccount`
#[test]
fn teleport_native_assets_from_system_para_to_relay_fails() {
	// Init values for Relay Chain
	let amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;
	let destination = AssetHubWestend::parent_location();
	let beneficiary_id = WestendReceiver::get();
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: AssetHubWestendSender::get(),
		receiver: WestendReceiver::get(),
		args: TestArgs::new_para(destination, beneficiary_id, amount_to_send, assets, None, 0),
	};

	let mut test = SystemParaToRelayTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<AssetHubWestend>(para_origin_assertions);
	test.set_assertion::<Westend>(relay_dest_assertions_fail);
	test.set_dispatchable::<AssetHubWestend>(system_para_teleport_assets);
	test.assert();

	let delivery_fees = AssetHubWestend::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<AssetHubWestendXcmConfig as xcm_executor::Config>::XcmSender,
		>(test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest)
	});

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	// Receiver's balance does not change
	assert_eq!(receiver_balance_after, receiver_balance_before);
}

#[test]
fn teleport_to_other_system_parachains_works() {
	let amount = ASSET_HUB_WESTEND_ED * 100;
	let native_asset: Assets = (Parent, amount).into();

	test_parachain_is_trusted_teleporter!(
		AssetHubWestend,          // Origin
		AssetHubWestendXcmConfig, // XCM Configuration
		vec![BridgeHubWestend],   // Destinations
		(native_asset, amount)
	);
}

/// Bidirectional teleports of local Penpal assets to Asset Hub as foreign assets should work
/// (using native reserve-based transfer for fees)
#[test]
fn bidirectional_teleport_foreign_assets_between_para_and_asset_hub() {
	let ah_as_seen_by_penpal = PenpalB::sibling_location_of(AssetHubWestend::para_id());
	let asset_location_on_penpal = PenpalLocalTeleportableToAssetHubV3::get();
	let asset_id_on_penpal = match asset_location_on_penpal.last() {
		Some(v3::Junction::GeneralIndex(id)) => *id as u32,
		_ => unreachable!(),
	};
	let asset_owner_on_penpal = PenpalBSender::get();
	let foreign_asset_at_asset_hub_westend =
		v3::Location::new(1, [v3::Junction::Parachain(PenpalB::para_id().into())])
			.appended_with(asset_location_on_penpal)
			.unwrap();
	super::penpal_create_foreign_asset_on_asset_hub(
		asset_id_on_penpal,
		foreign_asset_at_asset_hub_westend,
		ah_as_seen_by_penpal.clone(),
		false,
		asset_owner_on_penpal,
		ASSET_MIN_BALANCE * 1_000_000,
	);
	let penpal_to_ah_beneficiary_id = AssetHubWestendReceiver::get();

	let fee_amount_to_send = ASSET_HUB_WESTEND_ED * 1000;
	let asset_amount_to_send = ASSET_MIN_BALANCE * 1000;

	let asset_location_on_penpal_latest: Location = asset_location_on_penpal.try_into().unwrap();
	let penpal_assets: Assets = vec![
		(Parent, fee_amount_to_send).into(),
		(asset_location_on_penpal_latest, asset_amount_to_send).into(),
	]
	.into();
	let fee_asset_index = penpal_assets
		.inner()
		.iter()
		.position(|r| r == &(Parent, fee_amount_to_send).into())
		.unwrap() as u32;

	// Penpal to AH test args
	let penpal_to_ah_test_args = TestContext {
		sender: PenpalBSender::get(),
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

	let penpal_sender_balance_before = penpal_to_ah.sender.balance;
	let ah_receiver_balance_before = penpal_to_ah.receiver.balance;

	let penpal_sender_assets_before = PenpalB::execute_with(|| {
		type Assets = <PenpalB as PenpalBPallet>::Assets;
		<Assets as Inspect<_>>::balance(asset_id_on_penpal, &PenpalBSender::get())
	});
	let ah_receiver_assets_before = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(
			foreign_asset_at_asset_hub_westend,
			&AssetHubWestendReceiver::get(),
		)
	});

	penpal_to_ah.set_assertion::<PenpalB>(penpal_to_ah_foreign_assets_sender_assertions);
	penpal_to_ah.set_assertion::<AssetHubWestend>(penpal_to_ah_foreign_assets_receiver_assertions);
	penpal_to_ah.set_dispatchable::<PenpalB>(para_to_system_para_transfer_assets);
	penpal_to_ah.assert();

	let penpal_sender_balance_after = penpal_to_ah.sender.balance;
	let ah_receiver_balance_after = penpal_to_ah.receiver.balance;

	let penpal_sender_assets_after = PenpalB::execute_with(|| {
		type Assets = <PenpalB as PenpalBPallet>::Assets;
		<Assets as Inspect<_>>::balance(asset_id_on_penpal, &PenpalBSender::get())
	});
	let ah_receiver_assets_after = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<Assets as Inspect<_>>::balance(
			foreign_asset_at_asset_hub_westend,
			&AssetHubWestendReceiver::get(),
		)
	});

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
			foreign_asset_at_asset_hub_westend,
			AssetHubWestendSender::get().into(),
			asset_amount_to_send,
		));
	});

	let foreign_asset_at_asset_hub_westend_latest: Location =
		foreign_asset_at_asset_hub_westend.try_into().unwrap();
	let ah_to_penpal_beneficiary_id = PenpalBReceiver::get();
	let penpal_as_seen_by_ah = AssetHubWestend::sibling_location_of(PenpalB::para_id());
	let ah_assets: Assets = vec![
		(Parent, fee_amount_to_send).into(),
		(foreign_asset_at_asset_hub_westend_latest, asset_amount_to_send).into(),
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
		receiver: PenpalBReceiver::get(),
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
	let penpal_receiver_balance_before = ah_to_penpal.receiver.balance;

	let ah_sender_assets_before = AssetHubWestend::execute_with(|| {
		type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(
			foreign_asset_at_asset_hub_westend,
			&AssetHubWestendSender::get(),
		)
	});
	let penpal_receiver_assets_before = PenpalB::execute_with(|| {
		type Assets = <PenpalB as PenpalBPallet>::Assets;
		<Assets as Inspect<_>>::balance(asset_id_on_penpal, &PenpalBReceiver::get())
	});

	ah_to_penpal.set_assertion::<AssetHubWestend>(ah_to_penpal_foreign_assets_sender_assertions);
	ah_to_penpal.set_assertion::<PenpalB>(ah_to_penpal_foreign_assets_receiver_assertions);
	ah_to_penpal.set_dispatchable::<AssetHubWestend>(system_para_to_para_transfer_assets);
	ah_to_penpal.assert();

	let ah_sender_balance_after = ah_to_penpal.sender.balance;
	let penpal_receiver_balance_after = ah_to_penpal.receiver.balance;

	let ah_sender_assets_after = AssetHubWestend::execute_with(|| {
		type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		<ForeignAssets as Inspect<_>>::balance(
			foreign_asset_at_asset_hub_westend,
			&AssetHubWestendSender::get(),
		)
	});
	let penpal_receiver_assets_after = PenpalB::execute_with(|| {
		type Assets = <PenpalB as PenpalBPallet>::Assets;
		<Assets as Inspect<_>>::balance(asset_id_on_penpal, &PenpalBReceiver::get())
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
