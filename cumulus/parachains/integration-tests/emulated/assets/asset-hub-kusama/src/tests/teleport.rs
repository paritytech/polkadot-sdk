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

#![allow(dead_code)] // <https://github.com/paritytech/cumulus/issues/3027>

use crate::*;

fn relay_origin_assertions(t: RelayToSystemParaTest) {
	type RuntimeEvent = <Kusama as Chain>::RuntimeEvent;

	Kusama::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(631_531_000, 7_186)));

	assert_expected_events!(
		Kusama,
		vec![
			// Amount to teleport is withdrawn from Sender
			RuntimeEvent::Balances(pallet_balances::Event::Withdraw { who, amount }) => {
				who: *who == t.sender.account_id,
				amount: *amount == t.args.amount,
			},
			// Amount to teleport is deposited in Relay's `CheckAccount`
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { who, amount }) => {
				who: *who == <Kusama as KusamaPallet>::XcmPallet::check_account(),
				amount:  *amount == t.args.amount,
			},
		]
	);
}

fn relay_dest_assertions(t: SystemParaToRelayTest) {
	type RuntimeEvent = <Kusama as Chain>::RuntimeEvent;

	Kusama::assert_ump_queue_processed(
		true,
		Some(AssetHubKusama::para_id()),
		Some(Weight::from_parts(307_225_000, 7_186)),
	);

	assert_expected_events!(
		Kusama,
		vec![
			// Amount is witdrawn from Relay Chain's `CheckAccount`
			RuntimeEvent::Balances(pallet_balances::Event::Withdraw { who, amount }) => {
				who: *who == <Kusama as KusamaPallet>::XcmPallet::check_account(),
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
	Kusama::assert_ump_queue_processed(
		false,
		Some(AssetHubKusama::para_id()),
		Some(Weight::from_parts(148_433_000, 3_593)),
	);
}

fn para_origin_assertions(t: SystemParaToRelayTest) {
	type RuntimeEvent = <AssetHubKusama as Chain>::RuntimeEvent;

	AssetHubKusama::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(
		534_872_000,
		7_133,
	)));

	AssetHubKusama::assert_parachain_system_ump_sent();

	assert_expected_events!(
		AssetHubKusama,
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
	type RuntimeEvent = <AssetHubKusama as Chain>::RuntimeEvent;

	AssetHubKusama::assert_dmp_queue_complete(Some(Weight::from_parts(165_592_000, 0)));

	assert_expected_events!(
		AssetHubKusama,
		vec![
			// Amount minus fees are deposited in Receiver's account
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { who, .. }) => {
				who: *who == t.receiver.account_id,
			},
		]
	);
}

fn relay_limited_teleport_assets(t: RelayToSystemParaTest) -> DispatchResult {
	<Kusama as KusamaPallet>::XcmPallet::limited_teleport_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn relay_teleport_assets(t: RelayToSystemParaTest) -> DispatchResult {
	<Kusama as KusamaPallet>::XcmPallet::teleport_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
	)
}

fn system_para_limited_teleport_assets(t: SystemParaToRelayTest) -> DispatchResult {
	<AssetHubKusama as AssetHubKusamaPallet>::PolkadotXcm::limited_teleport_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

// TODO: Uncomment when https://github.com/paritytech/polkadot/pull/7424 is merged
// fn system_para_teleport_assets(t: SystemParaToRelayTest) -> DispatchResult {
// 	<AssetHubKusama as AssetHubKusamaPallet>::PolkadotXcm::teleport_assets(
// 		t.signed_origin,
// 		bx!(t.args.dest),
// 		bx!(t.args.beneficiary),
// 		bx!(t.args.assets),
// 		t.args.fee_asset_item,
// 	)
// }

/// Limited Teleport of native asset from Relay Chain to the System Parachain should work
#[test]
fn limited_teleport_native_assets_from_relay_to_system_para_works() {
	// Init values for Relay Chain
	let amount_to_send: Balance = KUSAMA_ED * 1000;
	let test_args = TestContext {
		sender: KusamaSender::get(),
		receiver: AssetHubKusamaReceiver::get(),
		args: relay_test_args(amount_to_send),
	};

	let mut test = RelayToSystemParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<Kusama>(relay_origin_assertions);
	test.set_assertion::<AssetHubKusama>(para_dest_assertions);
	test.set_dispatchable::<Kusama>(relay_limited_teleport_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send, sender_balance_after);
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
	let amount_to_send: Balance = ASSET_HUB_KUSAMA_ED * 1000;
	let destination = AssetHubKusama::parent_location();
	let beneficiary_id = KusamaReceiver::get();
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: AssetHubKusamaSender::get(),
		receiver: KusamaReceiver::get(),
		args: system_para_test_args(destination, beneficiary_id, amount_to_send, assets, None),
	};

	let mut test = SystemParaToRelayTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<AssetHubKusama>(para_origin_assertions);
	test.set_assertion::<Kusama>(relay_dest_assertions);
	test.set_dispatchable::<AssetHubKusama>(system_para_limited_teleport_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send, sender_balance_after);
	// Receiver's balance is increased
	assert!(receiver_balance_after > receiver_balance_before);
}

/// Limited Teleport of native asset from System Parachain to Relay Chain
/// should't work when there is not enough balance in Relay Chain's `CheckAccount`
#[test]
fn limited_teleport_native_assets_from_system_para_to_relay_fails() {
	// Init values for Relay Chain
	let amount_to_send: Balance = ASSET_HUB_KUSAMA_ED * 1000;
	let destination = AssetHubKusama::parent_location().into();
	let beneficiary_id = KusamaReceiver::get().into();
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: AssetHubKusamaSender::get(),
		receiver: KusamaReceiver::get(),
		args: system_para_test_args(destination, beneficiary_id, amount_to_send, assets, None),
	};

	let mut test = SystemParaToRelayTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<AssetHubKusama>(para_origin_assertions);
	test.set_assertion::<Kusama>(relay_dest_assertions_fail);
	test.set_dispatchable::<AssetHubKusama>(system_para_limited_teleport_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send, sender_balance_after);
	// Receiver's balance does not change
	assert_eq!(receiver_balance_after, receiver_balance_before);
}

