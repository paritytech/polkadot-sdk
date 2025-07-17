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

/// A contract entry.
struct Entry {
	/// The path to the contract source file.
	path: PathBuf,
	/// The contract name.
	name: String,
	/// The directory containing the contract files.
	dir: PathBuf,
}

impl Entry {
	/// Create a new contract entry from the given path.
	fn new(path: PathBuf) -> Self {
		let dir = path.parent().expect("path has parent; qed").to_path_buf();
		let name = path.file_stem().expect("file exits; qed").to_str().expect("name is valid unicode; qed").to_string();
		Self { path, name, dir }
	}

	/// Return the path to the contract source file.
	fn path(&self) -> &str {
		self.path.to_str().expect("path is valid unicode; qed")
	}

	/// Return the name of the contract.
	fn name(&self) -> &str {
		&self.name
	}

	/// Return the name of the polkavm file.
	fn out_filename(&self) -> String {
		format!("{}.polkavm", self.name())
	}

	/// Return the path to the Solidity file if it exists.
	fn solidity_path(&self) -> Option<PathBuf> {
		let sol_path = self.dir.join(format!("{}.sol", self.name));
		if sol_path.exists() {
			Some(sol_path)
		} else {
			None
		}
	}

	/// Return the name of the Solidity polkavm file.
	fn solidity_out_filename(&self) -> String {
		format!("{}_sol.polkavm", self.name())
	}
}

/// Collect all contract entries from the given source directory.
fn collect_entries(contracts_dir: &Path) -> Vec<Entry> {
	fs::read_dir(contracts_dir)
		.expect("src dir exists; qed")
		.filter_map(|entry| {
			let path = entry.expect("file exists; qed").path();
			if !path.is_dir() {
				return None
			}

			let name = path.file_name().expect("valid dir name; qed").to_str().expect("valid unicode; qed");
			let rust_file = path.join(format!("{}.rs", name));
			
			if rust_file.exists() {
				Some(Entry::new(rust_file))
			} else {
				None
			}
		})
		.collect::<Vec<_>>()
}

/// Create a `Cargo.toml` to compile the given contract entries.
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

fn invoke_solidity_build(entries: &[Entry], out_dir: &Path) -> Result<()> {
	for entry in entries {
		if let Some(sol_path) = entry.solidity_path() {
			println!("Compiling Solidity contract: {} to {}", sol_path.display(), out_dir.display());
			
			let output = Command::new("resolc")
				.arg(&sol_path)
				.arg("--bin")
				.arg("--output-dir")
				.arg(out_dir)
				.arg("--overwrite")
				.output()
				.with_context(|| format!("Failed to execute resolc for {}", sol_path.display()))?;
			
			if !output.status.success() {
				let stderr = String::from_utf8_lossy(&output.stderr);
				eprintln!("Solidity compilation failed for {}: {}", sol_path.display(), stderr);
				bail!("Failed to compile Solidity contract: {}", sol_path.display());
			}
			
			// resolc outputs files with different naming, so we need to find and rename them
			let contract_name = entry.name();
			let expected_output = out_dir.join(format!("{}.polkavm", contract_name));
			let desired_output = out_dir.join(entry.solidity_out_filename());
			
			if expected_output.exists() {
				fs::rename(&expected_output, &desired_output)
					.with_context(|| format!("Failed to rename {} to {}", expected_output.display(), desired_output.display()))?;
			}
		}
	}
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
		return Ok(())
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

/// Write the compiled contracts to the given output directory.
fn write_output(build_dir: &Path, out_dir: &Path, entries: Vec<Entry>) -> Result<()> {
	for entry in entries {
		post_process(
			&build_dir
				.join("target/riscv64emac-unknown-none-polkavm/release")
				.join(entry.name()),
			&out_dir.join(entry.out_filename()),
		)?;
	}

	Ok(())
}

/// Create a directory in the `target` as output directory
fn create_out_dir() -> Result<PathBuf> {
	let temp_dir: PathBuf =
		env::var("OUT_DIR").context("Failed to fetch `OUT_DIR` env variable")?.into();

	// this is set in case the user has overriden the target directory
	let out_dir = if let Ok(path) = env::var("CARGO_TARGET_DIR") {
		let path = PathBuf::from(path);

		if path.is_absolute() {
			path
		} else {
			let output = std::process::Command::new(env!("CARGO"))
				.arg("locate-project")
				.arg("--workspace")
				.arg("--message-format=plain")
				.output()
				.context("Failed to determine workspace root")?
				.stdout;

			let workspace_root = Path::new(
				std::str::from_utf8(&output)
					.context("Invalid output from `locate-project`")?
					.trim(),
			)
			.parent()
			.expect("Workspace root path contains the `Cargo.toml`; qed");

			PathBuf::from(workspace_root).join(path)
		}
	} else {
		// otherwise just traverse up from the out dir
		let mut out_dir: PathBuf = temp_dir.clone();
		loop {
			if !out_dir.pop() {
				bail!("Cannot find project root.")
			}
			if out_dir.join("Cargo.lock").exists() {
				break;
			}
		}
		out_dir.join("target")
	}
	.join("pallet-revive-fixtures");

	// clean up some leftover symlink from previous versions of this script
	let mut out_exists = out_dir.exists();
	if out_exists && !out_dir.is_dir() {
		fs::remove_file(&out_dir).context("Failed to remove `OUT_DIR`.")?;
		out_exists = false;
	}

	if !out_exists {
		fs::create_dir(&out_dir)
			.context(format!("Failed to create output directory: {})", out_dir.display(),))?;
	}

	// write the location of the out dir so it can be found later
	let mut file = fs::File::create(temp_dir.join("fixture_location.rs"))
		.context("Failed to create fixture_location.rs")?;
	write!(
		file,
		r#"
			#[allow(dead_code)]
			const FIXTURE_DIR: &str = "{0}";
			macro_rules! fixture {{
				($name: literal) => {{
					include_bytes!(concat!("{0}", "/", $name, ".polkavm"))
				}};
			}}
		"#,
		out_dir.display()
	)
	.context("Failed to write to fixture_location.rs")?;

	Ok(out_dir)
}

pub fn main() -> Result<()> {
	let fixtures_dir: PathBuf = env::var("CARGO_MANIFEST_DIR")?.into();
	let contracts_dir = fixtures_dir.join("contracts");
	let out_dir = create_out_dir().context("Cannot determine output directory")?;
	let build_dir = out_dir.join("build");
	fs::create_dir_all(&build_dir).context("Failed to create build directory")?;

	println!("cargo::rerun-if-env-changed={OVERRIDE_RUSTUP_TOOLCHAIN_ENV_VAR}");
	println!("cargo::rerun-if-env-changed={OVERRIDE_STRIP_ENV_VAR}");
	println!("cargo::rerun-if-env-changed={OVERRIDE_OPTIMIZE_ENV_VAR}");
	println!("cargo::rerun-if-changed={}", out_dir.display());

	// the fixtures have a dependency on the uapi crate
	println!("cargo::rerun-if-changed={}", fixtures_dir.display());
	let uapi_dir = fixtures_dir.parent().expect("parent dir exits; qed").join("uapi");
	if uapi_dir.exists() {
		println!("cargo::rerun-if-changed={}", uapi_dir.display());
	}

	let entries = collect_entries(&contracts_dir);
	if entries.is_empty() {
		return Ok(())
	}

	create_cargo_toml(&fixtures_dir, entries.iter(), &build_dir)?;
	invoke_build(&build_dir)?;
	invoke_solidity_build(&entries, &out_dir)?;
	write_output(&build_dir, &out_dir, entries)?;

	Ok(())
}
