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
	test_utils::{builder::Contract, ALICE, BOB, BOB_ADDR},
	tests::{builder, ExtBuilder, Test, test_utils},
	Code, Config,
    address::AddressMapper,
    H256,
    Key,
    System,
};
use alloy_core::{primitives::U256, sol_types::SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, FixtureType};
use pretty_assertions::assert_eq;
use frame_support::traits::Get;

fn make_evm_bytecode_from_runtime_code(runtime_code: &str) -> Vec<u8> {
    let runtime_code_len = runtime_code.len() / 2;
    assert!(runtime_code_len < 256);
    let init_code = format!(
        "6080\
        6040\
        52\
        6040\
        51\
        60{runtime_code_len:02x}\
        6013\
        82\
        39\
        60{runtime_code_len:02x}\
        90\
        f3\
        fe"
    );
    hex::decode(format!("{init_code}{runtime_code}")).unwrap()
}

#[test]
fn jump_works() {
    let expected_value = 0xfefefefe_u64;
    let runtime_code = concat!(
        "63fefefefe",       // PUSH4 0xfefefefe
        "5f",               // push0
        "52",               // mstore
        "6011",             // PUSH1 0x11
        "56",               // JUMP
        "63deadbeef",       // PUSH4 0xdeadbeef
        "5f",               // push0
        "52",               // mstore
        "5b",               // JUMPDEST
        "6020",             // push1 0x20
        "5f",               // push0
        "f3"                // RETURN
    );
    let code = make_evm_bytecode_from_runtime_code(runtime_code);

    ExtBuilder::default().build().execute_with(|| {
        <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
        let Contract { addr, .. } =
            builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
        
        let result = builder::bare_call(addr)
            .gas_limit(1_000_000_000.into())
            .data(vec![])
            .build_and_unwrap_result();
        
        assert_eq!(
            U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
            U256::from(expected_value),
            "memory test should return 0xfefefefe"
        );
    });
}

#[test]
fn jumpi_works() {
    let expected_value = 0xfefefefe_u64;
    let unexpected_value = 0xaabbccdd_u64;
    let runtime_code = concat!(
        "5f",               // push0
        "35",               // CALLDATALOAD
        "63fefefefe",       // PUSH4 0xfefefefe
        "03",               // SUB
        "6016",             // PUSH1 0x16
        "57",               // jumpi
        "63fefefefe",       // PUSH4 0xfefefefe
        "5f",               // push0
        "52",               // mstore
        "6020",             // push1 0x20
        "5f",               // push0
        "f3",               // RETURN
        "5b",               // JUMPDEST
        "63deadbeef",       // PUSH4 0xdeadbeef
        "5f",               // push0
        "52",               // mstore
        "6020",             // push1 0x20
        "5f",               // push0
        "f3"                // RETURN
    );
    let code = make_evm_bytecode_from_runtime_code(runtime_code);

    ExtBuilder::default().build().execute_with(|| {
        <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
        let Contract { addr, .. } =
            builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
        
        {
            // JUMPI was *not* triggered, contract returns 0xfefefefe
            let argument = U256::from(expected_value).to_be_bytes::<32>().to_vec();

            let result = builder::bare_call(addr)
                .gas_limit(1_000_000_000.into())
                .data(argument)
                .build_and_unwrap_result();
            
            assert_eq!(
                U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                U256::from(expected_value),
                "memory test should return 0xfefefefe"
            );
        }
        
        {
            // JUMPI was triggered, contract returns 0xdeadbeef
            let argument = U256::from(unexpected_value).to_be_bytes::<32>().to_vec();

            let result = builder::bare_call(addr)
                .gas_limit(1_000_000_000.into())
                .data(argument)
                .build_and_unwrap_result();
            
            assert_eq!(
                U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                U256::from(0xdeadbeef_u64),
                "memory test should return 0xdeadbeef"
            );
        }
    });
}