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

//! Functions to procedurally construct contract code used for benchmarking.
//!
//! In order to be able to benchmark events that are triggered by contract execution
//! (API calls into seal, individual instructions), we need to generate contracts that
//! perform those events. Because those contracts can get very big we cannot simply define
//! them as text (.wat) as this will be too slow and consume too much memory. Therefore
//! we define this simple definition of a contract that can be passed to `create_code` that
//! compiles it down into a `WasmModule` that can be used as a contract's code.

use crate::limits;
use alloc::{fmt::Write, string::ToString, vec::Vec};
use pallet_revive_fixtures::bench as bench_fixtures;
use sp_core::H256;
use sp_io::hashing::keccak_256;

/// A wasm module ready to be put on chain.
#[derive(Clone)]
pub struct WasmModule {
	pub code: Vec<u8>,
	pub hash: H256,
}

impl WasmModule {
	/// Return a contract code that does nothing.
	pub fn dummy() -> Self {
		Self::new(bench_fixtures::DUMMY.to_vec())
	}

	/// Same as [`Self::dummy`] but uses `replace_with` to make the code unique.
	pub fn dummy_unique(replace_with: u32) -> Self {
		Self::new(bench_fixtures::dummy_unique(replace_with))
	}

	/// Same as as `with_num_instructions` but based on the blob size.
	///
	/// This is needed when we weigh a blob without knowing how much instructions it
	/// contains.
	pub fn sized(size: u32) -> Self {
		// Due to variable length encoding of instructions this is not precise. But we only
		// need rough numbers for our benchmarks.
		Self::with_num_instructions(size / 3)
	}

	/// A contract code of specified number of instructions that uses all its bytes for instructions
	/// but will return immediately.
	///
	/// All the basic blocks are maximum sized (only the first is important though). This is to
	/// account for the fact that the interpreter will compile one basic block at a time even
	/// when no code is executed. Hence this contract will trigger the compilation of a maximum
	/// sized basic block and then return with its first instruction.
	///
	/// All the code will be put into the "call" export. Hence this code can be safely used for the
	/// `instantiate_with_code` benchmark where no compilation of any block should be measured.
	pub fn with_num_instructions(num_instructions: u32) -> Self {
		let mut text = "
		pub @deploy:
		ret
		pub @call:
		"
		.to_string();
		for i in 0..num_instructions {
			match i {
				// return execution right away without breaking up basic block
				// SENTINEL is a hard coded syscall that terminates execution
				0 => writeln!(text, "ecalli {}", crate::SENTINEL).unwrap(),
				i if i % (limits::code::BASIC_BLOCK_SIZE - 1) == 0 =>
					text.push_str("fallthrough\n"),
				_ => text.push_str("a0 = a1 + a2\n"),
			}
		}
		text.push_str("ret\n");
		let code = polkavm_common::assembler::assemble(&text).unwrap();
		Self::new(code)
	}

	/// A contract code that calls the "noop" host function in a loop depending in the input.
	pub fn noop() -> Self {
		Self::new(bench_fixtures::NOOP.to_vec())
	}

	/// A contract code that executes some ALU instructions in a loop.
	pub fn instr() -> Self {
		Self::new(bench_fixtures::INSTR.to_vec())
	}

	fn new(code: Vec<u8>) -> Self {
		let hash = keccak_256(&code);
		Self { code, hash: H256(hash) }
	}
}
