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
	new_test_ext, precompile_address, AssetConversion as AssetConversionPallet, Assets,
	NativeAndAssets, RuntimeOrigin, Test,
};
use alloy::primitives::U256;
use codec::Encode;
use frame_support::{
	assert_ok,
	traits::{fungibles::Inspect, tokens::fungible::NativeOrWithId},
};
use pallet_revive::{
	precompiles::{alloy::sol_types::SolCall, TransactionLimits},
	AddressMapper, Code, ExecConfig,
};
use sp_runtime::Weight;

/// SCALE-encode asset kinds for use in precompile calls.
fn encode_native() -> Vec<u8> {
	NativeOrWithId::<u32>::Native.encode()
}

fn encode_asset(id: u32) -> Vec<u8> {
	NativeOrWithId::<u32>::WithId(id).encode()
}

/// Convert an account id to an alloy Address.
fn account_addr(id: &u64) -> alloy::primitives::Address {
	let h160 = <Test as pallet_revive::Config>::AddressMapper::to_address(id);
	alloy::primitives::Address::from(h160.0)
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
		assert_ok!(Assets::mint(RuntimeOrigin::signed(provider), 1, swapper, 1_000));

		let swapper_asset1_before =
			<NativeAndAssets as Inspect<u64>>::balance(NativeOrWithId::WithId(1), &swapper);
		let recipient_native_before =
			<NativeAndAssets as Inspect<u64>>::balance(NativeOrWithId::Native, &recipient);

		let data = IAssetConversion::swapExactTokensForTokensCall {
			path: vec![encode_asset(1).into(), encode_native().into()],
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
		assert_eq!(swapper_asset1_before - swapper_asset1_after, 100);

		let recipient_native_after =
			<NativeAndAssets as Inspect<u64>>::balance(NativeOrWithId::Native, &recipient);
		assert_eq!(
			U256::from(recipient_native_after - recipient_native_before),
			amount_out,
			"received amount must match return value"
		);
	});
}

