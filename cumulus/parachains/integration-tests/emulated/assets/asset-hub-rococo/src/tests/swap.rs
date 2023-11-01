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
use frame_support::{instances::Instance2, BoundedVec};
use parachains_common::rococo::currency::EXISTENTIAL_DEPOSIT;
use sp_runtime::{DispatchError, ModuleError};

#[test]
fn swap_locally_on_chain_using_local_assets() {
	let asset_native = Box::new(asset_hub_rococo_runtime::xcm_config::TokenLocation::get());
	let asset_one = Box::new(MultiLocation {
		parents: 0,
		interior: X2(PalletInstance(ASSETS_PALLET_ID), GeneralIndex(ASSET_ID.into())),
	});

	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;

		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::Assets::create(
			<AssetHubRococo as Chain>::RuntimeOrigin::signed(AssetHubRococoSender::get()),
			ASSET_ID.into(),
			AssetHubRococoSender::get().into(),
			1000,
		));
		assert!(<AssetHubRococo as AssetHubRococoPallet>::Assets::asset_exists(ASSET_ID));

		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::Assets::mint(
			<AssetHubRococo as Chain>::RuntimeOrigin::signed(AssetHubRococoSender::get()),
			ASSET_ID.into(),
			AssetHubRococoSender::get().into(),
			100_000_000_000_000,
		));

		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::Balances::force_set_balance(
			<AssetHubRococo as Chain>::RuntimeOrigin::root(),
			AssetHubRococoSender::get().into(),
			100_000_000_000_000,
		));

		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::AssetConversion::create_pool(
			<AssetHubRococo as Chain>::RuntimeOrigin::signed(AssetHubRococoSender::get()),
			asset_native.clone(),
			asset_one.clone(),
		));

		assert_expected_events!(
			AssetHubRococo,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { .. }) => {},
			]
		);

		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::AssetConversion::add_liquidity(
			<AssetHubRococo as Chain>::RuntimeOrigin::signed(AssetHubRococoSender::get()),
			asset_native.clone(),
			asset_one.clone(),
			1_000_000_000_000,
			2_000_000_000_000,
			0,
			0,
			AssetHubRococoSender::get().into()
		));

		assert_expected_events!(
			AssetHubRococo,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {lp_token_minted, .. }) => { lp_token_minted: *lp_token_minted == 1414213562273, },
			]
		);

		let path = BoundedVec::<_, _>::truncate_from(vec![asset_native.clone(), asset_one.clone()]);

		assert_ok!(
			<AssetHubRococo as AssetHubRococoPallet>::AssetConversion::swap_exact_tokens_for_tokens(
				<AssetHubRococo as Chain>::RuntimeOrigin::signed(AssetHubRococoSender::get()),
				path,
				100,
				1,
				AssetHubRococoSender::get().into(),
				true
			)
		);

		assert_expected_events!(
			AssetHubRococo,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapExecuted { amount_in, amount_out, .. }) => {
					amount_in: *amount_in == 100,
					amount_out: *amount_out == 199,
				},
			]
		);

		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::AssetConversion::remove_liquidity(
			<AssetHubRococo as Chain>::RuntimeOrigin::signed(AssetHubRococoSender::get()),
			asset_native,
			asset_one,
			1414213562273 - EXISTENTIAL_DEPOSIT * 2, // all but the 2 EDs can't be retrieved.
			0,
			0,
			AssetHubRococoSender::get().into(),
		));
	});
}

