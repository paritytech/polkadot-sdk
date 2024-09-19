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

use emulated_integration_tests_common::accounts::{ALICE, BOB, CHARLIE};
use emulated_integration_tests_common::impls::AccountId32;
use emulated_integration_tests_common::xcm_emulator::log;
use crate::{
    imports::*,
};

use frame_support::{sp_runtime::{traits::Dispatchable}, assert_ok, LOG_TARGET};
use westend_system_emulated_network::penpal_emulated_chain::penpal_runtime;
use westend_system_emulated_network::asset_hub_westend_emulated_chain::asset_hub_westend_runtime;
use westend_system_emulated_network::westend_emulated_chain::westend_runtime::xcm_config::AssetHub;
use xcm_executor::traits::ConvertLocation;
use crate::imports::ahw_xcm_config::{LocationToAccountId, UniversalLocation};

#[test]
fn test_set_asset_claimer_within_a_chain() {
    let (alice_account, alice_location) = account_and_location(ALICE);
    let (bob_account, bob_location) = account_and_location(BOB);

    PenpalA::execute_with(|| {
        type System = <PenpalA as PenpalAPallet>::System;
        type RuntimeOrigin = <PenpalA as Chain>::RuntimeOrigin;
        assert_ok!(System::set_storage(
            RuntimeOrigin::root(),
            vec![(penpal_runtime::xcm_config::RelayNetworkId::key().to_vec(), NetworkId::Westend.encode())]
        ));
    });

    let amount_to_send = 16_000_000_000_000;
    let native_asset_location = RelayLocation::get();
    let assets: Assets = (Parent, amount_to_send).into();

    fund_account(&alice_account, amount_to_send * 2);
    assert_eq!(query_balance(&alice_account, &native_asset_location), amount_to_send * 2);

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
    let test = ParaToParaThroughAHTest::new(test_args);
    execute_test(test.clone(), bob_location.clone(), transfer_assets);

    let alice_assets_after = query_balance(&alice_account, &native_asset_location);
    assert_eq!(alice_assets_after, amount_to_send);

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
    let test = ParaToParaThroughAHTest::new(test_args);
    execute_test(test.clone(), bob_location.clone(), claim_assets);

    let bob_assets_after = query_balance(&bob_account, &native_asset_location);
    assert_eq!(bob_assets_after, amount_to_send);
}

fn account_and_location(account: &str) -> (AccountId32, Location) {
    let account_id = PenpalA::account_id_of(account);
    let account_clone = account_id.clone();
    let location: Location = [Junction::AccountId32 { network: Some(Westend), id: account_id.into() }].into();
    (account_clone, location)
}

fn fund_account(account: &AccountId, amount: u128) {
    let asset_owner = PenpalAssetOwner::get();
    PenpalA::mint_foreign_asset(
        <PenpalA as Chain>::RuntimeOrigin::signed(asset_owner),
        Location::parent(),
        account.clone(),
        amount,
    );
}

fn query_balance(account: &AccountId, asset_location: &Location) -> u128 {
    PenpalA::execute_with(|| {
        type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
        <ForeignAssets as Inspect<_>>::balance(asset_location.clone(), account)
    })
}

fn execute_test(
    test: ParaToParaThroughAHTest,
    claimer: Location,
    xcm_fn: impl Fn(ParaToParaThroughAHTest, Location) -> <PenpalA as Chain>::RuntimeCall,
) {
    let call = xcm_fn(test.clone(), claimer.clone());
    PenpalA::execute_with(|| {
        assert!(call.dispatch(test.signed_origin).is_ok());
    });
}

fn transfer_assets(
    test: ParaToParaThroughAHTest,
    claimer: Location
) -> <PenpalA as Chain>::RuntimeCall {
    type RuntimeCall = <PenpalA as Chain>::RuntimeCall;

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
    test: ParaToParaThroughAHTest,
    claimer: Location
) -> <PenpalA as Chain>::RuntimeCall {
    type RuntimeCall = <PenpalA as Chain>::RuntimeCall;

    let local_xcm = Xcm::<RuntimeCall>::builder_unsafe()
        .claim_asset(test.args.assets.clone(), Here)
        .deposit_asset(AllCounted(test.args.assets.len() as u32), claimer)
        .build();

    RuntimeCall::PolkadotXcm(pallet_xcm::Call::execute {
        message: bx!(VersionedXcm::from(local_xcm)),
        max_weight: Weight::from_parts(4_000_000_000_000, 300_000),
    })
}

