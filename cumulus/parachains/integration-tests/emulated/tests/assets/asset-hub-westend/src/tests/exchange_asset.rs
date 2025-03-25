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

use crate::{create_pool_with_wnd_on, imports::{asset_hub_westend_runtime::ExistentialDeposit, *}};
use asset_hub_westend_runtime::{
	xcm_config::WestendLocation, Balances, ForeignAssets, PolkadotXcm, RuntimeOrigin,
};
use emulated_integration_tests_common::{accounts::ALICE, xcm_emulator::TestExt};
use frame_support::{
	assert_ok,
	traits::fungible::{Inspect, Mutate},
};
use parachains_common::{AccountId, Balance};
use std::convert::Into;
use xcm::latest::{Assets, Location, Xcm};

const UNITS: Balance = 1_000_000_000;

#[test]
fn exchange_asset_success() {
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
			1,
		));
	});

	create_pool_with_wnd_on!(AssetHubWestend, asset_location.clone(), true, alice.clone());

	AssetHubWestend::execute_with(|| {
		assert_ok!(Balances::mint_into(&alice, 1_000 * UNITS));

		let give: Assets = (native_asset_id, 500 * UNITS).into();
		let want: Assets = (asset_id, 660 * UNITS).into();

		let xcm = Xcm(vec![ExchangeAsset {
			give: give.into(),
			want: want.into(),
			maximal: true,
		}]);

		assert_ok!(PolkadotXcm::execute(
			origin.clone(),
			bx!(xcm::VersionedXcm::from(xcm)),
			Weight::MAX,
		));

		let foreign_balance = ForeignAssets::balance(asset_location, &alice);
		assert!(
			foreign_balance >= 660 * UNITS,
			"Expected at least 660 units, got {foreign_balance}"
		);

		let wnd_balance = Balances::total_balance(&alice);
		assert!(
			wnd_balance < 1_500 * UNITS,
			"Expected WND balance to decrease, got {wnd_balance}"
		);
	});
}
/*
#[test]
fn exchange_asset_insufficient_liquidity() {
	AssetHubWestend::execute_with(|| {
		let alice: AccountId = Westend::account_id_of(ALICE);

		// Setup pool
		create_pool_with_wnd_on!(AssetHubWestend, asset_location, true, alice.clone());

		// Mint extra WND
		assert_ok!(Balances::mint_into(&alice, 3_000 * UNITS));

		// Try swapping more than pool can handle
		let give = Assets::from(vec![Asset {
			id: AssetId(WestendLocation::get()),
			fun: Fungible(2_000 * UNITS),
		}]);
		let want = Assets::from(vec![Asset {
			id: AssetId(asset_location),
			fun: Fungible(3_000 * UNITS),
		}]);

		let xcm = VersionedXcm::V5(Xcm(vec![ExchangeAsset {
			give: give.into(),
			want: want.into(),
			maximal: true,
		}]));

		assert_ok!(PolkadotXcm::execute(
			RuntimeOrigin::signed(alice.clone()),
			Box::new(xcm),
			Weight::from_parts(1_000_000_000, 1024),
		));

		// Expect partial or no swap
		let foreign_balance = foreign_balance_on!(AssetHubWestend, &asset_location, &alice);
		assert!(
			foreign_balance < 3_000 * UNITS,
			"Expected less than 3,000 units due to liquidity, got {}",
			foreign_balance
		);
	});
}

#[test]
fn exchange_asset_insufficient_balance() {
	AssetHubWestend::execute_with(|| {
		let alice: AccountId = Westend::account_id_of(ALICE);

		// Setup pool
		create_pool_with_wnd_on!(AssetHubWestend, asset_location, true, alice.clone());

		// Mint minimal WND (less than 500)
		assert_ok!(Balances::mint_into(&alice, 400 * UNITS));

		let give = Assets::from(vec![Asset {
			id: AssetId(WestendLocation::get()),
			fun: Fungible(500 * UNITS),
		}]);
		let want = Assets::from(vec![Asset {
			id: AssetId(asset_location),
			fun: Fungible(660 * UNITS),
		}]);

		let xcm = VersionedXcm::V5(Xcm(vec![ExchangeAsset {
			give: give.into(),
			want: want.into(),
			maximal: true,
		}]));

		assert!(
			PolkadotXcm::execute(
				RuntimeOrigin::signed(alice.clone()),
				Box::new(xcm),
				Weight::from_parts(1_000_000_000, 1024),
			)
			.is_err(),
			"Expected failure due to insufficient WND balance"
		);
	});
}

#[test]
fn exchange_asset_pool_not_created() {
	AssetHubWestend::execute_with(|| {
		let alice: AccountId = Westend::account_id_of(ALICE);

		// Mint WND and foreign asset, but don’t create pool
		assert_ok!(Balances::mint_into(&alice, 1_000 * UNITS));
		assert_ok!(ForeignAssets::force_create(
			RuntimeOrigin::root(),
			asset_location.into(),
			alice.clone().into(),
			true,
			1,
		));
		assert_ok!(ForeignAssets::mint(
			RuntimeOrigin::signed(alice.clone()),
			asset_location.into(),
			alice.clone().into(),
			2_000 * UNITS,
		));

		let give = Assets::from(vec![Asset {
			id: AssetId(WestendLocation::get()),
			fun: Fungible(500 * UNITS),
		}]);
		let want = Assets::from(vec![Asset {
			id: AssetId(asset_location),
			fun: Fungible(660 * UNITS),
		}]);

		let xcm = VersionedXcm::V5(Xcm(vec![ExchangeAsset {
			give: give.into(),
			want: want.into(),
			maximal: true,
		}]));

		assert!(
			PolkadotXcm::execute(
				RuntimeOrigin::signed(alice.clone()),
				Box::new(xcm),
				Weight::from_parts(1_000_000_000, 1024),
			)
			.is_err(),
			"Expected failure due to missing pool"
		);
	});
}
*/
