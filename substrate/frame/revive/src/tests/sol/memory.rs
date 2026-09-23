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
	Code, Config, Error, ExecReturnValue, LOG_TARGET,
	evm::decode_revert_reason,
	test_utils::{ALICE, builder::Contract},
	tests::{ExtBuilder, Test, builder},
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

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn copies_overlapping_and_boundary_ranges(fixture_type: FixtureType) {
	// Arrange
	struct CopyCase {
		destination: U256,
		source: U256,
		length: u32,
		head: U256,
		tail: U256,
	}
	let (code, _) = compile_module_with_type("Memory", fixture_type).unwrap();
	let cases = [
		CopyCase {
			destination: U256::from(crate::limits::EVM_MEMORY_BYTES + 1),
			source: U256::from(crate::limits::EVM_MEMORY_BYTES + 1),
			length: 0,
			head: U256::MAX,
			tail: U256::ZERO,
		},
		CopyCase {
			destination: U256::from(1),
			source: U256::ZERO,
			length: 64,
			head: U256::MAX,
			tail: U256::from(0xff) << 248,
		},
		CopyCase {
			destination: U256::ZERO,
			source: U256::from(1),
			length: 64,
			head: U256::MAX << 8,
			tail: U256::ZERO,
		},
		CopyCase {
			destination: U256::from(1),
			source: U256::ZERO,
			length: crate::limits::EVM_MEMORY_BYTES - 1,
			head: U256::MAX,
			tail: U256::from(0xff) << 248,
		},
		CopyCase {
			destination: U256::ZERO,
			source: U256::ZERO,
			length: crate::limits::EVM_MEMORY_BYTES,
			head: U256::MAX,
			tail: U256::ZERO,
		},
	];
	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
		for case in cases {
			// Act
			let result = builder::bare_call(addr)
				.data(
					Memory::copyMemoryRangeCall {
						dst: case.destination,
						src: case.source,
						size: U256::from(case.length),
					}
					.abi_encode(),
				)
				.build()
				.result;

			// Assert
			let expected = match fixture_type {
				FixtureType::Resolc if case.length >= crate::limits::EVM_MEMORY_BYTES - 1 => {
					Err(Error::<Test>::ContractTrapped.into())
				},
				FixtureType::Solc | FixtureType::Resolc => Ok(ExecReturnValue {
					flags: ReturnFlags::empty(),
					data: [case.head.to_be_bytes::<32>(), case.tail.to_be_bytes::<32>()].concat(),
				}),
				FixtureType::Rust | FixtureType::SolcRuntime => {
					unreachable!("only Solc and Resolc fixtures are tested")
				},
			};
			assert_eq!(
				result, expected,
				"copy dst={} src={} len={}",
				case.destination, case.source, case.length
			);
		}
	});
}

#[test_case(FixtureType::Solc)]
#[test_case(FixtureType::Resolc)]
fn empty_copy_with_full_width_offsets_obeys_backend_pointer_limits(fixture_type: FixtureType) {
	// Arrange
	let (code, _) = compile_module_with_type("Memory", fixture_type).unwrap();
	ExtBuilder::default().build().execute_with(|| {
		<Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
		let Contract { addr, .. } =
			builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

		// Act
		let result = builder::bare_call(addr)
			.data(
				Memory::copyMemoryRangeCall { dst: U256::MAX, src: U256::MAX, size: U256::ZERO }
					.abi_encode(),
			)
			.build_and_unwrap_result();

		// Assert
		let expected = match fixture_type {
			FixtureType::Solc => ExecReturnValue {
				flags: ReturnFlags::empty(),
				data: [U256::MAX.to_be_bytes::<32>(), U256::ZERO.to_be_bytes::<32>()].concat(),
			},
			FixtureType::Resolc => ExecReturnValue { flags: ReturnFlags::REVERT, data: Vec::new() },
			FixtureType::Rust | FixtureType::SolcRuntime => {
				unreachable!("only Solc and Resolc fixtures are tested")
			},
		};
		assert_eq!(result, expected);
	});
}
