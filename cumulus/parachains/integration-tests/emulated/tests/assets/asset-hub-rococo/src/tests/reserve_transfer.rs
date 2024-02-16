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
use asset_hub_rococo_runtime::xcm_config::XcmConfig as AssetHubRococoXcmConfig;
use rococo_runtime::xcm_config::XcmConfig as RococoXcmConfig;
use rococo_system_emulated_network::penpal_emulated_chain::XcmConfig as PenpalRococoXcmConfig;

fn relay_to_para_sender_assertions(t: RelayToParaTest) {
	type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;
	Rococo::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(864_610_000, 8_799)));
	assert_expected_events!(
		Rococo,
		vec![
			// Amount to reserve transfer is transferred to Parachain's Sovereign account
			RuntimeEvent::Balances(
				pallet_balances::Event::Transfer { from, to, amount }
			) => {
				from: *from == t.sender.account_id,
				to: *to == Rococo::sovereign_account_id_of(
					t.args.dest.clone()
				),
				amount: *amount == t.args.amount,
			},
		]
	);
}

fn system_para_to_para_sender_assertions(t: SystemParaToParaTest) {
	type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
	AssetHubRococo::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(
		864_610_000,
		8_799,
	)));
	assert_expected_events!(
		AssetHubRococo,
		vec![
			// Amount to reserve transfer is transferred to Parachain's Sovereign account
			RuntimeEvent::Balances(
				pallet_balances::Event::Transfer { from, to, amount }
			) => {
				from: *from == t.sender.account_id,
				to: *to == AssetHubRococo::sovereign_account_id_of(
					t.args.dest.clone()
				),
				amount: *amount == t.args.amount,
			},
		]
	);
}

fn para_receiver_assertions<Test>(_: Test) {
	type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
	assert_expected_events!(
		PenpalA,
		vec![
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { .. }) => {},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn para_to_system_para_sender_assertions(t: ParaToSystemParaTest) {
	type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
	PenpalA::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(864_610_000, 8_799)));
	assert_expected_events!(
		PenpalA,
		vec![
			// Amount to reserve transfer is transferred to Parachain's Sovereign account
			RuntimeEvent::Balances(
				pallet_balances::Event::Withdraw { who, amount }
			) => {
				who: *who == t.sender.account_id,
				amount: *amount == t.args.amount,
			},
		]
	);
}

fn para_to_system_para_receiver_assertions(t: ParaToSystemParaTest) {
	type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
	let sov_penpal_on_ahr = AssetHubRococo::sovereign_account_id_of(
		AssetHubRococo::sibling_location_of(PenpalA::para_id()),
	);
	assert_expected_events!(
		AssetHubRococo,
		vec![
			// Amount to reserve transfer is withdrawn from Parachain's Sovereign account
			RuntimeEvent::Balances(
				pallet_balances::Event::Withdraw { who, amount }
			) => {
				who: *who == sov_penpal_on_ahr.clone().into(),
				amount: *amount == t.args.amount,
			},
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { .. }) => {},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn system_para_to_para_assets_sender_assertions(t: SystemParaToParaTest) {
	type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
	AssetHubRococo::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(
		864_610_000,
		8799,
	)));
	assert_expected_events!(
		AssetHubRococo,
		vec![
			// Amount to reserve transfer is transferred to Parachain's Sovereign account
			RuntimeEvent::Assets(
				pallet_assets::Event::Transferred { asset_id, from, to, amount }
			) => {
				asset_id: *asset_id == ASSET_ID,
				from: *from == t.sender.account_id,
				to: *to == AssetHubRococo::sovereign_account_id_of(
					t.args.dest.clone()
				),
				amount: *amount == t.args.amount,
			},
		]
	);
}

