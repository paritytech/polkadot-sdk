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

use super::{slope, weight_of_solc_call};
use crate::{
	Code, Config, Error, ExecReturnValue, LOG_TARGET,
	evm::decode_revert_reason,
	limits::EVM_MEMORY_BYTES,
	test_utils::{ALICE, builder::Contract},
	tests::{ExtBuilder, Test, builder},
	weights::WeightInfo,
};
use alloy_core::{
	primitives::U256,
	sol_types::{SolCall, SolInterface},
};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{FixtureType, Memory, compile_module_with_type};
use pallet_revive_uapi::ReturnFlags;
use pretty_assertions::assert_eq;
use test_case::test_case;

type Weights = <Test as Config>::WeightInfo;

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

#[test]
fn mcopy_charges_each_byte_after_the_first_64() {
	let weight_of = |len: u32| {
		weight_of_solc_call(
			"Memory",
			Memory::mcopyWithoutGrowthCall { len: U256::from(len) }.abi_encode(),
		)
	};

	let empty = weight_of(0);
	let fixed = weight_of(64);
	let one_more = weight_of(65);

	// A zero-length copy pays the fixed part too.
	assert_eq!(fixed, empty);
	assert_eq!(one_more, fixed.saturating_add(slope(Weights::evm_mcopy_per_byte)));
}

#[test]
fn mstore_at_a_high_offset_charges_growth_and_reallocation() {
	// The fixture expands memory to 1 KiB before the store.
	const EXPANDED_BYTES: u32 = 1024;
	let weight_of = |offset: u32| {
		weight_of_solc_call(
			"Memory",
			Memory::mstoreAtCall { offset: U256::from(offset) }.abi_encode(),
		)
	};

	let without_growth = weight_of(0);
	let to_the_limit = weight_of(EVM_MEMORY_BYTES - 32);

	// Growing past the 4 KiB buffer replaces it with one of the next power of two.
	let growth = slope(Weights::evm_memory_grow)
		.saturating_mul(((EVM_MEMORY_BYTES - EXPANDED_BYTES) / 32).into())
		.saturating_add(Weights::evm_memory_reallocate(EVM_MEMORY_BYTES.next_power_of_two() / 32));
	assert_eq!(to_the_limit, without_growth.saturating_add(growth));
}

#[test]
fn return_charges_copying_its_output() {
	// The fixture expands memory to 64 KiB before returning.
	const LEN: u32 = 64 * 1024;
	let weight_of = |len: u32| {
		weight_of_solc_call(
			"Memory",
			Memory::returnPrefixCall { len: U256::from(len) }.abi_encode(),
		)
	};

	let empty = weight_of(0);
	let large = weight_of(LEN);

	assert_eq!(large, empty.saturating_add(Weights::evm_memory_copy_out(LEN)));
}

#[test]
fn zero_length_calldatacopy_skips_the_copy_charge() {
	let weight_of = |len: u32| {
		weight_of_solc_call(
			"Memory",
			Memory::calldatacopyWithoutGrowthCall { len: U256::from(len) }.abi_encode(),
		)
	};

	let empty = weight_of(0);
	let word = weight_of(32);

	assert_eq!(word, empty.saturating_add(Weights::seal_copy_to_contract(32)));
}
