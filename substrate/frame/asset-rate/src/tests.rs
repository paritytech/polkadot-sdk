// This file is part of Substrate.

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

//! The crate's tests.

use super::*;
use crate::pallet as pallet_asset_rate;
use frame_support::{assert_noop, assert_ok};
use mock::{new_test_ext, AssetRate, RuntimeOrigin, Test};
use sp_runtime::FixedU128;

const ASSET_ID: u32 = 42;

#[test]
fn create_works() {
	new_test_ext().execute_with(|| {
		assert!(pallet_asset_rate::ConversionRateToNative::<Test>::get(ASSET_ID).is_none());
		assert_ok!(AssetRate::create(
			RuntimeOrigin::root(),
			Box::new(ASSET_ID),
			FixedU128::from_float(0.1)
		));

		assert_eq!(
			pallet_asset_rate::ConversionRateToNative::<Test>::get(ASSET_ID),
			Some(FixedU128::from_float(0.1))
		);
	});
}

#[test]
fn ref_asset_rates() {
	new_test_ext().execute_with(|| {
		const KSM_UNIT: u64 = 1_000_000_000_000;
		const DOT_UNIT: u64 = 10_000_000_000;
		const USD_UNIT: u64 = 1_000_000;
		const FIXED_U128_DECIMAL: u128 = 1_000_000_000_000_000_000;

		// USDt/d to DOT rate
		assert!(pallet_asset_rate::ConversionRateToNative::<Test>::get(1).is_none());
		// current rate on-chain rate: DOT = 10 * USD
		// DOT and USD units have different decimals: DOT=10^10, USD=10^6.
		// To compute the rate (1 USD = X DOT), we must scale appropriately for FixedU128:
		// 1 USD = (DOT_UNIT / (10 * USD_UNIT)) DOT
		// Convert this ratio to FixedU128 by scaling with FIXED_U128_DECIMAL.
		let usd_to_dot_rate = FixedU128::from_inner(
			(DOT_UNIT as u128).saturating_mul(FIXED_U128_DECIMAL) / (10 * USD_UNIT) as u128,
		);

		assert_ok!(AssetRate::create(RuntimeOrigin::root(), Box::new(1), usd_to_dot_rate));

		let conversion_from_asset = <AssetRate as ConversionFromAssetBalance<
			BalanceOf<Test>,
			<Test as pallet_asset_rate::Config>::AssetKind,
			BalanceOf<Test>,
		>>::from_asset_balance(10 * USD_UNIT, 1);
		assert_eq!(conversion_from_asset.expect("Conversion rate exists for asset"), DOT_UNIT);

		// DOT to KSM rate
		assert!(pallet_asset_rate::ConversionRateToNative::<Test>::get(2).is_none());
		// current on-chain rate: KSM = 4 * DOT
		// KSM and DOT units have different decimals: KSM=10^12, DOT=10^10.
		// To compute the rate (1 DOT = X KSM), scale appropriately for FixedU128:
		// 1 DOT = (KSM_UNIT / (4 * DOT_UNIT)) KSM
		// Convert to FixedU128 by scaling with FIXED_U128_DECIMAL.
		let dot_to_ksm_rate = FixedU128::from_inner(
			(KSM_UNIT as u128).saturating_mul(FIXED_U128_DECIMAL) / (4 * DOT_UNIT) as u128,
		);
		assert_ok!(AssetRate::create(RuntimeOrigin::root(), Box::new(2), dot_to_ksm_rate));

		let conversion_from_asset = <AssetRate as ConversionFromAssetBalance<
			BalanceOf<Test>,
			<Test as pallet_asset_rate::Config>::AssetKind,
			BalanceOf<Test>,
		>>::from_asset_balance(4 * DOT_UNIT, 2);
		assert_eq!(conversion_from_asset.expect("Conversion rate exists for asset"), KSM_UNIT);

		use codec::Decode;
		use hex_literal::hex;

		// current encoded value of KSM/DOT rate on Kusama
		// wrong, since it did not account for FixedU128 decimal, the value is 0.000000000000000025
		// should be 25
		let rate_str = hex!("19000000000000000000000000000000");
		let rate = FixedU128::decode(&mut rate_str.as_slice()).expect("Valid rate string");
		assert_eq!(rate, FixedU128::from_float(0.000000000000000025));
		assert_ne!(rate, dot_to_ksm_rate);
		println!("rate: {:?}", rate);

		// current encoded value of DOT/USD rate on Polkadot
		// value is 1000.000000000000000000
		let rate_str = hex!("0000a0dec5adc9353600000000000000");
		let rate = FixedU128::decode(&mut rate_str.as_slice()).expect("Valid rate string");
		assert_eq!(rate, FixedU128::from_float(1000.0));
		assert_eq!(rate, usd_to_dot_rate);
		println!("rate: {:?}", rate);
	});
}

