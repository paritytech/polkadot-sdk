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
	test_utils::{builder::Contract, ALICE},
	tests::{builder, ExtBuilder, Test},
	Code, Config,
};

use alloy_core::{primitives::U256, sol_types::SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, Memory, FixtureType};
use pretty_assertions::assert_eq;

#[test]
fn memory_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Memory", fixture_type).unwrap();

        ExtBuilder::default().build().execute_with(|| {
            <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
            
            builder::bare_call(addr)
                .gas_limit(1_000_000_000.into())
                .data(
                    Memory::MemoryCalls::testMemory(Memory::testMemoryCall { })
                        .abi_encode(),
                )
                .build_and_unwrap_result();
        });
    }
}

#[test]
fn mstore8_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Memory", fixture_type).unwrap();

        let expected_byte = 0xBE_u8;

        ExtBuilder::default().build().execute_with(|| {
            <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
            
            let result = builder::bare_call(addr)
                .gas_limit(1_000_000_000.into())
                .data(
                    Memory::MemoryCalls::testMstore8(Memory::testMstore8Call { offset: U256::from(0), value: U256::from(expected_byte) })
                        .abi_encode(),
                )
                .build_and_unwrap_result();
            let expected_bytes = [expected_byte; 32];
            let expected = U256::from_be_bytes(expected_bytes);
            assert_eq!(
                U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                expected,
                "memory test should return 0"
            );
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
                .gas_limit(1_000_000_000.into())
                .data(
                    Memory::MemoryCalls::testMsize(Memory::testMsizeCall { offset: U256::from(512) })
                        .abi_encode(),
                )
                .build_and_unwrap_result();
            assert_eq!(
                U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                U256::from(offset+32),
                "memory test should return 0"
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
                .gas_limit(1_000_000_000.into())
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
            assert_eq!(
                U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                expected_value,
                "memory test should return 0"
            );
        });
    }
}