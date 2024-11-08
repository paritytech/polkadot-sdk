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

use assets_common::runtime_api::runtime_decl_for_fungibles_api::FungiblesApiV2;
use emulated_integration_tests_common::test_chain_can_claim_assets;
use frame_support::traits::fungible::Mutate;
use xcm_executor::traits::DropAssets;

#[test]
fn assets_can_be_claimed() {
	let amount = AssetHubWestendExistentialDeposit::get();
	let assets: Assets = (Parent, amount).into();

	test_chain_can_claim_assets!(
		AssetHubWestend,
		RuntimeCall,
		NetworkId::ByGenesis(WESTEND_GENESIS_HASH),
		assets,
		amount
	);
}

#[test]
fn chain_can_claim_assets_for_its_users() {
	// Many Penpal users have assets trapped in AssetHubWestend.
	let beneficiaries: Vec<(Location, Assets)> = vec![
		// Some WND.
		(
			Location::new(1, [Parachain(2000), AccountId32 { id: [0u8; 32], network: None }]),
			(Parent, 10_000_000_000_000u128).into(),
		),
		// Some USDT.
		(
			Location::new(1, [Parachain(2000), AccountId32 { id: [1u8; 32], network: None }]),
			([PalletInstance(ASSETS_PALLET_ID), GeneralIndex(USDT_ID.into())], 100_000_000u128)
				.into(),
		),
	];

	// Start with those assets trapped.
	AssetHubWestend::execute_with(|| {
		for (location, assets) in &beneficiaries {
			<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::drop_assets(
				location,
				assets.clone().into(),
				&XcmContext { origin: None, message_id: [0u8; 32], topic: None },
			);
		}
	});

	let penpal_to_asset_hub = PenpalA::sibling_location_of(AssetHubWestend::para_id());
	let mut builder = Xcm::<()>::builder()
		.withdraw_asset((Parent, 1_000_000_000_000u128))
		.pay_fees((Parent, 100_000_000_000u128));

	// Loop through all beneficiaries.
	for (location, assets) in &beneficiaries {
		builder = builder.execute_with_origin(
			// We take only the last part, the `AccountId32` junction.
			Some((*location.interior().last().unwrap()).into()),
			Xcm::<()>::builder_unsafe()
				.claim_asset(assets.clone(), Location::new(0, [GeneralIndex(5)])) // Means lost assets were version 5.
				.deposit_asset(assets.clone(), location.clone())
				.build(),
		)
	}

	// Finish assembling the message.
	let message = builder.build();

	// Fund PenpalA's sovereign account on AssetHubWestend so it can pay for fees.
	AssetHubWestend::execute_with(|| {
		let penpal_as_seen_by_asset_hub = AssetHubWestend::sibling_location_of(PenpalA::para_id());
		let penpal_sov_account_on_asset_hub =
			AssetHubWestend::sovereign_account_id_of(penpal_as_seen_by_asset_hub);
		type Balances = <AssetHubWestend as AssetHubWestendPallet>::Balances;
		assert_ok!(<Balances as Mutate<_>>::mint_into(
			&penpal_sov_account_on_asset_hub,
			2_000_000_000_000u128,
		));
	});

	// We can send a message from Penpal root that claims all those assets for each beneficiary.
	PenpalA::execute_with(|| {
		assert_ok!(<PenpalA as PenpalAPallet>::PolkadotXcm::send(
			<PenpalA as Chain>::RuntimeOrigin::root(),
			bx!(penpal_to_asset_hub.into()),
			bx!(VersionedXcm::from(message)),
		));
	});

	// We assert beneficiaries have received their funds.
	AssetHubWestend::execute_with(|| {
		for (location, expected_assets) in &beneficiaries {
			let sov_account = AssetHubWestend::sovereign_account_id_of(location.clone());
			let actual_assets =
				<AssetHubWestend as Chain>::Runtime::query_account_balances(sov_account).unwrap();
			assert_eq!(VersionedAssets::from(expected_assets.clone()), actual_assets);
		}
	});
}
