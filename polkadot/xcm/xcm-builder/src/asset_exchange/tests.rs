// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Tests for the [`SingleAssetExchangeAdapter`] type.

use super::mock::*;
use frame_support::{
	assert_ok,
	traits::{
		fungible::NativeOrWithId,
		fungibles::{Inspect, Mutate},
	},
};
use xcm::prelude::*;
use xcm_executor::{traits::AssetExchange, AssetsInHolding};

// ========== Happy path ==========

/// Scenario:
/// Account #3 wants to use the local liquidity pool between two custom assets,
/// 1 and 2.
#[test]
fn maximal_exchange() {
	let _ = env_logger::builder().is_test(true).try_init().unwrap();
	new_test_ext().execute_with(|| {
		let assets = PoolAssetsExchanger::exchange_asset(
			None,
			vec![([PalletInstance(2), GeneralIndex(1)], 10_000_000).into()].into(),
			&vec![(Here, 2_000_000).into()].into(),
			true, // Maximal
		)
		.unwrap();
		let amount = get_amount_from_first_fungible(assets);
		let pool_fee = 6;
		assert_eq!(amount, 50_000_000 - pool_fee);
	});
}

#[test]
fn minimal_exchange() {
	new_test_ext().execute_with(|| {
		let assets = PoolAssetsExchanger::exchange_asset(
			None,
			vec![([PalletInstance(2), GeneralIndex(1)], 10_000_000).into()].into(),
			&vec![(Here, 2_000_000).into()].into(),
			false, // Minimal
		)
		.unwrap();
		let amount = get_amount_from_first_fungible(assets);
		assert_eq!(amount, 2_000_000);
	});
}

#[test]
fn maximal_quote() {
	new_test_ext().execute_with(|| {
		let _amount = quote(
			&([PalletInstance(2), GeneralIndex(1)], 1).into(),
			&(Here, 2_000_000).into(),
			true,
		);
	});
}

#[test]
fn minimal_quote() {
	new_test_ext().execute_with(|| {
		let _amount = quote(
			&([PalletInstance(2), GeneralIndex(1)], 10).into(),
			&(Here, 2_000_000).into(),
			false,
		);
	});
}

// ========== Unhappy path ==========

#[test]
fn no_asset_in_give() {
	new_test_ext().execute_with(|| {
		assert!(PoolAssetsExchanger::exchange_asset(
			None,
			vec![].into(),
			&vec![(Here, 2_000_000).into()].into(),
			true
		)
		.is_err());
	});
}

#[test]
fn more_than_one_asset_in_give() {
	new_test_ext().execute_with(|| {
		assert!(PoolAssetsExchanger::exchange_asset(
			None,
			vec![([PalletInstance(2), GeneralIndex(1)], 1).into(), (Here, 2).into()].into(),
			&vec![(Here, 2_000_000).into()].into(),
			true
		)
		.is_err());
	});
}

#[test]
fn no_asset_in_want() {
	new_test_ext().execute_with(|| {
		assert!(PoolAssetsExchanger::exchange_asset(
			None,
			vec![([PalletInstance(2), GeneralIndex(1)], 10_000_000).into()].into(),
			&vec![].into(),
			true
		)
		.is_err());
	});
}

#[test]
fn more_than_one_asset_in_want() {
	new_test_ext().execute_with(|| {
		assert!(PoolAssetsExchanger::exchange_asset(
			None,
			vec![([PalletInstance(2), GeneralIndex(1)], 10_000_000).into()].into(),
			&vec![(Here, 2_000_000).into(), ([PalletInstance(2), GeneralIndex(1)], 1).into()]
				.into(),
			true
		)
		.is_err());
	});
}

#[test]
fn give_asset_does_not_match() {
	new_test_ext().execute_with(|| {
		let nonexistent_asset_id = 1000;
		assert!(PoolAssetsExchanger::exchange_asset(
			None,
			vec![([PalletInstance(2), GeneralIndex(nonexistent_asset_id)], 10_000_000).into()]
				.into(),
			&vec![(Here, 2_000_000).into()].into(),
			true
		)
		.is_err());
	});
}

#[test]
fn want_asset_does_not_match() {
	new_test_ext().execute_with(|| {
		let nonexistent_asset_id = 1000;
		assert!(PoolAssetsExchanger::exchange_asset(
			None,
			vec![(Here, 2_000_000).into()].into(),
			&vec![([PalletInstance(2), GeneralIndex(nonexistent_asset_id)], 10_000_000).into()]
				.into(),
			true
		)
		.is_err());
	});
}

#[test]
fn exchange_fails() {}

// ========== Helper functions ==========

fn get_amount_from_first_fungible(assets: AssetsInHolding) -> u128 {
	let first_fungible = assets.fungible_assets_iter().next().unwrap();
	let Fungible(amount) = first_fungible.fun else {
		unreachable!("Asset should be fungible");
	};
	amount
}

fn quote(asset_1: &Asset, asset_2: &Asset, maximal: bool) -> Option<u128> {
	PoolAssetsExchanger::quote_exchange_price(asset_1, asset_2, maximal)
}
