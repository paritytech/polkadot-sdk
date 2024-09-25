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

use crate::imports::*;
use emulated_integration_tests_common::{
	accounts::{ALICE, BOB},
	impls::AccountId32,
};

use frame_support::{assert_ok, sp_runtime::traits::Dispatchable};
use xcm_executor::traits::ConvertLocation;
use crate::imports::bhw_xcm_config::{LocationToAccountId, UniversalLocation};

#[test]
fn test_set_asset_claimer_within_a_chain() {
	let (alice_account, alice_location) = account_and_location(ALICE);
	let (bob_account, bob_location) = account_and_location(BOB);

	let amount_to_send = 16_000_000_000_000;
	let assets: Assets = (Parent, amount_to_send).into();

	let alice_balance_before = <AssetHubWestend as Chain>::account_data_of(alice_account.clone()).free;
	AssetHubWestend::fund_accounts(vec![(alice_account.clone(), amount_to_send * 2)]);
	let alice_balance_after = <AssetHubWestend as Chain>::account_data_of(alice_account.clone()).free;
	assert_eq!(alice_balance_after - alice_balance_before, amount_to_send * 2);

	let test_args = TestContext {
		sender: alice_account.clone(),
		receiver: bob_account.clone(),
		args: TestArgs::new_para(
			bob_location.clone(),
			bob_account.clone(),
			amount_to_send,
			assets.clone(),
			None,
			0,
		),
	};
	let test = SystemParaToParaTest::new(test_args);
	AssetHubWestend::execute_with(|| {
		assert_ok!(trap_assets_with_claimer(test.clone(), bob_location.clone()).dispatch(test.signed_origin));
	});

	let balance_after_trap = <AssetHubWestend as Chain>::account_data_of(alice_account.clone()).free;
	assert_eq!(alice_balance_after - balance_after_trap, amount_to_send);

	let bob_balance_before = <AssetHubWestend as Chain>::account_data_of(bob_account.clone()).free;
	let test_args = TestContext {
		sender: bob_account.clone(),
		receiver: alice_account.clone(),
		args: TestArgs::new_para(
			alice_location.clone(),
			alice_account.clone(),
			amount_to_send,
			assets.clone(),
			None,
			0,
		),
	};
	let test = SystemParaToParaTest::new(test_args);
	AssetHubWestend::execute_with(|| {
		assert_ok!(claim_assets(test.clone(), bob_location.clone()).dispatch(test.signed_origin));
	});

	let bob_balance_after = <AssetHubWestend as Chain>::account_data_of(bob_account.clone()).free;
	assert_eq!(bob_balance_after - bob_balance_before, amount_to_send);
}

fn account_and_location(account: &str) -> (AccountId32, Location) {
	let account_id = AssetHubWestend::account_id_of(account);
	let account_clone = account_id.clone();
	let location: Location =
		[Junction::AccountId32 { network: Some(Westend), id: account_id.into() }].into();
	(account_clone, location)
}

fn trap_assets_with_claimer(
	test: SystemParaToParaTest,
	claimer: Location,
) -> <AssetHubWestend as Chain>::RuntimeCall {
	type RuntimeCall = <AssetHubWestend as Chain>::RuntimeCall;

	let local_xcm = Xcm::<RuntimeCall>::builder_unsafe()
		.set_asset_claimer(claimer.clone())
		.withdraw_asset(test.args.assets.clone())
		.clear_origin()
		.build();

	RuntimeCall::PolkadotXcm(pallet_xcm::Call::execute {
		message: bx!(VersionedXcm::from(local_xcm)),
		max_weight: Weight::from_parts(4_000_000_000_000, 300_000),
	})
}

fn claim_assets(
	test: SystemParaToParaTest,
	claimer: Location,
) -> <AssetHubWestend as Chain>::RuntimeCall {
	type RuntimeCall = <AssetHubWestend as Chain>::RuntimeCall;

	let local_xcm = Xcm::<RuntimeCall>::builder_unsafe()
		.claim_asset(test.args.assets.clone(), Here)
		.deposit_asset(AllCounted(test.args.assets.len() as u32), claimer)
		.build();

	RuntimeCall::PolkadotXcm(pallet_xcm::Call::execute {
		message: bx!(VersionedXcm::from(local_xcm)),
		max_weight: Weight::from_parts(4_000_000_000_000, 300_000),
	})
}

