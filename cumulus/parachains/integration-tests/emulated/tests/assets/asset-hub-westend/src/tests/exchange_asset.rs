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
	traits::fungible::{Inspect, Mutate},
};
use parachains_common::{AccountId, Balance};
use sp_tracing::capture_test_logs;
use std::convert::Into;
use xcm::latest::{Assets, Location, Xcm};

const UNITS: Balance = 1_000_000_000;

#[test]
fn exchange_asset_success() {
	test_exchange_asset(true, 500 * UNITS, 665 * UNITS, true);
}

#[test]
fn exchange_asset_insufficient_liquidity() {
	let log_capture = capture_test_logs!({
		test_exchange_asset(true, 1_000 * UNITS, 2_000 * UNITS, false);
	});
	assert!(log_capture.contains("NoDeal"));
}

#[test]
fn exchange_asset_insufficient_balance() {
	let log_capture = capture_test_logs!({
		test_exchange_asset(true, 5_000 * UNITS, 1_665 * UNITS, false);
	});
	assert!(log_capture.contains("Funds are unavailable"));
}

#[test]
fn exchange_asset_pool_not_created() {
	test_exchange_asset(false, 500 * UNITS, 665 * UNITS, false);
}

fn test_exchange_asset(
	create_pool: bool,
	give_amount: Balance,
	want_amount: Balance,
	should_succeed: bool,
) {
	let alice: AccountId = Westend::account_id_of(ALICE);
	let native_asset_location = WestendLocation::get();
	let native_asset_id = AssetId(native_asset_location.clone());
	let origin = RuntimeOrigin::signed(alice.clone());
	let asset_location = Location::new(1, [Parachain(2001)]);
	let asset_id = AssetId(asset_location.clone());

	// Setup initial state
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

	// Execute and verify swap
	AssetHubWestend::execute_with(|| {
		let foreign_balance_before = ForeignAssets::balance(asset_location.clone(), &alice);
		let wnd_balance_before = Balances::total_balance(&alice);

		let give: Assets = (native_asset_id, give_amount).into();
		let want: Assets = (asset_id, want_amount).into();
		let xcm = Xcm(vec![
			WithdrawAsset(give.clone().into()),
			ExchangeAsset { give: give.into(), want: want.into(), maximal: true },
			DepositAsset { assets: Wild(All), beneficiary: alice.clone().into() },
		]);

		let result = PolkadotXcm::execute(origin, bx!(xcm::VersionedXcm::from(xcm)), Weight::MAX);

		let foreign_balance_after = ForeignAssets::balance(asset_location, &alice);
		let wnd_balance_after = Balances::total_balance(&alice);

		if should_succeed {
			assert_ok!(result);
			assert!(
				foreign_balance_after >= foreign_balance_before + want_amount,
				"Expected foreign balance to increase by at least {want_amount} units, got {foreign_balance_after} from {foreign_balance_before}"
			);
			assert_eq!(
				wnd_balance_after, wnd_balance_before - give_amount,
				"Expected WND balance to decrease by {give_amount} units, got {wnd_balance_after} from {wnd_balance_before}"
			);
		} else {
			assert_err_ignore_postinfo!(
				result,
				pallet_xcm::Error::<Runtime>::LocalExecutionIncomplete
			);
			assert_eq!(
				foreign_balance_after, foreign_balance_before,
				"Foreign balance changed unexpectedly: got {foreign_balance_after}, expected {foreign_balance_before}"
			);
			assert_eq!(
				wnd_balance_after, wnd_balance_before,
				"WND balance changed unexpectedly: got {wnd_balance_after}, expected {wnd_balance_before}"
			);
		}
	});
}
