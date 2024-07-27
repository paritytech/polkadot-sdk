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

use crate::imports::*;

/// Relay Chain should be able to execute `Transact` instructions in System Parachain
/// when `OriginKind::Superuser`.
#[test]
fn send_transact_as_superuser_from_relay_to_system_para_works() {
	AssetHubRococo::force_create_asset_from_relay_as_root(
		ASSET_ID,
		ASSET_MIN_BALANCE,
		true,
		AssetHubRococoSender::get().into(),
		Some(Weight::from_parts(1_019_445_000, 200_000)),
	)
}

/// We tests two things here:
/// - Parachain should be able to send XCM paying its fee with system asset in the System Parachain
/// - Parachain should be able to create a new Foreign Asset in the System Parachain
#[test]
fn send_xcm_from_para_to_system_para_paying_fee_with_system_assets_works() {
	let para_sovereign_account = AssetHubRococo::sovereign_account_id_of(
		AssetHubRococo::sibling_location_of(PenpalA::para_id()),
	);
	let asset_location_on_penpal = v3::Location::new(
		0,
		[
			v3::Junction::PalletInstance(ASSETS_PALLET_ID),
			v3::Junction::GeneralIndex(ASSET_ID.into()),
		],
	);
	let foreign_asset_at_asset_hub =
		v3::Location::new(1, [v3::Junction::Parachain(PenpalA::para_id().into())])
			.appended_with(asset_location_on_penpal)
			.unwrap();

	// Encoded `create_asset` call to be executed in AssetHub
	let call = AssetHubRococo::create_foreign_asset_call(
		foreign_asset_at_asset_hub,
		ASSET_MIN_BALANCE,
		para_sovereign_account.clone(),
	);

	let origin_kind = OriginKind::Xcm;
	let fee_amount = ASSET_HUB_ROCOCO_ED * 1000000;
	let system_asset = (Parent, fee_amount).into();

	let root_origin = <PenpalA as Chain>::RuntimeOrigin::root();
	let system_para_destination = PenpalA::sibling_location_of(AssetHubRococo::para_id()).into();
	let xcm = xcm_transact_paid_execution(
		call,
		origin_kind,
		system_asset,
		para_sovereign_account.clone(),
	);

	// SA-of-Penpal-on-AHR needs to have balance to pay for fees and asset creation deposit
	AssetHubRococo::fund_accounts(vec![(
		para_sovereign_account.clone().into(),
		ASSET_HUB_ROCOCO_ED * 10000000000,
	)]);

	PenpalA::execute_with(|| {
		assert_ok!(<PenpalA as PenpalAPallet>::PolkadotXcm::send(
			root_origin,
			bx!(system_para_destination),
			bx!(xcm),
		));

		PenpalA::assert_xcm_pallet_sent();
	});

	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;

		AssetHubRococo::assert_xcmp_queue_success(Some(Weight::from_parts(
			15_594_564_000,
			562_893,
		)));

		assert_expected_events!(
			AssetHubRococo,
			vec![
				// Burned the fee
				RuntimeEvent::Balances(pallet_balances::Event::Burned { who, amount }) => {
					who: *who == para_sovereign_account,
					amount: *amount == fee_amount,
				},
				// Foreign Asset created
				RuntimeEvent::ForeignAssets(pallet_assets::Event::Created { asset_id, creator, owner }) => {
					asset_id: *asset_id == foreign_asset_at_asset_hub,
					creator: *creator == para_sovereign_account.clone(),
					owner: *owner == para_sovereign_account,
				},
			]
		);

		type ForeignAssets = <AssetHubRococo as AssetHubRococoPallet>::ForeignAssets;
		assert!(ForeignAssets::asset_exists(foreign_asset_at_asset_hub));
	});
}

/// We tests two things here:
/// - Parachain should be able to send XCM paying its fee with system assets in the System Parachain
/// - Parachain should be able to create a new Asset in the System Parachain
#[test]
fn send_xcm_from_para_to_system_para_paying_fee_with_assets_works() {
	let para_sovereign_account = AssetHubRococo::sovereign_account_id_of(
		AssetHubRococo::sibling_location_of(PenpalA::para_id()),
	);

	// Force create and mint assets for Parachain's sovereign account
	AssetHubRococo::force_create_and_mint_asset(
		ASSET_ID,
		ASSET_MIN_BALANCE,
		true,
		para_sovereign_account.clone(),
		Some(Weight::from_parts(1_019_445_000, 200_000)),
		ASSET_MIN_BALANCE * 1000000000,
	);

	// Just a different `asset_id`` that does not exist yet
	let new_asset_id = ASSET_ID + 1;

	// Encoded `create_asset` call to be executed in AssetHub
	let call = AssetHubRococo::create_asset_call(
		new_asset_id,
		ASSET_MIN_BALANCE,
		para_sovereign_account.clone(),
	);

	let origin_kind = OriginKind::SovereignAccount;
	let fee_amount = ASSET_MIN_BALANCE * 1000000;
	let asset =
		([PalletInstance(ASSETS_PALLET_ID), GeneralIndex(ASSET_ID.into())], fee_amount).into();

	let root_origin = <PenpalA as Chain>::RuntimeOrigin::root();
	let system_para_destination = PenpalA::sibling_location_of(AssetHubRococo::para_id()).into();
	let xcm = xcm_transact_paid_execution(call, origin_kind, asset, para_sovereign_account.clone());

	// SA-of-Penpal-on-AHR needs to have balance to pay for asset creation deposit
	AssetHubRococo::fund_accounts(vec![(
		para_sovereign_account.clone().into(),
		ASSET_HUB_ROCOCO_ED * 10000000000,
	)]);

	PenpalA::execute_with(|| {
		assert_ok!(<PenpalA as PenpalAPallet>::PolkadotXcm::send(
			root_origin,
			bx!(system_para_destination),
			bx!(xcm),
		));

		PenpalA::assert_xcm_pallet_sent();
	});

	AssetHubRococo::execute_with(|| {
		type RuntimeEvent = <AssetHubRococo as Chain>::RuntimeEvent;

		AssetHubRococo::assert_xcmp_queue_success(Some(Weight::from_parts(
			15_594_564_000,
			562_893,
		)));

		assert_expected_events!(
			AssetHubRococo,
			vec![
				// Burned the fee
				RuntimeEvent::Assets(pallet_assets::Event::Burned { asset_id, owner, balance }) => {
					asset_id: *asset_id == ASSET_ID,
					owner: *owner == para_sovereign_account,
					balance: *balance == fee_amount,
				},
				// Asset created
				RuntimeEvent::Assets(pallet_assets::Event::Created { asset_id, creator, owner }) => {
					asset_id: *asset_id == new_asset_id,
					creator: *creator == para_sovereign_account.clone(),
					owner: *owner == para_sovereign_account,
				},
			]
		);
	});
}