#[test]
fn test_sac_between_the_chains() {
    let alice = AssetHubWestend::account_id_of(ALICE);
    let bob = BridgeHubWestend::account_id_of(BOB);
    let destination = AssetHubWestend::sibling_location_of(BridgeHubWestend::para_id());
    let bob_location = Location::new(0, Junction::AccountId32 { network: Some(Westend), id: bob.clone().into() });

    let amount_to_send = 16_000_000_000_000u128;
    BridgeHubWestend::fund_accounts(vec![(
        bob.clone(),
        amount_to_send * 2,
    )]);
    let balance = <BridgeHubWestend as Chain>::account_data_of(bob.clone()).free;

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
    let test = BridgeToAssetHubTest::new(test_args);
    let alice_on_ah = Location::new(
        1,
        [
            Parachain(1000),
            Junction::AccountId32 {
                network: Some(Westend),
                id: alice.clone().into()}],
    );

    execute_bob_bh_test(test.clone(), alice_on_ah.clone(), trap_assets_bh);

    let balance = <BridgeHubWestend as Chain>::account_data_of(bob.clone()).free;


    let amount_to_send = 16_000_000_000_000u128;
    AssetHubWestend::fund_accounts(vec![(
        alice.clone(),
        amount_to_send * 2,
    )]);

    println!("before LocationToAccountId");
    // let alLoc = LocationToAccountId::convert_location(&alice_on_ah).unwrap();
    // println!("alice Loc is: {:?}", alLoc);
    // // let balance = <BridgeHubWestend as Chain>::account_data_of(alLoc.clone()).free;
    // println!("[AH] Alice balance before {:?}", balance);

    // let test_args = TestContext {
    //     sender: alice.clone(),
    //     receiver: bob.clone(),
    //     args: TestArgs::new_para(
    //         destination.clone(),
    //         bob.clone(),
    //         amount_to_send,
    //         assets.clone(),
    //         None,
    //         0,
    //     ),
    // };
    // let alice_on_ah = Location::new(
    //     1,
    //     [
    //         Parachain(1000),
    //         Junction::AccountId32 {
    //             network: Some(Westend),
    //             id: alice.clone().into()}],
    // );
    // let test = AssetHubToBridgeHubTest::new(test_args);
    // let bridge_hub = AssetHubWestend::sibling_location_of(
    //     BridgeHubWestend::para_id()
    // ).into();
    // let xcm_there = Xcm::<()>::builder_unsafe()
    //     .claim_asset(test.args.assets.clone(), Here)
    //     .pay_fees((Parent, 15_000_000_000_000u128))
    //     .deposit_asset(All, alice_on_ah.clone())
    //     .build();
    // AssetHubWestend::execute_with(|| {
    //     assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::send(
    //         test.signed_origin,
    //         bx!(bridge_hub),
    //         bx!(VersionedXcm::from(xcm_there)),
    //     ));
    // });
    //
    //
    // let alLoc = ahw_xcm_config::LocationToAccountId::convert_location(&alice_on_ah).unwrap();
    // let balance = <BridgeHubWestend as Chain>::account_data_of(alLoc).free;
    // println!("[AH] Alice balance after {:?}", balance);

}

fn execute_bob_bh_test(
    test: BridgeToAssetHubTest,
    claimer: Location,
    xcm_fn: impl Fn(BridgeToAssetHubTest, Location) -> <BridgeHubWestend as Chain>::RuntimeCall,
) {
    let call = xcm_fn(test.clone(), claimer.clone());
    BridgeHubWestend::execute_with(|| {
        assert!(call.dispatch(test.signed_origin).is_ok());
    });
}

fn trap_assets_bh(
    test: BridgeToAssetHubTest,
    claimer: Location
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