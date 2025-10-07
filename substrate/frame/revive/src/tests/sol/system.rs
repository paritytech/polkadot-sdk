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
	Code, Config, U256,
};
use alloy_core::sol_types::SolCall;
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{
	compile_module_with_type, Callee, FixtureType, System as SystemFixture,
};
use pretty_assertions::assert_eq;
use revm::primitives::Bytes;
use sp_core::H160;
use sp_io::hashing::keccak_256;
use test_case::test_case;

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn keccak_256_works(fixture_type: FixtureType) {
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

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn address_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(SystemFixture::addressFuncCall {}.abi_encode())
			.build_and_unwrap_result();

		let decoded = SystemFixture::addressFuncCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(addr, H160::from_slice(decoded.as_slice()));
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn caller_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(SystemFixture::callerCall {}.abi_encode())
			.build_and_unwrap_result();

		let decoded = SystemFixture::callerCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(ALICE_ADDR, H160::from_slice(decoded.as_slice()));
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn callvalue_works(fixture_type: FixtureType) {
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

		let decoded = SystemFixture::callvalueCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(value, decoded);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn calldataload_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(SystemFixture::calldataloadCall { offset: 4u64 }.abi_encode())
			.build_and_unwrap_result();

		// Call calldataload(offset=4) → returns the argument "4"
		let decoded = SystemFixture::calldataloadCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(U256::from(4u32), U256::from_big_endian(decoded.as_slice()));
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn calldatasize_works(fixture_type: FixtureType) {
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
		let decoded = SystemFixture::calldatasizeCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(4u64, decoded);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn calldatacopy_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("System", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let call_data = SystemFixture::calldatacopyCall {
			destOffset: 0u64, // unused
			offset: 4u64,     // skip selector
			size: 64u64,      // copy destOffset + offset
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

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn codesize_works(fixture_type: FixtureType) {
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

		let decoded = SystemFixture::codesizeCall::abi_decode_returns(&result.data).unwrap();
		let expected_size = code.len() as u64;

		assert_eq!(expected_size, decoded);
	});
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

		let magic_number = 42u64;
		let result = builder::bare_call(addr)
			.data(
				SystemFixture::returndatasizeCall {
					_callee: callee_addr.0.into(),
					_data: Callee::echoCall { _data: magic_number }.abi_encode().into(),
					_gas: u64::MAX,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		let decoded = SystemFixture::returndatasizeCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(decoded, 32);
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
					_data: Callee::echoCall { _data: 42u64 }.abi_encode().into(),
					_gas: u64::MAX,
					destOffset: 0u64,
					offset: 0u64,
					size: 32u64,
				}
				.abi_encode(),
			)
			.build_and_unwrap_result();

		let decoded = SystemFixture::returndatacopyCall::abi_decode_returns(&result.data).unwrap();
		let decoded_value = U256::from_big_endian(decoded.as_ref());
		assert_eq!(magic_number, decoded_value)
	});
}
