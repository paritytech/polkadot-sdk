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
use westend_system_emulated_network::penpal_emulated_chain::LocalTeleportableToAssetHubV3 as PenpalLocalTeleportableToAssetHubV3;

#[test]
fn swap_locally_on_chain_using_local_assets() {
	let asset_native = Box::new(asset_hub_westend_runtime::xcm_config::WestendLocationV3::get());
	let asset_one = Box::new(v3::Location {
		parents: 0,
		interior: [
			v3::Junction::PalletInstance(ASSETS_PALLET_ID),
			v3::Junction::GeneralIndex(ASSET_ID.into()),
		]
		.into(),
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

		let path = vec![asset_native.clone(), asset_one.clone()];

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
			asset_native.clone(),
			asset_one.clone(),
			1414213562273 - 2_000_000_000, // all but the 2 EDs can't be retrieved.
			0,
			0,
			AssetHubWestendSender::get().into(),
		));
	});
}

#[test]
fn swap_locally_on_chain_using_foreign_assets() {
	let asset_native = Box::new(asset_hub_westend_runtime::xcm_config::WestendLocationV3::get());
	let ah_as_seen_by_penpal = PenpalB::sibling_location_of(AssetHubWestend::para_id());
	let asset_location_on_penpal = PenpalLocalTeleportableToAssetHubV3::get();
	let asset_id_on_penpal = match asset_location_on_penpal.last() {
		Some(v3::Junction::GeneralIndex(id)) => *id as u32,
		_ => unreachable!(),
	};
	let asset_owner_on_penpal = PenpalBSender::get();
	let foreign_asset_at_asset_hub_westend =
		v3::Location::new(1, [v3::Junction::Parachain(PenpalB::para_id().into())])
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

		// 4. Create pool:
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::create_pool(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			asset_native.clone(),
			Box::new(foreign_asset_at_asset_hub_westend),
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
			Box::new(foreign_asset_at_asset_hub_westend),
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
		let path = vec![asset_native.clone(), Box::new(foreign_asset_at_asset_hub_westend)];

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
			asset_native.clone(),
			Box::new(foreign_asset_at_asset_hub_westend),
			1414213562273 - 2_000_000_000, // all but the 2 EDs can't be retrieved.
			0,
			0,
			sov_penpal_on_ahw.into(),
		));
	});
}

#[test]
fn cannot_create_pool_from_pool_assets() {
	let asset_native = Box::new(asset_hub_westend_runtime::xcm_config::WestendLocationV3::get());
	let mut asset_one = asset_hub_westend_runtime::xcm_config::PoolAssetsPalletLocationV3::get();
	asset_one
		.append_with(v3::Junction::GeneralIndex(ASSET_ID.into()))
		.expect("pool assets");

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
				asset_native,
				Box::new(asset_one),
			),
			Err(DispatchError::Module(ModuleError{index: _, error: _, message})) => assert_eq!(message, Some("Unknown"))
		);
	});
}

#[test]
fn pay_xcm_fee_with_some_asset_swapped_for_native() {
	let asset_native = asset_hub_westend_runtime::xcm_config::WestendLocationV3::get();
	let asset_one = xcm::v3::Location {
		parents: 0,
		interior: [
			xcm::v3::Junction::PalletInstance(ASSETS_PALLET_ID),
			xcm::v3::Junction::GeneralIndex(ASSET_ID.into()),
		]
		.into(),
	};
	let penpal = AssetHubWestend::sovereign_account_id_of(AssetHubWestend::sibling_location_of(
		PenpalB::para_id(),
	));

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		// set up pool with ASSET_ID <> NATIVE pair
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::Assets::create(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			ASSET_ID.into(),
			AssetHubWestendSender::get().into(),
			ASSET_MIN_BALANCE,
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
			Box::new(asset_native),
			Box::new(asset_one),
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { .. }) => {},
			]
		);

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::add_liquidity(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			Box::new(asset_native),
			Box::new(asset_one),
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

		// ensure `penpal` sovereign account has no native tokens and mint some `ASSET_ID`
		assert_eq!(
			<AssetHubWestend as AssetHubWestendPallet>::Balances::free_balance(penpal.clone()),
			0
		);

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::Assets::touch_other(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			ASSET_ID.into(),
			penpal.clone().into(),
		));

		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::Assets::mint(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(AssetHubWestendSender::get()),
			ASSET_ID.into(),
			penpal.clone().into(),
			10_000_000_000_000,
		));
	});

	PenpalB::execute_with(|| {
		// send xcm transact from `penpal` account which as only `ASSET_ID` tokens on
		// `AssetHubWestend`
		let call = AssetHubWestend::force_create_asset_call(
			ASSET_ID + 1000,
			penpal.clone(),
			true,
			ASSET_MIN_BALANCE,
		);

		let penpal_root = <PenpalB as Chain>::RuntimeOrigin::root();
		let fee_amount = 4_000_000_000_000u128;
		let asset_one =
			([PalletInstance(ASSETS_PALLET_ID), GeneralIndex(ASSET_ID.into())], fee_amount).into();
		let asset_hub_location = PenpalB::sibling_location_of(AssetHubWestend::para_id()).into();
		let xcm = xcm_transact_paid_execution(
			call,
			OriginKind::SovereignAccount,
			asset_one,
			penpal.clone(),
		);

		assert_ok!(<PenpalB as PenpalBPallet>::PolkadotXcm::send(
			penpal_root,
			bx!(asset_hub_location),
			bx!(xcm),
		));

		PenpalB::assert_xcm_pallet_sent();
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		AssetHubWestend::assert_xcmp_queue_success(None);
		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapCreditExecuted { .. },) => {},
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true,.. }) => {},
			]
		);
	});
}
