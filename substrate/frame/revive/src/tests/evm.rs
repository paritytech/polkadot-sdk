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

//! The pallet-revive EVM integration test suite.

use crate::{
	test_utils::{builder::Contract, *},
	tests::{
		builder,
		test_utils::{ensure_stored, get_contract_checked},
		ExtBuilder, System, Test,
	},
	Code, Config,
};

use alloy_core::{
	primitives::{Bytes, U256},
	sol_types::{SolConstructor, SolInterface},
};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures_solidity::contracts::*;
use pretty_assertions::assert_eq;
use sp_io::hashing::keccak_256;

/// Tests that the EVM can calculat a fibonacci number.
#[test]
fn basic_evm_flow_works() {
	let code = playground_bin();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code.clone())).build_and_unwrap_contract();

		// check the code exists
		let contract = get_contract_checked(&addr).unwrap();
		ensure_stored(contract.code_hash);

		let result = builder::bare_call(addr)
			.data(
				Playground::PlaygroundCalls::fib(Playground::fibCall { n: U256::from(10u64) })
					.abi_encode(),
			)
			.build_and_unwrap_result();
		assert_eq!(U256::from(55u32), U256::from_be_bytes::<32>(result.data.try_into().unwrap()));
	});
}

/// Tests that the blocknumber opcode works as expected.
#[test]
fn block_number_works() {
	let code = playground_bin();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code.clone())).build_and_unwrap_contract();

		System::set_block_number(42);

		let result = builder::bare_call(addr)
			.data(Playground::PlaygroundCalls::bn(Playground::bnCall {}).abi_encode())
			.build_and_unwrap_result();
		assert_eq!(U256::from(42u32), U256::from_be_bytes::<32>(result.data.try_into().unwrap()));
	});
}

/// Tests that the sha3 keccak256 cryptographic opcode works as expected.
#[test]
fn keccak_256_works() {
	let code = crypto_bin();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let pre = "revive".to_string();
		let expected = keccak_256(pre.as_bytes());

		let result = builder::bare_call(addr)
			.data(TestSha3::TestSha3Calls::test(TestSha3::testCall { _pre: pre }).abi_encode())
			.build_and_unwrap_result();

		assert_eq!(&expected, result.data.as_slice());
	});
}

/// Tests that the create2 opcode works as expected.
#[test]
fn predictable_addresses() {
	let code = address_predictor_bin();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { .. } = builder::bare_instantiate(Code::Upload(code))
			.data(
				AddressPredictor::constructorCall::new((
					U256::from(123),
					Bytes::from(predicted_bin_runtime()),
				))
				.abi_encode(),
			)
			.build_and_unwrap_contract();
	});
}
