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
use westend_runtime::xcm_config::XcmConfig as WestendXcmConfig;
use westend_system_emulated_network::penpal_emulated_chain::XcmConfig as PenpalWestendXcmConfig;

fn relay_to_para_sender_assertions(t: RelayToParaTest) {
	type RuntimeEvent = <Westend as Chain>::RuntimeEvent;

	Westend::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(864_610_000, 8_799)));

	assert_expected_events!(
		Westend,
		vec![
			// Amount to reserve transfer is transferred to Parachain's Sovereign account
			RuntimeEvent::Balances(
				pallet_balances::Event::Transfer { from, to, amount }
			) => {
				from: *from == t.sender.account_id,
				to: *to == Westend::sovereign_account_id_of(
					t.args.dest.clone()
				),
				amount: *amount == t.args.amount,
			},
		]
	);
}

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
					t.args.dest.clone()
				),
				amount: *amount == t.args.amount,
			},
		]
	);
}

fn para_receiver_assertions<Test>(_: Test) {
	type RuntimeEvent = <PenpalB as Chain>::RuntimeEvent;
	assert_expected_events!(
		PenpalB,
		vec![
			RuntimeEvent::Balances(pallet_balances::Event::Minted { .. }) => {},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn para_to_system_para_sender_assertions(t: ParaToSystemParaTest) {
	type RuntimeEvent = <PenpalB as Chain>::RuntimeEvent;

	PenpalB::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(864_610_000, 8_799)));

	assert_expected_events!(
		PenpalB,
		vec![
			// Amount to reserve transfer is transferred to Parachain's Sovereign account
			RuntimeEvent::Balances(
				pallet_balances::Event::Burned { who, amount }
			) => {
				who: *who == t.sender.account_id,
				amount: *amount == t.args.amount,
			},
		]
	);
}

fn para_to_system_para_receiver_assertions(t: ParaToSystemParaTest) {
	type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

	let sov_penpal_on_ahw = AssetHubWestend::sovereign_account_id_of(
		AssetHubWestend::sibling_location_of(PenpalB::para_id()),
	);

	assert_expected_events!(
		AssetHubWestend,
		vec![
			// Amount to reserve transfer is transferred to Parachain's Sovereign account
			RuntimeEvent::Balances(
				pallet_balances::Event::Burned { who, amount }
			) => {
				who: *who == sov_penpal_on_ahw.clone().into(),
				amount: *amount == t.args.amount,
			},
			RuntimeEvent::Balances(pallet_balances::Event::Minted { .. }) => {},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn system_para_to_para_assets_sender_assertions(t: SystemParaToParaTest) {
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
					t.args.dest.clone()
				),
				amount: *amount == t.args.amount,
			},
		]
	);
}

