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

use super::*;
use crate::mock::{
	asset_to_address, new_test_ext, precompile_address, AssetConversion as AssetConversionPallet,
	Assets, NativeAndAssets, RuntimeOrigin, Test,
};
use alloy::primitives::U256;
use frame_support::{
	assert_ok,
	traits::{fungibles::Inspect, tokens::fungible::NativeOrWithId},
};
use pallet_revive::{precompiles::TransactionLimits, ExecConfig};
use sp_runtime::Weight;

/// Convert H160 to alloy Address for use in precompile call encoding.
fn addr(h: H160) -> alloy::primitives::Address {
	alloy::primitives::Address::from(h.0)
}

/// Convert an account id to an alloy Address.
fn account_addr(id: &u64) -> alloy::primitives::Address {
	addr(<Test as pallet_revive::Config>::AddressMapper::to_address(id))
}

const NATIVE: H160 = H160([0u8; 20]);

fn asset1() -> H160 {
	asset_to_address(1)
}

/// Helper: set up asset 1, create a pool (Native <-> Asset1), and add liquidity.
fn setup_pool(provider: u64, native_amount: u64, asset_amount: u64) {
	let asset_id = 1u32;
	let native = NativeOrWithId::Native;
	let token = NativeOrWithId::WithId(asset_id);

	// Create asset.
	assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, provider, true, 1));
	// Mint more than needed: add_liquidity will reserve AssetAccountDeposit when creating
	// the pool's asset account, so the provider needs balance beyond the liquidity amount.
	assert_ok!(
		Assets::mint(RuntimeOrigin::signed(provider), asset_id, provider, asset_amount * 2,)
	);

	// Create pool.
	assert_ok!(AssetConversionPallet::create_pool(
		RuntimeOrigin::signed(provider),
		Box::new(native.clone()),
		Box::new(token.clone()),
	));
	// Add liquidity.
	assert_ok!(AssetConversionPallet::add_liquidity(
		RuntimeOrigin::signed(provider),
		Box::new(native),
		Box::new(token),
		native_amount,
		asset_amount,
		0,
		0,
		provider,
	));
}

/// Helper: call the precompile via `bare_call` and return the result.
fn bare_call(
	caller: u64,
	data: Vec<u8>,
) -> pallet_revive::ContractResult<pallet_revive::ExecReturnValue, u64> {
	pallet_revive::Pallet::<Test>::bare_call(
		RuntimeOrigin::signed(caller),
		precompile_address(),
		0u64.into(),
		TransactionLimits::WeightAndDeposit { weight_limit: Weight::MAX, deposit_limit: u64::MAX },
		data,
		&ExecConfig::new_substrate_tx(),
	)
}

#[test]
fn swap_exact_tokens_for_tokens_works() {
	new_test_ext().execute_with(|| {
		let provider = 1u64;
		let swapper = 2u64;
		let recipient = 3u64;

		setup_pool(provider, 10_000, 10_000);

		// Give swapper some asset1 to swap.
		assert_ok!(Assets::mint(RuntimeOrigin::signed(provider), 1, swapper, 1_000));

		let swapper_asset1_before =
			<NativeAndAssets as Inspect<u64>>::balance(NativeOrWithId::WithId(1), &swapper);
		let recipient_native_before =
			<NativeAndAssets as Inspect<u64>>::balance(NativeOrWithId::Native, &recipient);

		// Swap 100 asset1 -> native, send to recipient.
		let data = IAssetConversion::swapExactTokensForTokensCall {
			path: vec![addr(asset1()), addr(NATIVE)],
			amountIn: U256::from(100),
			amountOutMin: U256::from(1),
			sendTo: account_addr(&recipient),
			keepAlive: false,
		}
		.abi_encode();

		let result = bare_call(swapper, data);
		let return_data = result.result.expect("swap must succeed");
		assert!(!return_data.did_revert(), "swap must not revert");

		let amount_out =
			IAssetConversion::swapExactTokensForTokensCall::abi_decode_returns(&return_data.data)
				.expect("return data must decode");
		assert!(amount_out > U256::ZERO, "must receive some tokens");

		let swapper_asset1_after =
			<NativeAndAssets as Inspect<u64>>::balance(NativeOrWithId::WithId(1), &swapper);
		assert_eq!(
			swapper_asset1_before - swapper_asset1_after,
			100,
			"swapper must spend exactly 100 asset1"
		);

		let recipient_native_after =
			<NativeAndAssets as Inspect<u64>>::balance(NativeOrWithId::Native, &recipient);
		assert!(
			recipient_native_after > recipient_native_before,
			"recipient must receive native tokens"
		);
		assert_eq!(
			U256::from(recipient_native_after - recipient_native_before),
			amount_out,
			"received amount must match return value"
		);
	});
}

