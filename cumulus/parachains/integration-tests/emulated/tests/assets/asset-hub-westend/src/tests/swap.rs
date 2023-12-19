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

use crate::*;
use westend_system_emulated_network::penpal_emulated_chain::LocalTeleportableToAssetHub as PenpalLocalTeleportableToAssetHub;

#[test]
fn swap_locally_on_chain_using_local_assets() {
	let asset_native = Box::new(asset_hub_westend_runtime::xcm_config::WestendLocation::get());
	let asset_one = Box::new(MultiLocation {
		parents: 0,
		interior: X2(PalletInstance(ASSETS_PALLET_ID), GeneralIndex(ASSET_ID.into())),
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::Assets::create(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			ASSET_ID.into(),
			AssetHubWestendSender::get().into(),
			1000,
		));
		assert!(<AssetHubWestend as AssetHubWestendPallet>::Assets::asset_exists(ASSET_ID));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::Assets::mint(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			ASSET_ID.into(),
			AssetHubWestendSender::get().into(),
			3_000_000_000_000,
		));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::create_pool(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			asset_native.clone(),
			asset_one.clone(),
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { .. }) => {},
			]
		);

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::add_liquidity(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			asset_native.clone(),
			asset_one.clone(),
			1_000_000_000_000,
			2_000_000_000_000,
			0,
			0,
			AssetHubWestendSender::get().into()
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {lp_token_minted, .. }) => { lp_token_minted: *lp_token_minted == 1414213562273, },
			]
		);

		let path = BoundedVec::<_, _>::truncate_from(vec![asset_native.clone(), asset_one.clone()]);

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::swap_exact_tokens_for_tokens(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			path,
			100,
			1,
			AssetHubWestendSender::get().into(),
			true
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapExecuted { amount_in, amount_out, .. }) => {
					amount_in: *amount_in == 100,
					amount_out: *amount_out == 199,
				},
			]
		);

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::remove_liquidity(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			asset_native,
			asset_one,
			1414213562273 - 2_000_000_000, // all but the 2 EDs can't be retrieved.
			0,
			0,
			AssetHubWestendSender::get().into(),
		));
	});
}

#[test]
fn swap_locally_on_chain_using_foreign_assets() {
	let asset_native = Box::new(asset_hub_westend_runtime::xcm_config::WestendLocation::get());
	let ah_as_seen_by_penpal = PenpalB::sibling_location_of(AssetHubWestend::para_id());
	let asset_location_on_penpal = PenpalLocalTeleportableToAssetHub::get();
	let asset_id_on_penpal = match asset_location_on_penpal.last() {
		Some(GeneralIndex(id)) => *id as u32,
		_ => unreachable!(),
	};
	let asset_owner_on_penpal = PenpalBSender::get();
	let foreign_asset_at_asset_hub_westend =
		MultiLocation { parents: 1, interior: X1(Parachain(PenpalB::para_id().into())) }
			.appended_with(asset_location_on_penpal)
			.unwrap();

	// 1. Create asset on penpal and, 2. Create foreign asset on asset_hub_westend
	super::penpal_create_foreign_asset_on_asset_hub(
		asset_id_on_penpal,
		foreign_asset_at_asset_hub_westend,
		ah_as_seen_by_penpal,
		true,
		asset_owner_on_penpal,
		ASSET_MIN_BALANCE * 1_000_000,
	);

	let penpal_as_seen_by_ah = AssetHubWestend::sibling_location_of(PenpalB::para_id());
	let sov_penpal_on_ahw = AssetHubWestend::sovereign_account_id_of(penpal_as_seen_by_ah);
	AssetHubWestend::fund_accounts(vec![
		(AssetHubWestendSender::get().into(), 5_000_000 * WESTEND_ED), /* An account to swap dot
		                                                                * for something else. */
	]);

	AssetHubWestend::execute_with(|| {
		// 3: Mint foreign asset on asset_hub_westend:
		//
		// (While it might be nice to use batch,
		// currently that's disabled due to safe call filters.)

		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		// 3. Mint foreign asset (in reality this should be a teleport or some such)
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(sov_penpal_on_ahw.clone().into()),
			foreign_asset_at_asset_hub_westend,
			sov_penpal_on_ahw.clone().into(),
			3_000_000_000_000,
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { .. }) => {},
			]
		);

		let foreign_asset_at_asset_hub_westend = Box::new(foreign_asset_at_asset_hub_westend);
		// 4. Create pool:
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::create_pool(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			asset_native.clone(),
			foreign_asset_at_asset_hub_westend.clone(),
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { .. }) => {},
			]
		);

		// 5. Add liquidity:
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::add_liquidity(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(sov_penpal_on_ahw.clone()),
			asset_native.clone(),
			foreign_asset_at_asset_hub_westend.clone(),
			1_000_000_000_000,
			2_000_000_000_000,
			0,
			0,
			sov_penpal_on_ahw.clone().into()
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {lp_token_minted, .. }) => {
					lp_token_minted: *lp_token_minted == 1414213562273,
				},
			]
		);

		// 6. Swap!
		let path = BoundedVec::<_, _>::truncate_from(vec![
			asset_native.clone(),
			foreign_asset_at_asset_hub_westend.clone(),
		]);

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::swap_exact_tokens_for_tokens(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			path,
			100000,
			1000,
			AssetHubWestendSender::get().into(),
			true
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapExecuted { amount_in, amount_out, .. },) => {
					amount_in: *amount_in == 100000,
					amount_out: *amount_out == 199399,
				},
			]
		);

		// 7. Remove liquidity
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::remove_liquidity(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(sov_penpal_on_ahw.clone()),
			asset_native,
			foreign_asset_at_asset_hub_westend,
			1414213562273 - 2_000_000_000, // all but the 2 EDs can't be retrieved.
			0,
			0,
			sov_penpal_on_ahw.clone().into(),
		));
	});
}

#[test]
fn cannot_create_pool_from_pool_assets() {
	let asset_native = Box::new(asset_hub_westend_runtime::xcm_config::WestendLocation::get());
	let mut asset_one = asset_hub_westend_runtime::xcm_config::PoolAssetsPalletLocation::get();
	asset_one.append_with(GeneralIndex(ASSET_ID.into())).expect("pool assets");

	AssetHubWestend::execute_with(|| {
		let pool_owner_account_id = asset_hub_westend_runtime::AssetConversionOrigin::get();

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::PoolAssets::create(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(pool_owner_account_id.clone()),
			ASSET_ID.into(),
			pool_owner_account_id.clone().into(),
			1000,
		));
		assert!(<AssetHubWestend as AssetHubWestendPallet>::PoolAssets::asset_exists(ASSET_ID));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::PoolAssets::mint(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(pool_owner_account_id),
			ASSET_ID.into(),
			AssetHubWestendSender::get().into(),
			3_000_000_000_000,
		));

		assert_matches::assert_matches!(
			<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::create_pool(
				<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
				asset_native.clone(),
				Box::new(asset_one),
			),
			Err(DispatchError::Module(ModuleError{index: _, error: _, message})) => assert_eq!(message, Some("UnsupportedAsset"))
		);
	});
}
