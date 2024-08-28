// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Asset Conversion Ops pallet tests.

use crate::{mock::*, *};
use frame_support::{
	assert_noop, assert_ok,
	traits::{
		fungible::{Inspect as FungibleInspect, NativeOrWithId},
		fungibles::{Create, Inspect},
		Incrementable,
	},
};

#[test]
fn migrate_pool_account_id_with_native() {
	new_test_ext().execute_with(|| {
		type PoolLocator = <Test as pallet_asset_conversion::Config>::PoolLocator;
		let user = 1;
		let token_1 = NativeOrWithId::Native;
		let token_2 = NativeOrWithId::WithId(2);
		let pool_id = PoolLocator::pool_id(&token_1, &token_2).unwrap();
		let lp_token =
			<Test as pallet_asset_conversion::Config>::PoolAssetId::initial_value().unwrap();

		// setup pool and provide some liquidity.
		assert_ok!(NativeAndAssets::create(token_2.clone(), user, false, 1));

		assert_ok!(AssetConversion::create_pool(
			RuntimeOrigin::signed(user),
			Box::new(token_1.clone()),
			Box::new(token_2.clone())
		));

		let ed = Balances::minimum_balance();
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), user, 10000 * 2 + ed));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(user), 2, user, 1000));

		assert_ok!(AssetConversion::add_liquidity(
			RuntimeOrigin::signed(user),
			Box::new(token_1.clone()),
			Box::new(token_2.clone()),
			10000,
			10,
			10000,
			10,
			user,
		));

		// assert user's balance.
		assert_eq!(NativeAndAssets::balance(token_1.clone(), &user), 10000 + ed);
		assert_eq!(NativeAndAssets::balance(token_2.clone(), &user), 1000 - 10);
		assert_eq!(PoolAssets::balance(lp_token, &user), 216);

		// record total issuances before migration.
		let total_issuance_token1 = NativeAndAssets::total_issuance(token_1.clone());
		let total_issuance_token2 = NativeAndAssets::total_issuance(token_2.clone());
		let total_issuance_lp_token = PoolAssets::total_issuance(lp_token);

		let pool_account = PoolLocator::address(&pool_id).unwrap();
		let (prior_pool_account, new_pool_account) =
			AssetConversionOps::addresses(&pool_id).unwrap();
		assert_eq!(pool_account, prior_pool_account);

		// assert pool's balances before migration.
		assert_eq!(NativeAndAssets::balance(token_1.clone(), &prior_pool_account), 10000);
		assert_eq!(NativeAndAssets::balance(token_2.clone(), &prior_pool_account), 10);
		assert_eq!(PoolAssets::balance(lp_token, &prior_pool_account), 100);

		// migrate.
		assert_ok!(AssetConversionOps::migrate_to_new_account(
			RuntimeOrigin::signed(user),
			Box::new(token_1.clone()),
			Box::new(token_2.clone()),
		));

		// assert user's balance has not changed.
		assert_eq!(NativeAndAssets::balance(token_1.clone(), &user), 10000 + ed);
		assert_eq!(NativeAndAssets::balance(token_2.clone(), &user), 1000 - 10);
		assert_eq!(PoolAssets::balance(lp_token, &user), 216);

		// assert pool's balance on new account id is same as on prior account id.
		assert_eq!(NativeAndAssets::balance(token_1.clone(), &new_pool_account), 10000);
		assert_eq!(NativeAndAssets::balance(token_2.clone(), &new_pool_account), 10);
		assert_eq!(PoolAssets::balance(lp_token, &new_pool_account), 100);

		// assert pool's balance on prior account id is zero.
		assert_eq!(NativeAndAssets::balance(token_1.clone(), &prior_pool_account), 0);
		assert_eq!(NativeAndAssets::balance(token_2.clone(), &prior_pool_account), 0);
		assert_eq!(PoolAssets::balance(lp_token, &prior_pool_account), 0);

		// assert total issuance has not changed.
		assert_eq!(total_issuance_token1, NativeAndAssets::total_issuance(token_1));
		assert_eq!(total_issuance_token2, NativeAndAssets::total_issuance(token_2));
		assert_eq!(total_issuance_lp_token, PoolAssets::total_issuance(lp_token));
	});
}