#[test]
fn swap_locally_on_chain_using_foreign_assets() {
	use frame_support::weights::WeightToFee;

	let asset_native = Box::new(asset_hub_rococo_runtime::xcm_config::TokenLocation::get());

	let foreign_asset1_at_asset_hub_rococo = Box::new(MultiLocation {
		parents: 1,
		interior: X3(
			Parachain(PenpalRococoA::para_id().into()),
			PalletInstance(ASSETS_PALLET_ID),
			GeneralIndex(ASSET_ID.into()),
		),
	});

	let assets_para_destination: VersionedMultiLocation =
		MultiLocation { parents: 1, interior: X1(Parachain(AssetHubRococo::para_id().into())) }
			.into();

	let penpal_location =
		MultiLocation { parents: 1, interior: X1(Parachain(PenpalRococoA::para_id().into())) };

	// 1. Create asset on penpal:
	PenpalRococoA::execute_with(|| {
		assert_ok!(<PenpalRococoA as PenpalRococoAPallet>::Assets::create(
			<PenpalRococoA as Chain>::RuntimeOrigin::signed(PenpalRococoASender::get()),
			ASSET_ID.into(),
			PenpalRococoASender::get().into(),
			1000,
		));

		assert!(<PenpalRococoA as PenpalRococoAPallet>::Assets::asset_exists(ASSET_ID));
	});

	// 2. Create foreign asset on asset_hub_rococo:

	let require_weight_at_most = Weight::from_parts(1_100_000_000_000, 30_000);
	let origin_kind = OriginKind::Xcm;
	let sov_penpal_on_asset_hub_rococo = AssetHubRococo::sovereign_account_id_of(penpal_location);

	AssetHubRococo::fund_accounts(vec![
		(AssetHubRococoSender::get().into(), 5_000_000 * ROCOCO_ED), /* An account to swap dot
		                                                              * for something else. */
		(sov_penpal_on_asset_hub_rococo.clone().into(), 1000_000_000_000_000_000 * ROCOCO_ED),
	]);

	let sov_penpal_on_asset_hub_rococo_as_location: MultiLocation = MultiLocation {
		parents: 0,
		interior: X1(AccountId32Junction {
			network: None,
			id: sov_penpal_on_asset_hub_rococo.clone().into(),
		}),
	};

	let call_foreign_assets_create =
		<AssetHubRococo as Chain>::RuntimeCall::ForeignAssets(pallet_assets::Call::<
			<AssetHubRococo as Chain>::Runtime,
			Instance2,
		>::create {
			id: *foreign_asset1_at_asset_hub_rococo,
			min_balance: 1000,
			admin: sov_penpal_on_asset_hub_rococo.clone().into(),
		})
		.encode()
		.into();

	let buy_execution_fee_amount = parachains_common::rococo::fee::WeightToFee::weight_to_fee(
		&Weight::from_parts(10_100_000_000_000, 300_000),
	);
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
			beneficiary: sov_penpal_on_asset_hub_rococo_as_location,
		},
	]));

	// Send XCM message from penpal => asset_hub_rococo
	let sudo_penpal_origin = <PenpalRococoA as Chain>::RuntimeOrigin::root();
	PenpalRococoA::execute_with(|| {
		assert_ok!(<PenpalRococoA as PenpalRococoAPallet>::PolkadotXcm::send(
			sudo_penpal_origin.clone(),
			bx!(assets_para_destination.clone()),
			bx!(xcm),
		));

		type RuntimeEvent = <PenpalRococoA as Chain>::RuntimeEvent;

		assert_expected_events!(
			PenpalRococoA,
			vec![
				RuntimeEvent::PolkadotXcm(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	// Receive XCM message in Assets Parachain
	AssetHubRococo::execute_with(|| {
		assert!(<AssetHubRococo as AssetHubRococoPallet>::ForeignAssets::asset_exists(
			*foreign_asset1_at_asset_hub_rococo
		));

		// 3: Mint foreign asset on asset_hub_rococo:
		//
		// (While it might be nice to use batch,
		// currently that's disabled due to safe call filters.)

		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;
		// 3. Mint foreign asset (in reality this should be a teleport or some such)
		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::ForeignAssets::mint(
			<AssetHubRococo as Chain>::RuntimeOrigin::signed(
				sov_penpal_on_asset_hub_rococo.clone().into()
			),
			*foreign_asset1_at_asset_hub_rococo,
			sov_penpal_on_asset_hub_rococo.clone().into(),
			3_000_000_000_000,
		));

		assert_expected_events!(
			AssetHubRococo,
			vec![
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { .. }) => {},
			]
		);

		// 4. Create pool:
		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::AssetConversion::create_pool(
			<AssetHubRococo as Chain>::RuntimeOrigin::signed(AssetHubRococoSender::get()),
			asset_native.clone(),
			foreign_asset1_at_asset_hub_rococo.clone(),
		));

		assert_expected_events!(
			AssetHubRococo,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { .. }) => {},
			]
		);

		// 5. Add liquidity:
		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::AssetConversion::add_liquidity(
			<AssetHubRococo as Chain>::RuntimeOrigin::signed(
				sov_penpal_on_asset_hub_rococo.clone()
			),
			asset_native.clone(),
			foreign_asset1_at_asset_hub_rococo.clone(),
			1_000_000_000_000,
			2_000_000_000_000,
			0,
			0,
			sov_penpal_on_asset_hub_rococo.clone().into()
		));

		assert_expected_events!(
			AssetHubRococo,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded {lp_token_minted, .. }) => {
					lp_token_minted: *lp_token_minted == 1414213562273,
				},
			]
		);

		// 6. Swap!
		let path = BoundedVec::<_, _>::truncate_from(vec![
			asset_native.clone(),
			foreign_asset1_at_asset_hub_rococo.clone(),
		]);

		assert_ok!(
			<AssetHubRococo as AssetHubRococoPallet>::AssetConversion::swap_exact_tokens_for_tokens(
				<AssetHubRococo as Chain>::RuntimeOrigin::signed(AssetHubRococoSender::get()),
				path,
				100000,
				1000,
				AssetHubRococoSender::get().into(),
				true
			)
		);

		assert_expected_events!(
			AssetHubRococo,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapExecuted { amount_in, amount_out, .. },) => {
					amount_in: *amount_in == 100000,
					amount_out: *amount_out == 199399,
				},
			]
		);

		// 7. Remove liquidity
		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::AssetConversion::remove_liquidity(
			<AssetHubRococo as Chain>::RuntimeOrigin::signed(
				sov_penpal_on_asset_hub_rococo.clone()
			),
			asset_native,
			foreign_asset1_at_asset_hub_rococo,
			1414213562273 - 2_000_000_000, // all but the 2 EDs can't be retrieved.
			0,
			0,
			sov_penpal_on_asset_hub_rococo.clone().into(),
		));
	});
}

