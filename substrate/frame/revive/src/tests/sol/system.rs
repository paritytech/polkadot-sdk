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
use alloy_core::{primitives::U256, sol_types::SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, FixtureType, System as SystemFixture};
use pretty_assertions::assert_eq;
use revm::primitives::Bytes;
use sp_core::H160;
use sp_io::hashing::keccak_256;

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
				.data(
					SystemFixture::SystemCalls::keccak256Func(SystemFixture::keccak256FuncCall {
						data: Bytes::from(pre),
					})
					.abi_encode(),
				)
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
				.data(
					SystemFixture::SystemCalls::addressFunc(SystemFixture::addressFuncCall {})
						.abi_encode(),
				)
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
				.data(SystemFixture::SystemCalls::caller(SystemFixture::callerCall).abi_encode())
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
				.data(
					SystemFixture::SystemCalls::callvalue(SystemFixture::callvalueCall)
						.abi_encode(),
				)
				.build_and_unwrap_result();

			let returned_val = U256::from_be_bytes::<32>(result.data.try_into().unwrap());
			assert_eq!(returned_val, U256::from(value));
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
					SystemFixture::SystemCalls::calldataload(
						SystemFixture::calldataloadCall { offset: U256::from(4u32) }, /* skip selector */
					)
					.abi_encode(),
				)
				.build_and_unwrap_result();

			// Call calldataload(offset=4) → returns the argument "4"
			let returned = U256::from_be_bytes::<32>(result.data.as_slice().try_into().unwrap());
			assert_eq!(returned, U256::from(4u32));
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
				.data(
					SystemFixture::SystemCalls::calldatasize(SystemFixture::calldatasizeCall {})
						.abi_encode(),
				)
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

			let call_data =
				SystemFixture::SystemCalls::calldatacopy(SystemFixture::calldatacopyCall {
					destOffset: U256::from(0u32), // unused
					offset: U256::from(4u32),     // skip selector
					size: U256::from(64u32),      // copy destOffset + offset
				})
				.abi_encode();

			let result = builder::bare_call(addr).data(call_data.clone()).build_and_unwrap_result();

			let returned_data_with_header = result.data.as_slice();

			// Check that the returned `bytes` has the correct length.
			// The returned data will be 32 bytes for the offset, 32 bytes for the length header +
			// 64 bytes for the data.
			assert_eq!(returned_data_with_header.len(), 32 + 32 + 64);

			// Extract the actual data without the header.
			let returned_data = &returned_data_with_header[64..];

			// The expected data is the slice of the original calldata that was copied.
			let expected_data = &call_data.as_slice()[4..(4 + 64) as usize];

			// Assert that the copied data matches the expected data.
			assert_eq!(returned_data, expected_data);
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
				.data(
					SystemFixture::SystemCalls::codesize(SystemFixture::codesizeCall {})
						.abi_encode(),
				)
				.build_and_unwrap_result();

			// Now fetch the actual *runtime* code size from storage
			let code = Contracts::code(&addr);

			let returned_size =
				U256::from_be_bytes::<32>(result.data.as_slice().try_into().unwrap());
			let expected_size = U256::from(code.len());

			assert_eq!(returned_size, expected_size);
		});
	}
}
