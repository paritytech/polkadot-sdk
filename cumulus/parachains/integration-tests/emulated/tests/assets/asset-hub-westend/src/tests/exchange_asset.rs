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

use crate::{
	create_pool_with_wnd_on,
	imports::{
		asset_hub_westend_runtime::{ExistentialDeposit, Runtime},
		*,
	},
};
use asset_hub_westend_runtime::{
	xcm_config::WestendLocation, Balances, ForeignAssets, PolkadotXcm, RuntimeOrigin,
};
use emulated_integration_tests_common::{accounts::ALICE, xcm_emulator::TestExt};
use frame_support::{
	assert_err_ignore_postinfo, assert_ok,
	dispatch::DispatchResultWithPostInfo,
	traits::fungible::{Inspect, Mutate},
};
use parachains_common::{AccountId, Balance};
use std::convert::Into;
use xcm::latest::{Assets, Location, Xcm};

const UNITS: Balance = 1_000_000_000;

#[test]
fn exchange_asset_success() {
	let (alice, native_asset_id, origin, asset_location, asset_id) = setup_pool(true);

	AssetHubWestend::execute_with(|| {
		let give: Assets = (native_asset_id, 500 * UNITS).into();
		let want: Assets = (asset_id, 660 * UNITS).into();

		let foreign_balance_before = ForeignAssets::balance(asset_location.clone(), &alice);
		let wnd_balance_before = Balances::total_balance(&alice);

		assert_ok!(swap(alice.clone(), origin, give.clone(), want.clone()));

		let foreign_balance_after = ForeignAssets::balance(asset_location, &alice);
		assert!(
			foreign_balance_after >= foreign_balance_before + 660 * UNITS,
			"Expected foreign balance to increase by at least 660 units, got {foreign_balance_after} from {foreign_balance_before}"
		);

		let wnd_balance_after = Balances::total_balance(&alice);
		assert_eq!(
			wnd_balance_after, wnd_balance_before - 500 * UNITS,
			"Expected WND balance to decrease by exactly 500 units, got {wnd_balance_after} from {wnd_balance_before}"
		);
	});
}

#[test]
fn exchange_asset_insufficient_liquidity() {
	let (alice, native_asset_id, origin, asset_location, asset_id) = setup_pool(true);

	AssetHubWestend::execute_with(|| {
		let give: Assets = (native_asset_id, 500 * UNITS).into();
		let want: Assets = (asset_id, 1_000 * UNITS).into();

		let foreign_balance_before = ForeignAssets::balance(asset_location.clone(), &alice);
		let wnd_balance_before = Balances::total_balance(&alice);

		let result = swap(alice.clone(), origin, give.clone(), want.clone());
		assert_err_ignore_postinfo!(result, pallet_xcm::Error::<Runtime>::LocalExecutionIncomplete);

		let foreign_balance_after = ForeignAssets::balance(asset_location, &alice);
		assert_eq!(
			foreign_balance_after, foreign_balance_before,
			"Expected foreign balance to remain unchanged"
		);

		let wnd_balance_after = Balances::total_balance(&alice);
		assert_eq!(wnd_balance_after, wnd_balance_before, "Expected WND balance to remain unchanged");
	});
}

#[test]
fn exchange_asset_insufficient_balance() {
	let (alice, native_asset_id, origin, asset_location, asset_id) = setup_pool(true);

	AssetHubWestend::execute_with(|| {
		let foreign_balance_before = ForeignAssets::balance(asset_location.clone(), &alice);
		let wnd_balance_before = Balances::total_balance(&alice);

		let give: Assets = (native_asset_id, wnd_balance_before + 1 * UNITS).into();
		let want: Assets = (asset_id, foreign_balance_before + 1 * UNITS).into();

		let result = swap(alice.clone(), origin, give.clone(), want.clone());
		assert_err_ignore_postinfo!(
            result,
            pallet_xcm::Error::<Runtime>::LocalExecutionIncomplete
        );

		let foreign_balance_after = ForeignAssets::balance(asset_location, &alice);
		assert_eq!(
			foreign_balance_after, foreign_balance_before,
			"Expected foreign balance to remain unchanged"
		);

		let wnd_balance_after = Balances::total_balance(&alice);
		assert_eq!(
			wnd_balance_after, wnd_balance_before,
			"Expected WND balance to remain unchanged"
		);
	});
}

#[test]
fn exchange_asset_pool_not_created() {
	let (alice, native_asset_id, origin, asset_location, asset_id) = setup_pool(false);

	AssetHubWestend::execute_with(|| {
		let give: Assets = (native_asset_id, 500 * UNITS).into();
		let want: Assets = (asset_id, 660 * UNITS).into();

		let foreign_balance_before = ForeignAssets::balance(asset_location.clone(), &alice);
		let wnd_balance_before = Balances::total_balance(&alice);

		let result = swap(alice.clone(), origin, give.clone(), want.clone());
		assert_err_ignore_postinfo!(
            result,
            pallet_xcm::Error::<Runtime>::LocalExecutionIncomplete
        );

		let foreign_balance_after = ForeignAssets::balance(asset_location, &alice);
		assert_eq!(
			foreign_balance_after, foreign_balance_before,
			"Expected foreign balance to remain unchanged"
		);

		let wnd_balance_after = Balances::total_balance(&alice);
		assert_eq!(
			wnd_balance_after, wnd_balance_before,
			"Expected WND balance to remain unchanged"
		);
	});
}

fn swap(
	alice: AccountId,
	origin: RuntimeOrigin,
	give: Assets,
	want: Assets,
) -> DispatchResultWithPostInfo {
	let xcm = Xcm(vec![
		WithdrawAsset(give.clone().into()),
		ExchangeAsset { give: give.into(), want: want.into(), maximal: true },
		DepositAsset { assets: Wild(All), beneficiary: alice.into() },
	]);

	PolkadotXcm::execute(origin, bx!(xcm::VersionedXcm::from(xcm)), Weight::MAX)
}

fn setup_pool(create_pool: bool) -> (AccountId, AssetId, RuntimeOrigin, Location, AssetId) {
	let alice: AccountId = Westend::account_id_of(ALICE);
	let native_asset_location = WestendLocation::get();
	let native_asset_id = AssetId(native_asset_location.clone());
	let origin = RuntimeOrigin::signed(alice.clone());
	let asset_location = Location::new(1, [Parachain(2001)]);
	let asset_id = AssetId(asset_location.clone());

	AssetHubWestend::execute_with(|| {
		assert_ok!(<Balances as Mutate<_>>::mint_into(
			&alice,
			ExistentialDeposit::get() + (1_000 * UNITS)
		));

		assert_ok!(ForeignAssets::force_create(
			RuntimeOrigin::root(),
			asset_location.clone().into(),
			alice.clone().into(),
			true,
			1
		));
	});

	if create_pool {
		create_pool_with_wnd_on!(AssetHubWestend, asset_location.clone(), true, alice.clone());
	}

	(alice, native_asset_id, origin, asset_location, asset_id)
}