#[test]
fn migrate_pool_account_id_with_insufficient_assets() {
	new_test_ext().execute_with(|| {
		type PoolLocator = <Test as pallet_asset_conversion::Config>::PoolLocator;
		let user = 1;
		let token_1 = NativeOrWithId::WithId(1);
		let token_2 = NativeOrWithId::WithId(2);
		let pool_id = PoolLocator::pool_id(&token_1, &token_2).unwrap();
		let lp_token =
			<Test as pallet_asset_conversion::Config>::PoolAssetId::initial_value().unwrap();

		// setup pool and provide some liquidity.
		assert_ok!(NativeAndAssets::create(token_1.clone(), user, false, 1));
		assert_ok!(NativeAndAssets::create(token_2.clone(), user, false, 1));

		assert_ok!(AssetConversion::create_pool(
			RuntimeOrigin::signed(user),
			Box::new(token_1.clone()),
			Box::new(token_2.clone())
		));

		assert_ok!(Assets::mint(RuntimeOrigin::signed(user), 1, user, 20000));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(user), 2, user, 1000));

		assert_ok!(AssetConversion::add_liquidity(
			RuntimeOrigin::signed(user),
			Box::new(token_1.clone()),
			Box::new(token_2.clone()),
			10000,
			10,
			10000,
			10,
			user,
		));

		// assert user's balance.
		assert_eq!(NativeAndAssets::balance(token_1.clone(), &user), 10000);
		assert_eq!(NativeAndAssets::balance(token_2.clone(), &user), 1000 - 10);
		assert_eq!(PoolAssets::balance(lp_token, &user), 216);

		// record total issuances before migration.
		let total_issuance_token1 = NativeAndAssets::total_issuance(token_1.clone());
		let total_issuance_token2 = NativeAndAssets::total_issuance(token_2.clone());
		let total_issuance_lp_token = PoolAssets::total_issuance(lp_token);

		let pool_account = PoolLocator::address(&pool_id).unwrap();
		let (prior_pool_account, new_pool_account) =
			AssetConversionOps::addresses(&pool_id).unwrap();
		assert_eq!(pool_account, prior_pool_account);

		// assert pool's balances before migration.
		assert_eq!(NativeAndAssets::balance(token_1.clone(), &prior_pool_account), 10000);
		assert_eq!(NativeAndAssets::balance(token_2.clone(), &prior_pool_account), 10);
		assert_eq!(PoolAssets::balance(lp_token, &prior_pool_account), 100);

		// migrate.
		assert_ok!(AssetConversionOps::migrate_to_new_account(
			RuntimeOrigin::signed(user),
			Box::new(token_1.clone()),
			Box::new(token_2.clone()),
		));

		// assert user's balance has not changed.
		assert_eq!(NativeAndAssets::balance(token_1.clone(), &user), 10000);
		assert_eq!(NativeAndAssets::balance(token_2.clone(), &user), 1000 - 10);
		assert_eq!(PoolAssets::balance(lp_token, &user), 216);

		// assert pool's balance on new account id is same as on prior account id.
		assert_eq!(NativeAndAssets::balance(token_1.clone(), &new_pool_account), 10000);
		assert_eq!(NativeAndAssets::balance(token_2.clone(), &new_pool_account), 10);
		assert_eq!(PoolAssets::balance(lp_token, &new_pool_account), 100);

		// assert pool's balance on prior account id is zero.
		assert_eq!(NativeAndAssets::balance(token_1.clone(), &prior_pool_account), 0);
		assert_eq!(NativeAndAssets::balance(token_2.clone(), &prior_pool_account), 0);
		assert_eq!(PoolAssets::balance(lp_token, &prior_pool_account), 0);

		// assert total issuance has not changed.
		assert_eq!(total_issuance_token1, NativeAndAssets::total_issuance(token_1));
		assert_eq!(total_issuance_token2, NativeAndAssets::total_issuance(token_2));
		assert_eq!(total_issuance_lp_token, PoolAssets::total_issuance(lp_token));
	});
}

