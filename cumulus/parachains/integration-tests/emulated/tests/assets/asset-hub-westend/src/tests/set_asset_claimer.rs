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

use emulated_integration_tests_common::accounts::{ALICE, BOB};
use emulated_integration_tests_common::impls::AccountId32;
use crate::{
    imports::*,
};

use frame_support::{
    sp_runtime::{traits::Dispatchable},
};
#[test]
fn test_set_asset_claimer_within_a_chain() {
    let (alice_account, alice_location) = account_and_location(ALICE);
    let (bob_account, bob_location) = account_and_location(BOB);

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

fn account_and_location(account: &str) -> (AccountId32, Location) {
    let account_id = PenpalA::account_id_of(account);
    let clone = account_id.clone();
    let location: Location = [Junction::AccountId32 { network: Some(Rococo), id: account_id.into() }].into();
    (clone, location)
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
    location: Location,
    xcm_fn: impl Fn(ParaToParaThroughAHTest, Location) -> <PenpalA as Chain>::RuntimeCall,
) {
    let call = xcm_fn(test.clone(), location.clone());
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
