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

use crate::imports::*;
use emulated_integration_tests_common::{
	test_parachain_is_trusted_teleporter, test_parachain_is_trusted_teleporter_for_relay,
	test_relay_is_trusted_teleporter,
};

#[test]
fn teleport_via_limited_teleport_assets_from_and_to_relay() {
	let amount = WESTEND_ED * 100;
	let native_asset: Assets = (Here, amount).into();

	test_relay_is_trusted_teleporter!(
		Westend,
		vec![PeopleWestend],
		(native_asset, amount),
		limited_teleport_assets
	);

	test_parachain_is_trusted_teleporter_for_relay!(
		PeopleWestend,
		Westend,
		amount,
		limited_teleport_assets
	);
}

#[test]
fn teleport_via_transfer_assets_from_and_to_relay() {
	let amount = WESTEND_ED * 100;
	let native_asset: Assets = (Here, amount).into();

	test_relay_is_trusted_teleporter!(
		Westend,
		vec![PeopleWestend],
		(native_asset, amount),
		transfer_assets
	);

	test_parachain_is_trusted_teleporter_for_relay!(
		PeopleWestend,
		Westend,
		amount,
		transfer_assets
	);
}

#[test]
fn teleport_via_limited_teleport_assets_to_other_system_parachains_works() {
	let amount = WESTEND_ED * 100;
	let native_asset: Assets = (Parent, amount).into();

	test_parachain_is_trusted_teleporter!(
		PeopleWestend,         // Origin
		vec![AssetHubWestend], // Destinations
		(native_asset, amount),
		limited_teleport_assets
	);
}

#[test]
fn teleport_via_transfer_assets_to_other_system_parachains_works() {
	let amount = WESTEND_ED * 100;
	let native_asset: Assets = (Parent, amount).into();

	test_parachain_is_trusted_teleporter!(
		PeopleWestend,         // Origin
		vec![AssetHubWestend], // Destinations
		(native_asset, amount),
		transfer_assets
	);
}

/// Limited Teleport of native asset from System Parachain to Asset Hub
/// shouldn't work when there is not enough balance in Asset Hub's `CheckAccount`
#[test]
fn limited_teleport_native_assets_from_relay_to_asset_hub_checking_acc_fails() {
	let check_account = AssetHubWestend::execute_with(|| {
		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::check_account()
	});
	let amount_to_send_larger_than_checking_acc: Balance =
		AssetHubWestend::account_data_of(check_account).free + 1;
	let destination = PeopleWestend::sibling_location_of(AssetHubWestend::para_id());
	let beneficiary_id = AssetHubWestendReceiver::get();
	let assets = (Parent, amount_to_send_larger_than_checking_acc).into();

	// Fund a sender
	PeopleWestend::fund_accounts(vec![(PeopleWestendSender::get(), WESTEND_ED * 2_000u128)]);

	let test_args = TestContext {
		sender: PeopleWestendSender::get(),
		receiver: AssetHubWestendReceiver::get(),
		args: TestArgs::new_para(
			destination,
			beneficiary_id,
			amount_to_send_larger_than_checking_acc,
			assets,
			None,
			0,
		),
	};

	let mut test = SystemParaToSystemParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	fn para_dest_assertions_fails(_t: SystemParaToSystemParaTest) {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::MessageQueue(
					pallet_message_queue::Event::Processed { success: false, .. }
				) => {},
			]
		);
	}

	fn para_origin_assertions(t: SystemParaToSystemParaTest) {
		type RuntimeEvent = <PeopleWestend as Chain>::RuntimeEvent;

		PeopleWestend::assert_xcm_pallet_attempted_complete(None);

		assert_expected_events!(
			PeopleWestend,
			vec![
				// Amount is withdrawn from Sender's account
				RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount }) => {
					who: *who == t.sender.account_id,
					amount: *amount == t.args.amount,
				},
			]
		);
	}

	fn system_para_limited_teleport_assets(t: SystemParaToSystemParaTest) -> DispatchResult {
		<PeopleWestend as PeopleWestendPallet>::PolkadotXcm::limited_teleport_assets(
			t.signed_origin,
			bx!(t.args.dest.into()),
			bx!(t.args.beneficiary.into()),
			bx!(t.args.assets.into()),
			t.args.fee_asset_item,
			t.args.weight_limit,
		)
	}

	test.set_assertion::<PeopleWestend>(para_origin_assertions);
	test.set_assertion::<AssetHubWestend>(para_dest_assertions_fails);
	test.set_dispatchable::<PeopleWestend>(system_para_limited_teleport_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	let delivery_fees = PeopleWestend::execute_with(|| {
		xcm_helpers::teleport_assets_delivery_fees::<
			<PeopleWestendXcmConfig as xcm_executor::Config>::XcmSender,
		>(
			test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest
		)
	});

	// Sender's balance is reduced
	assert_eq!(
		sender_balance_before - amount_to_send_larger_than_checking_acc - delivery_fees,
		sender_balance_after
	);
	// Receiver's balance does not change
	assert_eq!(receiver_balance_after, receiver_balance_before);
}
