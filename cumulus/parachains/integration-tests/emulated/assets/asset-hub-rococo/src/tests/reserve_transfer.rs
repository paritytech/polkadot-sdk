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
use penpal_runtime::xcm_config::XcmConfig as PenpalRococoXcmConfig;
use rococo_runtime::xcm_config::XcmConfig as RococoXcmConfig;

fn relay_origin_assertions(t: RelayToSystemParaTest) {
	type RuntimeEvent = <Rococo as Chain>::RuntimeEvent;

	Rococo::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(630_092_000, 6_196)));

	assert_expected_events!(
		Rococo,
		vec![
			// Amount to reserve transfer is transferred to System Parachain's Sovereign account
			RuntimeEvent::Balances(pallet_balances::Event::Transfer { from, to, amount }) => {
				from: *from == t.sender.account_id,
				to: *to == Rococo::sovereign_account_id_of(
					t.args.dest
				),
				amount:  *amount == t.args.amount,
			},
		]
	);
}

fn system_para_dest_assertions_incomplete(_t: RelayToSystemParaTest) {
	AssetHubRococo::assert_dmp_queue_incomplete(
		Some(Weight::from_parts(1_000_000_000, 0)),
		Some(Error::UntrustedReserveLocation),
	);
}

fn system_para_to_relay_assertions(_t: SystemParaToRelayTest) {
	AssetHubRococo::assert_xcm_pallet_attempted_error(Some(XcmError::Barrier))
}

fn system_para_to_para_assertions(t: SystemParaToParaTest) {
	type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;

	AssetHubRococo::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(
		630_092_000,
		6_196,
	)));

	assert_expected_events!(
		AssetHubRococo,
		vec![
			// Amount to reserve transfer is transferred to Parachain's Sovereing account
			RuntimeEvent::Balances(
				pallet_balances::Event::Transfer { from, to, amount }
			) => {
				from: *from == t.sender.account_id,
				to: *to == AssetHubRococo::sovereign_account_id_of(
					t.args.dest
				),
				amount: *amount == t.args.amount,
			},
		]
	);
}

fn system_para_to_para_assets_assertions(t: SystemParaToParaTest) {
	type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;

	AssetHubRococo::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(
		676_119_000,
		6196,
	)));

	assert_expected_events!(
		AssetHubRococo,
		vec![
			// Amount to reserve transfer is transferred to Parachain's Sovereing account
			RuntimeEvent::Assets(
				pallet_assets::Event::Transferred { asset_id, from, to, amount }
			) => {
				asset_id: *asset_id == ASSET_ID,
				from: *from == t.sender.account_id,
				to: *to == AssetHubRococo::sovereign_account_id_of(
					t.args.dest
				),
				amount: *amount == t.args.amount,
			},
		]
	);
}

fn para_to_para_sender_assertions(t: ParaToParaTest) {
	type RuntimeEvent = <PenpalRococoA as Chain>::RuntimeEvent;
	PenpalRococoA::assert_xcm_pallet_attempted_complete(None);
	assert_expected_events!(
		PenpalRococoA,
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
		Rococo::sovereign_account_id_of(Rococo::child_location_of(PenpalRococoA::para_id()));
	let sov_penpal_b_on_rococo =
		Rococo::sovereign_account_id_of(Rococo::child_location_of(PenpalRococoB::para_id()));
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
	type RuntimeEvent = <PenpalRococoB as Chain>::RuntimeEvent;
	assert_expected_events!(
		PenpalRococoB,
		vec![
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { .. }) => {},
			RuntimeEvent::DmpQueue(
				cumulus_pallet_dmp_queue::Event::ExecutedDownward {
					outcome: Outcome::Complete(..),
					..
				}
			) => {},
		]
	);
}

