// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Tests for using both the XCM fee payment API and the transfers API.

use sp_api::ProvideRuntimeApi;
use sp_runtime::testing::H256;
use xcm_fee_payment_runtime_api::{XcmPaymentApi, XcmDryRunApi};
use xcm::prelude::*;

mod mock;
use mock::{
    TestClient, HereLocation, TestXt, RuntimeCall,
    new_test_ext_with_balances, extra, DeliveryFees, ExistentialDeposit,
    new_test_ext_with_balances_and_assets,
};

// Scenario: User `1` in the local chain wants to transfer assets to account `[0u8; 32]` on "AssetHub".
// He wants to make sure he has enough for fees, so before he calls the `transfer_asset` extrinsic to do the transfer,
// he decides to use the `XcmDryRunApi` and `XcmPaymentApi` runtime APIs to estimate fees.
// This uses a teleport because we're dealing with the native token of the chain, which is registered on "AssetHub".
#[test]
fn fee_estimation_for_teleport() {
    let _ = env_logger::builder()
        .is_test(true)
        .try_init();
    let balances = vec![(1, 100 + DeliveryFees::get() + ExistentialDeposit::get())];
    new_test_ext_with_balances(balances).execute_with(|| {
        let client = TestClient;
        let runtime_api = client.runtime_api();
        let who = 1; // AccountId = u64.
        let extrinsic = TestXt::new(
            RuntimeCall::XcmPallet(pallet_xcm::Call::transfer_assets {
                dest: Box::new(VersionedLocation::V4((Parent, Parachain(1000)).into())),
                beneficiary: Box::new(VersionedLocation::V4(AccountId32 { id: [0u8; 32], network: None }.into())),
                assets: Box::new(VersionedAssets::V4((Here, 100u128).into())),
                fee_asset_item: 0,
                weight_limit: Unlimited,
            }),
            Some((who, extra())),
        );
        let dry_run_effects = runtime_api.dry_run_extrinsic(
            H256::zero(),
            extrinsic,
        ).unwrap().unwrap();

        assert_eq!(
            dry_run_effects.local_program,
            VersionedXcm::V4(
                Xcm::builder_unsafe()
                    .withdraw_asset((Here, 100u128).into())
                    .burn_asset((Here, 100u128).into())
                    .build()
            ),
        );
        assert_eq!(
            dry_run_effects.forwarded_messages,
            vec![
                (
                    VersionedLocation::V4(Location::new(1, [Parachain(1000)])),
                    VersionedXcm::V4(
                        Xcm::<()>::builder_unsafe()
                            .receive_teleported_asset(((Parent, Parachain(2000)), 100u128).into())
                            .clear_origin()
                            .buy_execution(((Parent, Parachain(2000)), 100u128).into(), Unlimited)
                            .deposit_asset(AllCounted(1).into(), AccountId32 { id: [0u8; 32], network: None }.into())
                            .build()
                    )
                ),
            ],
        );

        // TODO: Weighing the local program is not relevant for extrinsics that already
        // take this weight into account.
        // In this case, we really only care about delivery fees.
        let local_program = dry_run_effects.local_program;

        // We get a double result since the actual call returns a result and the runtime api returns results.
        let weight = runtime_api.query_xcm_weight(
            H256::zero(),
            local_program.clone(),
        ).unwrap().unwrap();
        assert_eq!(weight, Weight::from_parts(200, 20));
        let execution_fees = runtime_api.query_weight_to_asset_fee(
            H256::zero(),
            weight,
            VersionedAssetId::V4(HereLocation::get().into())
        ).unwrap().unwrap();
        assert_eq!(execution_fees, 220);

        let mut forwarded_messages_iter = dry_run_effects.forwarded_messages.into_iter();

        let (destination, remote_message) = forwarded_messages_iter.next().unwrap();

        let delivery_fees = runtime_api.query_delivery_fees(
            H256::zero(),
            destination.clone(),
            remote_message.clone(),
        ).unwrap().unwrap();
        assert_eq!(delivery_fees, VersionedAssets::V4((Here, 20u128).into()));

        // TODO: This would have to be the runtime API of the destination,
        // which we have the location for.
        // If I had a mock runtime configured for "AssetHub" then I would use the
        // runtime APIs from that.
        let remote_execution_weight = runtime_api.query_xcm_weight(
            H256::zero(),
            remote_message.clone(),
        ).unwrap().unwrap();
        let remote_execution_fees = runtime_api.query_weight_to_asset_fee(
            H256::zero(),
            remote_execution_weight,
            VersionedAssetId::V4(HereLocation::get().into()),
        ).unwrap().unwrap();
        assert_eq!(remote_execution_fees, 440u128);

        // Now we know that locally we need to use `execution_fees` and
        // `delivery_fees`.
        // On the message we forward to the destination, we need to
        // put `remote_execution_fees` in `BuyExecution`.
        // For the `transfer_assets` extrinsic, it just means passing the correct amount
        // of fees in the parameters.
    });
}

#[test]
fn dry_run_reserve_asset_transfer() {
    let _ = env_logger::builder()
        .is_test(true)
        .try_init();
    let who = 1; // AccountId = u64.
    // Native token used for fees.
    let balances = vec![(who, DeliveryFees::get() + ExistentialDeposit::get())];
    // Relay token is the one we want to transfer.
    let assets = vec![(1, who, 100)]; // id, account_id, balance.
    new_test_ext_with_balances_and_assets(balances, assets).execute_with(|| {
        let client = TestClient;
        let runtime_api = client.runtime_api();
        let extrinsic = TestXt::new(
            RuntimeCall::XcmPallet(pallet_xcm::Call::transfer_assets {
                dest: Box::new(VersionedLocation::V4((Parent, Parachain(1000)).into())),
                beneficiary: Box::new(VersionedLocation::V4(AccountId32 { id: [0u8; 32], network: None }.into())),
                assets: Box::new(VersionedAssets::V4((Parent, 100u128).into())),
                fee_asset_item: 0,
                weight_limit: Unlimited,
            }),
            Some((who, extra())),
        );
        let dry_run_effects = runtime_api.dry_run_extrinsic(
            H256::zero(),
            extrinsic,
        ).unwrap().unwrap();

        assert_eq!(
            dry_run_effects.local_program,
            VersionedXcm::V4(
                Xcm::builder_unsafe()
                    .withdraw_asset((Parent, 100u128).into())
                    .burn_asset((Parent, 100u128).into())
                    .build()
            ),
        );

        // In this case, the transfer type is `DestinationReserve`, so the remote xcm just withdraws the assets.
        assert_eq!(
            dry_run_effects.forwarded_messages,
            vec![
                (
                    VersionedLocation::V4(Location::new(1, Parachain(1000))),
                    VersionedXcm::V4(
                        Xcm::<()>::builder_unsafe()
                            .withdraw_asset((Parent, 100u128).into())
                            .clear_origin()
                            .buy_execution((Parent, 100u128).into(), Unlimited)
                            .deposit_asset(AllCounted(1).into(), AccountId32 { id: [0u8; 32], network: None }.into())
                            .build()
                    ),
                ),
            ],
        );
    });
}
