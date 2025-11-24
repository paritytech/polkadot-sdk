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

//! Shared code for building fixtures from Rust source.
//! Used by both build.rs and test code.

use anyhow::{Context, Result};
use std::{
	env,
	fs,
	path::{Path, PathBuf},
	process::Command,
};

/// Entry representing a contract to build.
pub struct BuildEntry {
	pub name: String,
	pub path: String,
}

impl BuildEntry {
	pub fn new(name: impl Into<String>, path: impl Into<String>) -> Self {
		Self { name: name.into(), path: path.into() }
	}
}

/// Create a Cargo.toml for building contracts using the template from build/_Cargo.toml.
pub fn create_cargo_toml(
	entries: &[BuildEntry],
	output_dir: &Path,
) -> Result<()> {
	let mut cargo_toml: toml::Value = toml::from_str(include_str!("../build/_Cargo.toml"))?;

	// Set uapi dependency path
	let uapi_dep = cargo_toml["dependencies"]["uapi"].as_table_mut().unwrap();
	let manifest_dir = env!("CARGO_MANIFEST_DIR");
	let uapi_path = PathBuf::from(manifest_dir).parent().unwrap().join("uapi");
	uapi_dep.insert(
		"path".to_string(),
		toml::Value::String(uapi_path.to_str().unwrap().to_string()),
	);

	// Set binary targets
	cargo_toml["bin"] = toml::Value::Array(
		entries
			.iter()
			.map(|entry| {
				let mut table = toml::map::Map::new();
				table.insert("name".to_string(), toml::Value::String(entry.name.clone()));
				table.insert("path".to_string(), toml::Value::String(entry.path.clone()));
				toml::Value::Table(table)
			})
			.collect::<Vec<_>>(),
	);

	let cargo_toml_str = toml::to_string_pretty(&cargo_toml)?;
	fs::write(output_dir.join("Cargo.toml"), cargo_toml_str)
		.with_context(|| format!("Failed to write Cargo.toml to {:?}", output_dir))?;
	Ok(())
}

/// Invoke cargo build to compile contracts to RISC-V ELF.
pub fn invoke_build(current_dir: &Path) -> Result<()> {
	let encoded_rustflags = ["-Dwarnings"].join("\x1f");

	let mut args = polkavm_linker::TargetJsonArgs::default();
	args.is_64_bit = true;

	let mut build_command = Command::new("cargo");
	build_command
		.current_dir(current_dir)
		.env_clear()
		.env("PATH", env::var("PATH").unwrap_or_default())
		.env("CARGO_ENCODED_RUSTFLAGS", encoded_rustflags)
		.env("RUSTUP_HOME", env::var("RUSTUP_HOME").unwrap_or_default())
		.env("RUSTC_BOOTSTRAP", "1")
		.args([
			"build",
			"--release",
			"-Zbuild-std=core",
			"-Zbuild-std-features=panic_immediate_abort",
		])
		.arg("--target")
		.arg(polkavm_linker::target_json_path(args).unwrap());

	if let Ok(toolchain) = env::var("PALLET_REVIVE_FIXTURES_RUSTUP_TOOLCHAIN") {
		build_command.env("RUSTUP_TOOLCHAIN", &toolchain);
	}

	let build_res = build_command.output().expect("failed to execute process");

	if !build_res.status.success() {
		let stderr = String::from_utf8_lossy(&build_res.stderr);
		eprintln!("{}", stderr);
		anyhow::bail!("Failed to build contracts");
	}

	Ok(())
}

/// Compile a Rust contract source to RISC-V ELF.
pub fn compile_rust_to_elf(
	contract_path: &Path,
	contract_name: &str,
	output_dir: &Path,
) -> Result<PathBuf> {
	// Create Cargo.toml with single entry
	let entry = BuildEntry::new(contract_name, contract_path.to_str().unwrap());
	create_cargo_toml(&[entry], output_dir)?;

	// Build
	invoke_build(output_dir)?;

	// Return path to ELF
	let elf_path = output_dir
		.join("target/riscv64emac-unknown-none-polkavm/release")
		.join(contract_name);

	if !elf_path.exists() {
		anyhow::bail!("ELF not found at {:?}", elf_path);
	}

	Ok(elf_path)
}

/// Link a RISC-V ELF to PolkaVM bytecode.
pub fn link_elf_to_polkavm(elf_path: &Path) -> Result<Vec<u8>> {
	let elf_bytes = std::fs::read(elf_path)
		.with_context(|| format!("Failed to read ELF from {:?}", elf_path))?;

	let config = polkavm_linker::Config::default();
	let linked = polkavm_linker::program_from_elf(
		config,
		polkavm_linker::TargetInstructionSet::ReviveV1,
		&elf_bytes,
	)
	.map_err(|err| anyhow::anyhow!("Failed to link polkavm program from {:?}: {}", elf_path, err))?;

	Ok(linked)
}

/// Compile a Rust contract source all the way to PolkaVM bytecode.
pub fn compile_rust_to_polkavm(
	contract_path: &Path,
	contract_name: &str,
	temp_dir: &Path,
) -> Result<Vec<u8>> {
	let elf_path = compile_rust_to_elf(contract_path, contract_name, temp_dir)?;
	link_elf_to_polkavm(&elf_path)
}
