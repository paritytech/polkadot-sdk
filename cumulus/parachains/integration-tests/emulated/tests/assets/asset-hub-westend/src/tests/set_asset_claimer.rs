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

//! Tests related to claiming assets trapped during XCM execution.

use crate::imports::{bhw_xcm_config::LocationToAccountId, *};
use emulated_integration_tests_common::{
	accounts::{ALICE, BOB},
	impls::AccountId32,
};
use frame_support::{assert_ok, sp_runtime::traits::Dispatchable};
use westend_system_emulated_network::{
	asset_hub_westend_emulated_chain::asset_hub_westend_runtime::RuntimeOrigin as AssetHubRuntimeOrigin,
	bridge_hub_westend_emulated_chain::bridge_hub_westend_runtime::RuntimeOrigin as BridgeHubRuntimeOrigin,
};
use xcm_executor::traits::ConvertLocation;

#[test]
fn test_set_asset_claimer_within_a_chain() {
	let (alice_account, _) = account_and_location(ALICE);
	let (bob_account, bob_location) = account_and_location(BOB);

	let trap_amount = 16_000_000_000_000;
	let assets: Assets = (Parent, trap_amount).into();

	let alice_balance_before =
		<AssetHubWestend as Chain>::account_data_of(alice_account.clone()).free;
	AssetHubWestend::fund_accounts(vec![(alice_account.clone(), trap_amount * 2)]);
	let alice_balance_after =
		<AssetHubWestend as Chain>::account_data_of(alice_account.clone()).free;
	assert_eq!(alice_balance_after - alice_balance_before, trap_amount * 2);

	type RuntimeCall = <AssetHubWestend as Chain>::RuntimeCall;
	let asset_trap_xcm = Xcm::<RuntimeCall>::builder_unsafe()
		.set_asset_claimer(bob_location.clone())
		.withdraw_asset(assets.clone())
		.clear_origin()
		.build();

	AssetHubWestend::execute_with(|| {
		assert_ok!(RuntimeCall::PolkadotXcm(pallet_xcm::Call::execute {
			message: bx!(VersionedXcm::from(asset_trap_xcm)),
			max_weight: Weight::from_parts(4_000_000_000_000, 300_000),
		})
		.dispatch(AssetHubRuntimeOrigin::signed(alice_account.clone())));
	});

	let balance_after_trap =
		<AssetHubWestend as Chain>::account_data_of(alice_account.clone()).free;
	assert_eq!(alice_balance_after - balance_after_trap, trap_amount);

	let bob_balance_before = <AssetHubWestend as Chain>::account_data_of(bob_account.clone()).free;
	let claim_xcm = Xcm::<RuntimeCall>::builder_unsafe()
		.claim_asset(assets.clone(), Here)
		.deposit_asset(AllCounted(assets.len() as u32), bob_location.clone())
		.build();

	AssetHubWestend::execute_with(|| {
		assert_ok!(RuntimeCall::PolkadotXcm(pallet_xcm::Call::execute {
			message: bx!(VersionedXcm::from(claim_xcm)),
			max_weight: Weight::from_parts(4_000_000_000_000, 300_000),
		})
		.dispatch(AssetHubRuntimeOrigin::signed(bob_account.clone())));
	});

	let bob_balance_after = <AssetHubWestend as Chain>::account_data_of(bob_account.clone()).free;
	assert_eq!(bob_balance_after - bob_balance_before, trap_amount);
}

fn account_and_location(account: &str) -> (AccountId32, Location) {
	let account_id = AssetHubWestend::account_id_of(account);
	let account_clone = account_id.clone();
	let location: Location =
		[Junction::AccountId32 { network: Some(Westend), id: account_id.into() }].into();
	(account_clone, location)
}

// The test:
// 1. Funds Bob account on BridgeHub, withdraws the funds, sets asset claimer to
// sibling-account-of(AssetHub/Alice) and traps the funds.
// 2. Alice on AssetHub sends an XCM to BridgeHub to claim assets, pay fees and deposit
// remaining to her sibling account on BridgeHub.
#[test]
fn test_set_asset_claimer_between_the_chains() {
	let alice = AssetHubWestend::account_id_of(ALICE);
	let alice_bh_sibling = Location::new(
		1,
		[
			Parachain(AssetHubWestend::para_id().into()),
			Junction::AccountId32 { network: Some(Westend), id: alice.clone().into() },
		],
	);

	let bob = BridgeHubWestend::account_id_of(BOB);
	let trap_amount = 16_000_000_000_000u128;
	BridgeHubWestend::fund_accounts(vec![(bob.clone(), trap_amount * 2)]);

	let assets: Assets = (Parent, trap_amount).into();
	type RuntimeCall = <BridgeHubWestend as Chain>::RuntimeCall;
	let trap_xcm = Xcm::<RuntimeCall>::builder_unsafe()
		.set_asset_claimer(alice_bh_sibling.clone())
		.withdraw_asset(assets.clone())
		.clear_origin()
		.build();

	BridgeHubWestend::execute_with(|| {
		assert_ok!(RuntimeCall::PolkadotXcm(pallet_xcm::Call::execute {
			message: bx!(VersionedXcm::from(trap_xcm)),
			max_weight: Weight::from_parts(4_000_000_000_000, 700_000),
		})
		.dispatch(BridgeHubRuntimeOrigin::signed(bob.clone())));
	});

	let alice_bh_acc = LocationToAccountId::convert_location(&alice_bh_sibling).unwrap();
	let balance = <BridgeHubWestend as Chain>::account_data_of(alice_bh_acc.clone()).free;
	assert_eq!(balance, 0);

	let pay_fees = 6_000_000_000_000u128;
	let xcm_on_bh = Xcm::<()>::builder_unsafe()
		.claim_asset(assets.clone(), Here)
		.pay_fees((Parent, pay_fees))
		.deposit_asset(All, alice_bh_sibling.clone())
		.build();
	let bh_on_ah = AssetHubWestend::sibling_location_of(BridgeHubWestend::para_id()).into();
	AssetHubWestend::execute_with(|| {
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::send(
			AssetHubRuntimeOrigin::signed(alice.clone()),
			bx!(bh_on_ah),
			bx!(VersionedXcm::from(xcm_on_bh)),
		));
	});

	let alice_bh_acc = LocationToAccountId::convert_location(&alice_bh_sibling).unwrap();
	let balance = <BridgeHubWestend as Chain>::account_data_of(alice_bh_acc).free;
	assert_eq!(balance, trap_amount - pay_fees);
}
