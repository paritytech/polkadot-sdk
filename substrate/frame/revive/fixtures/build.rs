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

//! Compile text fixtures to PolkaVM binaries.
use anyhow::{bail, Context, Result};
use cargo_metadata::MetadataCommand;
use std::{
	env, fs,
	io::Write,
	path::{Path, PathBuf},
	process::Command,
};

const OVERRIDE_RUSTUP_TOOLCHAIN_ENV_VAR: &str = "PALLET_REVIVE_FIXTURES_RUSTUP_TOOLCHAIN";
const OVERRIDE_STRIP_ENV_VAR: &str = "PALLET_REVIVE_FIXTURES_STRIP";
const OVERRIDE_OPTIMIZE_ENV_VAR: &str = "PALLET_REVIVE_FIXTURES_OPTIMIZE";
/// Do not build the fixtures, they will resolve to `None`.
///
/// Depending on the usage, they will probably panic at runtime.
const SKIP_PALLET_REVIVE_FIXTURES: &str = "SKIP_PALLET_REVIVE_FIXTURES";

/// A contract entry.
#[derive(Clone)]
struct Entry {
	/// The path to the contract source file.
	path: PathBuf,
	/// The type of the contract (rust or solidity).
	contract_type: ContractType,
}

#[derive(Clone, Copy)]
enum ContractType {
	Rust,
	Solidity,
}

impl Entry {
	/// Create a new contract entry from the given path.
	fn new(path: PathBuf, contract_type: ContractType) -> Self {
		Self { path, contract_type }
	}

	/// Return the path to the contract source file.
	fn path(&self) -> &str {
		self.path.to_str().expect("path is valid unicode; qed")
	}

	/// Return the name of the contract.
	fn name(&self) -> &str {
		self.path
			.file_stem()
			.expect("file exits; qed")
			.to_str()
			.expect("name is valid unicode; qed")
	}

	/// Return the name of the bytecode file.
	fn out_filename(&self) -> String {
		match self.contract_type {
			ContractType::Rust => format!("{}.polkavm", self.name()),
			ContractType::Solidity => format!("{}.resolc.polkavm", self.name()),
		}
	}
}

/// Collect all contract entries from the given source directory.
fn collect_entries(contracts_dir: &Path) -> Vec<Entry> {
	fs::read_dir(contracts_dir)
		.expect("src dir exists; qed")
		.filter_map(|file| {
			let path = file.expect("file exists; qed").path();
			let extension = path.extension();

			match extension.and_then(|ext| ext.to_str()) {
				Some("rs") => Some(Entry::new(path, ContractType::Rust)),
				Some("sol") => Some(Entry::new(path, ContractType::Solidity)),
				_ => None,
			}
		})
		.collect::<Vec<_>>()
}

/// Create a `Cargo.toml` to compile the given Rust contract entries.
fn create_cargo_toml<'a>(
	fixtures_dir: &Path,
	entries: impl Iterator<Item = &'a Entry>,
	output_dir: &Path,
) -> Result<()> {
	let mut cargo_toml: toml::Value = toml::from_str(include_str!("./build/_Cargo.toml"))?;
	let uapi_dep = cargo_toml["dependencies"]["uapi"].as_table_mut().unwrap();

	let manifest_path = fixtures_dir.join("Cargo.toml");
	let metadata = MetadataCommand::new().manifest_path(&manifest_path).exec().unwrap();
	let dependency_graph = metadata.resolve.unwrap();

	// Resolve the pallet-revive-fixtures package id
	let fixtures_pkg_id = metadata
		.packages
		.iter()
		.find(|pkg| pkg.manifest_path.as_std_path() == manifest_path)
		.map(|pkg| pkg.id.clone())
		.unwrap();
	let fixtures_pkg_node =
		dependency_graph.nodes.iter().find(|node| node.id == fixtures_pkg_id).unwrap();

	// Get the pallet-revive-uapi package id
	let uapi_pkg_id = fixtures_pkg_node
		.deps
		.iter()
		.find(|dep| dep.name == "pallet_revive_uapi")
		.map(|dep| dep.pkg.clone())
		.expect("pallet-revive-uapi is a build dependency of pallet-revive-fixtures; qed");

	// Get pallet-revive-uapi package
	let uapi_pkg = metadata.packages.iter().find(|pkg| pkg.id == uapi_pkg_id).unwrap();

	if uapi_pkg.source.is_none() {
		uapi_dep.insert(
			"path".to_string(),
			toml::Value::String(
				fixtures_dir.join("../uapi").canonicalize()?.to_str().unwrap().to_string(),
			),
		);
	} else {
		uapi_dep.insert("version".to_string(), toml::Value::String(uapi_pkg.version.to_string()));
	}

	cargo_toml["bin"] = toml::Value::Array(
		entries
			.map(|entry| {
				let name = entry.name();
				let path = entry.path();
				toml::Value::Table(toml::toml! {
					name = name
					path = path
				})
			})
			.collect::<Vec<_>>(),
	);

	let cargo_toml = toml::to_string_pretty(&cargo_toml)?;
	fs::write(output_dir.join("Cargo.toml"), cargo_toml.clone())
		.with_context(|| format!("Failed to write {cargo_toml:?}"))?;
	Ok(())
}

