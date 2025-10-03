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

use crate::{
	evm::decode_revert_reason,
	test_utils::{builder::Contract, ALICE},
	tests::{builder, ExtBuilder, Test},
	Code, Config, Error, ExecReturnValue, LOG_TARGET,
};
use alloy_core::sol_types::{SolCall, SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, FixtureType, Memory};
use pallet_revive_uapi::ReturnFlags;
use pretty_assertions::assert_eq;
use test_case::test_case;

#[test]
fn memory_limit_works() {
	let (code, _) = compile_module_with_type("Memory", FixtureType::Solc).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let test_cases = [
			(
				"Writing 1 byte from 0 to the limit - 1 should work.",
				Memory::expandMemoryCall {
					memorySize: (crate::limits::EVM_MEMORY_BYTES - 1) as u64,
				},
				Ok(ExecReturnValue { data: vec![0u8; 32], flags: ReturnFlags::empty() }),
			),
			(
				"Writing 1 byte from the limit should revert.",
				Memory::expandMemoryCall { memorySize: crate::limits::EVM_MEMORY_BYTES as u64 },
				Err(Error::<Test>::OutOfGas.into()),
			),
		];

		for (reason, data, expected_result) in test_cases {
			let result = builder::bare_call(addr).data(data.abi_encode()).build().result;
			assert_eq!(result, expected_result, "{reason}");
		}
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn memory_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Memory", fixture_type).unwrap();

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(Memory::MemoryCalls::testMemory(Memory::testMemoryCall {}).abi_encode())
			.build_and_unwrap_result();
		if result.flags == ReturnFlags::REVERT {
			if let Some(revert_msg) = decode_revert_reason(&result.data) {
				log::error!(target: LOG_TARGET, "Revert message: {}", revert_msg);
			} else {
				log::error!(target: LOG_TARGET, "Revert without message, raw data: {:?}", result.data);
			}
		}
		assert!(!result.did_revert(), "test reverted");
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn msize_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Memory", fixture_type).unwrap();

	let offset = 512u64;

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(Memory::MemoryCalls::testMsize(Memory::testMsizeCall { offset }).abi_encode())
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "test reverted");
		let decoded = Memory::testMsizeCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(offset + 32, decoded, "memory test should return {}", offset + 32);
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn mcopy_works(fixture_type: FixtureType) {
	let (code, _) = compile_module_with_type("Memory", fixture_type).unwrap();

	let expected_value = 0xBE;

	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		let result = builder::bare_call(addr)
			.data(
				Memory::MemoryCalls::testMcopy(Memory::testMcopyCall {
					dstOffset: 512,
					offset: 0,
					size: 32,
					value: expected_value,
				})
				.abi_encode(),
			)
			.build_and_unwrap_result();
		assert!(!result.did_revert(), "test reverted");
		let decoded = Memory::testMcopyCall::abi_decode_returns(&result.data).unwrap();
		assert_eq!(expected_value, decoded, "memory test should return {expected_value}");
	});
}