#[test]
fn swap_tokens_for_exact_tokens_works() {
	new_test_ext().execute_with(|| {
		let provider = 1u64;
		let swapper = 2u64;

		setup_pool(provider, 10_000, 10_000);
		assert_ok!(Assets::mint(RuntimeOrigin::signed(provider), 1, swapper, 1_000));

		let swapper_native_before =
			<NativeAndAssets as Inspect<u64>>::balance(NativeOrWithId::Native, &swapper);

		// Swap native -> asset1, requesting exactly 50 asset1 output.
		let data = IAssetConversion::swapTokensForExactTokensCall {
			path: vec![encode_native().into(), encode_asset(1).into()],
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
		assert_eq!(swapper_asset1_after, 1_050, "swapper must receive exactly 50 asset1");

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
fn quote_exact_tokens_for_tokens_works() {
	new_test_ext().execute_with(|| {
		let provider = 1u64;

		setup_pool(provider, 10_000, 10_000);

		let data = IAssetConversion::quoteExactTokensForTokensCall {
			asset1: encode_asset(1).into(),
			asset2: encode_native().into(),
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

		// With 10000/10000 pool and 0.3% fee, swapping 100 asset1:
		// amount_out = (100 * 997 * 10000) / (10000 * 1000 + 100 * 997) = 98
		assert_eq!(quoted, U256::from(98), "quoted amount must match expected AMM output");
	});
}

#[test]
fn quote_tokens_for_exact_tokens_works() {
	new_test_ext().execute_with(|| {
		let provider = 1u64;

		setup_pool(provider, 10_000, 10_000);

		let data = IAssetConversion::quoteTokensForExactTokensCall {
			asset1: encode_native().into(),
			asset2: encode_asset(1).into(),
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
		// For 100 tokens out from a 10000/10000 pool with 0.3% fee:
		// amount_in = (100 * 1000 * 10000) / ((10000 - 100) * 997) + 1 = 102
		assert_eq!(quoted, U256::from(102), "quoted amount must match expected AMM input");
	});
}

#[test]
fn quote_matches_swap() {
	new_test_ext().execute_with(|| {
		let provider = 1u64;
		let swapper = 2u64;

		setup_pool(provider, 10_000, 10_000);
		assert_ok!(Assets::mint(RuntimeOrigin::signed(provider), 1, swapper, 1_000));

		// Get quote.
		let quote_data = IAssetConversion::quoteExactTokensForTokensCall {
			asset1: encode_asset(1).into(),
			asset2: encode_native().into(),
			amount: U256::from(100),
			includeFee: true,
		}
		.abi_encode();

		let quote_result = bare_call(provider, quote_data);
		let quoted = IAssetConversion::quoteExactTokensForTokensCall::abi_decode_returns(
			&quote_result.result.unwrap().data,
		)
		.unwrap();

		// Do the actual swap.
		let swap_data = IAssetConversion::swapExactTokensForTokensCall {
			path: vec![encode_asset(1).into(), encode_native().into()],
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
fn swap_fails_with_insufficient_output() {
	new_test_ext().execute_with(|| {
		let provider = 1u64;
		let swapper = 2u64;

		setup_pool(provider, 10_000, 10_000);
		assert_ok!(Assets::mint(RuntimeOrigin::signed(provider), 1, swapper, 1_000));

		let data = IAssetConversion::swapExactTokensForTokensCall {
			path: vec![encode_asset(1).into(), encode_native().into()],
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
			asset1: encode_asset(99).into(),
			asset2: encode_native().into(),
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

#[test]
fn quote_fails_with_invalid_encoding() {
	new_test_ext().execute_with(|| {
		let caller = 1u64;

		setup_pool(caller, 10_000, 10_000);

		let data = IAssetConversion::quoteExactTokensForTokensCall {
			asset1: alloy::primitives::Bytes::from(vec![0xff, 0xff, 0xff]),
			asset2: encode_native().into(),
			amount: U256::from(100),
			includeFee: true,
		}
		.abi_encode();

		let result = bare_call(caller, data);
		let failed =
			result.result.is_err() || result.result.as_ref().map_or(false, |v| v.did_revert());
		assert!(failed, "quote with invalid SCALE encoding must fail");
	});
}

alloy::sol! {
	interface ICaller {
		function delegate(address callee, bytes data, uint64 gas) external returns (bool success, bytes output);
	}
}

/// The delegatecall guard rejects all calls via delegatecall.
#[test]
fn delegatecall_is_rejected() {
	new_test_ext().execute_with(|| {
		let deployer = 123456789u64;
		use frame_support::traits::{fungible::Mutate, Currency};
		pallet_balances::Pallet::<Test>::make_free_balance_be(&deployer, 1_000_000_000_000_000u64);

		// Initialize pallet-revive's internal account (needed for storage deposits).
		let revive_account = pallet_revive::Pallet::<Test>::account_id();
		pallet_balances::Pallet::<Test>::mint_into(
			&revive_account,
			<Test as pallet_balances::Config>::ExistentialDeposit::get(),
		)
		.unwrap();

		let (init_code, _) = pallet_revive_fixtures::compile_module_with_type(
			"Caller",
			pallet_revive_fixtures::FixtureType::Solc,
		)
		.expect("Caller fixture must be compiled");
		let caller_addr = pallet_revive::Pallet::<Test>::bare_instantiate(
			RuntimeOrigin::signed(deployer),
			0u64.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u64::MAX,
			},
			Code::Upload(init_code),
			vec![],
			None,
			&ExecConfig::new_substrate_tx(),
		)
		.result
		.expect("Caller deployment must succeed")
		.addr;

		let calldata = ICaller::delegateCall {
			callee: alloy::primitives::Address::from(precompile_address().0),
			data: IAssetConversion::quoteExactTokensForTokensCall {
				asset1: encode_native().into(),
				asset2: encode_asset(1).into(),
				amount: U256::from(100),
				includeFee: true,
			}
			.abi_encode()
			.into(),
			gas: u64::MAX,
		}
		.abi_encode();

		let result = pallet_revive::Pallet::<Test>::bare_call(
			RuntimeOrigin::signed(deployer),
			caller_addr,
			0u64.into(),
			TransactionLimits::WeightAndDeposit {
				weight_limit: Weight::MAX,
				deposit_limit: u64::MAX,
			},
			calldata,
			&ExecConfig::new_substrate_tx(),
		)
		.result
		.expect("outer call must succeed");

		let ret = ICaller::delegateCall::abi_decode_returns(&result.data)
			.expect("return must decode as (bool, bytes)");
		assert!(!ret.success, "DELEGATECALL to asset-conversion precompile must be rejected");
		assert!(
			ret.output.is_empty(),
			"expected empty output from PrecompileDelegateDenied trap, got {} bytes",
			ret.output.len(),
		);
	});
}
