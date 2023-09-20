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

const MAX_CAPACITY: u32 = 8;
const MAX_MESSAGE_SIZE: u32 = 8192;

/// Opening HRMP channels between Parachains should work
#[test]
fn open_hrmp_channel_between_paras_works() {
	// Parchain A init values
	let para_a_id = PenpalRococoA::para_id();
	let para_a_root_origin = <PenpalRococoA as Chain>::RuntimeOrigin::root();

	// Parachain B init values
	let para_b_id = PenpalRococoB::para_id();
	let para_b_root_origin = <PenpalRococoB as Chain>::RuntimeOrigin::root();

	let fee_amount = ROCOCO_ED * 1000;
	let fund_amount = ROCOCO_ED * 1000_000_000;

	// Fund Parachain's Sovereign accounts to be able to reserve the deposit
	let para_a_sovereign_account = Rococo::fund_para_sovereign(fund_amount, para_a_id);
	let para_b_sovereign_account = Rococo::fund_para_sovereign(fund_amount, para_b_id);

	let relay_destination: VersionedMultiLocation = PenpalRococoA::parent_location().into();

	// ---- Init Open channel from Parachain to System Parachain
	let mut call = Rococo::init_open_channel_call(para_b_id, MAX_CAPACITY, MAX_MESSAGE_SIZE);
	let origin_kind = OriginKind::Native;
	let native_asset: MultiAsset = (Here, fee_amount).into();
	let beneficiary = Rococo::sovereign_account_id_of_child_para(para_a_id);

	let mut xcm = xcm_transact_paid_execution(call, origin_kind, native_asset.clone(), beneficiary);

	PenpalRococoA::execute_with(|| {
		assert_ok!(<PenpalRococoA as PenpalRococoAPallet>::PolkadotXcm::send(
			para_a_root_origin,
			bx!(relay_destination.clone()),
			bx!(xcm),
		));

		PenpalRococoA::assert_xcm_pallet_sent();
	});

	Rococo::execute_with(|| {
		type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;

		Rococo::assert_ump_queue_processed(
			true,
			Some(para_a_id),
			Some(Weight::from_parts(1_323_596_000, 207_186)),
		);

		assert_expected_events!(
			Rococo,
			vec![
				// Parachain's Sovereign account balance is withdrawn to pay XCM fees
				RuntimeEvent::Balances(pallet_balances::Event::Withdraw { who, amount }) => {
					who: *who == para_a_sovereign_account.clone(),
					amount: *amount == fee_amount,
				},
				// Open channel requested from Para A to Para B
				RuntimeEvent::Hrmp(
					polkadot_runtime_parachains::hrmp::Event::OpenChannelRequested(
						sender, recipient, max_capacity, max_message_size
					)
				) => {
					sender: *sender == para_a_id.into(),
					recipient: *recipient == para_b_id.into(),
					max_capacity: *max_capacity == MAX_CAPACITY,
					max_message_size: *max_message_size == MAX_MESSAGE_SIZE,
				},
			]
		);
	});

	// ---- Accept Open channel from Parachain to System Parachain
	call = Rococo::accept_open_channel_call(para_a_id);
	let beneficiary = Rococo::sovereign_account_id_of_child_para(para_b_id);

	xcm = xcm_transact_paid_execution(call, origin_kind, native_asset, beneficiary);

	PenpalRococoB::execute_with(|| {
		assert_ok!(<PenpalRococoB as PenpalRococoBPallet>::PolkadotXcm::send(
			para_b_root_origin,
			bx!(relay_destination),
			bx!(xcm),
		));

		PenpalRococoB::assert_xcm_pallet_sent();
	});

	PenpalRococoB::execute_with(|| {});

	Rococo::execute_with(|| {
		type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;

		Rococo::assert_ump_queue_processed(
			true,
			Some(para_b_id),
			Some(Weight::from_parts(1_312_558_000, 200_000)),
		);

		assert_expected_events!(
			Rococo,
			vec![
				// Parachain's Sovereign account balance is withdrawn to pay XCM fees
				RuntimeEvent::Balances(pallet_balances::Event::Withdraw { who, amount }) => {
					who: *who == para_b_sovereign_account.clone(),
					amount: *amount == fee_amount,
				},
				// Open channel accepted for Para A to Para B
				RuntimeEvent::Hrmp(
					polkadot_runtime_parachains::hrmp::Event::OpenChannelAccepted(
						sender, recipient
					)
				) => {
					sender: *sender == para_a_id.into(),
					recipient: *recipient == para_b_id.into(),
				},
			]
		);
	});

	Rococo::force_process_hrmp_open(para_a_id, para_b_id);
}

/// Opening HRMP channels between System Parachains and Parachains should work
#[test]
fn force_open_hrmp_channel_for_system_para_works() {
	// Relay Chain init values
	let relay_root_origin = <Rococo as Chain>::RuntimeOrigin::root();

	// System Para init values
	let system_para_id = AssetHubRococo::para_id();

	// Parachain A init values
	let para_a_id = PenpalRococoA::para_id();

	Rococo::execute_with(|| {
		assert_ok!(<Rococo as RococoPallet>::Hrmp::force_open_hrmp_channel(
			relay_root_origin,
			system_para_id,
			para_a_id,
			MAX_CAPACITY,
			MAX_MESSAGE_SIZE
		));

		type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;

		assert_expected_events!(
			Rococo,
			vec![
				// HRMP channel forced opened
				RuntimeEvent::Hrmp(
					polkadot_runtime_parachains::hrmp::Event::HrmpChannelForceOpened(
						sender, recipient, max_capacity, max_message_size
					)
				) => {
					sender: *sender == system_para_id.into(),
					recipient: *recipient == para_a_id.into(),
					max_capacity: *max_capacity == MAX_CAPACITY,
					max_message_size: *max_message_size == MAX_MESSAGE_SIZE,
				},
			]
		);
	});

	Rococo::force_process_hrmp_open(system_para_id, para_a_id);
}
