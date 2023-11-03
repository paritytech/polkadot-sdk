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
use asset_hub_westend_runtime::xcm_config::XcmConfig;

fn system_para_to_para_sender_assertions(t: SystemParaToParaTest) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

	AssetHubWestend::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(
		676_119_000,
		6196,
	)));

	assert_expected_events!(
		AssetHubWestend,
		vec![
			// Amount to reserve transfer is transferred to Parachain's Sovereign account
			RuntimeEvent::Balances(
				pallet_balances::Event::Transfer { from, to, amount }
			) => {
				from: *from == t.sender.account_id,
				to: *to == AssetHubWestend::sovereign_account_id_of(
					t.args.dest
				),
				amount: *amount == t.args.amount,
			},
		]
	);
}

fn para_receiver_assertions<Test>(_: Test) {
	type RuntimeEvent = <PenpalWestendA as Chain>::RuntimeEvent;
	assert_expected_events!(
		PenpalWestendA,
		vec![
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { .. }) => {},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn system_para_to_para_assets_assertions(t: SystemParaToParaTest) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

	AssetHubWestend::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(
		676_119_000,
		6196,
	)));

	assert_expected_events!(
		AssetHubWestend,
		vec![
			// Amount to reserve transfer is transferred to Parachain's Sovereign account
			RuntimeEvent::Assets(
				pallet_assets::Event::Transferred { asset_id, from, to, amount }
			) => {
				asset_id: *asset_id == ASSET_ID,
				from: *from == t.sender.account_id,
				to: *to == AssetHubWestend::sovereign_account_id_of(
					t.args.dest
				),
				amount: *amount == t.args.amount,
			},
		]
	);
}

fn system_para_to_para_limited_reserve_transfer_assets(t: SystemParaToParaTest) -> DispatchResult {
	<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::limited_reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

/// Reserve Transfers of native asset from Relay Chain to the System Parachain shouldn't work
#[test]
fn reserve_transfer_native_asset_from_relay_to_system_para_fails() {
	let signed_origin = <Westend as Chain>::RuntimeOrigin::signed(WestendSender::get().into());
	let destination = Westend::child_location_of(AssetHubWestend::para_id());
	let beneficiary: MultiLocation =
		AccountId32Junction { network: None, id: AssetHubWestendReceiver::get().into() }.into();
	let amount_to_send: Balance = WESTEND_ED * 1000;
	let assets: MultiAssets = (Here, amount_to_send).into();
	let fee_asset_item = 0;

	// this should fail
	Westend::execute_with(|| {
		let result = <Westend as WestendPallet>::XcmPallet::limited_reserve_transfer_assets(
			signed_origin,
			bx!(destination.into()),
			bx!(beneficiary.into()),
			bx!(assets.into()),
			fee_asset_item,
			WeightLimit::Unlimited,
		);
		assert_err!(
			result,
			DispatchError::Module(sp_runtime::ModuleError {
				index: 99,
				error: [2, 0, 0, 0],
				message: Some("Filtered")
			})
		);
	});
}

/// Reserve Transfers of native asset from System Parachain to Relay Chain shouldn't work
#[test]
fn reserve_transfer_native_asset_from_system_para_to_relay_fails() {
	// Init values for System Parachain
	let signed_origin =
		<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get().into());
	let destination = AssetHubWestend::parent_location();
	let beneficiary_id = WestendReceiver::get();
	let beneficiary: MultiLocation =
		AccountId32Junction { network: None, id: beneficiary_id.into() }.into();
	let amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;
	let assets: MultiAssets = (Parent, amount_to_send).into();
	let fee_asset_item = 0;

	// this should fail
	AssetHubWestend::execute_with(|| {
		let result =
			<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::limited_reserve_transfer_assets(
				signed_origin,
				bx!(destination.into()),
				bx!(beneficiary.into()),
				bx!(assets.into()),
				fee_asset_item,
				WeightLimit::Unlimited,
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
}

/// Reserve Transfers of native asset from System Parachain to Parachain should work
#[test]
fn reserve_transfer_native_asset_from_system_para_to_para() {
	// Init values for System Parachain
	let destination = AssetHubWestend::sibling_location_of(PenpalWestendA::para_id());
	let beneficiary_id = PenpalWestendAReceiver::get();
	let amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: AssetHubWestendSender::get(),
		receiver: PenpalWestendAReceiver::get(),
		args: system_para_test_args(destination, beneficiary_id, amount_to_send, assets, None),
	};

	let mut test = SystemParaToParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<AssetHubWestend>(system_para_to_para_sender_assertions);
	test.set_assertion::<PenpalWestendA>(para_receiver_assertions);
	test.set_dispatchable::<AssetHubWestend>(system_para_to_para_limited_reserve_transfer_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	let delivery_fees = AssetHubWestend::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<<XcmConfig as xcm_executor::Config>::XcmSender>(
			test.args.assets.clone(),
			0,
			test.args.weight_limit,
			test.args.beneficiary,
			test.args.dest,
		)
	});

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	// Receiver's balance is increased
	assert!(receiver_balance_after > receiver_balance_before);
}

/// Reserve Transfers of a local asset from System Parachain to Parachain should work
#[test]
fn reserve_transfer_asset_from_system_para_to_para() {
	// Force create asset from Relay Chain and mint assets for System Parachain's sender account
	AssetHubWestend::force_create_and_mint_asset(
		ASSET_ID,
		ASSET_MIN_BALANCE,
		true,
		AssetHubWestendSender::get(),
		Some(Weight::from_parts(1_019_445_000, 200_000)),
		ASSET_MIN_BALANCE * 1000000,
	);

	// Init values for System Parachain
	let destination = AssetHubWestend::sibling_location_of(PenpalWestendA::para_id());
	let beneficiary_id = PenpalWestendAReceiver::get();
	let amount_to_send = ASSET_MIN_BALANCE * 1000;
	let assets =
		(X2(PalletInstance(ASSETS_PALLET_ID), GeneralIndex(ASSET_ID.into())), amount_to_send)
			.into();

	let system_para_test_args = TestContext {
		sender: AssetHubWestendSender::get(),
		receiver: PenpalWestendAReceiver::get(),
		args: system_para_test_args(destination, beneficiary_id, amount_to_send, assets, None),
	};

	let mut system_para_test = SystemParaToParaTest::new(system_para_test_args);

	system_para_test.set_assertion::<AssetHubWestend>(system_para_to_para_assets_assertions);
	// TODO: Add assertions when Penpal is able to manage assets
	system_para_test
		.set_dispatchable::<AssetHubWestend>(system_para_to_para_limited_reserve_transfer_assets);
	system_para_test.assert();
}
