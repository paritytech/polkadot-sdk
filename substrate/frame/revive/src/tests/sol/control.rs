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
	test_utils::{builder::Contract, ALICE, },
	tests::{builder, ExtBuilder, Test, 
    sol::make_evm_bytecode_from_runtime_code},
	Code, Config,
};
use alloy_core::primitives::U256;
use frame_support::traits::fungible::Mutate;
use pretty_assertions::assert_eq;
use pallet_revive_uapi::ReturnFlags;

use revm::bytecode::opcode::*;

#[test]
fn jump_works() {
    let expected_value = 0xfefefefe_u64;
    let runtime_code: Vec<u8> = vec![
        vec![PUSH4, 0xfe, 0xfe, 0xfe, 0xfe],
        vec![PUSH0],
        vec![MSTORE],
        vec![PUSH1, 0x11_u8],
        vec![JUMP],
        vec![PUSH4, 0xde, 0xad, 0xbe, 0xef],
        vec![PUSH0],
        vec![MSTORE],
        vec![JUMPDEST],
        vec![PUSH1, 0x20_u8],
        vec![PUSH0],
        vec![RETURN],
    ]
    .into_iter()
    .flatten()
    .collect();
    let code = make_evm_bytecode_from_runtime_code(&runtime_code);

    ExtBuilder::default().build().execute_with(|| {
        <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
        let Contract { addr, .. } =
            builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
        
        let result = builder::bare_call(addr)
            .data(vec![])
            .build_and_unwrap_result();
        
        assert!(
            !result.did_revert(),
            "test reverted"
        );
        assert_eq!(
            U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
            U256::from(expected_value),
            "memory test should return {expected_value}"
        );
    });
}

#[test]
fn jumpi_works() {
    let expected_value = 0xfefefefe_u64;
    let unexpected_value = 0xaabbccdd_u64;
    let runtime_code: Vec<u8> = vec![
        vec![PUSH0],
        vec![CALLDATALOAD],
        vec![PUSH4, 0xfe, 0xfe, 0xfe, 0xfe],
        vec![SUB],
        vec![PUSH1, 0x16_u8],
        vec![JUMPI],
        vec![PUSH4, 0xfe, 0xfe, 0xfe, 0xfe],
        vec![PUSH0],
        vec![MSTORE],
        vec![PUSH1, 0x20_u8],
        vec![PUSH0],
        vec![RETURN],
        vec![JUMPDEST],
        vec![PUSH4, 0xde, 0xad, 0xbe, 0xef],
        vec![PUSH0],
        vec![MSTORE],
        vec![PUSH1, 0x20_u8],
        vec![PUSH0],
        vec![RETURN],
    ]
    .into_iter()
    .flatten()
    .collect();
    let code = make_evm_bytecode_from_runtime_code(&runtime_code);

    ExtBuilder::default().build().execute_with(|| {
        <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
        let Contract { addr, .. } =
            builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();
        
        {
            // JUMPI was *not* triggered, contract returns 0xfefefefe
            let argument = U256::from(expected_value).to_be_bytes::<32>().to_vec();

            let result = builder::bare_call(addr)
                .data(argument)
                .build_and_unwrap_result();
            assert!(
                !result.did_revert(),
                "test reverted"
            );
            assert_eq!(
                U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                U256::from(expected_value),
                "memory test should return {expected_value}"
            );
        }
        
        {
            // JUMPI was triggered, contract returns 0xdeadbeef
            let argument = U256::from(unexpected_value).to_be_bytes::<32>().to_vec();

            let result = builder::bare_call(addr)
                .data(argument)
                .build_and_unwrap_result();
            assert!(
                !result.did_revert(),
                "test reverted"
            );
            
            assert_eq!(
                U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
                U256::from(0xdeadbeef_u64),
                "memory test should return 0xdeadbeef"
            );
        }
    });
}

#[test]
fn ret_works() {
    let expected_value = 0xfefefefe_u64;
    let runtime_code: Vec<u8> = vec![
        vec![PUSH4, 0xfe, 0xfe, 0xfe, 0xfe],
        vec![PUSH0],
        vec![MSTORE],
        vec![PUSH1, 0x20_u8],
        vec![PUSH0],
        vec![RETURN],
    ]
    .into_iter()
    .flatten()
    .collect();
    let code = make_evm_bytecode_from_runtime_code(&runtime_code);

    ExtBuilder::default().build().execute_with(|| {
        <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
        let Contract { addr, .. } =
            builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

        let result = builder::bare_call(addr)
            .data(vec![])
            .build_and_unwrap_result();
        
        assert!(
            !result.did_revert(),
            "test reverted"
        );
        assert_eq!(
            U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
            U256::from(expected_value),
            "memory test should return {expected_value}"
        );
    });
}

#[test]
fn revert_works() {
    let expected_value = 0xfefefefe_u64;
    let runtime_code: Vec<u8> = vec![
        vec![PUSH4, 0xfe, 0xfe, 0xfe, 0xfe],
        vec![PUSH0],
        vec![MSTORE],
        vec![PUSH1, 0x20_u8],
        vec![PUSH0],
        vec![REVERT],
    ]
    .into_iter()
    .flatten()
    .collect();
    let code = make_evm_bytecode_from_runtime_code(&runtime_code);

    ExtBuilder::default().build().execute_with(|| {
        <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
        let Contract { addr, .. } =
            builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

        let result = builder::bare_call(addr)
            .data(vec![])
            .build_and_unwrap_result();
        
        assert!(
            result.flags == ReturnFlags::REVERT,
            "test did not revert"
        );
        assert_eq!(
            U256::from_be_bytes::<32>(result.data.try_into().unwrap()),
            U256::from(expected_value),
            "memory test should return {expected_value}"
        );
    });
}

#[test]
fn stop_works() {
    let expected_value = 0xfefefefe_u64;
    let runtime_code: Vec<u8> = vec![
        vec![STOP],
    ]
    .into_iter()
    .flatten()
    .collect();
    let code = make_evm_bytecode_from_runtime_code(&runtime_code);

    ExtBuilder::default().build().execute_with(|| {
        <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
        let Contract { addr, .. } =
            builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

        let result = builder::bare_call(addr)
            .data(vec![])
            .build_and_unwrap_result();
        
        assert!(
            !result.did_revert(),
            "test reverted"
        );
        
    });
}

#[test]
fn invalid_works() {
    let expected_gas = 12_345_000_u64;
    let runtime_code: Vec<u8> = vec![
        vec![INVALID],
    ]
    .into_iter()
    .flatten()
    .collect();
    let code = make_evm_bytecode_from_runtime_code(&runtime_code);

    ExtBuilder::default().build().execute_with(|| {
        <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
        let Contract { addr, .. } =
            builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

        let output = builder::bare_call(addr)
            .gas_limit(expected_gas.into())
            .data(vec![])
            .build();
        
        let result = output.result;
        assert!(result.is_err(), "test did not error");
        let err = result.err().unwrap();
        if let sp_runtime::DispatchError::Module(module_error) = err {
            assert!(module_error.message.is_some(), "no message in module error");
            assert_eq!(module_error.message.unwrap(), "ContractTrapped", "Expected ContractTrapped error");
            assert_eq!(output.gas_consumed.ref_time(), expected_gas, "Gas consumed does not match expected gas");
            assert_eq!(output.gas_consumed.proof_size(), expected_gas, "Gas consumed does not match expected gas");
        } else {
            panic!("Expected ModuleError, got: {:?}", err);
        }
    });
}