fn relay_limited_reserve_transfer_assets(t: RelayToSystemParaTest) -> DispatchResult {
	<Rococo as RococoPallet>::XcmPallet::limited_reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn relay_reserve_transfer_assets(t: RelayToSystemParaTest) -> DispatchResult {
	<Rococo as RococoPallet>::XcmPallet::reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
	)
}

fn system_para_limited_reserve_transfer_assets(t: SystemParaToRelayTest) -> DispatchResult {
	<AssetHubRococo as AssetHubRococoPallet>::PolkadotXcm::limited_reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn system_para_reserve_transfer_assets(t: SystemParaToRelayTest) -> DispatchResult {
	<AssetHubRococo as AssetHubRococoPallet>::PolkadotXcm::reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
	)
}

fn system_para_to_para_limited_reserve_transfer_assets(t: SystemParaToParaTest) -> DispatchResult {
	<AssetHubRococo as AssetHubRococoPallet>::PolkadotXcm::limited_reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn system_para_to_para_reserve_transfer_assets(t: SystemParaToParaTest) -> DispatchResult {
	<AssetHubRococo as AssetHubRococoPallet>::PolkadotXcm::reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
	)
}

// function assumes fees and assets have the same remote reserve
fn remote_reserve_transfer_program(
	reserve: MultiLocation,
	dest: MultiLocation,
	beneficiary: MultiLocation,
	assets: Vec<MultiAsset>,
	fees: MultiAsset,
	weight_limit: WeightLimit,
) -> Xcm<penpal_runtime::RuntimeCall> {
	use xcm_emulator::{Get, Parachain};
	let max_assets = assets.len() as u32;
	let context = X1(Parachain(<PenpalRococoA as Parachain>::ParachainInfo::get().into()));
	// we spend up to half of fees for execution on reserve and other half for execution on
	// destination
	let (fees_half_1, fees_half_2) = match fees.fun {
		Fungible(amount) => {
			let fee1 = amount.saturating_div(2);
			let fee2 = amount.saturating_sub(fee1);
			assert!(fee1 > 0);
			assert!(fee2 > 0);
			(MultiAsset::from((fees.id, fee1)), MultiAsset::from((fees.id, fee2)))
		},
		NonFungible(_) => unreachable!(),
	};
	// identifies fee item as seen by `reserve` - to be used at reserve chain
	let reserve_fees = fees_half_1.reanchored(&reserve, context).unwrap();
	// identifies fee item as seen by `dest` - to be used at destination chain
	let dest_fees = fees_half_2.reanchored(&dest, context).unwrap();
	// identifies `dest` as seen by `reserve`
	let dest = dest.reanchored(&reserve, context).unwrap();
	// xcm to be executed at dest
	let xcm_on_dest = Xcm(vec![
		BuyExecution { fees: dest_fees, weight_limit: weight_limit.clone() },
		DepositAsset { assets: Wild(AllCounted(max_assets)), beneficiary },
	]);
	// xcm to be executed on reserve
	let xcm_on_reserve = Xcm(vec![
		BuyExecution { fees: reserve_fees, weight_limit },
		DepositReserveAsset { assets: Wild(AllCounted(max_assets)), dest, xcm: xcm_on_dest },
	]);
	Xcm(vec![
		WithdrawAsset(assets.into()),
		InitiateReserveWithdraw {
			assets: Wild(AllCounted(max_assets)),
			reserve,
			xcm: xcm_on_reserve,
		},
	])
}

fn para_to_para_remote_reserve_transfer_native_assets(t: ParaToParaTest) -> DispatchResult {
	let xcm = remote_reserve_transfer_program(
		Parent.into(),
		t.args.dest,
		t.args.beneficiary,
		t.args.assets.clone().into_inner(),
		t.args.assets.into_inner().pop().unwrap(),
		t.args.weight_limit,
	);
	<PenpalRococoA as PenpalRococoAPallet>::PolkadotXcm::execute(
		t.signed_origin,
		bx!(xcm::VersionedXcm::V3(xcm)),
		Weight::MAX,
	)
	.unwrap();
	Ok(())
}

