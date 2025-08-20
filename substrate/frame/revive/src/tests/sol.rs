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

mod arithmetic;
mod bitwise;
mod block_info;
mod control;
mod host;
mod memory;
mod misc;
mod stack;
mod system;

use revm::bytecode::opcode::*;

pub fn make_evm_bytecode_from_runtime_code(runtime_code: &Vec<u8>) -> Vec<u8> {
    let runtime_code_len = runtime_code.len();
    assert!(runtime_code_len < 256, "runtime code length must be less than 256 bytes");
    let mut init_code: Vec<u8> = vec![
        vec![PUSH1, 0x80_u8],
        vec![PUSH1, 0x40_u8],
        vec![MSTORE],
        vec![PUSH1, 0x40_u8],
        vec![MLOAD],
        vec![PUSH1, runtime_code_len as u8],
        vec![PUSH1, 0x13_u8],
        vec![DUP3],
        vec![CODECOPY],
        vec![PUSH1, runtime_code_len as u8],
        vec![SWAP1],
        vec![RETURN],
        vec![INVALID],
    ]
    .into_iter()
    .flatten()
    .collect();
    init_code.extend(runtime_code);
    init_code
}