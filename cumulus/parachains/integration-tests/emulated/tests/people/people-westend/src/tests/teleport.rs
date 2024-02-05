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
use people_westend_runtime::xcm_config::XcmConfig as PeopleWestendXcmConfig;
use westend_runtime::xcm_config::XcmConfig as WestendXcmConfig;

fn relay_origin_assertions(t: RelayToSystemParaTest) {
	type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
	Westend::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(627_959_000, 7_200)));

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
		Some(PeopleWestend::para_id()),
		Some(Weight::from_parts(304_266_000, 7_186)),
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
		Some(PeopleWestend::para_id()),
		Some(Weight::from_parts(157_718_000, 3_593)),
	);
}

fn para_origin_assertions(t: SystemParaToRelayTest) {
	type RuntimeEvent = <PeopleWestend as Chain>::RuntimeEvent;

	PeopleWestend::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(
		351_425_000,
		3_593,
	)));

	PeopleWestend::assert_parachain_system_ump_sent();

	assert_expected_events!(
		PeopleWestend,
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
	type RuntimeEvent = <PeopleWestend as Chain>::RuntimeEvent;

	PeopleWestend::assert_dmp_queue_complete(Some(Weight::from_parts(162_456_000, 0)));

	assert_expected_events!(
		PeopleWestend,
		vec![
			// Amount minus fees are deposited in Receiver's account
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { who, .. }) => {
				who: *who == t.receiver.account_id,
			},
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

fn system_para_limited_teleport_assets(t: SystemParaToRelayTest) -> DispatchResult {
	<PeopleWestend as PeopleWestendPallet>::PolkadotXcm::limited_teleport_assets(
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
	let dest = Westend::child_location_of(PeopleWestend::para_id());
	let beneficiary_id = PeopleWestendReceiver::get();
	let test_args = TestContext {
		sender: WestendSender::get(),
		receiver: PeopleWestendReceiver::get(),
		args: TestArgs::new_relay(dest, beneficiary_id, amount_to_send),
	};

	let mut test = RelayToSystemParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<Westend>(relay_origin_assertions);
	test.set_assertion::<PeopleWestend>(para_dest_assertions);
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

	let amount_to_send: Balance = PEOPLE_WESTEND_ED * 1000;
	let destination = PeopleWestend::parent_location();
	let beneficiary_id = WestendReceiver::get();
	let assets = (Parent, amount_to_send).into();

	// Fund a sender
	PeopleWestend::fund_accounts(vec![(PeopleWestendSender::get(), WESTEND_ED * 2_000u128)]);

	let test_args = TestContext {
		sender: PeopleWestendSender::get(),
		receiver: WestendReceiver::get(),
		args: TestArgs::new_para(destination, beneficiary_id, amount_to_send, assets, None, 0),
	};

	let mut test = SystemParaToRelayTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<PeopleWestend>(para_origin_assertions);
	test.set_assertion::<Westend>(relay_dest_assertions);
	test.set_dispatchable::<PeopleWestend>(system_para_limited_teleport_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	let delivery_fees = PeopleWestend::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<PeopleWestendXcmConfig as xcm_executor::Config>::XcmSender,
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
	let amount_to_send: Balance = WESTEND_ED * 1000;
	let destination = PeopleWestend::parent_location();
	let beneficiary_id = WestendReceiver::get();
	let assets = (Parent, amount_to_send).into();

	// Fund a sender
	PeopleWestend::fund_accounts(vec![(PeopleWestendSender::get(), WESTEND_ED * 2_000u128)]);

	let test_args = TestContext {
		sender: PeopleWestendSender::get(),
		receiver: WestendReceiver::get(),
		args: TestArgs::new_para(destination, beneficiary_id, amount_to_send, assets, None, 0),
	};

	let mut test = SystemParaToRelayTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<PeopleWestend>(para_origin_assertions);
	test.set_assertion::<Westend>(relay_dest_assertions_fail);
	test.set_dispatchable::<PeopleWestend>(system_para_limited_teleport_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	let delivery_fees = PeopleWestend::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<PeopleWestendXcmConfig as xcm_executor::Config>::XcmSender,
		>(test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest)
	});

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	// Receiver's balance does not change
	assert_eq!(receiver_balance_after, receiver_balance_before);
}
