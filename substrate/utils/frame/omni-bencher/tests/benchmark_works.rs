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

use assert_cmd::cargo::cargo_bin;
use std::{
	fs,
	path::{Path, PathBuf},
	process::{Command, ExitStatus},
};

#[test]
fn benchmark_overhead_runtime_works() -> std::result::Result<(), String> {
	let tmp_dir = tempfile::tempdir().expect("Should be able to create tmp dir.");
	let base_path = tmp_dir.path();
	let wasm = cumulus_test_runtime::WASM_BINARY.ok_or("WASM binary not available".to_string())?;
	let runtime_path = base_path.join("runtime.wasm");
	let _ =
		fs::write(&runtime_path, wasm).map_err(|e| format!("Unable to write runtime file: {}", e));

	// Invoke `benchmark overhead` with all options to make sure that they are valid.
	let status = std::process::Command::new(cargo_bin("frame-omni-bencher"))
		.args(["v1", "benchmark", "overhead", "--runtime", runtime_path.to_str().unwrap()])
		.arg("-d")
		.arg(base_path)
		.arg("--weight-path")
		.arg(base_path)
		.args(["--warmup", "5", "--repeat", "5"])
		// Exotic para id to see that we are actually patching.
		.args(["--para-id", "666"])
		.args(["--add", "100", "--mul", "1.2", "--metric", "p75"])
		// Only put 5 extrinsics into the block otherwise it takes forever to build it
		// especially for a non-release builds.
		.args(["--max-ext-per-block", "5"])
		.status()
		.map_err(|e| format!("command failed: {:?}", e))?;

	assert_benchmark_success(status, base_path)
}
#[test]
fn benchmark_overhead_chain_spec_works() -> std::result::Result<(), String> {
	let tmp_dir = tempfile::tempdir().expect("Should be able to create tmp dir.");
	let (base_path, chain_spec_path) = setup_chain_spec(tmp_dir.path(), false)?;

	let status = create_benchmark_spec_command(&base_path, &chain_spec_path)
		.args(["--genesis-builder-policy", "spec-runtime"])
		.args(["--para-id", "666"])
		.status()
		.map_err(|e| format!("command failed: {:?}", e))?;

	assert_benchmark_success(status, &base_path)
}

#[test]
fn benchmark_overhead_chain_spec_works_plain_spec() -> std::result::Result<(), String> {
	let tmp_dir = tempfile::tempdir().expect("Should be able to create tmp dir.");
	let (base_path, chain_spec_path) = setup_chain_spec(tmp_dir.path(), false)?;

	let status = create_benchmark_spec_command(&base_path, &chain_spec_path)
		.args(["--genesis-builder-policy", "spec"])
		.args(["--para-id", "100"])
		.status()
		.map_err(|e| format!("command failed: {:?}", e))?;

	assert_benchmark_success(status, &base_path)
}

#[test]
fn benchmark_overhead_chain_spec_works_raw() -> std::result::Result<(), String> {
	let tmp_dir = tempfile::tempdir().expect("Should be able to create tmp dir.");
	let (base_path, chain_spec_path) = setup_chain_spec(tmp_dir.path(), true)?;

	let status = create_benchmark_spec_command(&base_path, &chain_spec_path)
		.args(["--genesis-builder-policy", "spec"])
		.args(["--para-id", "100"])
		.status()
		.map_err(|e| format!("command failed: {:?}", e))?;

	assert_benchmark_success(status, &base_path)
}

#[test]
fn benchmark_overhead_chain_spec_fails_wrong_para_id() -> std::result::Result<(), String> {
	let tmp_dir = tempfile::tempdir().expect("Should be able to create tmp dir.");
	let (base_path, chain_spec_path) = setup_chain_spec(tmp_dir.path(), false)?;

	let status = create_benchmark_spec_command(&base_path, &chain_spec_path)
		.args(["--genesis-builder-policy", "spec"])
		.args(["--para-id", "666"])
		.status()
		.map_err(|e| format!("command failed: {:?}", e))?;

	if status.success() {
		return Err("Command should have failed!".into())
	}

	// Weight files should not have been created
	assert!(!base_path.join("block_weights.rs").exists());
	assert!(!base_path.join("extrinsic_weights.rs").exists());
	Ok(())
}

/// Sets up a temporary directory and creates a chain spec file
fn setup_chain_spec(tmp_dir: &Path, raw: bool) -> Result<(PathBuf, PathBuf), String> {
	let base_path = tmp_dir.to_path_buf();
	let chain_spec_path = base_path.join("chain_spec.json");

	let wasm = cumulus_test_runtime::WASM_BINARY.ok_or("WASM binary not available".to_string())?;

	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "UNIT".into());
	properties.insert("tokenDecimals".into(), 12.into());

	let chain_spec = sc_chain_spec::GenericChainSpec::<()>::builder(wasm, Default::default())
		.with_name("some-chain")
		.with_id("some-id")
		.with_properties(properties)
		.with_chain_type(sc_chain_spec::ChainType::Development)
		.with_genesis_config_preset_name(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET)
		.build();

	let json = chain_spec.as_json(raw).unwrap();
	fs::write(&chain_spec_path, json)
		.map_err(|e| format!("Unable to write chain-spec file: {}", e))?;

	Ok((base_path, chain_spec_path))
}

/// Creates a Command for the benchmark with common arguments
fn create_benchmark_spec_command(base_path: &Path, chain_spec_path: &Path) -> Command {
	let mut cmd = Command::new(cargo_bin("frame-omni-bencher"));
	cmd.args(["v1", "benchmark", "overhead", "--chain", chain_spec_path.to_str().unwrap()])
		.arg("-d")
		.arg(base_path)
		.arg("--weight-path")
		.arg(base_path)
		.args(["--warmup", "5", "--repeat", "5"])
		.args(["--add", "100", "--mul", "1.2", "--metric", "p75"])
		// Only put 5 extrinsics into the block otherwise it takes forever to build it
		.args(["--max-ext-per-block", "5"]);
	cmd
}

/// Checks if the benchmark completed successfully and created weight files
fn assert_benchmark_success(status: ExitStatus, base_path: &Path) -> Result<(), String> {
	if !status.success() {
		return Err("Command failed".into())
	}

	// Weight files have been created
	assert!(base_path.join("block_weights.rs").exists());
	assert!(base_path.join("extrinsic_weights.rs").exists());
	Ok(())
}
