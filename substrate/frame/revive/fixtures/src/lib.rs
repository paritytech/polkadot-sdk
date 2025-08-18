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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

// generated file that tells us where to find the fixtures
include!(concat!(env!("OUT_DIR"), "/fixture_location.rs"));

/// Load a given polkavm module and returns a polkavm binary contents along with its hash.
#[cfg(feature = "std")]
pub fn compile_module(fixture_name: &str) -> anyhow::Result<(Vec<u8>, sp_core::H256)> {
	let out_dir: std::path::PathBuf = FIXTURE_DIR.into();
	let fixture_path = out_dir.join(format!("{fixture_name}.polkavm"));
	let binary = std::fs::read(fixture_path)?;
	let code_hash = sp_io::hashing::keccak_256(&binary);
	Ok((binary, sp_core::H256(code_hash)))
}

/// Fixtures used in runtime benchmarks.
///
/// We explicitly include those fixtures into the binary to make them
/// available in no-std environments (runtime benchmarks).
pub mod bench {
	use alloc::vec::Vec;
	pub const DUMMY: &[u8] = fixture!("dummy");
	pub const NOOP: &[u8] = fixture!("noop");

	pub fn dummy_unique(replace_with: u32) -> Vec<u8> {
		let mut dummy = DUMMY.to_vec();
		let idx = dummy
			.windows(4)
			.position(|w| w == &[0xDE, 0xAD, 0xBE, 0xEF])
			.expect("Benchmark fixture contains this pattern; qed");
		dummy[idx..idx + 4].copy_from_slice(&replace_with.to_le_bytes());
		dummy
	}
}

#[cfg(test)]
mod test {
	#[test]
	fn out_dir_should_have_compiled_mocks() {
		let out_dir: std::path::PathBuf = crate::FIXTURE_DIR.into();
		assert!(out_dir.join("dummy.polkavm").exists());
	}
}