/// Teleport of native asset from Relay Chain to the System Parachain should work
#[test]
fn teleport_native_assets_from_relay_to_system_para_works() {
	// Init values for Relay Chain
	let amount_to_send: Balance = KUSAMA_ED * 1000;
	let test_args = TestContext {
		sender: KusamaSender::get(),
		receiver: AssetHubKusamaReceiver::get(),
		args: relay_test_args(amount_to_send),
	};

	let mut test = RelayToSystemParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<Kusama>(relay_origin_assertions);
	test.set_assertion::<AssetHubKusama>(para_dest_assertions);
	test.set_dispatchable::<Kusama>(relay_teleport_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send, sender_balance_after);
	// Receiver's balance is increased
	assert!(receiver_balance_after > receiver_balance_before);
}

// TODO: Uncomment when https://github.com/paritytech/polkadot/pull/7424 is merged

// Right now it is failing in the Relay Chain with a
// `messageQueue.ProcessingFailed` event `error: Unsupported`.
// The reason is the `Weigher` in `pallet_xcm` is not properly calculating the `remote_weight`
// and it cause an `Overweight` error in `AllowTopLevelPaidExecutionFrom` barrier

// /// Teleport of native asset from System Parachains to the Relay Chain
// /// should work when there is enough balance in Relay Chain's `CheckAccount`
// #[test]
// fn teleport_native_assets_back_from_system_para_to_relay_works() {
// 	// Dependency - Relay Chain's `CheckAccount` should have enough balance
// 	teleport_native_assets_from_relay_to_system_para_works();

// 	// Init values for Relay Chain
// 	let amount_to_send: Balance = ASSET_HUB_KUSAMA_ED * 1000;
// 	let test_args = TestContext {
// 		sender: AssetHubKusamaSender::get(),
// 		receiver: KusamaReceiver::get(),
// 		args: get_para_dispatch_args(amount_to_send),
// 	};

// 	let mut test = SystemParaToRelayTest::new(test_args);

// 	let sender_balance_before = test.sender.balance;
// 	let receiver_balance_before = test.receiver.balance;

// 	test.set_assertion::<AssetHubKusama>(para_origin_assertions);
// 	test.set_assertion::<Kusama>(relay_dest_assertions);
// 	test.set_dispatchable::<AssetHubKusama>(system_para_teleport_assets);
// 	test.assert();

// 	let sender_balance_after = test.sender.balance;
// 	let receiver_balance_after = test.receiver.balance;

// 	// Sender's balance is reduced
// 	assert_eq!(sender_balance_before - amount_to_send, sender_balance_after);
// 	// Receiver's balance is increased
// 	assert!(receiver_balance_after > receiver_balance_before);
// }

// /// Teleport of native asset from System Parachain to Relay Chain
// /// shouldn't work when there is not enough balance in Relay Chain's `CheckAccount`
// #[test]
// fn teleport_native_assets_from_system_para_to_relay_fails() {
// 	// Init values for Relay Chain
// 	let amount_to_send: Balance = ASSET_HUB_KUSAMA_ED * 1000;
//  let assets = (Parent, amount_to_send).into();
//
// 	let test_args = TestContext {
// 		sender: AssetHubKusamaSender::get(),
// 		receiver: KusamaReceiver::get(),
// 		args: system_para_test_args(amount_to_send),
//      assets,
//      None
// 	};

// 	let mut test = SystemParaToRelayTest::new(test_args);

// 	let sender_balance_before = test.sender.balance;
// 	let receiver_balance_before = test.receiver.balance;

// 	test.set_assertion::<AssetHubKusama>(para_origin_assertions);
// 	test.set_assertion::<Kusama>(relay_dest_assertions);
// 	test.set_dispatchable::<AssetHubKusama>(system_para_teleport_assets);
// 	test.assert();

// 	let sender_balance_after = test.sender.balance;
// 	let receiver_balance_after = test.receiver.balance;

// 	// Sender's balance is reduced
// 	assert_eq!(sender_balance_before - amount_to_send, sender_balance_after);
// 	// Receiver's balance does not change
// 	assert_eq!(receiver_balance_after, receiver_balance_before);
// }
