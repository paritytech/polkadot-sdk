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

<<<<<<< HEAD
use emulated_integration_tests_common::accounts::{ALICE, BOB, CHARLIE};
use emulated_integration_tests_common::impls::AccountId32;
use crate::{
    imports::*,
};
=======
use crate::imports::*;
>>>>>>> 0290e0057fa ([WIP] set_asset_claimer e2e test)

use frame_support::{
    dispatch::RawOrigin,
    sp_runtime::{traits::Dispatchable, DispatchResult},
};
<<<<<<< HEAD

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
    let mut test = ParaToParaThroughAHTest::new(test_args);
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
    let mut test = ParaToParaThroughAHTest::new(test_args);
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
=======
use emulated_integration_tests_common::test_chain_can_claim_assets;
use xcm_executor::traits::DropAssets;
use xcm_runtime_apis::{
    dry_run::runtime_decl_for_dry_run_api::DryRunApiV1,
    fees::runtime_decl_for_xcm_payment_api::XcmPaymentApiV1,
};

#[test]
fn azs() {
    let bob = Location::new(0, [AccountId32 { id: [2; 32], network: None }]);
    let destination = PenpalA::sibling_location_of(PenpalB::para_id());
    let sender = PenpalASender::get();
    let beneficiary_id = PenpalBReceiver::get();
    let amount_to_send = 1_000_000_000_000;
    let assets: Assets = (Parent, amount_to_send).into();

    // Fund accounts again.
    PenpalA::mint_foreign_asset(
        <PenpalA as Chain>::RuntimeOrigin::signed(PenpalAssetOwner::get()),
        Location::parent().clone(),
        sender.clone(),
        amount_to_send * 2,
    );

    let test_args = TestContext {
        sender: PenpalASender::get(),     // Bob in PenpalB.
        receiver: PenpalBReceiver::get(), // Alice.
        args: TestArgs::new_para(
            destination,
            beneficiary_id.clone(),
            amount_to_send,
            assets,
            None,
            0,
        ),
    };
    let mut test = ParaToParaThroughAHTest::new(test_args);
    transfer_assets(test.clone(), bob.clone());
    // let call = transfer_assets(test.clone(), bob.clone());


    // test.set_assertion::<PenpalA>(sender_assertions);
    // test.set_call(call);
    // test.assert();
>>>>>>> 0290e0057fa ([WIP] set_asset_claimer e2e test)
}

fn transfer_assets(
    test: ParaToParaThroughAHTest,
    claimer: Location
) -> <PenpalA as Chain>::RuntimeCall {
    type RuntimeCall = <PenpalA as Chain>::RuntimeCall;

<<<<<<< HEAD
    let local_xcm = Xcm::<RuntimeCall>::builder_unsafe()
        .set_asset_claimer(claimer.clone())
        .withdraw_asset(test.args.assets.clone())
        .clear_origin()
=======


    let local_xcm = Xcm::<RuntimeCall>::builder_unsafe()
        .clear_origin()
        .set_asset_claimer(claimer.clone())
        .withdraw_asset(test.args.assets.clone())
        .pay_fees((Parent, 0))
>>>>>>> 0290e0057fa ([WIP] set_asset_claimer e2e test)
        .build();

    RuntimeCall::PolkadotXcm(pallet_xcm::Call::execute {
        message: bx!(VersionedXcm::from(local_xcm)),
<<<<<<< HEAD
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
=======
        max_weight: Weight::from_parts(3_000_000_000, 200_000),
    })
}

fn sender_assertions(test: ParaToParaThroughAHTest) {
    type RuntimeEvent = <PenpalA as Chain>::RuntimeEvent;
    // PenpalA::assert_xcm_pallet_attempted_complete(None);
    assert_expected_events!(
		PenpalA,
		vec![
			RuntimeEvent::ForeignAssets(
				pallet_assets::Event::Burned { asset_id, owner, balance }
			) => {
				asset_id: *asset_id == Location::new(1, []),
				owner: *owner == test.sender.account_id,
				balance: *balance == test.args.amount,
			},
		]
	);
}
>>>>>>> 0290e0057fa ([WIP] set_asset_claimer e2e test)
