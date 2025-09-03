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
	Code, Config,
};

use alloy_core::{
	primitives::U256,
	sol_types::{SolCall, SolInterface},
};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, FixtureType, Memory};
use pallet_revive_uapi::ReturnFlags;
use pretty_assertions::assert_eq;

#[test]
fn memory_limit_works() {
	for fixture_type in [FixtureType::Solc] {
		let (code, _) = compile_module_with_type("Memory", fixture_type).unwrap();

		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let test_cases = [
				(
					Memory::expandMemoryCall {
						memorySize: U256::from(crate::limits::code::BASELINE_MEMORY_LIMIT - 1),
					},
					false,
				),
				(
					Memory::expandMemoryCall {
						memorySize: U256::from(crate::limits::code::BASELINE_MEMORY_LIMIT),
					},
					true,
				),
			];

			for (data, should_revert) in test_cases {
				let result =
					builder::bare_call(addr).data(data.abi_encode()).build_and_unwrap_result();
				assert_eq!(result.did_revert(), should_revert);
			}
		});
	}
}
#[test]
fn memory_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
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
					log::error!("Revert message: {}", revert_msg);
				} else {
					log::error!("Revert without message, raw data: {:?}", result.data);
				}
			}
			assert!(!result.did_revert(), "test reverted");
		});
	}
}

#[test]
fn msize_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Memory", fixture_type).unwrap();

		let offset = 512u64;

		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let result = builder::bare_call(addr)
				.data(
					Memory::MemoryCalls::testMsize(Memory::testMsizeCall {
						offset: U256::from(512),
					})
					.abi_encode(),
				)
				.build_and_unwrap_result();
			assert!(!result.did_revert(), "test reverted");
			assert_eq!(
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				U256::from(offset + 32),
				"memory test should return {}",
				offset + 32
			);
		});
	}
}

#[test]
fn mcopy_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Memory", fixture_type).unwrap();

		let expected_value = U256::from(0xBE);

		ExtBuilder::default().build().execute_with(|| {
			<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

			let result = builder::bare_call(addr)
				.data(
					Memory::MemoryCalls::testMcopy(Memory::testMcopyCall {
						dstOffset: U256::from(512),
						offset: U256::from(0),
						size: U256::from(32),
						value: expected_value,
					})
					.abi_encode(),
				)
				.build_and_unwrap_result();
			assert!(!result.did_revert(), "test reverted");
			assert_eq!(
				U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
				expected_value,
				"memory test should return {expected_value}"
			);
		});
	}
}
