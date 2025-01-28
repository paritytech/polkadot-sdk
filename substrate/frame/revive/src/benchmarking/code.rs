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

use alloc::vec::Vec;
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

	/// A contract code of specified sizte that does nothing.
	pub fn sized(_size: u32) -> Self {
		Self::dummy()
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
