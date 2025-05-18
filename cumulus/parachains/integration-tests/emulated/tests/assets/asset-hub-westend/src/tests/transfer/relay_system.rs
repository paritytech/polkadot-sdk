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

fn relay_dest_assertions_fail(_t: SystemParaToRelayTest) {
	Westend::assert_ump_queue_processed(
		false,
		Some(AssetHubWestend::para_id()),
		Some(Weight::from_parts(157_718_000, 3_593)),
	);
}

fn para_origin_assertions(t: SystemParaToRelayTest) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

	AssetHubWestend::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(
		730_053_000,
		4_000,
	)));

	AssetHubWestend::assert_parachain_system_ump_sent();

	assert_expected_events!(
		AssetHubWestend,
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
	<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::limited_teleport_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

/// Reserve Transfers of native asset from Relay Chain to the Asset Hub shouldn't work
#[test]
fn reserve_transfer_native_asset_from_relay_to_asset_hub_fails() {
	// Init values for Relay Chain
	let signed_origin = <Westend as Chain>::RuntimeOrigin::signed(WestendSender::get().into());
	let destination = Westend::child_location_of(AssetHubWestend::para_id());
	let beneficiary: Location =
		AccountId32Junction { network: None, id: AssetHubWestendReceiver::get().into() }.into();
	let amount_to_send: Balance = WESTEND_ED * 1000;
	let assets: Assets = (Here, amount_to_send).into();
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

/// Reserve Transfers of native asset from Asset Hub to Relay Chain shouldn't work
#[test]
fn reserve_transfer_native_asset_from_asset_hub_to_relay_fails() {
	// Init values for Asset Hub
	let signed_origin =
		<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get().into());
	let destination = AssetHubWestend::parent_location();
	let beneficiary_id = WestendReceiver::get();
	let beneficiary: Location =
		AccountId32Junction { network: None, id: beneficiary_id.into() }.into();
	let amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;

	let assets: Assets = (Parent, amount_to_send).into();
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

#[test]
fn teleport_from_and_to_relay() {
	let amount = WESTEND_ED * 100;
	let native_asset: Assets = (Here, amount).into();

	test_relay_is_trusted_teleporter!(
		Westend,
		WestendXcmConfig,
		vec![AssetHubWestend],
		(native_asset, amount)
	);

	test_parachain_is_trusted_teleporter_for_relay!(
		AssetHubWestend,
		AssetHubWestendXcmConfig,
		Westend,
		amount
	);
}

/// Limited Teleport of native asset from System Parachain to Relay Chain
/// shouldn't work when there is not enough balance in Relay Chain's `CheckAccount`
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
		xcm_helpers::teleport_assets_delivery_fees::<
			<AssetHubWestendXcmConfig as xcm_executor::Config>::XcmSender,
		>(
			test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest
		)
	});

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	// Receiver's balance does not change
	assert_eq!(receiver_balance_after, receiver_balance_before);
}