#[test]
fn create_existing_throws() {
	new_test_ext().execute_with(|| {
		assert!(pallet_asset_rate::ConversionRateToNative::<Test>::get(ASSET_ID).is_none());
		assert_ok!(AssetRate::create(
			RuntimeOrigin::root(),
			Box::new(ASSET_ID),
			FixedU128::from_float(0.1)
		));

		assert_noop!(
			AssetRate::create(
				RuntimeOrigin::root(),
				Box::new(ASSET_ID),
				FixedU128::from_float(0.1)
			),
			Error::<Test>::AlreadyExists
		);
	});
}

#[test]
fn remove_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(AssetRate::create(
			RuntimeOrigin::root(),
			Box::new(ASSET_ID),
			FixedU128::from_float(0.1)
		));

		assert_ok!(AssetRate::remove(RuntimeOrigin::root(), Box::new(ASSET_ID),));
		assert!(pallet_asset_rate::ConversionRateToNative::<Test>::get(ASSET_ID).is_none());
	});
}

#[test]
fn remove_unknown_throws() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			AssetRate::remove(RuntimeOrigin::root(), Box::new(ASSET_ID),),
			Error::<Test>::UnknownAssetKind
		);
	});
}

#[test]
fn update_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(AssetRate::create(
			RuntimeOrigin::root(),
			Box::new(ASSET_ID),
			FixedU128::from_float(0.1)
		));
		assert_ok!(AssetRate::update(
			RuntimeOrigin::root(),
			Box::new(ASSET_ID),
			FixedU128::from_float(0.5)
		));

		assert_eq!(
			pallet_asset_rate::ConversionRateToNative::<Test>::get(ASSET_ID),
			Some(FixedU128::from_float(0.5))
		);
	});
}

#[test]
fn update_unknown_throws() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			AssetRate::update(
				RuntimeOrigin::root(),
				Box::new(ASSET_ID),
				FixedU128::from_float(0.5)
			),
			Error::<Test>::UnknownAssetKind
		);
	});
}

#[test]
fn convert_works() {
	new_test_ext().execute_with(|| {
		assert_ok!(AssetRate::create(
			RuntimeOrigin::root(),
			Box::new(ASSET_ID),
			FixedU128::from_float(2.51)
		));

		let conversion_from_asset = <AssetRate as ConversionFromAssetBalance<
			BalanceOf<Test>,
			<Test as pallet_asset_rate::Config>::AssetKind,
			BalanceOf<Test>,
		>>::from_asset_balance(10, ASSET_ID);
		assert_eq!(conversion_from_asset.expect("Conversion rate exists for asset"), 25);

		let conversion_to_asset = <AssetRate as ConversionToAssetBalance<
			BalanceOf<Test>,
			<Test as pallet_asset_rate::Config>::AssetKind,
			BalanceOf<Test>,
		>>::to_asset_balance(25, ASSET_ID);
		assert_eq!(conversion_to_asset.expect("Conversion rate exists for asset"), 9);
	});
}

#[test]
fn convert_unknown_throws() {
	new_test_ext().execute_with(|| {
		let conversion = <AssetRate as ConversionFromAssetBalance<
			BalanceOf<Test>,
			<Test as pallet_asset_rate::Config>::AssetKind,
			BalanceOf<Test>,
		>>::from_asset_balance(10, ASSET_ID);
		assert!(conversion.is_err());
	});
}

#[test]
fn convert_overflow_throws() {
	new_test_ext().execute_with(|| {
		assert_ok!(AssetRate::create(
			RuntimeOrigin::root(),
			Box::new(ASSET_ID),
			FixedU128::from_u32(0)
		));

		let conversion = <AssetRate as ConversionToAssetBalance<
			BalanceOf<Test>,
			<Test as pallet_asset_rate::Config>::AssetKind,
			BalanceOf<Test>,
		>>::to_asset_balance(10, ASSET_ID);
		assert!(conversion.is_err());
	});
}