#[test]
fn quote_exact_tokens_for_tokens_works() {
	new_test_ext().execute_with(|| {
		let provider = 1u64;

		setup_pool(provider, 10_000, 10_000);

		let data = IAssetConversion::quoteExactTokensForTokensCall {
			asset1: addr(asset1()),
			asset2: addr(NATIVE),
			amount: U256::from(100),
			includeFee: true,
		}
		.abi_encode();

		let result = bare_call(provider, data);
		let return_data = result.result.expect("quote must succeed");
		assert!(!return_data.did_revert(), "quote must not revert");

		let quoted =
			IAssetConversion::quoteExactTokensForTokensCall::abi_decode_returns(&return_data.data)
				.expect("return data must decode");
		assert!(quoted > U256::ZERO, "quoted amount must be positive");

		// With 10000/10000 pool and 0.3% fee, swapping 100 asset1:
		// amount_out = (100 * 997 * 10000) / (10000 * 1000 + 100 * 997) = 98
		assert_eq!(quoted, U256::from(98), "quoted amount must match expected AMM output");
	});
}

#[test]
fn quote_matches_swap() {
	new_test_ext().execute_with(|| {
		let provider = 1u64;
		let swapper = 2u64;

		setup_pool(provider, 10_000, 10_000);
		assert_ok!(Assets::mint(RuntimeOrigin::signed(provider), 1, swapper, 1_000));

		let quote_data = IAssetConversion::quoteExactTokensForTokensCall {
			asset1: addr(asset1()),
			asset2: addr(NATIVE),
			amount: U256::from(100),
			includeFee: true,
		}
		.abi_encode();

		let quote_result = bare_call(provider, quote_data);
		let quoted = IAssetConversion::quoteExactTokensForTokensCall::abi_decode_returns(
			&quote_result.result.unwrap().data,
		)
		.unwrap();

		let swap_data = IAssetConversion::swapExactTokensForTokensCall {
			path: vec![addr(asset1()), addr(NATIVE)],
			amountIn: U256::from(100),
			amountOutMin: U256::from(1),
			sendTo: account_addr(&swapper),
			keepAlive: false,
		}
		.abi_encode();

		let swap_result = bare_call(swapper, swap_data);
		let actual = IAssetConversion::swapExactTokensForTokensCall::abi_decode_returns(
			&swap_result.result.unwrap().data,
		)
		.unwrap();

		assert_eq!(quoted, actual, "quote and swap must return the same amount");
	});
}

