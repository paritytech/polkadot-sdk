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

//! Compile contracts to wasm and RISC-V binaries.
use anyhow::{bail, Context, Result};
use parity_wasm::elements::{deserialize_file, serialize_to_file, Internal};
use std::{
	env, fs,
	hash::Hasher,
	path::{Path, PathBuf},
	process::Command,
};
use twox_hash::XxHash32;

/// Read the file at `path` and return its hash as a hex string.
fn file_hash(path: &Path) -> String {
	let data = fs::read(path).expect("file exists; qed");
	let mut hasher = XxHash32::default();
	hasher.write(&data);
	hasher.write(include_bytes!("build.rs"));
	let hash = hasher.finish();
	format!("{:x}", hash)
}

/// A contract entry.
struct Entry {
	/// The path to the contract source file.
	path: PathBuf,
	/// The hash of the contract source file.
	hash: String,
}

impl Entry {
	/// Create a new contract entry from the given path.
	fn new(path: PathBuf) -> Self {
		let hash = file_hash(&path);
		Self { path, hash }
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

	/// Return whether the contract has already been compiled.
	fn is_cached(&self, out_dir: &Path) -> bool {
		out_dir.join(self.name()).join(&self.hash).exists()
	}

	/// Update the cache file for the contract.
	fn update_cache(&self, out_dir: &Path) -> Result<()> {
		let cache_dir = out_dir.join(self.name());

		// clear the cache dir if it exists
		if cache_dir.exists() {
			fs::remove_dir_all(&cache_dir)?;
		}

		// re-populate the cache dir with the new hash
		fs::create_dir_all(&cache_dir)?;
		fs::write(out_dir.join(&self.hash), "")?;
		Ok(())
	}

	/// Return the name of the output wasm file.
	fn out_wasm_filename(&self) -> String {
		format!("{}.wasm", self.name())
	}

	/// Return the name of the RISC-V polkavm file.
	#[cfg(feature = "riscv")]
	fn out_riscv_filename(&self) -> String {
		format!("{}.polkavm", self.name())
	}
}

/// Collect all contract entries from the given source directory.
/// Contracts that have already been compiled are filtered out.
fn collect_entries(contracts_dir: &Path, out_dir: &Path) -> Vec<Entry> {
	fs::read_dir(contracts_dir)
		.expect("src dir exists; qed")
		.filter_map(|file| {
			let path = file.expect("file exists; qed").path();
			if path.extension().map_or(true, |ext| ext != "rs") {
				return None
			}

			let entry = Entry::new(path);
			if entry.is_cached(out_dir) {
				None
			} else {
				Some(entry)
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
	let mut cargo_toml: toml::Value = toml::from_str(include_str!("./build/Cargo.toml"))?;
	let mut set_dep = |name, path| -> Result<()> {
		cargo_toml["dependencies"][name]["path"] = toml::Value::String(
			fixtures_dir.join(path).canonicalize()?.to_str().unwrap().to_string(),
		);
		Ok(())
	};
	set_dep("uapi", "../uapi")?;
	set_dep("common", "./contracts/common")?;

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
	fs::write(output_dir.join("Cargo.toml"), cargo_toml).map_err(Into::into)
}

/// Invoke `cargo fmt` to check that fixtures files are formatted.
fn invoke_cargo_fmt<'a>(
	config_path: &Path,
	files: impl Iterator<Item = &'a Path>,
	contract_dir: &Path,
) -> Result<()> {
	// If rustfmt is not installed, skip the check.
	if !Command::new("rustup")
		.args(["run", "nightly", "rustfmt", "--version"])
		.output()
		.map_or(false, |o| o.status.success())
	{
		return Ok(())
	}

	let fmt_res = Command::new("rustup")
		.args(["run", "nightly", "rustfmt", "--check", "--config-path"])
		.arg(config_path)
		.args(files)
		.output()
		.expect("failed to execute process");

	if fmt_res.status.success() {
		return Ok(())
	}

	let stdout = String::from_utf8_lossy(&fmt_res.stdout);
	let stderr = String::from_utf8_lossy(&fmt_res.stderr);
	eprintln!("{}\n{}", stdout, stderr);
	eprintln!(
		"Fixtures files are not formatted.\n
		Please run `rustup run nightly rustfmt --config-path {} {}/*.rs`",
		config_path.display(),
		contract_dir.display()
	);

	anyhow::bail!("Fixtures files are not formatted")
}

/// Build contracts for wasm.
fn invoke_wasm_build(current_dir: &Path) -> Result<()> {
	let encoded_rustflags = [
		"-Clink-arg=-zstack-size=65536",
		"-Clink-arg=--import-memory",
		"-Clinker-plugin-lto",
		"-Ctarget-cpu=mvp",
		"-Dwarnings",
	]
	.join("\x1f");

	let build_res = Command::new(env::var("CARGO")?)
		.current_dir(current_dir)
		.env("CARGO_ENCODED_RUSTFLAGS", encoded_rustflags)
		.args(["build", "--release", "--target=wasm32-unknown-unknown"])
		.output()
		.expect("failed to execute process");

	if build_res.status.success() {
		return Ok(())
	}

	let stderr = String::from_utf8_lossy(&build_res.stderr);
	eprintln!("{}", stderr);
	bail!("Failed to build wasm contracts");
}

/// Post-process the compiled wasm contracts.
fn post_process_wasm(input_path: &Path, output_path: &Path) -> Result<()> {
	let mut module =
		deserialize_file(input_path).with_context(|| format!("Failed to read {:?}", input_path))?;
	if let Some(section) = module.export_section_mut() {
		section.entries_mut().retain(|entry| {
			matches!(entry.internal(), Internal::Function(_)) &&
				(entry.field() == "call" || entry.field() == "deploy")
		});
	}

	serialize_to_file(output_path, module).map_err(Into::into)
}

/// Build contracts for RISC-V.
#[cfg(feature = "riscv")]
fn invoke_riscv_build(current_dir: &Path) -> Result<()> {
	let encoded_rustflags = [
		"-Crelocation-model=pie",
		"-Clink-arg=--emit-relocs",
		"-Clink-arg=--export-dynamic-symbol=__polkavm_symbol_export_hack__*",
	]
	.join("\x1f");

	let build_res = Command::new(env::var("CARGO")?)
		.current_dir(current_dir)
		.env_clear()
		.env("PATH", env::var("PATH").unwrap_or_default())
		.env("CARGO_ENCODED_RUSTFLAGS", encoded_rustflags)
		.env("RUSTUP_TOOLCHAIN", "rve-nightly")
		.env("RUSTUP_HOME", env::var("RUSTUP_HOME").unwrap_or_default())
		.args(["build", "--release", "--target=riscv32ema-unknown-none-elf"])
		.output()
		.expect("failed to execute process");

	if build_res.status.success() {
		return Ok(())
	}

	let stderr = String::from_utf8_lossy(&build_res.stderr);

	if stderr.contains("'rve-nightly' is not installed") {
		eprintln!("RISC-V toolchain is not installed.\nDownload and install toolchain from https://github.com/paritytech/rustc-rv32e-toolchain.");
		eprintln!("{}", stderr);
	} else {
		eprintln!("{}", stderr);
	}

	bail!("Failed to build contracts");
}
/// Post-process the compiled wasm contracts.
#[cfg(feature = "riscv")]
fn post_process_riscv(input_path: &Path, output_path: &Path) -> Result<()> {
	let mut config = polkavm_linker::Config::default();
	config.set_strip(true);
	let orig = fs::read(input_path).with_context(|| format!("Failed to read {:?}", input_path))?;
	let linked = polkavm_linker::program_from_elf(config, orig.as_ref())
		.map_err(|err| anyhow::format_err!("Failed to link polkavm program: {}", err))?;
	fs::write(output_path, linked.as_bytes()).map_err(Into::into)
}

/// Write the compiled contracts to the given output directory.
fn write_output(build_dir: &Path, out_dir: &Path, entries: Vec<Entry>) -> Result<()> {
	for entry in entries {
		let wasm_output = entry.out_wasm_filename();
		post_process_wasm(
			&build_dir.join("target/wasm32-unknown-unknown/release").join(&wasm_output),
			&out_dir.join(&wasm_output),
		)?;

		#[cfg(feature = "riscv")]
		post_process_riscv(
			&build_dir.join("target/riscv32ema-unknown-none-elf/release").join(entry.name()),
			&out_dir.join(entry.out_riscv_filename()),
		)?;

		entry.update_cache(out_dir)?;
	}

	Ok(())
}

/// Returns the root path of the wasm workspace.
fn find_workspace_root(current_dir: &Path) -> Option<PathBuf> {
	let mut current_dir = current_dir.to_path_buf();

	while current_dir.parent().is_some() {
		if current_dir.join("Cargo.toml").exists() {
			let cargo_toml_contents =
				std::fs::read_to_string(current_dir.join("Cargo.toml")).ok()?;
			if cargo_toml_contents.contains("[workspace]") {
				return Some(current_dir)
			}
		}

		current_dir.pop();
	}

	None
}

fn main() -> Result<()> {
	let fixtures_dir: PathBuf = env::var("CARGO_MANIFEST_DIR")?.into();
	let contracts_dir = fixtures_dir.join("contracts");
	let out_dir: PathBuf = env::var("OUT_DIR")?.into();
	let workspace_root = find_workspace_root(&fixtures_dir).expect("workspace root exists; qed");

	let entries = collect_entries(&contracts_dir, &out_dir);
	if entries.is_empty() {
		return Ok(())
	}

	let tmp_dir = tempfile::tempdir()?;
	let tmp_dir_path = tmp_dir.path();

	create_cargo_toml(&fixtures_dir, entries.iter(), tmp_dir.path())?;
	invoke_cargo_fmt(
		&workspace_root.join(".rustfmt.toml"),
		entries.iter().map(|entry| &entry.path as _),
		&contracts_dir,
	)?;

	invoke_wasm_build(tmp_dir_path)?;

	#[cfg(feature = "riscv")]
	invoke_riscv_build(tmp_dir_path)?;

	write_output(tmp_dir_path, &out_dir, entries)?;
	Ok(())
}