#[test]
fn migrate_pool_account_id_with_sufficient_assets() {
	new_test_ext().execute_with(|| {
		type PoolLocator = <Test as pallet_asset_conversion::Config>::PoolLocator;
		let user = 1;
		let token_1 = NativeOrWithId::WithId(1);
		let token_2 = NativeOrWithId::WithId(2);
		let pool_id = PoolLocator::pool_id(&token_1, &token_2).unwrap();
		let lp_token =
			<Test as pallet_asset_conversion::Config>::PoolAssetId::initial_value().unwrap();

		// setup pool and provide some liquidity.
		assert_ok!(NativeAndAssets::create(token_1.clone(), user, true, 1));
		assert_ok!(NativeAndAssets::create(token_2.clone(), user, true, 1));

		assert_ok!(AssetConversion::create_pool(
			RuntimeOrigin::signed(user),
			Box::new(token_1.clone()),
			Box::new(token_2.clone())
		));

		assert_ok!(Assets::mint(RuntimeOrigin::signed(user), 1, user, 20000));
		assert_ok!(Assets::mint(RuntimeOrigin::signed(user), 2, user, 1000));

		assert_ok!(AssetConversion::add_liquidity(
			RuntimeOrigin::signed(user),
			Box::new(token_1.clone()),
			Box::new(token_2.clone()),
			10000,
			10,
			10000,
			10,
			user,
		));

		// assert user's balance.
		assert_eq!(NativeAndAssets::balance(token_1.clone(), &user), 10000);
		assert_eq!(NativeAndAssets::balance(token_2.clone(), &user), 1000 - 10);
		assert_eq!(PoolAssets::balance(lp_token, &user), 216);

		// record total issuances before migration.
		let total_issuance_token1 = NativeAndAssets::total_issuance(token_1.clone());
		let total_issuance_token2 = NativeAndAssets::total_issuance(token_2.clone());
		let total_issuance_lp_token = PoolAssets::total_issuance(lp_token);

		let pool_account = PoolLocator::address(&pool_id).unwrap();
		let (prior_pool_account, new_pool_account) =
			AssetConversionOps::addresses(&pool_id).unwrap();
		assert_eq!(pool_account, prior_pool_account);

		// assert pool's balances before migration.
		assert_eq!(NativeAndAssets::balance(token_1.clone(), &prior_pool_account), 10000);
		assert_eq!(NativeAndAssets::balance(token_2.clone(), &prior_pool_account), 10);
		assert_eq!(PoolAssets::balance(lp_token, &prior_pool_account), 100);

		// migrate.
		assert_ok!(AssetConversionOps::migrate_to_new_account(
			RuntimeOrigin::signed(user),
			Box::new(token_1.clone()),
			Box::new(token_2.clone()),
		));

		// assert user's balance has not changed.
		assert_eq!(NativeAndAssets::balance(token_1.clone(), &user), 10000);
		assert_eq!(NativeAndAssets::balance(token_2.clone(), &user), 1000 - 10);
		assert_eq!(PoolAssets::balance(lp_token, &user), 216);

		// assert pool's balance on new account id is same as on prior account id.
		assert_eq!(NativeAndAssets::balance(token_1.clone(), &new_pool_account), 10000);
		assert_eq!(NativeAndAssets::balance(token_2.clone(), &new_pool_account), 10);
		assert_eq!(PoolAssets::balance(lp_token, &new_pool_account), 100);

		// assert pool's balance on prior account id is zero.
		assert_eq!(NativeAndAssets::balance(token_1.clone(), &prior_pool_account), 0);
		assert_eq!(NativeAndAssets::balance(token_2.clone(), &prior_pool_account), 0);
		assert_eq!(PoolAssets::balance(lp_token, &prior_pool_account), 0);

		// assert total issuance has not changed.
		assert_eq!(total_issuance_token1, NativeAndAssets::total_issuance(token_1));
		assert_eq!(total_issuance_token2, NativeAndAssets::total_issuance(token_2));
		assert_eq!(total_issuance_lp_token, PoolAssets::total_issuance(lp_token));
	});
}

#[test]
fn migrate_empty_pool_account_id() {
	new_test_ext().execute_with(|| {
		let user = 1;
		let token_1 = NativeOrWithId::Native;
		let token_2 = NativeOrWithId::WithId(2);

		// setup pool and provide some liquidity.
		assert_ok!(NativeAndAssets::create(token_2.clone(), user, false, 1));

		assert_ok!(AssetConversion::create_pool(
			RuntimeOrigin::signed(user),
			Box::new(token_1.clone()),
			Box::new(token_2.clone())
		));

		// migrate.
		assert_noop!(
			AssetConversionOps::migrate_to_new_account(
				RuntimeOrigin::signed(user),
				Box::new(token_1.clone()),
				Box::new(token_2.clone()),
			),
			Error::<Test>::ZeroBalance
		);
	});
}
