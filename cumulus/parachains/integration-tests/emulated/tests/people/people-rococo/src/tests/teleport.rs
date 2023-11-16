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

use people_rococo_runtime::xcm_config::XcmConfig as PeopleRococoXcmConfig;
use rococo_runtime::xcm_config::XcmConfig as RococoXcmConfig;

fn relay_origin_assertions(t: RelayToSystemParaTest) {
	type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;

	Rococo::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(631_531_000, 7_186)));

	assert_expected_events!(
		Rococo,
		vec![
			// Amount to teleport is withdrawn from Sender
			RuntimeEvent::Balances(pallet_balances::Event::Withdraw { who, amount }) => {
				who: *who == t.sender.account_id,
				amount: *amount == t.args.amount,
			},
			// Amount to teleport is deposited in Relay's `CheckAccount`
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { who, amount }) => {
				who: *who == <Rococo as RococoPallet>::XcmPallet::check_account(),
				amount:  *amount == t.args.amount,
			},
		]
	);
}

fn relay_dest_assertions(t: SystemParaToRelayTest) {
	type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;

	Rococo::assert_ump_queue_processed(
		true,
		Some(PeopleRococo::para_id()),
		Some(Weight::from_parts(307_225_000, 7_186)),
	);

	assert_expected_events!(
		Rococo,
		vec![
			// Amount is withdrawn from Relay Chain's `CheckAccount`
			RuntimeEvent::Balances(pallet_balances::Event::Withdraw { who, amount }) => {
				who: *who == <Rococo as RococoPallet>::XcmPallet::check_account(),
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
	Rococo::assert_ump_queue_processed(
		false,
		Some(PeopleRococo::para_id()),
		Some(Weight::from_parts(157_718_000, 3_593)),
	);
}

fn para_origin_assertions(t: SystemParaToRelayTest) {
	type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;

	PeopleRococo::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(
		720_053_000,
		7_203,
	)));

	PeopleRococo::assert_parachain_system_ump_sent();

	assert_expected_events!(
		PeopleRococo,
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
	type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;

	PeopleRococo::assert_dmp_queue_complete(Some(Weight::from_parts(157_718_000, 3593)));

	assert_expected_events!(
		PeopleRococo,
		vec![
			// Amount minus fees are deposited in Receiver's account
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { who, .. }) => {
				who: *who == t.receiver.account_id,
			},
		]
	);
}

fn relay_limited_teleport_assets(t: RelayToSystemParaTest) -> DispatchResult {
	<Rococo as RococoPallet>::XcmPallet::limited_teleport_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn relay_teleport_assets(t: RelayToSystemParaTest) -> DispatchResult {
	<Rococo as RococoPallet>::XcmPallet::teleport_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
	)
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

fn system_para_teleport_assets(t: SystemParaToRelayTest) -> DispatchResult {
	<PeopleRococo as PeopleRococoPallet>::PolkadotXcm::teleport_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
	)
}
