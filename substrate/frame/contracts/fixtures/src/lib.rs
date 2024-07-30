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
use std::{fs, path::PathBuf};

/// Load a given wasm module and returns a wasm binary contents along with it's hash.
/// Use the legacy compile_module as fallback, if the rust fixture does not exist yet.
pub fn compile_module<T>(
	fixture_name: &str,
) -> anyhow::Result<(Vec<u8>, <T::Hashing as Hash>::Output)>
where
	T: frame_system::Config,
{
	let out_dir: PathBuf = env!("OUT_DIR").into();
	let fixture_path = out_dir.join(format!("{fixture_name}.wasm"));
	let binary = fs::read(fixture_path)?;
	let code_hash = T::Hashing::hash(&binary);
	Ok((binary, code_hash))
}

#[cfg(test)]
mod test {
	#[test]
	fn out_dir_should_have_compiled_mocks() {
		let out_dir: std::path::PathBuf = env!("OUT_DIR").into();
		assert!(out_dir.join("dummy.wasm").exists());
		#[cfg(feature = "riscv")]
		assert!(out_dir.join("dummy.polkavm").exists());
	}
}
