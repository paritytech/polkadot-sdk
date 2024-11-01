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
use anyhow::Result;

fn main() -> Result<()> {
	build::run()
}

#[cfg(feature = "riscv")]
mod build {
	use super::Result;
	use anyhow::{bail, Context};
	use std::{
		cfg, env, fs,
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
	}

	impl Entry {
		/// Create a new contract entry from the given path.
		fn new(path: PathBuf) -> Self {
			Self { path }
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

		/// Return the name of the polkavm file.
		fn out_filename(&self) -> String {
			format!("{}.polkavm", self.name())
		}
	}

	/// Collect all contract entries from the given source directory.
	fn collect_entries(contracts_dir: &Path) -> Vec<Entry> {
		fs::read_dir(contracts_dir)
			.expect("src dir exists; qed")
			.filter_map(|file| {
				let path = file.expect("file exists; qed").path();
				if path.extension().map_or(true, |ext| ext != "rs") {
					return None
				}

				Some(Entry::new(path))
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

	fn invoke_build(target: &Path, current_dir: &Path) -> Result<()> {
		let encoded_rustflags = ["-Dwarnings"].join("\x1f");

		let mut build_command = Command::new(env::var("CARGO")?);
		build_command
			.current_dir(current_dir)
			.env_clear()
			.env("PATH", env::var("PATH").unwrap_or_default())
			.env("CARGO_ENCODED_RUSTFLAGS", encoded_rustflags)
			.env("RUSTC_BOOTSTRAP", "1")
			.env("RUSTUP_HOME", env::var("RUSTUP_HOME").unwrap_or_default())
			.args([
				"build",
				"--release",
				"-Zbuild-std=core",
				"-Zbuild-std-features=panic_immediate_abort",
			])
			.arg("--target")
			.arg(target);

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
		let strip = std::env::var(OVERRIDE_STRIP_ENV_VAR).map_or(false, |value| value == "1");
		let optimize = std::env::var(OVERRIDE_OPTIMIZE_ENV_VAR).map_or(true, |value| value == "1");

		let mut config = polkavm_linker::Config::default();
		config.set_strip(strip);
		config.set_optimize(optimize);
		let orig =
			fs::read(input_path).with_context(|| format!("Failed to read {:?}", input_path))?;
		let linked = polkavm_linker::program_from_elf(config, orig.as_ref())
			.map_err(|err| anyhow::format_err!("Failed to link polkavm program: {}", err))?;
		fs::write(output_path, linked).map_err(Into::into)
	}

	/// Write the compiled contracts to the given output directory.
	fn write_output(build_dir: &Path, out_dir: &Path, entries: Vec<Entry>) -> Result<()> {
		for entry in entries {
			post_process(
				&build_dir
					.join("target/riscv32emac-unknown-none-polkavm/release")
					.join(entry.name()),
				&out_dir.join(entry.out_filename()),
			)?;
		}

		Ok(())
	}

	pub fn run() -> Result<()> {
		let fixtures_dir: PathBuf = env::var("CARGO_MANIFEST_DIR")?.into();
		let contracts_dir = fixtures_dir.join("contracts");
		let out_dir: PathBuf = env::var("OUT_DIR")?.into();
		let target = fixtures_dir.join("riscv32emac-unknown-none-polkavm.json");

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
			return Ok(())
		}

		let tmp_dir = tempfile::tempdir()?;
		let tmp_dir_path = tmp_dir.path();

		create_cargo_toml(&fixtures_dir, entries.iter(), tmp_dir.path())?;
		invoke_build(&target, tmp_dir_path)?;

		write_output(tmp_dir_path, &out_dir, entries)?;

		#[cfg(unix)]
		if let Ok(symlink_dir) = env::var("CARGO_WORKSPACE_ROOT_DIR") {
			let symlink_dir: PathBuf = symlink_dir.into();
			let symlink_dir: PathBuf = symlink_dir.join("target").join("pallet-revive-fixtures");
			if symlink_dir.is_symlink() {
				fs::remove_file(&symlink_dir)?
			}
			std::os::unix::fs::symlink(&out_dir, &symlink_dir)?;
		}

		Ok(())
	}
}

#[cfg(not(feature = "riscv"))]
mod build {
	use super::Result;

	pub fn run() -> Result<()> {
		Ok(())
	}
}
