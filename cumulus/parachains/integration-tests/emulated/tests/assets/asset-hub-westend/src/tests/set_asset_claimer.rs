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

use emulated_integration_tests_common::accounts::BOB;
use crate::{
    imports::*,
};

use frame_support::{
    dispatch::RawOrigin,
    sp_runtime::{traits::Dispatchable, DispatchResult},
};
use emulated_integration_tests_common::test_chain_can_claim_assets;
use xcm_executor::traits::DropAssets;
use xcm_runtime_apis::{
    dry_run::runtime_decl_for_dry_run_api::DryRunApiV1,
    fees::runtime_decl_for_xcm_payment_api::XcmPaymentApiV1,
};

#[test]
fn azs() {
    let destination = PenpalA::sibling_location_of(PenpalB::para_id());
    let sender = PenpalASender::get();

    let amount_to_send = 16_000_000_000_000;
    let asset_owner = PenpalAssetOwner::get();
    let native_asset_location = RelayLocation::get();
    let assets: Assets = (Parent, amount_to_send).into();



    let bob_account: AccountId = PenpalA::account_id_of(BOB);
    let bob_location: Location =
        [Junction::AccountId32 { network: None, id: PenpalA::account_id_of(BOB).into() }]
            .into();



    // Fund accounts.
    let relay_native_asset_location = Location::parent();
    PenpalA::mint_foreign_asset(
        <PenpalA as Chain>::RuntimeOrigin::signed(asset_owner),
        relay_native_asset_location.clone(),
        sender.clone(),
        amount_to_send * 2,
    );

    let sender_assets_before = PenpalA::execute_with(|| {
        type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
        <ForeignAssets as Inspect<_>>::balance(native_asset_location.clone(), &sender)
    });

    dbg!(sender_assets_before);

    // Init values for Parachain Destination
    let beneficiary_id = PenpalBReceiver::get();
    let test_args = TestContext {
        sender: PenpalASender::get(),
        receiver: PenpalBReceiver::get(),
        args: TestArgs::new_para(
            destination.clone(),
            beneficiary_id.clone(),
            amount_to_send,
            assets.clone(),
            None,
            0,
        ),
    };
    let mut test = ParaToParaThroughAHTest::new(test_args);
    let call = transfer_assets(test.clone(), bob_location.clone());
    PenpalA::execute_with(|| {
        assert!(call.dispatch(test.signed_origin).is_ok());
    });

    let sender_assets_after = PenpalA::execute_with(|| {
        type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
        <ForeignAssets as Inspect<_>>::balance(native_asset_location.clone(), &sender)
    });

    dbg!(sender_assets_after);

    let test_args = TestContext {
        sender: PenpalASender::get(),
        receiver: PenpalBReceiver::get(),
        args: TestArgs::new_para(
            destination.clone(),
            beneficiary_id.clone(),
            amount_to_send,
            assets.clone(),
            None,
            0,
        ),
    };
    let mut test = ParaToParaThroughAHTest::new(test_args);
    let call = claim_assets(test.clone(), bob_location.clone());
    call.dispatch(<PenpalA as Chain>::RuntimeOrigin::signed(bob_account.clone()));
    // PenpalA::execute_with(|| {
    //     assert!(call.dispatch(test.signed_origin).is_ok());
    // });

    let bob_assets_after = PenpalA::execute_with(|| {
        type ForeignAssets = <PenpalA as PenpalAPallet>::ForeignAssets;
        <ForeignAssets as Inspect<_>>::balance(native_asset_location.clone(), &bob_account)
    });

    dbg!(bob_assets_after);
}

fn claim_assets(
    test: ParaToParaThroughAHTest,
    claimer: Location
) -> <PenpalA as Chain>::RuntimeCall {
    type RuntimeCall = <PenpalA as Chain>::RuntimeCall;

    let local_xcm = Xcm::<RuntimeCall>::builder_unsafe()
        .claim_asset(test.args.assets.clone(), Here)
        .deposit_asset(AllCounted(test.args.assets.len() as u32), claimer)
        .pay_fees((Parent, 10u128))
        .build();

    RuntimeCall::PolkadotXcm(pallet_xcm::Call::execute {
        message: bx!(VersionedXcm::from(local_xcm)),
        max_weight: Weight::from_parts(4_000_000_000_000, 300_000),
    })
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
        // .pay_fees((Parent, 4_000_000_000_000u128))
        .build();

    RuntimeCall::PolkadotXcm(pallet_xcm::Call::execute {
        message: bx!(VersionedXcm::from(local_xcm)),
        max_weight: Weight::from_parts(4_000_000_000_000, 300_000),
    })
}