fn invoke_build(current_dir: &Path) -> Result<()> {
	let encoded_rustflags = ["-Dwarnings"].join("\x1f");

	let mut build_command = Command::new("cargo");
	build_command
		.current_dir(current_dir)
		.env_clear()
		.env("PATH", env::var("PATH").unwrap_or_default())
		.env("CARGO_ENCODED_RUSTFLAGS", encoded_rustflags)
		.env("RUSTUP_HOME", env::var("RUSTUP_HOME").unwrap_or_default())
		// Support compilation on stable rust
		.env("RUSTC_BOOTSTRAP", "1")
		.args([
			"build",
			"--release",
			"-Zbuild-std=core",
			"-Zbuild-std-features=panic_immediate_abort",
		])
		.arg("--target")
		.arg(polkavm_linker::target_json_64_path().unwrap());

	if let Ok(toolchain) = env::var(OVERRIDE_RUSTUP_TOOLCHAIN_ENV_VAR) {
		build_command.env("RUSTUP_TOOLCHAIN", &toolchain);
	}

	let build_res = build_command.output().expect("failed to execute process");

	if build_res.status.success() {
		return Ok(());
	}

	let stderr = String::from_utf8_lossy(&build_res.stderr);
	eprintln!("{}", stderr);

	bail!("Failed to build contracts");
}

/// Post-process the compiled code.
fn post_process(input_path: &Path, output_path: &Path) -> Result<()> {
	let strip = env::var(OVERRIDE_STRIP_ENV_VAR).map_or(false, |value| value == "1");
	let optimize = env::var(OVERRIDE_OPTIMIZE_ENV_VAR).map_or(true, |value| value == "1");

	let mut config = polkavm_linker::Config::default();
	config.set_strip(strip);
	config.set_optimize(optimize);
	let orig = fs::read(input_path).with_context(|| format!("Failed to read {input_path:?}"))?;
	let linked = polkavm_linker::program_from_elf(config, orig.as_ref())
		.map_err(|err| anyhow::format_err!("Failed to link polkavm program: {}", err))?;
	fs::write(output_path, linked).with_context(|| format!("Failed to write {output_path:?}"))?;
	Ok(())
}

