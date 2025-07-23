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

//! The pallet-revive shared VM integration test suite.

use crate::{
	test_utils::{builder::Contract, *},
	tests::{builder, ExtBuilder, System, Test},
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

/// Tests that the blocknumber opcode works as expected.
#[test]
fn block_number_works() {
	for code in [playground_bin(), playground_pvm()] {
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			System::set_block_number(42);

			let result = builder::bare_call(addr)
				.data(Playground::PlaygroundCalls::bn(Playground::bnCall {}).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(
				U256::from(42u32),
				U256::from_be_bytes::<32>(result.data.try_into().unwrap())
			);
		});
	}
}

/// Tests that the sha3 keccak256 cryptographic opcode works as expected.
#[test]
fn keccak_256_works() {
	for code in [crypto_bin(), crypto_pvm()] {
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
}

/// Tests that the create2 opcode works as expected.
#[test]
fn predictable_addresses() {
	let bytecodes = [
		(address_predictor_pvm(), predicted_pvm()),
		(address_predictor_bin(), predicted_bin_runtime()),
	];

	// TODO: Remove `take(1)` to activate the EVM test.
	for (code, target) in bytecodes.into_iter().take(1) {
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

			// Publishing the target bytecode pre-image first is necessary on PVM.
			builder::bare_instantiate(Code::Upload(target.clone()))
				.data(vec![0; 32])
				.build_and_unwrap_contract();

			let Contract { .. } = builder::bare_instantiate(Code::Upload(code))
				.data(
					AddressPredictor::constructorCall::new((U256::from(123), Bytes::from(target)))
						.abi_encode(),
				)
				.build_and_unwrap_contract();
		});
	}
}

/// Tests that the sstore and sload storage opcodes work as expected.
#[test]
fn flipper() {
	// TODO: Remove `take(1)` to activate the EVM test.
	for code in [flipper_pvm(), flipper_bin()].into_iter().take(1) {
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let result = builder::bare_call(addr)
				.data(Flipper::FlipperCalls::coin(Flipper::coinCall {}).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(U256::ZERO, U256::from_be_bytes::<32>(result.data.try_into().unwrap()));

			// Should be false
			let result = builder::bare_call(addr)
				.data(Flipper::FlipperCalls::coin(Flipper::coinCall {}).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(U256::ZERO, U256::from_be_bytes::<32>(result.data.try_into().unwrap()));

			// Flip the coin
			builder::bare_call(addr).build_and_unwrap_result();

			// Should be true
			let result = builder::bare_call(addr)
				.data(Flipper::FlipperCalls::coin(Flipper::coinCall {}).abi_encode())
				.build_and_unwrap_result();
			assert_eq!(U256::ONE, U256::from_be_bytes::<32>(result.data.try_into().unwrap()));
		});
	}
}