fn system_para_to_para_assets_receiver_assertions<Test>(_: Test) {
	type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
	assert_expected_events!(
		PenpalA,
		vec![
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { .. }) => {},
			RuntimeEvent::Assets(pallet_assets::Event::Issued { .. }) => {},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn para_to_para_sender_assertions(t: ParaToParaTest) {
	type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
	PenpalA::assert_xcm_pallet_attempted_complete(None);
	assert_expected_events!(
		PenpalA,
		vec![
			// Amount to reserve transfer is transferred to Parachain's Sovereign account
			RuntimeEvent::Balances(
				pallet_balances::Event::Withdraw { who, amount }
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
	type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;
	let sov_penpal_a_on_rococo =
		Rococo::sovereign_account_id_of(Rococo::child_location_of(PenpalA::para_id()));
	let sov_penpal_b_on_rococo =
		Rococo::sovereign_account_id_of(Rococo::child_location_of(PenpalB::para_id()));
	assert_expected_events!(
		Rococo,
		vec![
			// Withdrawn from sender parachain SA
			RuntimeEvent::Balances(
				pallet_balances::Event::Withdraw { who, amount }
			) => {
				who: *who == sov_penpal_a_on_rococo,
				amount: *amount == t.args.amount,
			},
			// Deposited to receiver parachain SA
			RuntimeEvent::Balances(
				pallet_balances::Event::Deposit { who, .. }
			) => {
				who: *who == sov_penpal_b_on_rococo,
			},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn para_to_para_receiver_assertions(_: ParaToParaTest) {
	type RuntimeEvent = <PenpalB as Chain>::RuntimeEvent;
	assert_expected_events!(
		PenpalB,
		vec![
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { .. }) => {},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn relay_to_para_reserve_transfer_assets(t: RelayToParaTest) -> DispatchResult {
	<Rococo as RococoPallet>::XcmPallet::limited_reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn system_para_to_para_reserve_transfer_assets(t: SystemParaToParaTest) -> DispatchResult {
	<AssetHubRococo as AssetHubRococoPallet>::PolkadotXcm::limited_reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn para_to_system_para_reserve_transfer_assets(t: ParaToSystemParaTest) -> DispatchResult {
	<PenpalA as PenpalAPallet>::PolkadotXcm::limited_reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn para_to_para_limited_reserve_transfer_assets(t: ParaToParaTest) -> DispatchResult {
	<PenpalA as PenpalAPallet>::PolkadotXcm::limited_reserve_transfer_assets(
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
	let signed_origin = <Rococo as Chain>::RuntimeOrigin::signed(RococoSender::get().into());
	let destination = Rococo::child_location_of(AssetHubRococo::para_id());
	let beneficiary: Location =
		AccountId32Junction { network: None, id: AssetHubRococoReceiver::get().into() }.into();
	let amount_to_send: Balance = ROCOCO_ED * 1000;
	let assets: Assets = (Here, amount_to_send).into();
	let fee_asset_item = 0;

	// this should fail
	Rococo::execute_with(|| {
		let result = <Rococo as RococoPallet>::XcmPallet::limited_reserve_transfer_assets(
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
		<AssetHubRococo as Chain>::RuntimeOrigin::signed(AssetHubRococoSender::get().into());
	let destination = AssetHubRococo::parent_location();
	let beneficiary_id = RococoReceiver::get();
	let beneficiary: Location =
		AccountId32Junction { network: None, id: beneficiary_id.into() }.into();
	let amount_to_send: Balance = ASSET_HUB_ROCOCO_ED * 1000;

	let assets: Assets = (Parent, amount_to_send).into();
	let fee_asset_item = 0;

	// this should fail
	AssetHubRococo::execute_with(|| {
		let result =
			<AssetHubRococo as AssetHubRococoPallet>::PolkadotXcm::limited_reserve_transfer_assets(
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
	let destination = Rococo::child_location_of(PenpalA::para_id());
	let beneficiary_id = PenpalAReceiver::get();
	let amount_to_send: Balance = ROCOCO_ED * 1000;

	let test_args = TestContext {
		sender: RococoSender::get(),
		receiver: PenpalAReceiver::get(),
		args: TestArgs::new_relay(destination, beneficiary_id, amount_to_send),
	};

	let mut test = RelayToParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<Rococo>(relay_to_para_sender_assertions);
	test.set_assertion::<PenpalA>(para_receiver_assertions);
	test.set_dispatchable::<Rococo>(relay_to_para_reserve_transfer_assets);
	test.assert();

	let delivery_fees = Rococo::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<RococoXcmConfig as xcm_executor::Config>::XcmSender,
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
	let destination = AssetHubRococo::sibling_location_of(PenpalA::para_id());
	let beneficiary_id = PenpalAReceiver::get();
	let amount_to_send: Balance = ASSET_HUB_ROCOCO_ED * 1000;
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: AssetHubRococoSender::get(),
		receiver: PenpalAReceiver::get(),
		args: TestArgs::new_para(destination, beneficiary_id, amount_to_send, assets, None, 0),
	};

	let mut test = SystemParaToParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<AssetHubRococo>(system_para_to_para_sender_assertions);
	test.set_assertion::<PenpalA>(para_receiver_assertions);
	test.set_dispatchable::<AssetHubRococo>(system_para_to_para_reserve_transfer_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	let delivery_fees = AssetHubRococo::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<AssetHubRococoXcmConfig as xcm_executor::Config>::XcmSender,
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
	let destination = PenpalA::sibling_location_of(AssetHubRococo::para_id());
	let beneficiary_id = AssetHubRococoReceiver::get();
	let amount_to_send: Balance = ASSET_HUB_ROCOCO_ED * 1000;
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: PenpalASender::get(),
		receiver: AssetHubRococoReceiver::get(),
		args: TestArgs::new_para(destination, beneficiary_id, amount_to_send, assets, None, 0),
	};

	let mut test = ParaToSystemParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	let penpal_location_as_seen_by_ahr = AssetHubRococo::sibling_location_of(PenpalA::para_id());
	let sov_penpal_on_ahr = AssetHubRococo::sovereign_account_id_of(penpal_location_as_seen_by_ahr);

	// fund the Penpal's SA on AHR with the native tokens held in reserve
	AssetHubRococo::fund_accounts(vec![(sov_penpal_on_ahr.into(), amount_to_send * 2)]);

	test.set_assertion::<PenpalA>(para_to_system_para_sender_assertions);
	test.set_assertion::<AssetHubRococo>(para_to_system_para_receiver_assertions);
	test.set_dispatchable::<PenpalA>(para_to_system_para_reserve_transfer_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	let delivery_fees = PenpalA::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<PenpalRococoXcmConfig as xcm_executor::Config>::XcmSender,
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
	// Force create asset on AssetHubRococo and PenpalA from Relay Chain
	AssetHubRococo::force_create_and_mint_asset(
		ASSET_ID,
		ASSET_MIN_BALANCE,
		false,
		AssetHubRococoSender::get(),
		Some(Weight::from_parts(1_019_445_000, 200_000)),
		ASSET_MIN_BALANCE * 1_000_000,
	);
	PenpalA::force_create_and_mint_asset(
		ASSET_ID,
		ASSET_MIN_BALANCE,
		false,
		PenpalASender::get(),
		None,
		0,
	);

	// Init values for System Parachain
	let destination = AssetHubRococo::sibling_location_of(PenpalA::para_id());
	let beneficiary_id = PenpalAReceiver::get();
	let fee_amount_to_send = ASSET_HUB_ROCOCO_ED * 1000;
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
		sender: AssetHubRococoSender::get(),
		receiver: PenpalAReceiver::get(),
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

	// Create SA-of-Penpal-on-AHR with ED.
	let penpal_location = AssetHubRococo::sibling_location_of(PenpalA::para_id());
	let sov_penpal_on_ahr = AssetHubRococo::sovereign_account_id_of(penpal_location);
	AssetHubRococo::fund_accounts(vec![(sov_penpal_on_ahr.into(), ROCOCO_ED)]);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	let sender_assets_before = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::Assets;
		<Assets as Inspect<_>>::balance(ASSET_ID, &AssetHubRococoSender::get())
	});
	let receiver_assets_before = PenpalA::execute_with(|| {
		type Assets = <PenpalA as PenpalAPallet>::Assets;
		<Assets as Inspect<_>>::balance(ASSET_ID, &PenpalAReceiver::get())
	});

	test.set_assertion::<AssetHubRococo>(system_para_to_para_assets_sender_assertions);
	test.set_assertion::<PenpalA>(system_para_to_para_assets_receiver_assertions);
	test.set_dispatchable::<AssetHubRococo>(system_para_to_para_reserve_transfer_assets);
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

	let sender_assets_after = AssetHubRococo::execute_with(|| {
		type Assets = <AssetHubRococo as AssetHubRococoPallet>::Assets;
		<Assets as Inspect<_>>::balance(ASSET_ID, &AssetHubRococoSender::get())
	});
	let receiver_assets_after = PenpalA::execute_with(|| {
		type Assets = <PenpalA as PenpalAPallet>::Assets;
		<Assets as Inspect<_>>::balance(ASSET_ID, &PenpalAReceiver::get())
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
	let destination = PenpalA::sibling_location_of(PenpalB::para_id());
	let beneficiary_id = PenpalBReceiver::get();
	let amount_to_send: Balance = ASSET_HUB_ROCOCO_ED * 10000;
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: PenpalASender::get(),
		receiver: PenpalBReceiver::get(),
		args: TestArgs::new_para(destination, beneficiary_id, amount_to_send, assets, None, 0),
	};

	let mut test = ParaToParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	let sender_as_seen_by_relay = Rococo::child_location_of(PenpalA::para_id());
	let sov_of_sender_on_relay = Rococo::sovereign_account_id_of(sender_as_seen_by_relay);

	// fund the PenpalA's SA on Rococo with the native tokens held in reserve
	Rococo::fund_accounts(vec![(sov_of_sender_on_relay.into(), amount_to_send * 2)]);

	test.set_assertion::<PenpalA>(para_to_para_sender_assertions);
	test.set_assertion::<Rococo>(para_to_para_relay_hop_assertions);
	test.set_assertion::<PenpalB>(para_to_para_receiver_assertions);
	test.set_dispatchable::<PenpalA>(para_to_para_limited_reserve_transfer_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	let delivery_fees = PenpalA::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<PenpalRococoXcmConfig as xcm_executor::Config>::XcmSender,
		>(test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest)
	});

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	// Receiver's balance is increased
	assert!(receiver_balance_after > receiver_balance_before);
}
