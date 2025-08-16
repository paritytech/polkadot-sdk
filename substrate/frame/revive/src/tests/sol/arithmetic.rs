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
	test_utils::{builder::Contract, ALICE},
	tests::{builder, ExtBuilder, Test},
	Code, Config,
};
use pallet_revive_uapi::ReturnFlags;

use alloy_core::{primitives::U256, primitives::I256, sol_types::SolInterface};
use frame_support::traits::fungible::Mutate;
use pallet_revive_fixtures::{compile_module_with_type, Arithmetic, FixtureType};
use pretty_assertions::assert_eq;

fn decode_revert_message(data: &[u8]) -> Option<String> {
    if data.len() < 4 {
        return None;
    }
    
    // Check if it's a standard Error(string) - selector is 0x08c379a0
    let error_selector = [0x08, 0xc3, 0x79, 0xa0];
    if data[0..4] != error_selector {
        return None;
    }
    
    // Skip the selector (first 4 bytes)
    let abi_data = &data[4..];
    
    if abi_data.len() < 64 {
        return None;
    }
    
    // ABI encoding: first 32 bytes = offset to string data (should be 0x20)
    // Next 32 bytes = length of string
    // Remaining bytes = string data
    
    let string_length = u32::from_be_bytes([
        abi_data[28], abi_data[29], abi_data[30], abi_data[31]
    ]) as usize;
    
    if abi_data.len() < 64 + string_length {
        return None;
    }
    
    // Extract the string data
    let string_data = &abi_data[64..64 + string_length];
    String::from_utf8(string_data.to_vec()).ok()
}

#[test]
fn arithmetic_works() {
	for fixture_type in [FixtureType::Solc, FixtureType::Resolc] {
		let (code, _) = compile_module_with_type("Arithmetic", fixture_type).unwrap();
		ExtBuilder::default().build().execute_with(|| {
			let _ = <Test as Config>::Currency::set_balance(&ALICE, 100_000_000_000);
			let Contract { addr, .. } =
				builder::bare_instantiate(Code::Upload(code)).build_and_unwrap_contract();

            {            
                let result = builder::bare_call(addr)
                    .data(
                        Arithmetic::ArithmeticCalls::testArithmetic(Arithmetic::testArithmeticCall { })
                            .abi_encode(),
                    )
                    .build_and_unwrap_result();
                if result.flags == ReturnFlags::REVERT {
                    if let Some(revert_msg) = decode_revert_message(&result.data) {
                        log::error!("Revert message: {}", revert_msg);
                    } else {
                        log::error!("Revert without message, raw data: {:?}", result.data);
                    }
                }
                
                assert!(
                    result.flags != ReturnFlags::REVERT,
                    "arithmetic test reverted"
                );
            }
		});
	}
}
