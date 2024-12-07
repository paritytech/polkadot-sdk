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
use xcm::prelude::*;
use xcm_executor::{traits::AssetExchange, AssetsInHolding};

// ========== Happy path ==========

/// Scenario:
/// Account #3 wants to use the local liquidity pool between two custom assets,
/// 1 and 2.
#[test]
fn maximal_exchange() {
	new_test_ext().execute_with(|| {
		let assets = PoolAssetsExchanger::exchange_asset(
			None,
			vec![([PalletInstance(2), GeneralIndex(1)], 10_000_000).into()].into(),
			&vec![(Here, 2_000_000).into()].into(),
			true, // Maximal
		)
		.unwrap();
		let amount = get_amount_from_first_fungible(&assets);
		assert_eq!(amount, 4_533_054);
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
		let (first_amount, second_amount) = get_amount_from_fungibles(&assets);
		assert_eq!(first_amount, 2_000_000);
		assert_eq!(second_amount, 5_820_795);
	});
}

#[test]
fn maximal_quote() {
	new_test_ext().execute_with(|| {
		let assets = quote(
			&([PalletInstance(2), GeneralIndex(1)], 10_000_000).into(),
			&(Here, 2_000_000).into(),
			true,
		)
		.unwrap();
		let amount = get_amount_from_first_fungible(&assets.into());
		// The amount of the native token resulting from swapping all `10_000_000` of the custom
		// token.
		assert_eq!(amount, 4_533_054);
	});
}

#[test]
fn minimal_quote() {
	new_test_ext().execute_with(|| {
		let assets = quote(
			&([PalletInstance(2), GeneralIndex(1)], 10_000_000).into(),
			&(Here, 2_000_000).into(),
			false,
		)
		.unwrap();
		let amount = get_amount_from_first_fungible(&assets.into());
		// The amount of the custom token needed to get `2_000_000` of the native token.
		assert_eq!(amount, 4_179_205);
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
fn exchange_fails() {
	new_test_ext().execute_with(|| {
		assert!(PoolAssetsExchanger::exchange_asset(
			None,
			vec![([PalletInstance(2), GeneralIndex(1)], 10_000_000).into()].into(),
			// We're asking for too much of the native token...
			&vec![(Here, 200_000_000).into()].into(),
			false, // Minimal
		)
		.is_err());
	});
}

#[test]
fn non_fungible_asset_in_give() {
	new_test_ext().execute_with(|| {
		assert!(PoolAssetsExchanger::exchange_asset(
			None,
			// Using `u64` here will give us a non-fungible instead of a fungible.
			vec![([PalletInstance(2), GeneralIndex(2)], 10_000_000u64).into()].into(),
			&vec![(Here, 10_000_000).into()].into(),
			false, // Minimal
		)
		.is_err());
	});
}

// ========== Helper functions ==========

fn get_amount_from_first_fungible(assets: &AssetsInHolding) -> u128 {
	let mut fungibles_iter = assets.fungible_assets_iter();
	let first_fungible = fungibles_iter.next().unwrap();
	let Fungible(amount) = first_fungible.fun else {
		unreachable!("Asset should be fungible");
	};
	amount
}

fn get_amount_from_fungibles(assets: &AssetsInHolding) -> (u128, u128) {
	let mut fungibles_iter = assets.fungible_assets_iter();
	let first_fungible = fungibles_iter.next().unwrap();
	let Fungible(first_amount) = first_fungible.fun else {
		unreachable!("Asset should be fungible");
	};
	let second_fungible = fungibles_iter.next().unwrap();
	let Fungible(second_amount) = second_fungible.fun else {
		unreachable!("Asset should be fungible");
	};
	(first_amount, second_amount)
}

fn quote(asset_1: &Asset, asset_2: &Asset, maximal: bool) -> Option<Assets> {
	PoolAssetsExchanger::quote_exchange_price(
		&asset_1.clone().into(),
		&asset_2.clone().into(),
		maximal,
	)
}
