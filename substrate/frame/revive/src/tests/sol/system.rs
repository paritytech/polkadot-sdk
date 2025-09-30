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
	test_utils::{builder::Contract, ALICE, ALICE_ADDR},
	tests::{builder, Contracts, ExtBuilder, Test},
	Code, Config,
};
use alloy_core::{primitives::U256, sol_types::SolCall};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{
	compile_module_with_type, Callee, FixtureType, System as SystemFixture,
};
use pretty_assertions::assert_eq;
use revm::primitives::Bytes;
use sp_core::H160;
use sp_io::hashing::keccak_256;
use test_case::test_case;

#[test]
fn keccak_256_works() {
	for fixture_type in [FixtureType::Resolc, FixtureType::Solc] {
		let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let pre = b"revive";
			let expected = keccak_256(pre);

			let result = builder::bare_call(addr)
				.data(SystemFixture::keccak256FuncCall { data: Bytes::from(pre) }.abi_encode())
				.build_and_unwrap_result();

			assert_eq!(&expected, result.data.as_slice());
		});
	}
}

#[test]
fn address_works() {
	for fixture_type in [FixtureType::Resolc, FixtureType::Solc] {
		let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let result = builder::bare_call(addr)
				.data(SystemFixture::addressFuncCall {}.abi_encode())
				.build_and_unwrap_result();

			let returned_addr: H160 = H160::from_slice(&result.data[12..]);
			assert_eq!(addr, returned_addr);
		});
	}
}

#[test]
fn caller_works() {
	for fixture_type in [FixtureType::Resolc, FixtureType::Solc] {
		let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let result = builder::bare_call(addr)
				.data(SystemFixture::callerCall {}.abi_encode())
				.build_and_unwrap_result();

			let returned_caller = H160::from_slice(&result.data[12..]);
			assert_eq!(ALICE_ADDR, returned_caller);
		});
	}
}

#[test]
fn callvalue_works() {
	for fixture_type in [FixtureType::Resolc, FixtureType::Solc] {
		let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let value = 1337u64;

			let result = builder::bare_call(addr)
				.evm_value(value.into())
				.data(SystemFixture::callvalueCall {}.abi_encode())
				.build_and_unwrap_result();

			let returned_val = U256::from_be_bytes::<32>(result.data.try_into().unwrap());
			assert_eq!(U256::from(value), returned_val);
		});
	}
}

#[test]
fn calldataload_works() {
	for fixture_type in [FixtureType::Resolc, FixtureType::Solc] {
		let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let result = builder::bare_call(addr)
				.data(
						SystemFixture::calldataloadCall { offset: U256::from(4u32) } /* skip selector */
					.abi_encode(),
				)
				.build_and_unwrap_result();

			// Call calldataload(offset=4) → returns the argument "4"
			let returned = U256::from_be_bytes::<32>(result.data.as_slice().try_into().unwrap());
			assert_eq!(U256::from(4u32), returned);
		});
	}
}

#[test]
fn calldatasize_works() {
	for fixture_type in [FixtureType::Resolc, FixtureType::Solc] {
		let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			// calldata = selector + encoded argument
			let result = builder::bare_call(addr)
				.data(SystemFixture::calldatasizeCall {}.abi_encode())
				.build_and_unwrap_result();

			// ABI encodes: 4 (selector) + 0 (no args) = 4
			let returned = U256::from_be_bytes::<32>(result.data.as_slice().try_into().unwrap());
			assert_eq!(returned, U256::from(4u32));
		});
	}
}

#[test]
fn calldatacopy_works() {
	for fixture_type in [FixtureType::Resolc, FixtureType::Solc] {
		let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let call_data = SystemFixture::calldatacopyCall {
				destOffset: U256::from(0u32), // unused
				offset: U256::from(4u32),     // skip selector
				size: U256::from(64u32),      // copy destOffset + offset
			}
			.abi_encode();

			let result = builder::bare_call(addr).data(call_data.clone()).build_and_unwrap_result();

			let returned_data =
				SystemFixture::calldatacopyCall::abi_decode_returns(&result.data).unwrap();

			let returned_data = returned_data.as_ref();
			assert_eq!(returned_data.len(), 64);

			// The expected data is the slice of the original calldata that was copied.
			let expected_data = &call_data.as_slice()[4..(4 + 64) as usize];
			assert_eq!(expected_data, returned_data);
		});
	}
}

#[test]
fn codesize_works() {
	for fixture_type in [FixtureType::Resolc, FixtureType::Solc] {
		let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code.clone())).build_and_unwrap_contract();

			let result = builder::bare_call(addr)
				.data(SystemFixture::codesizeCall {}.abi_encode())
				.build_and_unwrap_result();

			// Now fetch the actual *runtime* code size from storage
			let code = Contracts::code(&addr);

			let returned_size =
				U256::from_be_bytes::<32>(result.data.as_slice().try_into().unwrap());
			let expected_size = U256::from(code.len());

			assert_eq!(expected_size, returned_size);
		});
	}
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn returndatasize_works(caller_type: FixtureType, callee_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);

		// Instantiate the callee contract, which can echo a value.
		let Contract { addr: callee_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let magic_number = U256::from(42);
		let result = builder::bare_call(addr)
			.data(
				SystemFixture::returndatasizeCall {
					_callee: callee_addr.0.into(),
					_data: Callee::echoCall { _data: magic_number }.abi_encode().into(),
					_gas: U256::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		let size = U256::from_be_bytes::<32>(result.data.try_into().unwrap());
		// Always 32 bytes for a single uint256
		assert_eq!(U256::from(32), size);
	});
}

#[test_case(FixtureType::Solc,   FixtureType::Solc;   "solc->solc")]
#[test_case(FixtureType::Solc,   FixtureType::Resolc; "solc->resolc")]
#[test_case(FixtureType::Resolc, FixtureType::Solc;   "resolc->solc")]
#[test_case(FixtureType::Resolc, FixtureType::Resolc; "resolc->resolc")]
fn returndatacopy_works(caller_type: FixtureType, callee_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", caller_type).unwrap();
	let (callee_code, _) = compile_module_with_type("Callee", callee_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		// Instantiate the callee contract, which can echo a value.
		let Contract { addr: callee_addr, .. } =
			builder::bare_instantiate(Code::Upload(callee_code)).build_and_unwrap_contract();

		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let magic_number = U256::from(42);
		let result = builder::bare_call(addr)
			.data(
				SystemFixture::returndatacopyCall {
					_callee: callee_addr.0.into(),
					_data: Callee::echoCall { _data: magic_number }.abi_encode().into(),
					_gas: U256::MAX,
					destOffset: U256::ZERO,
					offset: U256::ZERO,
					size: U256::from(32u32),
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		let result = SystemFixture::returndatacopyCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(magic_number, U256::from_be_bytes::<32>(result.as_ref().try_into().unwrap()))
	});
}