#[test]
fn cannot_create_pool_from_pool_assets() {
	let asset_native = Box::new(asset_hub_rococo_runtime::xcm_config::TokenLocation::get());
	let mut asset_one = asset_hub_rococo_runtime::xcm_config::PoolAssetsPalletLocation::get();
	asset_one.append_with(GeneralIndex(ASSET_ID.into())).expect("pool assets");

	AssetHubRococo::execute_with(|| {
		let pool_owner_account_id = asset_hub_rococo_runtime::AssetConversionOrigin::get();

		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::PoolAssets::create(
			<AssetHubRococo as Chain>::RuntimeOrigin::signed(pool_owner_account_id.clone()),
			ASSET_ID.into(),
			pool_owner_account_id.clone().into(),
			1000,
		));
		assert!(<AssetHubRococo as AssetHubRococoPallet>::PoolAssets::asset_exists(ASSET_ID));

		assert_ok!(<AssetHubRococo as AssetHubRococoPallet>::PoolAssets::mint(
			<AssetHubRococo as Chain>::RuntimeOrigin::signed(pool_owner_account_id),
			ASSET_ID.into(),
			AssetHubRococoSender::get().into(),
			3_000_000_000_000,
		));

		assert_matches::assert_matches!(
			<AssetHubRococo as AssetHubRococoPallet>::AssetConversion::create_pool(
				<AssetHubRococo as Chain>::RuntimeOrigin::signed(AssetHubRococoSender::get()),
				asset_native.clone(),
				Box::new(asset_one),
			),
			Err(DispatchError::Module(ModuleError{index: _, error: _, message})) => assert_eq!(message, Some("UnsupportedAsset"))
		);
	});
}
