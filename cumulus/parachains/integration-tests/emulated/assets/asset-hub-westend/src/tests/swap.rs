// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use crate::*;

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
	use frame_support::weights::WeightToFee;

	let asset_native = Box::new(asset_hub_westend_runtime::xcm_config::WestendLocation::get());

	let foreign_asset1_at_asset_hub_westend = Box::new(MultiLocation {
		parents: 1,
		interior: X3(
			Parachain(PenpalWestendA::para_id().into()),
			PalletInstance(ASSETS_PALLET_ID),
			GeneralIndex(ASSET_ID.into()),
		),
	});

	let assets_para_destination: VersionedMultiLocation =
		MultiLocation { parents: 1, interior: X1(Parachain(AssetHubWestend::para_id().into())) }
			.into();

	let penpal_location =
		MultiLocation { parents: 1, interior: X1(Parachain(PenpalWestendA::para_id().into())) };

	// 1. Create asset on penpal:
	PenpalWestendA::execute_with(|| {
		assert_ok!(<PenpalWestendA as PenpalWestendAPallet>::Assets::create(
			<PenpalWestendA as Chain>::RuntimeOrigin::signed(PenpalWestendASender::get()),
			ASSET_ID.into(),
			PenpalWestendASender::get().into(),
			1000,
		));

		assert!(<PenpalWestendA as PenpalWestendAPallet>::Assets::asset_exists(ASSET_ID));
	});

	// 2. Create foreign asset on asset_hub_westend:

	let require_weight_at_most = Weight::from_parts(1_100_000_000_000, 30_000);
	let origin_kind = OriginKind::Xcm;
	let sov_penpal_on_asset_hub_westend = AssetHubWestend::sovereign_account_id_of(penpal_location);

	AssetHubWestend::fund_accounts(vec![
		(AssetHubWestendSender::get().into(), 5_000_000 * WESTEND_ED),
		(sov_penpal_on_asset_hub_westend.clone().into(), 1000_000_000_000_000_000 * WESTEND_ED),
	]);

	let sov_penpal_on_asset_hub_westend_as_location: MultiLocation = MultiLocation {
		parents: 0,
		interior: X1(AccountId32Junction {
			network: None,
			id: sov_penpal_on_asset_hub_westend.clone().into(),
		}),
	};

	let call_foreign_assets_create =
		<AssetHubWestend as Chain>::RuntimeCall::ForeignAssets(pallet_assets::Call::<
			<AssetHubWestend as Chain>::Runtime,
			Instance2,
		>::create {
			id: *foreign_asset1_at_asset_hub_westend,
			min_balance: 1000,
			admin: sov_penpal_on_asset_hub_westend.clone().into(),
		})
		.encode()
		.into();

	let buy_execution_fee_amount =
		asset_hub_westend_runtime::constants::fee::WeightToFee::weight_to_fee(&Weight::from_parts(
			10_100_000_000_000,
			300_000,
		));
	let buy_execution_fee = MultiAsset {
		id: Concrete(MultiLocation { parents: 1, interior: Here }),
		fun: Fungible(buy_execution_fee_amount),
	};

	let xcm = VersionedXcm::from(Xcm(vec![
		WithdrawAsset { 0: vec![buy_execution_fee.clone()].into() },
		BuyExecution { fees: buy_execution_fee.clone(), weight_limit: Unlimited },
		Transact { require_weight_at_most, origin_kind, call: call_foreign_assets_create },
		RefundSurplus,
		DepositAsset {
			assets: All.into(),
			beneficiary: sov_penpal_on_asset_hub_westend_as_location,
		},
	]));

	// Send XCM message from penpal => asset_hub_westend
	let sudo_penpal_origin = <PenpalWestendA as Chain>::RuntimeOrigin::root();
	PenpalWestendA::execute_with(|| {
		assert_ok!(<PenpalWestendA as PenpalWestendAPallet>::PolkadotXcm::send(
			sudo_penpal_origin.clone(),
			bx!(assets_para_destination.clone()),
			bx!(xcm),
		));

		type RuntimeEvent = <PenpalWestendA as Chain>::RuntimeEvent;

		assert_expected_events!(
			PenpalWestendA,
			vec![
				RuntimeEvent::PolkadotXcm(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	// Receive XCM message in Assets Parachain
	AssetHubWestend::execute_with(|| {
		assert!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::asset_exists(
			*foreign_asset1_at_asset_hub_westend
		));

		// 3: Mint foreign asset on asset_hub_westend:
		//
		// (While it might be nice to use batch,
		// currently that's disabled due to safe call filters.)

		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		// 3. Mint foreign asset (in reality this should be a teleport or some such)
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::ForeignAssets::mint(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(
				sov_penpal_on_asset_hub_westend.clone().into()
			),
			*foreign_asset1_at_asset_hub_westend,
			sov_penpal_on_asset_hub_westend.clone().into(),
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
			foreign_asset1_at_asset_hub_westend.clone(),
		));

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { .. }) => {},
			]
		);

		// 5. Add liquidity:
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::AssetConversion::add_liquidity(
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(
				sov_penpal_on_asset_hub_westend.clone()
			),
			asset_native.clone(),
			foreign_asset1_at_asset_hub_westend.clone(),
			1_000_000_000_000,
			2_000_000_000_000,
			0,
			0,
			sov_penpal_on_asset_hub_westend.clone().into()
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
			foreign_asset1_at_asset_hub_westend.clone(),
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
			<AssetHubWestend as Chain>::RuntimeOrigin::signed(
				sov_penpal_on_asset_hub_westend.clone()
			),
			asset_native,
			foreign_asset1_at_asset_hub_westend,
			1414213562273 - 2_000_000_000, // all but the 2 EDs can't be retrieved.
			0,
			0,
			sov_penpal_on_asset_hub_westend.clone().into(),
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