fn system_para_to_para_assets_receiver_assertions<Test>(_: Test) {
	type RuntimeEvent = <PenpalB as Chain>::RuntimeEvent;
	assert_expected_events!(
		PenpalB,
		vec![
			RuntimeEvent::Balances(pallet_balances::Event::Minted { .. }) => {},
			RuntimeEvent::Assets(pallet_assets::Event::Issued { .. }) => {},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn para_to_para_sender_assertions(t: ParaToParaTest) {
	type RuntimeEvent = <PenpalB as Chain>::RuntimeEvent;
	PenpalB::assert_xcm_pallet_attempted_complete(None);
	assert_expected_events!(
		PenpalB,
		vec![
			// Amount to reserve transfer is transferred to Parachain's Sovereign account
			RuntimeEvent::Balances(
				pallet_balances::Event::Burned { who, amount }
			) => {
				who: *who == t.sender.account_id,
				amount: *amount == t.args.amount,
			},
			// XCM sent to relay reserve
			RuntimeEvent::ParachainSystem(
				cumulus_pallet_parachain_system::Event::UpwardMessageSent { .. }
			) => {},
		]
	);
}

fn para_to_para_relay_hop_assertions(t: ParaToParaTest) {
	type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
	let sov_penpal_b_on_westend =
		Westend::sovereign_account_id_of(Westend::child_location_of(PenpalB::para_id()));
	let sov_penpal_a_on_westend =
		Westend::sovereign_account_id_of(Westend::child_location_of(PenpalA::para_id()));
	assert_expected_events!(
		Westend,
		vec![
			// Withdrawn from sender parachain SA
			RuntimeEvent::Balances(
				pallet_balances::Event::Burned { who, amount }
			) => {
				who: *who == sov_penpal_b_on_westend,
				amount: *amount == t.args.amount,
			},
			// Deposited to receiver parachain SA
			RuntimeEvent::Balances(
				pallet_balances::Event::Minted { who, .. }
			) => {
				who: *who == sov_penpal_a_on_westend,
			},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn para_to_para_receiver_assertions(_: ParaToParaTest) {
	type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
	assert_expected_events!(
		PenpalA,
		vec![
			RuntimeEvent::Balances(pallet_balances::Event::Minted { .. }) => {},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn relay_to_para_reserve_transfer_assets(t: RelayToParaTest) -> DispatchResult {
	<Westend as WestendPallet>::XcmPallet::limited_reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn system_para_to_para_reserve_transfer_assets(t: SystemParaToParaTest) -> DispatchResult {
	<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::limited_reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn para_to_system_para_reserve_transfer_assets(t: ParaToSystemParaTest) -> DispatchResult {
	<PenpalB as PenpalBPallet>::PolkadotXcm::limited_reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn para_to_para_limited_reserve_transfer_assets(t: ParaToParaTest) -> DispatchResult {
	<PenpalB as PenpalBPallet>::PolkadotXcm::limited_reserve_transfer_assets(
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

/// Reserve Transfers of native asset from System Parachain to Relay Chain shouldn't work
#[test]
fn reserve_transfer_native_asset_from_system_para_to_relay_fails() {
	// Init values for System Parachain
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

/// Reserve Transfers of native asset from Relay to Parachain should work
#[test]
fn reserve_transfer_native_asset_from_relay_to_para() {
	// Init values for Relay
	let destination = Westend::child_location_of(PenpalB::para_id());
	let beneficiary_id = PenpalBReceiver::get();
	let amount_to_send: Balance = WESTEND_ED * 1000;

	let test_args = TestContext {
		sender: WestendSender::get(),
		receiver: PenpalBReceiver::get(),
		args: TestArgs::new_relay(destination, beneficiary_id, amount_to_send),
	};

	let mut test = RelayToParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<Westend>(relay_to_para_sender_assertions);
	test.set_assertion::<PenpalB>(para_receiver_assertions);
	test.set_dispatchable::<Westend>(relay_to_para_reserve_transfer_assets);
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
	// Receiver's balance increased by `amount_to_send - delivery_fees - bought_execution`;
	// `delivery_fees` might be paid from transfer or JIT, also `bought_execution` is unknown but
	// should be non-zero
	assert!(receiver_balance_after < receiver_balance_before + amount_to_send);
}

/// Reserve Transfers of native asset from System Parachain to Parachain should work
#[test]
fn reserve_transfer_native_asset_from_system_para_to_para() {
	// Init values for System Parachain
	let destination = AssetHubWestend::sibling_location_of(PenpalB::para_id());
	let beneficiary_id = PenpalBReceiver::get();
	let amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: AssetHubWestendSender::get(),
		receiver: PenpalBReceiver::get(),
		args: TestArgs::new_para(destination, beneficiary_id, amount_to_send, assets, None, 0),
	};

	let mut test = SystemParaToParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<AssetHubWestend>(system_para_to_para_sender_assertions);
	test.set_assertion::<PenpalB>(para_receiver_assertions);
	test.set_dispatchable::<AssetHubWestend>(system_para_to_para_reserve_transfer_assets);
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
	// Receiver's balance increased by `amount_to_send - delivery_fees - bought_execution`;
	// `delivery_fees` might be paid from transfer or JIT, also `bought_execution` is unknown but
	// should be non-zero
	assert!(receiver_balance_after < receiver_balance_before + amount_to_send);
}

/// Reserve Transfers of native asset from Parachain to System Parachain should work
#[test]
fn reserve_transfer_native_asset_from_para_to_system_para() {
	// Init values for Penpal Parachain
	let destination = PenpalB::sibling_location_of(AssetHubWestend::para_id());
	let beneficiary_id = AssetHubWestendReceiver::get();
	let amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: PenpalBSender::get(),
		receiver: AssetHubWestendReceiver::get(),
		args: TestArgs::new_para(destination, beneficiary_id, amount_to_send, assets, None, 0),
	};

	let mut test = ParaToSystemParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	let penpal_location_as_seen_by_ahw = AssetHubWestend::sibling_location_of(PenpalB::para_id());
	let sov_penpal_on_ahw =
		AssetHubWestend::sovereign_account_id_of(penpal_location_as_seen_by_ahw);

	// fund the Penpal's SA on AHW with the native tokens held in reserve
	AssetHubWestend::fund_accounts(vec![(sov_penpal_on_ahw.into(), amount_to_send * 2)]);

	test.set_assertion::<PenpalB>(para_to_system_para_sender_assertions);
	test.set_assertion::<AssetHubWestend>(para_to_system_para_receiver_assertions);
	test.set_dispatchable::<PenpalB>(para_to_system_para_reserve_transfer_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	let delivery_fees = PenpalB::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<PenpalWestendXcmConfig as xcm_executor::Config>::XcmSender,
		>(test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest)
	});

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	// Receiver's balance is increased
	assert!(receiver_balance_after > receiver_balance_before);
	// Receiver's balance increased by `amount_to_send - delivery_fees - bought_execution`;
	// `delivery_fees` might be paid from transfer or JIT, also `bought_execution` is unknown but
	// should be non-zero
	assert!(receiver_balance_after < receiver_balance_before + amount_to_send);
}

/// Reserve Transfers of a local asset and native asset from System Parachain to Parachain should
/// work
#[test]
fn reserve_transfer_assets_from_system_para_to_para() {
	// Force create asset on AssetHubWestend and PenpalB from Relay Chain
	AssetHubWestend::force_create_and_mint_asset(
		ASSET_ID,
		ASSET_MIN_BALANCE,
		true,
		AssetHubWestendSender::get(),
		Some(Weight::from_parts(1_019_445_000, 200_000)),
		ASSET_MIN_BALANCE * 1_000_000,
	);
	PenpalB::force_create_and_mint_asset(
		ASSET_ID,
		ASSET_MIN_BALANCE,
		false,
		PenpalBSender::get(),
		None,
		0,
	);

	// Init values for System Parachain
	let destination = AssetHubWestend::sibling_location_of(PenpalB::para_id());
	let beneficiary_id = PenpalBReceiver::get();
	let fee_amount_to_send = ASSET_HUB_WESTEND_ED * 1000;
	let asset_amount_to_send = ASSET_MIN_BALANCE * 1000;
	let assets: Assets = vec![
		(Parent, fee_amount_to_send).into(),
		([PalletInstance(ASSETS_PALLET_ID), GeneralIndex(ASSET_ID.into())], asset_amount_to_send)
			.into(),
	]
	.into();
	let fee_asset_index = assets
		.inner()
		.iter()
		.position(|r| r == &(Parent, fee_amount_to_send).into())
		.unwrap() as u32;

	let para_test_args = TestContext {
		sender: AssetHubWestendSender::get(),
		receiver: PenpalBReceiver::get(),
		args: TestArgs::new_para(
			destination,
			beneficiary_id,
			asset_amount_to_send,
			assets,
			None,
			fee_asset_index,
		),
	};

	let mut test = SystemParaToParaTest::new(para_test_args);

	// Create SA-of-Penpal-on-AHW with ED.
	let penpal_location = AssetHubWestend::sibling_location_of(PenpalB::para_id());
	let sov_penpal_on_ahw = AssetHubWestend::sovereign_account_id_of(penpal_location);
	AssetHubWestend::fund_accounts(vec![(sov_penpal_on_ahw.into(), WESTEND_ED)]);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	let sender_assets_before = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
		<Assets as Inspect<_>>::balance(ASSET_ID, &AssetHubWestendSender::get())
	});
	let receiver_assets_before = PenpalB::execute_with(|| {
		type Assets = <PenpalB as PenpalBPallet>::Assets;
		<Assets as Inspect<_>>::balance(ASSET_ID, &PenpalBReceiver::get())
	});

	test.set_assertion::<AssetHubWestend>(system_para_to_para_assets_sender_assertions);
	test.set_assertion::<PenpalB>(system_para_to_para_assets_receiver_assertions);
	test.set_dispatchable::<AssetHubWestend>(system_para_to_para_reserve_transfer_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	// Sender's balance is reduced
	assert!(sender_balance_after < sender_balance_before);
	// Receiver's balance is increased
	assert!(receiver_balance_after > receiver_balance_before);
	// Receiver's balance increased by `amount_to_send - delivery_fees - bought_execution`;
	// `delivery_fees` might be paid from transfer or JIT, also `bought_execution` is unknown but
	// should be non-zero
	assert!(receiver_balance_after < receiver_balance_before + fee_amount_to_send);

	let sender_assets_after = AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
		<Assets as Inspect<_>>::balance(ASSET_ID, &AssetHubWestendSender::get())
	});
	let receiver_assets_after = PenpalB::execute_with(|| {
		type Assets = <PenpalB as PenpalBPallet>::Assets;
		<Assets as Inspect<_>>::balance(ASSET_ID, &PenpalBReceiver::get())
	});

	// Sender's balance is reduced by exact amount
	assert_eq!(sender_assets_before - asset_amount_to_send, sender_assets_after);
	// Receiver's balance is increased by exact amount
	assert_eq!(receiver_assets_after, receiver_assets_before + asset_amount_to_send);
}

/// Reserve Transfers of native asset from Parachain to Parachain (through Relay reserve) should
/// work
#[test]
fn reserve_transfer_native_asset_from_para_to_para() {
	// Init values for Penpal Parachain
	let destination = PenpalB::sibling_location_of(PenpalA::para_id());
	let beneficiary_id = PenpalAReceiver::get();
	let amount_to_send: Balance = ASSET_HUB_WESTEND_ED * 1000;
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: PenpalBSender::get(),
		receiver: PenpalAReceiver::get(),
		args: TestArgs::new_para(destination, beneficiary_id, amount_to_send, assets, None, 0),
	};

	let mut test = ParaToParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	let sender_as_seen_by_relay = Westend::child_location_of(PenpalB::para_id());
	let sov_of_sender_on_relay = Westend::sovereign_account_id_of(sender_as_seen_by_relay);

	// fund the PenpalB's SA on Westend with the native tokens held in reserve
	Westend::fund_accounts(vec![(sov_of_sender_on_relay.into(), amount_to_send * 2)]);

	test.set_assertion::<PenpalB>(para_to_para_sender_assertions);
	test.set_assertion::<Westend>(para_to_para_relay_hop_assertions);
	test.set_assertion::<PenpalA>(para_to_para_receiver_assertions);
	test.set_dispatchable::<PenpalB>(para_to_para_limited_reserve_transfer_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	let delivery_fees = PenpalB::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<PenpalWestendXcmConfig as xcm_executor::Config>::XcmSender,
		>(test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest)
	});

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	// Receiver's balance is increased
	assert!(receiver_balance_after > receiver_balance_before);
}