/// Limited Reserve Transfers of native asset from Relay Chain to the System Parachain shouldn't
/// work
#[test]
fn limited_reserve_transfer_native_asset_from_relay_to_system_para_fails() {
	// Init values for Relay Chain
	let amount_to_send: Balance = ROCOCO_ED * 1000;
	let test_args = TestContext {
		sender: RococoSender::get(),
		receiver: AssetHubRococoReceiver::get(),
		args: relay_test_args(amount_to_send),
	};

	let mut test = RelayToSystemParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<Rococo>(relay_origin_assertions);
	test.set_assertion::<AssetHubRococo>(system_para_dest_assertions_incomplete);
	test.set_dispatchable::<Rococo>(relay_limited_reserve_transfer_assets);
	test.assert();

	let delivery_fees = Rococo::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<RococoXcmConfig as xcm_executor::Config>::XcmSender,
		>(test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest)
	});

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	assert_eq!(receiver_balance_before, receiver_balance_after);
}

/// Limited Reserve Transfers of native asset from System Parachain to Relay Chain shoudln't work
#[test]
fn limited_reserve_transfer_native_asset_from_system_para_to_relay_fails() {
	// Init values for System Parachain
	let destination = AssetHubRococo::parent_location();
	let beneficiary_id = RococoReceiver::get();
	let amount_to_send: Balance = ASSET_HUB_ROCOCO_ED * 1000;
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: AssetHubRococoSender::get(),
		receiver: RococoReceiver::get(),
		args: system_para_test_args(destination, beneficiary_id, amount_to_send, assets, None),
	};

	let mut test = SystemParaToRelayTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<AssetHubRococo>(system_para_to_relay_assertions);
	test.set_dispatchable::<AssetHubRococo>(system_para_limited_reserve_transfer_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	assert_eq!(sender_balance_before, sender_balance_after);
	assert_eq!(receiver_balance_before, receiver_balance_after);
}

/// Reserve Transfers of native asset from Relay Chain to the System Parachain shouldn't work
#[test]
fn reserve_transfer_native_asset_from_relay_to_system_para_fails() {
	// Init values for Relay Chain
	let amount_to_send: Balance = ROCOCO_ED * 1000;
	let test_args = TestContext {
		sender: RococoSender::get(),
		receiver: AssetHubRococoReceiver::get(),
		args: relay_test_args(amount_to_send),
	};

	let mut test = RelayToSystemParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<Rococo>(relay_origin_assertions);
	test.set_assertion::<AssetHubRococo>(system_para_dest_assertions_incomplete);
	test.set_dispatchable::<Rococo>(relay_reserve_transfer_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	let delivery_fees = Rococo::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<RococoXcmConfig as xcm_executor::Config>::XcmSender,
		>(test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest)
	});

	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	assert_eq!(receiver_balance_before, receiver_balance_after);
}

/// Reserve Transfers of native asset from System Parachain to Relay Chain shouldn't work
#[test]
fn reserve_transfer_native_asset_from_system_para_to_relay_fails() {
	// Init values for System Parachain
	let destination = AssetHubRococo::parent_location();
	let beneficiary_id = RococoReceiver::get();
	let amount_to_send: Balance = ASSET_HUB_ROCOCO_ED * 1000;
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: AssetHubRococoSender::get(),
		receiver: RococoReceiver::get(),
		args: system_para_test_args(destination, beneficiary_id, amount_to_send, assets, None),
	};

	let mut test = SystemParaToRelayTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	test.set_assertion::<AssetHubRococo>(system_para_to_relay_assertions);
	test.set_dispatchable::<AssetHubRococo>(system_para_reserve_transfer_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	assert_eq!(sender_balance_before, sender_balance_after);
	assert_eq!(receiver_balance_before, receiver_balance_after);
}

