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
use crate::imports::ahw_xcm_config::UniversalLocation;

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

    // Fund accounts.
    fund_account(&alice_account, amount_to_send * 2);

    let alice_assets_before = query_balance(&alice_account, &native_asset_location);
    assert_eq!(alice_assets_before, amount_to_send * 2);

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

fn transfer_assets_ah(
    test: SystemParaToParaTest,
    claimer: Location
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

#[test]
fn test_set_asset_claimer_on_ah() {
    log::error!("anything_error");
    println!("printlined_error");

    let alice = AssetHubWestend::account_id_of(ALICE);
    let alice_location = Location::new(0, Junction::AccountId32 { network: None, id: alice.clone().into() });
    let assets: Assets = (Parent, 1000u128).into();
    // let alice_on_asset_hub = Location::new(1, [Parachain(1000), Junction::AccountId32 { id: [67u8; 32], network: Some(Westend) }]);


    let bob_on_penpal = Location::new(
        1,
        [
            Parachain(PenpalA::para_id().into()),
            Junction::AccountId32 {
                network: None,
                id: PenpalA::account_id_of(BOB).into()}],
    );
    println!("PenpalA:: sender is {:?}", PenpalASender::get());
    println!("BOB on PenpalA: {:?}", bob_on_penpal);
    // let bob_acc = AssetHubWestend::sovereign_account_id_of(PenpalASender::);

    let amount_to_send = 16_000_000_000_000u128;
    let alice_acc = AssetHubWestend::account_id_of(ALICE);
    AssetHubWestend::fund_accounts(vec![(
        alice_acc.clone(),
        amount_to_send * 2,
    )]);

    let balance = <AssetHubWestend as Chain>::account_data_of(alice_acc.clone()).free;
    println!("alice balance {:?}", balance);

    let test_args = TestContext {
        sender: alice_acc.clone(),
        receiver: alice_acc.clone(),
        args: TestArgs::new_para(
            alice_location.clone(),
            alice_acc.clone(),
            amount_to_send,
            assets.clone(),
            None,
            0,
        ),
    };
    let test = SystemParaToParaTest::new(test_args);
    execute_ah_test(test.clone(), bob_on_penpal.clone(), transfer_assets_ah);



    let balance = <AssetHubWestend as Chain>::account_data_of(alice_acc.clone()).free;
    println!("alice balance after {:?}", balance);




    // let bob_ah_from_bh = BridgeHubWestend::sovereign_account_id_of(Location::new(
    //     1,
    //     [Parachain(AssetHubWestend::para_id().into()), Junction::AccountId32 {network: Some(Westend), id: AssetHubWestend::account_id_of(BOB).into()}],
    // ));
    // let alice_sov_account_on_bridge_hub = BridgeHubWestend::sovereign_account_id_of(alice_on_asset_hub);
    // println!("alice_sov_account_on_bridge_hub {:?}", alice_sov_account_on_bridge_hub);

    // let (alice_account, alice_location) = account_and_location_ah(ALICE);
    // let (bob_account, bob_location) = account_and_location_ah(BOB);




    // PenpalA::execute_with(|| {
    //     type System = <PenpalA as PenpalAPallet>::System;
    //     type RuntimeOrigin = <PenpalA as Chain>::RuntimeOrigin;
    //     assert_ok!(System::set_storage(
    //         RuntimeOrigin::root(),
    //         vec![(penpal_runtime::xcm_config::RelayNetworkId::key().to_vec(), NetworkId::Westend.encode())]
    //     ));
    // });
    //

    // log::trace!("alice account: {}" , alice_account);

    let amount_to_send = 16_000_000_000_000u128;
    let alice_acc = <AssetHubWestend as Chain>::account_id_of(ALICE);
    // Fund accounts.
    AssetHubWestend::fund_accounts(vec![(
        alice_acc.clone(),
        amount_to_send * 2,
    )]);
    let balance = <AssetHubWestend as Chain>::account_data_of(alice_acc.clone()).free;
    println!("alice balance {:?}", balance);
    // AssetHubWestend::execute_with(||{
    // });

    let assets: Assets = (Parent, amount_to_send).into();
    let alice: AccountId = AssetHubWestend::account_id_of(ALICE);


    let alice_signed = <AssetHubWestend as Chain>::RuntimeOrigin::signed(alice.clone());
    // let bridge_hub = AssetHubWestend::sibling_location_of(BridgeHubWestend::para_id()).into();
    let xcm = Xcm::<()>::builder_unsafe()
        .claim_asset(assets.clone(), Here)
        .pay_fees(Asset { id: AssetId(Location::new(1, [])), fun: Fungibility::Fungible(1_000_000u128) })
        .build();
    // let bridge_hub = Location::new(1, Parachain(1002));

    let bob_acc = <BridgeHubWestend as Chain>::account_id_of(BOB);
    let bob_location = Location::new(0, Junction::AccountId32 { network: None, id: bob_acc.clone().into() });



    // let xcm = Xcm::<()>::builder_unsafe()
    //     .buy_execution((Parent, 100u128), WeightLimit::Unlimited)
    //     // .pay_fees(Asset { id: AssetId(Location::new(1, [])), fun: Fungibility::Fungible(1_000u128) })
    //     // .set_asset_claimer(bob_location)
    //     .build();

    // AssetHubWestend::execute_with(|| {
    //     assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::send(
	// 		alice_signed,
	// 		bx!(bridge_hub),
	// 		bx!(VersionedXcm::from(xcm)),
	// 	));
    //     // AssetHubWestend::assert_xcm_pallet_sent();
    // });



    // BridgeHubWestend::execute_with(|| {
    //     type Balances = <BridgeHubWestend as BridgeHubWestendPallet>::Balances;
    //
    //     dbg!(&Balances::free_balance(&alice_sov_account_on_bridge_hub));
    // });
    // // let bob_account = BridgeHubWestend::sibling_location_of(AssetHubWestend::account_id_of(BOB)).into();
    //
    // BridgeHubWestend::fund_accounts(vec![(
    //     alice_from_bh.clone(),
    //     amount_to_send * 2,
    // )]);



    // BH
    //  1. add assets to AccountId {parent 1, Chainid (1002 or 1000) Alice

    // let native_asset_location = RelayLocation::get();
    // let alice_assets_before = query_ah_balance(&alice_account, &native_asset_location);
    // assert_eq!(alice_assets_before, amount_to_send * 2);
    // log::debug!("{}" ,alice_assets_before);

    // let test_args = TestContext {
    //     sender: alice_account.clone(),
    //     receiver: bob_account.clone(),
    //     args: TestArgs::new_para(
    //         bob_location.clone(),
    //         bob_account.clone(),
    //         amount_to_send,
    //         assets.clone(),
    //         None,
    //         0,
    //     ),
    // };
    // let test = ParaToParaThroughAHTest::new(test_args);
    // execute_test(test.clone(), bob_location.clone(), transfer_assets_to_ah);

    // let alice_assets_after = query_balance(&alice_account, &native_asset_location);
    // assert_eq!(alice_assets_after, amount_to_send);


    // let test_args = TestContext {
    //     sender: bob_account.clone(),
    //     receiver: alice_account.clone(),
    //     args: TestArgs::new_para(
    //         alice_location.clone(),
    //         alice_account.clone(),
    //         amount_to_send,
    //         assets.clone(),
    //         None,
    //         0,
    //     ),
    // };
    // let test = ParaToParaThroughAHTest::new(test_args);
    // execute_test(test.clone(), bob_location.clone(), claim_assets);
    //
    // let bob_assets_after = query_balance(&bob_account, &native_asset_location);
    // assert_eq!(bob_assets_after, amount_to_send);
}

fn transfer_assets_to_ah(
    test: ParaToParaThroughAHTest,
    claimer: Location
) -> <PenpalA as Chain>::RuntimeCall {
    type RuntimeCall = <PenpalA as Chain>::RuntimeCall;


    let xcm_in_reserve = Xcm::<RuntimeCall>::builder_unsafe()
        .set_asset_claimer(claimer.clone())
        .withdraw_asset(test.args.assets)
        .build();

    let fee_asset: Asset = (Location::parent(), 1_000_000u128).into();


    // let local_xcm = Xcm::<RuntimeCall>::builder_unsafe()
    //     .set_asset_claimer(claimer.clone())
    //     .initiate_reserve_withdraw(fee_asset.clone(), Location::parent(), xcm_in_reserve)
    //     .build();

    RuntimeCall::PolkadotXcm(pallet_xcm::Call::execute {
        message: bx!(VersionedXcm::from(xcm_in_reserve)),
        max_weight: Weight::from_parts(4_000_000_000_000, 300_000),
    })
}

fn account_and_location_ah(account: &str) -> (AccountId32, Location) {
    let account_id = AssetHubWestend::account_id_of(account);
    let clone = account_id.clone();
    let location: Location = [Junction::AccountId32 { network: Some(Westend), id: account_id.into() }].into();
    (clone, location)
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

fn query_ah_balance(account: &AccountId, asset_location: &Location) -> u128 {
    AssetHubWestend::execute_with(|| {
        type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
        <ForeignAssets as Inspect<_>>::balance(asset_location.clone(), account)
    })
}

fn query_balance(account: &AccountId, asset_location: &Location) -> u128 {
    PenpalA::execute_with(|| {
        type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
        <ForeignAssets as Inspect<_>>::balance(asset_location.clone(), account)
    })
}

fn execute_ah_test(
    test: SystemParaToParaTest,
    claimer: Location,
    xcm_fn: impl Fn(SystemParaToParaTest, Location) -> <AssetHubWestend as Chain>::RuntimeCall,
) {
    let call = xcm_fn(test.clone(), claimer.clone());
    AssetHubWestend::execute_with(|| {
        assert!(call.dispatch(test.signed_origin).is_ok());
    });
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
