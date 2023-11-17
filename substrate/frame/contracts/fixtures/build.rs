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

use anyhow::Result;
use parity_wasm::elements::{deserialize_file, serialize_to_file, Internal};
use std::{
	env, fs,
	hash::Hasher,
	path::{Path, PathBuf},
	process::Command,
};
use twox_hash::XxHash32;

/// Salt used for hashing contract source files.
const SALT: &[u8] = &[2u8];

/// Read the file at `path` and return its hash as a hex string.
fn file_hash(path: &Path) -> String {
	let data = fs::read(path).expect("file exists; qed");
	let mut hasher = XxHash32::default();
	hasher.write(&data);
	hasher.write(SALT);
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

	/// Return the name of the output wasm file.
	fn out_wasm_filename(&self) -> String {
		format!("{}.wasm", self.name())
	}
}

/// Collect all contract entries from the given source directory.
/// Contracts that have already been compiled are filtered out.
fn collect_entries(src_dir: &Path, out_dir: &Path) -> Vec<Entry> {
	fs::read_dir(&src_dir)
		.expect("src dir exists; qed")
		.filter_map(|file| {
			let entry = Entry::new(file.expect("file exists; qed").path());
			if out_dir.join(&entry.hash).exists() {
				None
			} else {
				Some(entry)
			}
		})
		.collect::<Vec<_>>()
}

/// Create a `Cargo.toml` to compile the given contract entries.
fn create_cargo_toml<'a>(
	input_dir: &Path,
	entries: impl Iterator<Item = &'a Entry>,
	output_dir: &Path,
) -> Result<()> {
	let uapi_path = input_dir.join("../uapi").canonicalize()?;
	let mut cargo_toml: toml::Value = toml::from_str(&format!(
		"
[package]
name = 'contracts'
version = '0.1.0'

[[bin]]

[dependencies]
uapi = {{ package = 'pallet-contracts-uapi',  path = {uapi_path:?}, default-features = false}}

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
"
	))?;

	let binaries = entries
		.map(|entry| {
			let name = entry.name();
			let path = entry.path();
			toml::Value::Table(toml::toml! {
				name = name
				path = path
			})
		})
		.collect::<Vec<_>>();

	cargo_toml["bin"] = toml::Value::Array(binaries);
	let cargo_toml = toml::to_string_pretty(&cargo_toml)?;
	fs::write(output_dir.join("Cargo.toml"), cargo_toml).map_err(Into::into)
}

/// Invoke `cargo build` to compile the contracts.
fn invoke_build(current_dir: &Path) -> Result<()> {


	// panic and print all env vars
	eprintln!("env vars:");
	eprintln!("{:#?}", env::vars());

	if let Ok(_) = env::var("PATH") {
		panic!("done printing env vars");
	}


	let build_res = Command::new(env::var("CARGO")?)
		.current_dir(current_dir)
		.env_clear()
		.env("PATH", env::var("PATH").unwrap_or_default())
		.env("RUSTC", env::var("RUSTC").unwrap_or_default())
		.env(
			"RUSTFLAGS",
			"-C link-arg=-zstack-size=65536 -C link-arg=--import-memory -Clinker-plugin-lto -C target-cpu=mvp",
		)
		.arg("build")
		.arg("--release")
		.arg("--target=wasm32-unknown-unknown") // TODO pass risc-v target here as well
		.output()
		.unwrap();

	if build_res.status.success() {
		return Ok(())
	}

	let stderr = String::from_utf8_lossy(&build_res.stderr);
	anyhow::bail!("Failed to build contracts: {:?}", stderr);
}

/// Post-process the compiled wasm contracts.
fn post_process_wasm(input_path: &Path, output_path: &Path) -> Result<()> {
	let mut module = deserialize_file(input_path)?;
	if let Some(section) = module.export_section_mut() {
		section.entries_mut().retain(|entry| {
			matches!(entry.internal(), Internal::Function(_)) &&
				(entry.field() == "call" || entry.field() == "deploy")
		});
	}

	serialize_to_file(output_path, module).map_err(Into::into)
}

/// Write the compiled contracts to the given output directory.
fn write_output(build_dir: &Path, out_dir: &Path, entries: Vec<Entry>) -> Result<()> {
	for entry in entries {
		let wasm_output = entry.out_wasm_filename();
		post_process_wasm(
			&build_dir.join("target/wasm32-unknown-unknown/release").join(&wasm_output),
			&out_dir.join(&wasm_output),
		)?;
		fs::write(out_dir.join(&entry.hash), "")?;
	}

	Ok(())
}

fn main() -> Result<()> {
	let input_dir: PathBuf = ".".into();
	let out_dir: PathBuf = env::var("OUT_DIR")?.into();

	let entries = collect_entries(&input_dir.join("contracts").canonicalize()?, &out_dir);
	if entries.is_empty() {
		return Ok(());
	}

	let tmp_dir = tempfile::tempdir()?;
	create_cargo_toml(&input_dir, entries.iter(), tmp_dir.path())?;
	invoke_build(tmp_dir.path())?;
	write_output(tmp_dir.path(), &out_dir, entries)?;

	println!("cargo:rerun-if-changed=contracts");
	Ok(())
}
