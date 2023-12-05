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

use sp_runtime::traits::Hash;
use std::{env::var, fs, path::PathBuf};

fn wat_root_dir() -> PathBuf {
	match (var("CARGO_MANIFEST_DIR"), var("CARGO_PKG_NAME")) {
		// When `CARGO_MANIFEST_DIR` is not set, Rust resolves relative paths from the root folder
		(Err(_), _) => "substrate/frame/contracts/fixtures/data".into(),
		(Ok(path), Ok(s)) if s == "pallet-contracts" => PathBuf::from(path).join("fixtures/data"),
		(Ok(path), Ok(s)) if s == "pallet-contracts-mock-network" =>
			PathBuf::from(path).parent().unwrap().join("fixtures/data"),
		(Ok(_), pkg_name) => panic!("Failed to resolve fixture dir for tests from {pkg_name:?}."),
	}
}

/// Load a given wasm module represented by a .wat file and returns a wasm binary contents along
/// with it's hash.
///
/// The fixture files are located under the `fixtures/` directory.
fn legacy_compile_module<T>(
	fixture_name: &str,
) -> anyhow::Result<(Vec<u8>, <T::Hashing as Hash>::Output)>
where
	T: frame_system::Config,
{
	let fixture_path = wat_root_dir().join(format!("{fixture_name}.wat"));
	let wasm_binary = wat::parse_file(fixture_path)?;
	let code_hash = T::Hashing::hash(&wasm_binary);
	Ok((wasm_binary, code_hash))
}

/// Load a given wasm module and returns a wasm binary contents along with it's hash.
/// Use the legacy compile_module as fallback, if the rust fixture does not exist yet.
pub fn compile_module<T>(
	fixture_name: &str,
) -> anyhow::Result<(Vec<u8>, <T::Hashing as Hash>::Output)>
where
	T: frame_system::Config,
{
	let out_dir: std::path::PathBuf = env!("OUT_DIR").into();
	let fixture_path = out_dir.join(format!("{fixture_name}.wasm"));
	match fs::read(fixture_path) {
		Ok(wasm_binary) => {
			let code_hash = T::Hashing::hash(&wasm_binary);
			Ok((wasm_binary, code_hash))
		},
		Err(_) => legacy_compile_module::<T>(fixture_name),
	}
}

#[cfg(test)]
mod test {
	#[test]
	fn out_dir_should_have_compiled_mocks() {
		let out_dir: std::path::PathBuf = env!("OUT_DIR").into();
		let dummy_wasm = out_dir.join("dummy.wasm");
		println!("dummy_wasm: {:?}", dummy_wasm);
		assert!(dummy_wasm.exists());
	}
}