// The test:
// 1. Funds Bob account on BridgeHub, withdraws the funds, sets asset claimer to
// sibling account of Alice on AssetHub and traps the funds.
// 2. Sends an XCM from AssetHub to BridgeHub on behalf of Alice. The XCM: claims assets,
// pays fees and deposits assets to alice's sibling account.
#[test]
fn test_sac_between_the_chains() {
	let alice = AssetHubWestend::account_id_of(ALICE);
	let alice_bh_sibling = Location::new(
		1,
		[
			Parachain(AssetHubWestend::para_id().into()),
			Junction::AccountId32 { network: Some(Westend), id: alice.clone().into() },
		],
	);
	let bob = BridgeHubWestend::account_id_of(BOB);
	let bob_location =
		Location::new(0, Junction::AccountId32 { network: Some(Westend), id: bob.clone().into() });

	let amount_to_send = 16_000_000_000_000u128;
	BridgeHubWestend::fund_accounts(vec![(bob.clone(), amount_to_send * 2)]);

	let assets: Assets = (Parent, amount_to_send).into();
	let test_args = TestContext {
		sender: bob.clone(),
		receiver: alice.clone(),
		args: TestArgs::new_para(
			bob_location.clone(),
			bob.clone(),
			amount_to_send,
			assets.clone(),
			None,
			0,
		),
	};
	let test = BridgeHubToAssetHubTest::new(test_args);
	BridgeHubWestend::execute_with(|| {
		assert_ok!(trap_assets_bh(test.clone(), alice_bh_sibling.clone()).dispatch(test.signed_origin));
	});

	let alice_bh_acc = LocationToAccountId::convert_location(&alice_bh_sibling).unwrap();
	let balance = <BridgeHubWestend as Chain>::account_data_of(alice_bh_acc.clone()).free;
	assert_eq!(balance, 0);

	let destination = AssetHubWestend::sibling_location_of(BridgeHubWestend::para_id());
	let test_args = TestContext {
		sender: alice.clone(),
		receiver: bob.clone(),
		args: TestArgs::new_para(
			destination.clone(),
			bob.clone(),
			amount_to_send,
			assets.clone(),
			None,
			0,
		),
	};
	let alice_bh_sibling = Location::new(
		1,
		[
			Parachain(AssetHubWestend::para_id().into()),
			Junction::AccountId32 { network: Some(Westend), id: alice.clone().into() },
		],
	);
	let test = AssetHubToBridgeHubTest::new(test_args);
	let pay_fees = 6_000_000_000_000u128;
	let xcm_on_bh = Xcm::<()>::builder_unsafe()
		.claim_asset(test.args.assets.clone(), Here)
		.pay_fees((Parent, pay_fees))
		.deposit_asset(All, alice_bh_sibling.clone())
		.build();
	let bh_on_ah = AssetHubWestend::sibling_location_of(BridgeHubWestend::para_id()).into();
	AssetHubWestend::execute_with(|| {
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::send(
			test.signed_origin,
			bx!(bh_on_ah),
			bx!(VersionedXcm::from(xcm_on_bh)),
		));
	});

	let al_bh_acc = LocationToAccountId::convert_location(&alice_bh_sibling).unwrap();
	let balance = <BridgeHubWestend as Chain>::account_data_of(al_bh_acc).free;
	assert_eq!(balance, amount_to_send - pay_fees);
}

fn trap_assets_bh(
	test: BridgeHubToAssetHubTest,
	claimer: Location,
) -> <BridgeHubWestend as Chain>::RuntimeCall {
	type RuntimeCall = <BridgeHubWestend as Chain>::RuntimeCall;

	let local_xcm = Xcm::<RuntimeCall>::builder_unsafe()
		.set_asset_claimer(claimer.clone())
		.withdraw_asset(test.args.assets.clone())
		.clear_origin()
		.build();

	RuntimeCall::PolkadotXcm(pallet_xcm::Call::execute {
		message: bx!(VersionedXcm::from(local_xcm)),
		max_weight: Weight::from_parts(4_000_000_000_000, 700_000),
	})
}