/// Limited Reserve Transfers of native asset from System Parachain to Parachain should work
#[test]
fn limited_reserve_transfer_native_asset_from_system_para_to_para() {
	// Init values for System Parachain
	let destination = AssetHubRococo::sibling_location_of(PenpalRococoA::para_id());
	let beneficiary_id = PenpalRococoAReceiver::get();
	let amount_to_send: Balance = ASSET_HUB_ROCOCO_ED * 1000;
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: AssetHubRococoSender::get(),
		receiver: PenpalRococoAReceiver::get(),
		args: system_para_test_args(destination, beneficiary_id, amount_to_send, assets, None),
	};

	let mut test = SystemParaToParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;

	test.set_assertion::<AssetHubRococo>(system_para_to_para_assertions);
	// TODO: Add assertion for Penpal runtime. Right now message is failing with
	// `UntrustedReserveLocation`
	test.set_dispatchable::<AssetHubRococo>(system_para_to_para_limited_reserve_transfer_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;

	let delivery_fees = AssetHubRococo::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<AssetHubRococoXcmConfig as xcm_executor::Config>::XcmSender,
		>(test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest)
	});

	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	// TODO: Check receiver balance when Penpal runtime is improved to propery handle reserve
	// transfers
}

/// Reserve Transfers of native asset from System Parachain to Parachain should work
#[test]
fn reserve_transfer_native_asset_from_system_para_to_para() {
	// Init values for System Parachain
	let destination = AssetHubRococo::sibling_location_of(PenpalRococoA::para_id());
	let beneficiary_id = PenpalRococoAReceiver::get();
	let amount_to_send: Balance = ASSET_HUB_ROCOCO_ED * 1000;
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: AssetHubRococoSender::get(),
		receiver: PenpalRococoAReceiver::get(),
		args: system_para_test_args(destination, beneficiary_id, amount_to_send, assets, None),
	};

	let mut test = SystemParaToParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;

	test.set_assertion::<AssetHubRococo>(system_para_to_para_assertions);
	// TODO: Add assertion for Penpal runtime. Right now message is failing with
	// `UntrustedReserveLocation`
	test.set_dispatchable::<AssetHubRococo>(system_para_to_para_reserve_transfer_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;

	let delivery_fees = AssetHubRococo::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<AssetHubRococoXcmConfig as xcm_executor::Config>::XcmSender,
		>(test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest)
	});

	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	// TODO: Check receiver balance when Penpal runtime is improved to propery handle reserve
	// transfers
}

/// Limited Reserve Transfers of a local asset from System Parachain to Parachain should work
#[test]
fn limited_reserve_transfer_asset_from_system_para_to_para() {
	// Force create asset from Relay Chain and mint assets for System Parachain's sender account
	AssetHubRococo::force_create_and_mint_asset(
		ASSET_ID,
		ASSET_MIN_BALANCE,
		true,
		AssetHubRococoSender::get(),
		Some(Weight::from_parts(1_019_445_000, 200_000)),
		ASSET_MIN_BALANCE * 1000000,
	);

	// Init values for System Parachain
	let destination = AssetHubRococo::sibling_location_of(PenpalRococoA::para_id());
	let beneficiary_id = PenpalRococoAReceiver::get();
	let amount_to_send = ASSET_MIN_BALANCE * 1000;
	let assets =
		(X2(PalletInstance(ASSETS_PALLET_ID), GeneralIndex(ASSET_ID.into())), amount_to_send)
			.into();

	let system_para_test_args = TestContext {
		sender: AssetHubRococoSender::get(),
		receiver: PenpalRococoAReceiver::get(),
		args: system_para_test_args(destination, beneficiary_id, amount_to_send, assets, None),
	};

	let mut system_para_test = SystemParaToParaTest::new(system_para_test_args);

	system_para_test.set_assertion::<AssetHubRococo>(system_para_to_para_assets_assertions);
	// TODO: Add assertions when Penpal is able to manage assets
	system_para_test
		.set_dispatchable::<AssetHubRococo>(system_para_to_para_limited_reserve_transfer_assets);
	system_para_test.assert();
}