#[test]
fn swap_tokens_for_exact_tokens_works() {
	new_test_ext().execute_with(|| {
		let provider = 1u64;
		let swapper = 2u64;

		setup_pool(provider, 10_000, 10_000);

		// Give swapper native balance (already has 1M from genesis) and asset1.
		assert_ok!(Assets::mint(RuntimeOrigin::signed(provider), 1, swapper, 1_000));

		let swapper_native_before =
			<NativeAndAssets as Inspect<u64>>::balance(NativeOrWithId::Native, &swapper);

		// Swap native -> asset1, requesting exactly 50 asset1 output.
		let data = IAssetConversion::swapTokensForExactTokensCall {
			path: vec![addr(NATIVE), addr(asset1())],
			amountOut: U256::from(50),
			amountInMax: U256::from(10_000),
			sendTo: account_addr(&swapper),
			keepAlive: false,
		}
		.abi_encode();

		let result = bare_call(swapper, data);
		let return_data = result.result.expect("swap must succeed");
		assert!(!return_data.did_revert(), "swap must not revert");

		let amount_in =
			IAssetConversion::swapTokensForExactTokensCall::abi_decode_returns(&return_data.data)
				.expect("return data must decode");
		assert!(amount_in > U256::ZERO, "must spend some tokens");

		// Verify recipient got exactly 50 asset1.
		let swapper_asset1_after =
			<NativeAndAssets as Inspect<u64>>::balance(NativeOrWithId::WithId(1), &swapper);
		// swapper had 1000 asset1, should now have 1050.
		assert_eq!(swapper_asset1_after, 1_050, "swapper must receive exactly 50 asset1");

		// Verify native was spent.
		let swapper_native_after =
			<NativeAndAssets as Inspect<u64>>::balance(NativeOrWithId::Native, &swapper);
		assert_eq!(
			U256::from(swapper_native_before - swapper_native_after),
			amount_in,
			"spent native must match return value"
		);
	});
}

#[test]
fn quote_tokens_for_exact_tokens_works() {
	new_test_ext().execute_with(|| {
		let provider = 1u64;

		setup_pool(provider, 10_000, 10_000);

		// Quote: how much native needed to get exactly 100 asset1?
		let data = IAssetConversion::quoteTokensForExactTokensCall {
			asset1: addr(NATIVE),
			asset2: addr(asset1()),
			amount: U256::from(100),
			includeFee: true,
		}
		.abi_encode();

		let result = bare_call(provider, data);
		let return_data = result.result.expect("quote must succeed");
		assert!(!return_data.did_revert(), "quote must not revert");

		let quoted =
			IAssetConversion::quoteTokensForExactTokensCall::abi_decode_returns(&return_data.data)
				.expect("return data must decode");
		assert!(quoted > U256::ZERO, "quoted input amount must be positive");
		// Exact-out requires more input than the exact-in output for the same amount,
		// due to the fee structure. For 100 tokens out from a 10000/10000 pool with 0.3% fee:
		// amount_in = (100 * 1000 * 10000) / ((10000 - 100) * 997) + 1 = 102
		assert_eq!(quoted, U256::from(102), "quoted amount must match expected AMM input");
	});
}

#[test]
fn swap_fails_with_insufficient_output() {
	new_test_ext().execute_with(|| {
		let provider = 1u64;
		let swapper = 2u64;

		setup_pool(provider, 10_000, 10_000);
		assert_ok!(Assets::mint(RuntimeOrigin::signed(provider), 1, swapper, 1_000));

		let data = IAssetConversion::swapExactTokensForTokensCall {
			path: vec![addr(asset1()), addr(NATIVE)],
			amountIn: U256::from(100),
			amountOutMin: U256::from(999_999),
			sendTo: account_addr(&swapper),
			keepAlive: false,
		}
		.abi_encode();

		let result = bare_call(swapper, data);
		let failed =
			result.result.is_err() || result.result.as_ref().map_or(false, |v| v.did_revert());
		assert!(failed, "swap with excessive amountOutMin must fail");
	});
}

#[test]
fn quote_fails_for_nonexistent_pool() {
	new_test_ext().execute_with(|| {
		let caller = 1u64;

		let data = IAssetConversion::quoteExactTokensForTokensCall {
			asset1: addr(asset_to_address(99)),
			asset2: addr(NATIVE),
			amount: U256::from(100),
			includeFee: true,
		}
		.abi_encode();

		let result = bare_call(caller, data);
		let failed =
			result.result.is_err() || result.result.as_ref().map_or(false, |v| v.did_revert());
		assert!(failed, "quote for nonexistent pool must fail");
	});
}
