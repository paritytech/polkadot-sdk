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

use std::fs;
#[test]
fn benchmark_overhead_works() -> std::result::Result<(), String> {
	let tmp_dir = tempfile::tempdir().expect("could not create a temp dir");
	let base_path = tmp_dir.path();
	let wasm = cumulus_test_runtime::WASM_BINARY.ok_or("WASM binary not available".to_string())?;
	let runtime_path = base_path.join("runtime.wasm");
	let _ =
		fs::write(&runtime_path, wasm).map_err(|e| format!("Unable to write runtime file: {}", e));

	let path = assert_cmd::cargo::cargo_bin("frame-omni-bencher");
	// Invoke `benchmark overhead` with all options to make sure that they are valid.
	let status = std::process::Command::new(path)
		.args(["v1", "benchmark", "overhead", "--runtime", runtime_path.to_str().unwrap()])
		.arg("-d")
		.arg(base_path)
		.arg("--weight-path")
		.arg(base_path)
		.args(["--warmup", "5", "--repeat", "5"])
		.args(["--para-id", "666"])
		.args(["--add", "100", "--mul", "1.2", "--metric", "p75"])
		// Only put 5 extrinsics into the block otherwise it takes forever to build it
		// especially for a non-release builds.
		.args(["--max-ext-per-block", "5"])
		.status()
		.map_err(|e| format!("command failed: {:?}", e))?;

	if !status.success() {
		return Err("Command failed".into())
	}

	// Weight files have been created.
	assert!(base_path.join("block_weights.rs").exists());
	assert!(base_path.join("extrinsic_weights.rs").exists());
	Ok(())
}
