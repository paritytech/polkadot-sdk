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
	test_parachain_is_trusted_teleporter_for_relay, test_relay_is_trusted_teleporter,
};

#[test]
fn teleport_from_and_to_relay() {
	let amount = ROCOCO_ED * 100;
	let native_asset: Assets = (Here, amount).into();

	test_relay_is_trusted_teleporter!(
		Rococo,
		RococoXcmConfig,
		vec![PeopleRococo],
		(native_asset, amount)
	);

	test_parachain_is_trusted_teleporter_for_relay!(
		PeopleRococo,
		PeopleRococoXcmConfig,
		Rococo,
		amount
	);
}

fn relay_dest_assertions_fail(_t: SystemParaToRelayTest) {
	Rococo::assert_ump_queue_processed(false, Some(PeopleRococo::para_id()), None);
}

fn para_origin_assertions(t: SystemParaToRelayTest) {
	type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;

	PeopleRococo::assert_xcm_pallet_attempted_complete(None);

	PeopleRococo::assert_parachain_system_ump_sent();

	assert_expected_events!(
		PeopleRococo,
		vec![
			// Amount is withdrawn from Sender's account
			RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount }) => {
				who: *who == t.sender.account_id,
				amount: *amount == t.args.amount,
			},
		]
	);
}

fn system_para_limited_teleport_assets(t: SystemParaToRelayTest) -> DispatchResult {
	<PeopleRococo as PeopleRococoPallet>::PolkadotXcm::limited_teleport_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

/// Limited Teleport of native asset from System Parachain to Relay Chain
/// shouldn't work when there is not enough balance in Relay Chain's `CheckAccount`
#[test]
fn limited_teleport_native_assets_from_system_para_to_relay_fails() {
	// Init values for Relay Chain
	let amount_to_send: Balance = ROCOCO_ED * 1000;
	let destination = PeopleRococo::parent_location();
	let beneficiary_id = RococoReceiver::get();
	let assets = (Parent, amount_to_send).into();

	// Fund a sender
	PeopleRococo::fund_accounts(vec![(PeopleRococoSender::get(), ROCOCO_ED * 2_000u128)]);

	let test_args = TestContext {
		sender: PeopleRococoSender::get(),
		receiver: RococoReceiver::get(),
		args: TestArgs::new_para(destination, beneficiary_id, amount_to_send, assets, None, 0),
	};

	let mut test = SystemParaToRelayTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<PeopleRococo>(para_origin_assertions);
	test.set_assertion::<Rococo>(relay_dest_assertions_fail);
	test.set_dispatchable::<PeopleRococo>(system_para_limited_teleport_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	let delivery_fees = PeopleRococo::execute_with(|| {
		xcm_helpers::teleport_assets_delivery_fees::<
			<PeopleRococoXcmConfig as xcm_executor::Config>::XcmSender,
		>(test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest)
	});

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	// Receiver's balance does not change
	assert_eq!(receiver_balance_after, receiver_balance_before);
}
