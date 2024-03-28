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
use mock::{TestClient, HereLocation};

#[test]
fn can_get_both_execution_and_delivery_fees_for_a_transfer() {
    let client = TestClient;
    let runtime_api = client.runtime_api();
    // TODO: Build extrinsic
        // (Parent, Parachain(1000)).into(),
        // AccountId32 { id: [0u8; 32], network: None }.into(),
        // (Here, 100u128).into(),
    let messages = runtime_api.dry_run_extrinsic(
        H256::zero(),
        extrinsic,
    ).unwrap().unwrap();
    // assert_eq!(messages, [...]);

    let mut messages_iter = messages.iter();

    let (_, local_message) = messages_iter.next().unwrap();

    // We get a double result since the actual call returns a result and the runtime api returns results.
    let weight = runtime_api.query_xcm_weight(
        H256::zero(),
        local_message.clone(),
    ).unwrap().unwrap();
    assert_eq!(weight, Weight::from_parts(2_000_000_000_000, 2 * 1024 * 1024));
    let execution_fees = runtime_api.query_weight_to_asset_fee(
        H256::zero(),
        weight,
        VersionedAssetId::V4(HereLocation::get().into())
    ).unwrap().unwrap();
    assert_eq!(execution_fees, 2_000_002_097_152);

    let (destination, remote_message) = messages_iter.next().unwrap();

    let delivery_fees = runtime_api.query_delivery_fees(
        H256::zero(),
        destination.clone(),
        remote_message.clone(),
    ).unwrap().unwrap();

    // This would have to be the runtime API of the destination,
    // which we have the location for.
    let remote_execution_weight = runtime_api.query_xcm_weight(
        H256::zero(),
        remote_message.clone(),
    ).unwrap().unwrap();
    let remote_execution_fees = runtime_api.query_weight_to_asset_fee(
        H256::zero(),
        remote_execution_weight,
        VersionedAssetId::V4(HereLocation::get().into()),
    );

    // Now we know that locally we need to use `execution_fees` and
    // `delivery_fees`.
    // On the message we forward to the destination, we need to
    // put `remote_execution_fees` in `BuyExecution`.
}
