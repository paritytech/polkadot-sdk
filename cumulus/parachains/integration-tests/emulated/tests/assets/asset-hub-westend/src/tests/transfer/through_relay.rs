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

use crate::{
	foreign_balance_on,
	imports::*,
	tests::transfer::through_ah::{
		para_to_para_through_hop_receiver_assertions, para_to_para_through_hop_sender_assertions,
	},
};

fn para_to_para_relay_hop_assertions(t: ParaToParaThroughRelayTest) {
	type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
	let sov_penpal_a_on_westend =
		Westend::sovereign_account_id_of(Westend::child_location_of(PenpalA::para_id()));
	let sov_penpal_b_on_westend =
		Westend::sovereign_account_id_of(Westend::child_location_of(PenpalB::para_id()));

	assert_expected_events!(
		Westend,
		vec![
			// Withdrawn from sender parachain SA
			RuntimeEvent::Balances(
				pallet_balances::Event::Burned { who, amount }
			) => {
				who: *who == sov_penpal_a_on_westend,
				amount: *amount == t.args.amount,
			},
			// Deposited to receiver parachain SA
			RuntimeEvent::Balances(
				pallet_balances::Event::Minted { who, .. }
			) => {
				who: *who == sov_penpal_b_on_westend,
			},
			RuntimeEvent::MessageQueue(
				pallet_message_queue::Event::Processed { success: true, .. }
			) => {},
		]
	);
}

fn para_to_para_through_relay_limited_reserve_transfer_assets(
	t: ParaToParaThroughRelayTest,
) -> DispatchResult {
	let Junction::Parachain(para_id) = *t.args.dest.chain_location().last().unwrap() else {
		unimplemented!("Destination is not a parachain?")
	};

	Westend::ext_wrapper(|| {
		Dmp::make_parachain_reachable(para_id);
	});
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
// ===== Reserve Transfers - Native Asset - Parachain<>Relay<>Parachain ====
// =========================================================================
/// Reserve Transfers of native asset from Parachain to Parachain (through Relay reserve) should
/// work
#[test]
fn reserve_transfer_native_asset_from_para_to_para_through_relay() {
	// Init values for Parachain Origin
	let destination = PenpalA::sibling_location_of(PenpalB::para_id());
	let sender = PenpalASender::get();
	let amount_to_send: Balance = WESTEND_ED * 10000;
	let asset_owner = PenpalAssetOwner::get();
	let assets = (Parent, amount_to_send).into();
	let relay_native_asset_location = RelayLocation::get();
	let sender_as_seen_by_relay = Westend::child_location_of(PenpalA::para_id());
	let sov_of_sender_on_relay = Westend::sovereign_account_id_of(sender_as_seen_by_relay);

	// fund Parachain's sender account
	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(asset_owner),
		relay_native_asset_location.clone(),
		sender.clone(),
		amount_to_send * 2,
	);

	// fund the Parachain Origin's SA on Relay Chain with the native tokens held in reserve
	Westend::fund_accounts(vec![(sov_of_sender_on_relay.into(), amount_to_send * 2)]);

	// Init values for Parachain Destination
	let receiver = PenpalBReceiver::get();

	// Init Test
	let test_args = TestContext {
		sender: sender.clone(),
		receiver: receiver.clone(),
		args: TestArgs::new_para(destination, receiver.clone(), amount_to_send, assets, None, 0),
	};
	let mut test = ParaToParaThroughRelayTest::new(test_args);

	// Query initial balances
	let sender_assets_before =
		foreign_balance_on!(PenpalA, relay_native_asset_location.clone(), &sender);
	let receiver_assets_before =
		foreign_balance_on!(PenpalB, relay_native_asset_location.clone(), &receiver);

	// Set assertions and dispatchables
	test.set_assertion::<PenpalA>(para_to_para_through_hop_sender_assertions);
	test.set_assertion::<Westend>(para_to_para_relay_hop_assertions);
	test.set_assertion::<PenpalB>(para_to_para_through_hop_receiver_assertions);
	test.set_dispatchable::<PenpalA>(para_to_para_through_relay_limited_reserve_transfer_assets);
	test.assert();

	// Query final balances
	let sender_assets_after =
		foreign_balance_on!(PenpalA, relay_native_asset_location.clone(), &sender);
	let receiver_assets_after =
		foreign_balance_on!(PenpalB, relay_native_asset_location, &receiver);

	// Sender's balance is reduced by amount sent plus delivery fees.
	assert!(sender_assets_after < sender_assets_before - amount_to_send);
	// Receiver's balance is increased by `amount_to_send` minus delivery fees.
	assert!(receiver_assets_after > receiver_assets_before);
	assert!(receiver_assets_after < receiver_assets_before + amount_to_send);
}