/// Reserve Transfers of a local asset from System Parachain to Parachain should work
#[test]
fn reserve_transfer_asset_from_system_para_to_para() {
	// Force create asset from Relay Chain and mint assets for System Parachain's sender account
	AssetHubRococo::force_create_and_mint_asset(
		ASSET_ID,
		ASSET_MIN_BALANCE,
		true,
		AssetHubRococoSender::get(),
		Some(Weight::from_parts(1_019_445_000, 200_000)),
		ASSET_MIN_BALANCE * 1000000,
	);

	// Init values for System Parachain
	let destination = AssetHubRococo::sibling_location_of(PenpalRococoA::para_id());
	let beneficiary_id = PenpalRococoAReceiver::get();
	let amount_to_send = ASSET_MIN_BALANCE * 1000;
	let assets =
		(X2(PalletInstance(ASSETS_PALLET_ID), GeneralIndex(ASSET_ID.into())), amount_to_send)
			.into();

	let system_para_test_args = TestContext {
		sender: AssetHubRococoSender::get(),
		receiver: PenpalRococoAReceiver::get(),
		args: system_para_test_args(destination, beneficiary_id, amount_to_send, assets, None),
	};

	let mut system_para_test = SystemParaToParaTest::new(system_para_test_args);

	system_para_test.set_assertion::<AssetHubRococo>(system_para_to_para_assets_assertions);
	// TODO: Add assertions when Penpal is able to manage assets
	system_para_test
		.set_dispatchable::<AssetHubRococo>(system_para_to_para_reserve_transfer_assets);
	system_para_test.assert();
}

/// Reserve Transfers of native asset from Parachain to Parachain (through Relay reserve) should
/// work
#[test]
fn reserve_transfer_native_asset_from_para_to_para() {
	use integration_tests_common::PenpalRococoBReceiver;
	// Init values for Penpal Parachain
	let destination = PenpalRococoA::sibling_location_of(PenpalRococoB::para_id());
	let beneficiary_id = PenpalRococoBReceiver::get();
	let amount_to_send: Balance = ASSET_HUB_ROCOCO_ED * 10000;
	let assets = (Parent, amount_to_send).into();

	let test_args = TestContext {
		sender: PenpalRococoASender::get(),
		receiver: PenpalRococoBReceiver::get(),
		args: para_test_args(destination, beneficiary_id, amount_to_send, assets, None, 0),
	};

	let mut test = ParaToParaTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;

	let sender_as_seen_by_relay = Rococo::child_location_of(PenpalRococoA::para_id());
	let sov_of_sender_on_relay = Rococo::sovereign_account_id_of(sender_as_seen_by_relay);

	// fund the PenpalA's SA on Rococo with the native tokens held in reserve
	Rococo::fund_accounts(vec![(sov_of_sender_on_relay.into(), amount_to_send * 2)]);

	test.set_assertion::<PenpalRococoA>(para_to_para_sender_assertions);
	test.set_assertion::<Rococo>(para_to_para_relay_hop_assertions);
	test.set_assertion::<PenpalRococoB>(para_to_para_receiver_assertions);
	test.set_dispatchable::<PenpalRococoA>(para_to_para_remote_reserve_transfer_native_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	let delivery_fees = PenpalRococoA::execute_with(|| {
		xcm_helpers::transfer_assets_delivery_fees::<
			<PenpalRococoXcmConfig as xcm_executor::Config>::XcmSender,
		>(test.args.assets.clone(), 0, test.args.weight_limit, test.args.beneficiary, test.args.dest)
	});

	// Sender's balance is reduced
	assert_eq!(sender_balance_before - amount_to_send - delivery_fees, sender_balance_after);
	// Receiver's balance is increased
	assert!(receiver_balance_after > receiver_balance_before);
}