/// Compile a Solidity contract using standard JSON interface.
fn compile_with_standard_json(
	compiler: &str,
	contracts_dir: &Path,
	solidity_entries: &[&Entry],
) -> Result<serde_json::Value> {
	let mut input_json = serde_json::json!({
		"language": "Solidity",
		"sources": {},
		"settings": {
			"optimizer": {
				"enabled": false,
				"runs": 200
			},
			"outputSelection":

		serde_json::json!({
			"*": {
				"*": ["evm.bytecode"]
			}
		}),

		}
	});

	// Add all Solidity files to the input
	for entry in solidity_entries {
		let source_code = fs::read_to_string(entry.path())
			.with_context(|| format!("Failed to read Solidity source: {}", entry.path()))?;

		let file_key = entry.path().split('/').last().unwrap_or(entry.name());
		input_json["sources"][file_key] = serde_json::json!({
			"content": source_code
		});
	}

	let compiler_output = Command::new(compiler)
		.current_dir(contracts_dir)
		.arg("--standard-json")
		.stdin(std::process::Stdio::piped())
		.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::piped())
		.spawn()
		.with_context(|| {
			format!(
				"Failed to execute {compiler}. Make sure {compiler} is installed or \
				set env variable `{SKIP_PALLET_REVIVE_FIXTURES}=1` to skip fixtures compilation."
			)
		})?;

	let mut stdin = compiler_output.stdin.as_ref().unwrap();
	stdin
		.write_all(input_json.to_string().as_bytes())
		.with_context(|| format!("Failed to write to {} stdin", compiler))?;
	let _ = stdin;

	let compiler_result = compiler_output
		.wait_with_output()
		.with_context(|| format!("Failed to wait for {} output", compiler))?;

	if !compiler_result.status.success() {
		let stderr = String::from_utf8_lossy(&compiler_result.stderr);
		bail!("{} compilation failed: {}", compiler, stderr);
	}

	// Parse JSON output
	let compiler_json: serde_json::Value = serde_json::from_slice(&compiler_result.stdout)
		.with_context(|| format!("Failed to parse {} JSON output", compiler))?;

	// Abort on errors
	if let Some(errors) = compiler_json.get("errors") {
		if errors
			.as_array()
			.unwrap()
			.iter()
			.any(|object| object.get("severity").unwrap().as_str().unwrap() == "error")
		{
			bail!(
				"failed to compile the Solidity fixtures: {}",
				serde_json::to_string_pretty(errors)?
			);
		}
	}

	Ok(compiler_json)
}

/// Extract bytecode from compiler JSON output and write binary files.
fn extract_and_write_bytecode(
	compiler_json: &serde_json::Value,
	out_dir: &Path,
	file_suffix: &str,
) -> Result<()> {
	if let Some(contracts) = compiler_json["contracts"].as_object() {
		for (_file_key, file_contracts) in contracts {
			if let Some(contract_map) = file_contracts.as_object() {
				for (contract_name, contract_data) in contract_map {
					// Navigate through the JSON path to find the bytecode
					let mut current = contract_data;
					for path_segment in ["evm", "bytecode", "object"] {
						if let Some(next) = current.get(path_segment) {
							current = next;
						} else {
							// Skip if path doesn't exist (e.g., contract has no bytecode)
							continue;
						}
					}

					if let Some(bytecode_obj) = current.as_str() {
						let bytecode_hex = bytecode_obj.strip_prefix("0x").unwrap_or(bytecode_obj);
						let binary_content = hex::decode(bytecode_hex).map_err(|e| {
							anyhow::anyhow!("Failed to decode hex for {contract_name}: {e}")
						})?;

						let out_path = out_dir.join(format!("{}{}", contract_name, file_suffix));
						fs::write(&out_path, binary_content).with_context(|| {
							format!("Failed to write {out_path:?} for {contract_name}")
						})?;
					}
				}
			}
		}
	}
	Ok(())
}

/// Compile Solidity contracts using both solc and resolc.
fn compile_solidity_contracts(
	contracts_dir: &Path,
	out_dir: &Path,
	entries: &[Entry],
) -> Result<()> {
	let solidity_entries: Vec<_> = entries
		.iter()
		.filter(|entry| matches!(entry.contract_type, ContractType::Solidity))
		.collect();

	if solidity_entries.is_empty() {
		return Ok(());
	}

	let evm_only = vec!["HostEvmOnly"];
	let solidity_entries_pvm: Vec<_> = solidity_entries
		.iter()
		.cloned()
		.filter(|entry| !evm_only.contains(&entry.path.file_stem().unwrap().to_str().unwrap()))
		.collect();

	// Compile with solc for EVM bytecode
	let json = compile_with_standard_json("solc", contracts_dir, &solidity_entries)?;
	extract_and_write_bytecode(&json, out_dir, ".sol.bin")?;

	// Compile with resolc for PVM bytecode
	let json = compile_with_standard_json("resolc", contracts_dir, &solidity_entries_pvm)?;
	extract_and_write_bytecode(&json, out_dir, ".resolc.polkavm")?;

	Ok(())
}

/// Write the compiled Rust contracts to the given output directory.
fn write_output(build_dir: &Path, out_dir: &Path, entries: Vec<Entry>) -> Result<()> {
	for entry in entries {
		if matches!(entry.contract_type, ContractType::Rust) {
			post_process(
				&build_dir
					.join("target/riscv64emac-unknown-none-polkavm/release")
					.join(entry.name()),
				&out_dir.join(entry.out_filename()),
			)?;
		}
	}

	Ok(())
}

