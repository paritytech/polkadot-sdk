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

use crate::{foreign_balance_on, imports::*};

fn relay_to_para_sender_assertions(t: RelayToParaTest) {
	type RuntimeEvent = <Westend as Chain>::RuntimeEvent;

	Westend::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(350_000_000, 7000)));

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

fn para_to_relay_sender_assertions(t: ParaToRelayTest) {
	type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
	PenpalA::assert_xcm_pallet_attempted_complete(Some(Weight::from_parts(2_000_000_000, 140_000)));
	assert_expected_events!(
		PenpalA,
		vec![
			// Amount to reserve transfer is transferred to Parachain's Sovereign account
			RuntimeEvent::ForeignAssets(
				pallet_assets::Event::Burned { asset_id, owner, balance, .. }
			) => {
				asset_id: *asset_id == RelayLocation::get(),
				owner: *owner == t.sender.account_id,
				balance: *balance == t.args.amount,
			},
		]
	);
}

fn para_to_relay_receiver_assertions(t: ParaToRelayTest) {
	type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
	let sov_penpal_on_relay =
		Westend::sovereign_account_id_of(Westend::child_location_of(PenpalA::para_id()));

	Westend::assert_ump_queue_processed(
		true,
		Some(PenpalA::para_id()),
		Some(Weight::from_parts(306305000, 7_186)),
	);

	assert_expected_events!(
		Westend,
		vec![
			// Amount to reserve transfer is withdrawn from Parachain's Sovereign account
			RuntimeEvent::Balances(
				pallet_balances::Event::Burned { who, amount }
			) => {
				who: *who == sov_penpal_on_relay.clone().into(),
				amount: *amount == t.args.amount,
			},
			RuntimeEvent::Balances(pallet_balances::Event::Minted { .. }) => {},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn relay_to_para_assets_receiver_assertions(t: RelayToParaTest) {
	type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;

	assert_expected_events!(
		PenpalA,
		vec![
			RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
				asset_id: *asset_id == RelayLocation::get(),
				owner: *owner == t.receiver.account_id,
			},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn relay_to_para_reserve_transfer_assets(t: RelayToParaTest) -> DispatchResult {
	let Junction::Parachain(para_id) = *t.args.dest.chain_location().last().unwrap() else {
		unimplemented!("Destination is not a parachain?")
	};

	Dmp::make_parachain_reachable(para_id);
	<Westend as WestendPallet>::XcmPallet::limited_reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

fn para_to_relay_reserve_transfer_assets(t: ParaToRelayTest) -> DispatchResult {
	<PenpalA as PenpalAPallet>::PolkadotXcm::limited_reserve_transfer_assets(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.beneficiary.into()),
		bx!(t.args.assets.into()),
		t.args.fee_asset_item,
		t.args.weight_limit,
	)
}

// =========================================================================
// ========= Reserve Transfers - Native Asset - Relay<>Parachain ===========
// =========================================================================
/// Reserve Transfers of native asset from Relay to Parachain should work
#[test]
fn reserve_transfer_native_asset_from_relay_to_para() {
	// Init values for Relay
	let destination = Westend::child_location_of(PenpalA::para_id());
	let sender = WestendSender::get();
	let amount_to_send: Balance = WESTEND_ED * 1000;

	// Init values for Parachain
	let relay_native_asset_location = RelayLocation::get();
	let receiver = PenpalAReceiver::get();

	// Init Test
	let test_args = TestContext {
		sender,
		receiver: receiver.clone(),
		args: TestArgs::new_relay(destination.clone(), receiver.clone(), amount_to_send),
	};
	let mut test = RelayToParaTest::new(test_args);

	// Query initial balances
	let sender_balance_before = test.sender.balance;
	let receiver_assets_before =
		foreign_balance_on!(PenpalA, relay_native_asset_location.clone(), &receiver);

	// Set assertions and dispatchables
	test.set_assertion::<Westend>(relay_to_para_sender_assertions);
	test.set_assertion::<PenpalA>(relay_to_para_assets_receiver_assertions);
	test.set_dispatchable::<Westend>(relay_to_para_reserve_transfer_assets);
	test.assert();

	// Query final balances
	let sender_balance_after = test.sender.balance;
	let receiver_assets_after =
		foreign_balance_on!(PenpalA, relay_native_asset_location, &receiver);

	// Sender's balance is reduced by amount sent plus delivery fees
	assert!(sender_balance_after < sender_balance_before - amount_to_send);
	// Receiver's asset balance is increased
	assert!(receiver_assets_after > receiver_assets_before);
	// Receiver's asset balance increased by `amount_to_send - delivery_fees - bought_execution`;
	// `delivery_fees` might be paid from transfer or JIT, also `bought_execution` is unknown but
	// should be non-zero
	assert!(receiver_assets_after < receiver_assets_before + amount_to_send);
}

/// Reserve Transfers of native asset from Parachain to Relay should work
#[test]
fn reserve_transfer_native_asset_from_para_to_relay() {
	// Init values for Parachain
	let destination = PenpalA::parent_location();
	let sender = PenpalASender::get();
	let amount_to_send: Balance = WESTEND_ED * 1000;
	let assets: Assets = (Parent, amount_to_send).into();
	let asset_owner = PenpalAssetOwner::get();
	let relay_native_asset_location = RelayLocation::get();

	// fund Parachain's sender account
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(asset_owner),
		relay_native_asset_location.clone(),
		sender.clone(),
		amount_to_send * 2,
	);

	// Init values for Relay
	let receiver = WestendReceiver::get();
	let penpal_location_as_seen_by_relay = Westend::child_location_of(PenpalA::para_id());
	let sov_penpal_on_relay = Westend::sovereign_account_id_of(penpal_location_as_seen_by_relay);

	// fund Parachain's SA on Relay with the native tokens held in reserve
	Westend::fund_accounts(vec![(sov_penpal_on_relay.into(), amount_to_send * 2)]);

	// Init Test
	let test_args = TestContext {
		sender: sender.clone(),
		receiver: receiver.clone(),
		args: TestArgs::new_para(
			destination.clone(),
			receiver,
			amount_to_send,
			assets.clone(),
			None,
			0,
		),
	};
	let mut test = ParaToRelayTest::new(test_args);

	// Query initial balances
	let sender_assets_before =
		foreign_balance_on!(PenpalA, relay_native_asset_location.clone(), &sender);
	let receiver_balance_before = test.receiver.balance;

	// Set assertions and dispatchables
	test.set_assertion::<PenpalA>(para_to_relay_sender_assertions);
	test.set_assertion::<Westend>(para_to_relay_receiver_assertions);
	test.set_dispatchable::<PenpalA>(para_to_relay_reserve_transfer_assets);
	test.assert();

	// Query final balances
	let sender_assets_after = foreign_balance_on!(PenpalA, relay_native_asset_location, &sender);
	let receiver_balance_after = test.receiver.balance;

	// Sender's balance is reduced by amount sent plus delivery fees
	assert!(sender_assets_after < sender_assets_before - amount_to_send);
	// Receiver's asset balance is increased
	assert!(receiver_balance_after > receiver_balance_before);
	// Receiver's asset balance increased by `amount_to_send - delivery_fees - bought_execution`;
	// `delivery_fees` might be paid from transfer or JIT, also `bought_execution` is unknown but
	// should be non-zero
	assert!(receiver_balance_after < receiver_balance_before + amount_to_send);
}
