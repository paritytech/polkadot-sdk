// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

#![allow(dead_code)] // <https://github.com/paritytech/cumulus/issues/3027>

use crate::*;

fn relay_origin_assertions(t: RelayToSystemParaTest) {
	type RuntimeEvent = <Polkadot as Chain>::RuntimeEvent;

	Polkadot::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(632_207_000, 7_186)));

	assert_expected_events!(
		Polkadot,
		vec![
			// Amount to teleport is withdrawn from Sender
			RuntimeEvent::Balances(pallet_balances::Event::Withdraw { who, amount }) => {
				who: *who == t.sender.account_id,
				amount: *amount == t.args.amount,
			},
			// Amount to teleport is deposited in Relay's `CheckAccount`
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { who, amount }) => {
				who: *who == <Polkadot as PolkadotPallet>::XcmPallet::check_account(),
				amount:  *amount == t.args.amount,
			},
		]
	);
}

fn relay_dest_assertions(t: SystemParaToRelayTest) {
	type RuntimeEvent = <Polkadot as Chain>::RuntimeEvent;

	Polkadot::assert_ump_queue_processed(
		true,
		Some(AssetHubPolkadot::para_id()),
		Some(Weight::from_parts(368_931_000, 7_186)),
	);

	assert_expected_events!(
		Polkadot,
		vec![
			// Amount is witdrawn from Relay Chain's `CheckAccount`
			RuntimeEvent::Balances(pallet_balances::Event::Withdraw { who, amount }) => {
				who: *who == <Polkadot as PolkadotPallet>::XcmPallet::check_account(),
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
	Polkadot::assert_ump_queue_processed(
		false,
		Some(AssetHubPolkadot::para_id()),
		Some(Weight::from_parts(232_982_000, 3_593)),
	);
}

fn para_origin_assertions(t: SystemParaToRelayTest) {
	type RuntimeEvent = <AssetHubPolkadot as Chain>::RuntimeEvent;

	AssetHubPolkadot::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(
		632_207_000,
		7_186,
	)));

	AssetHubPolkadot::assert_parachain_system_ump_sent();

	assert_expected_events!(
		AssetHubPolkadot,
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
	type RuntimeEvent = <AssetHubPolkadot as Chain>::RuntimeEvent;

	AssetHubPolkadot::assert_dmp_queue_complete(Some(Weight::from_parts(161_196_000, 0)));

	assert_expected_events!(
		AssetHubPolkadot,
		vec![
			// Amount minus fees are deposited in Receiver's account
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { who, .. }) => {
				who: *who == t.receiver.account_id,
			},
		]
	);
}

fn relay_limited_teleport_assets(t: RelayToSystemParaTest) -> DispatchResult {
	<Polkadot as PolkadotPallet>::XcmPallet::limited_teleport_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn relay_teleport_assets(t: RelayToSystemParaTest) -> DispatchResult {
	<Polkadot as PolkadotPallet>::XcmPallet::teleport_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
	)
}

fn system_para_limited_teleport_assets(t: SystemParaToRelayTest) -> DispatchResult {
	<AssetHubPolkadot as AssetHubPolkadotPallet>::PolkadotXcm::limited_teleport_assets(
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
// 	<AssetHubPolkadot as AssetHubPolkadotPallet>::PolkadotXcm::teleport_assets(
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
	let amount_to_send: Balance = POLKADOT_ED * 1000;
	let test_args = TestContext {
		sender: PolkadotSender::get(),
		receiver: AssetHubPolkadotReceiver::get(),
		args: relay_test_args(amount_to_send),
	};

	let mut test = RelayToSystemParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<Polkadot>(relay_origin_assertions);
	test.set_assertion::<AssetHubPolkadot>(para_dest_assertions);
	test.set_dispatchable::<Polkadot>(relay_limited_teleport_assets);
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
	let amount_to_send: Balance = ASSET_HUB_POLKADOT_ED * 1000;
	let destination = AssetHubPolkadot::parent_location();
	let beneficiary_id = PolkadotReceiver::get();
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: AssetHubPolkadotSender::get(),
		receiver: PolkadotReceiver::get(),
		args: system_para_test_args(destination, beneficiary_id, amount_to_send, assets, None),
	};

	let mut test = SystemParaToRelayTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<AssetHubPolkadot>(para_origin_assertions);
	test.set_assertion::<Polkadot>(relay_dest_assertions);
	test.set_dispatchable::<AssetHubPolkadot>(system_para_limited_teleport_assets);
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
	let amount_to_send: Balance = ASSET_HUB_POLKADOT_ED * 1000;
	let destination = AssetHubPolkadot::parent_location().into();
	let beneficiary_id = PolkadotReceiver::get().into();
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: AssetHubPolkadotSender::get(),
		receiver: PolkadotReceiver::get(),
		args: system_para_test_args(destination, beneficiary_id, amount_to_send, assets, None),
	};

	let mut test = SystemParaToRelayTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<AssetHubPolkadot>(para_origin_assertions);
	test.set_assertion::<Polkadot>(relay_dest_assertions_fail);
	test.set_dispatchable::<AssetHubPolkadot>(system_para_limited_teleport_assets);
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
	let amount_to_send: Balance = POLKADOT_ED * 1000;
	let test_args = TestContext {
		sender: PolkadotSender::get(),
		receiver: AssetHubPolkadotReceiver::get(),
		args: relay_test_args(amount_to_send),
	};

	let mut test = RelayToSystemParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<Polkadot>(relay_origin_assertions);
	test.set_assertion::<AssetHubPolkadot>(para_dest_assertions);
	test.set_dispatchable::<Polkadot>(relay_teleport_assets);
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
// 	let amount_to_send: Balance = ASSET_HUB_POLKADOT_ED * 1000;
// 	let test_args = TestContext {
// 		sender: AssetHubPolkadotSender::get(),
// 		receiver: PolkadotReceiver::get(),
// 		args: get_para_dispatch_args(amount_to_send),
// 	};

// 	let mut test = SystemParaToRelayTest::new(test_args);

// 	let sender_balance_before = test.sender.balance;
// 	let receiver_balance_before = test.receiver.balance;

// 	test.set_assertion::<AssetHubPolkadot>(para_origin_assertions);
// 	test.set_assertion::<Polkadot>(relay_dest_assertions);
// 	test.set_dispatchable::<AssetHubPolkadot>(system_para_teleport_assets);
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
// 	let amount_to_send: Balance = ASSET_HUB_POLKADOT_ED * 1000;
//  let assets = (Parent, amount_to_send).into();
//
// 	let test_args = TestContext {
// 		sender: AssetHubPolkadotSender::get(),
// 		receiver: PolkadotReceiver::get(),
// 		args: system_para_test_args(amount_to_send),
//      assets,
//      None
// 	};

// 	let mut test = SystemParaToRelayTest::new(test_args);

// 	let sender_balance_before = test.sender.balance;
// 	let receiver_balance_before = test.receiver.balance;

// 	test.set_assertion::<AssetHubPolkadot>(para_origin_assertions);
// 	test.set_assertion::<Polkadot>(relay_dest_assertions);
// 	test.set_dispatchable::<AssetHubPolkadot>(system_para_teleport_assets);
// 	test.assert();

// 	let sender_balance_after = test.sender.balance;
// 	let receiver_balance_after = test.receiver.balance;

// 	// Sender's balance is reduced
// 	assert_eq!(sender_balance_before - amount_to_send, sender_balance_after);
// 	// Receiver's balance does not change
// 	assert_eq!(receiver_balance_after, receiver_balance_before);
// }