/// Generate the fixture_location.rs file with macros and sol! definitions.
fn generate_fixture_location(temp_dir: &Path, out_dir: &Path, entries: &[Entry]) -> Result<()> {
	let mut file = fs::File::create(temp_dir.join("fixture_location.rs"))
		.context("Failed to create fixture_location.rs")?;

	let (fixtures, fixtures_resolc) = if env::var(SKIP_PALLET_REVIVE_FIXTURES).is_err() {
		(
			format!(
				r#"Some(include_bytes!(concat!("{}", "/", $name, ".polkavm")))"#,
				out_dir.display()
			),
			format!(
				r#"Some(include_bytes!(concat!("{}", "/", $name, ".resolc.polkavm")))"#,
				out_dir.display()
			),
		)
	} else {
		("None".into(), "None".into())
	};

	write!(
		file,
		r#"
			#[allow(dead_code)]
			const FIXTURE_DIR: &str = "{0}";

			#[macro_export]
			macro_rules! fixture {{
				($name: literal) => {{
					{fixtures}
				}};
			}}

			#[macro_export]
			macro_rules! fixture_resolc {{
				($name: literal) => {{
					{fixtures_resolc}
				}};
			}}
		"#,
		out_dir.display()
	)
	.context("Failed to write to fixture_location.rs")?;

	// Generate sol! macros for Solidity contracts
	for entry in entries.iter().filter(|e| matches!(e.contract_type, ContractType::Solidity)) {
		let relative_path = format!("contracts/{}", entry.path().split('/').last().unwrap());
		writeln!(file, r#"#[cfg(feature = "std")] alloy_core::sol!("{}");"#, relative_path)
			.context("Failed to write sol! macro to fixture_location.rs")?;
	}

	Ok(())
}

pub fn main() -> Result<()> {
	// input pathes
	let fixtures_dir: PathBuf = env::var("CARGO_MANIFEST_DIR")?.into();
	let contracts_dir = fixtures_dir.join("contracts");

	// output pathes
	let out_dir: PathBuf =
		env::var("OUT_DIR").context("Failed to fetch `OUT_DIR` env variable")?.into();
	let out_fixtures_dir = out_dir.join("fixtures");
	let out_build_dir = out_dir.join("build");
	fs::create_dir_all(&out_fixtures_dir).context("Failed to create output fixture directory")?;
	fs::create_dir_all(&out_build_dir).context("Failed to create output build directory")?;

	println!("cargo::rerun-if-env-changed={OVERRIDE_RUSTUP_TOOLCHAIN_ENV_VAR}");
	println!("cargo::rerun-if-env-changed={OVERRIDE_STRIP_ENV_VAR}");
	println!("cargo::rerun-if-env-changed={OVERRIDE_OPTIMIZE_ENV_VAR}");

	// the fixtures have a dependency on the uapi crate
	println!("cargo::rerun-if-changed={}", fixtures_dir.display());
	let uapi_dir = fixtures_dir.parent().expect("parent dir exits; qed").join("uapi");
	if uapi_dir.exists() {
		println!("cargo::rerun-if-changed={}", uapi_dir.display());
	}

	let entries = collect_entries(&contracts_dir);
	if entries.is_empty() {
		return Ok(());
	}

	if env::var(SKIP_PALLET_REVIVE_FIXTURES).is_err() {
		// Compile Rust contracts
		let rust_entries: Vec<_> = entries
			.iter()
			.filter(|e| matches!(e.contract_type, ContractType::Rust))
			.collect();
		if !rust_entries.is_empty() {
			create_cargo_toml(&fixtures_dir, rust_entries.into_iter(), &out_build_dir)?;
			invoke_build(&out_build_dir)?;
			write_output(&out_build_dir, &out_fixtures_dir, entries.clone())?;
		}

		// Compile Solidity contracts
		compile_solidity_contracts(&contracts_dir, &out_fixtures_dir, &entries)?;
	}

	// Generate fixture_location.rs with sol! macros
	generate_fixture_location(&out_dir, &out_fixtures_dir, &entries)?;

	Ok(())
}
