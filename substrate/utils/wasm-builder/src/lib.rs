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

#![doc = include_str!("../README.md")]

use std::{
	env, fs,
	io::BufRead,
	path::{Path, PathBuf},
	process::Command,
};
use version::Version;

mod builder;
mod prerequisites;
mod version;
mod wasm_project;

pub use builder::{WasmBuilder, WasmBuilderSelectProject};

/// Environment variable that tells us to skip building the wasm binary.
const SKIP_BUILD_ENV: &str = "SKIP_WASM_BUILD";

/// Environment variable that tells us whether we should avoid network requests
const OFFLINE: &str = "CARGO_NET_OFFLINE";

/// Environment variable to force a certain build type when building the wasm binary.
/// Expects "debug", "release" or "production" as value.
///
/// When unset the WASM binary uses the same build type as the main cargo build with
/// the exception of a debug build: In this case the wasm build defaults to `release` in
/// order to avoid a slowdown when not explicitly requested.
const WASM_BUILD_TYPE_ENV: &str = "WASM_BUILD_TYPE";

/// Environment variable to extend the `RUSTFLAGS` variable given to the wasm build.
const WASM_BUILD_RUSTFLAGS_ENV: &str = "WASM_BUILD_RUSTFLAGS";

/// Environment variable to set the target directory to copy the final wasm binary.
///
/// The directory needs to be an absolute path.
const WASM_TARGET_DIRECTORY: &str = "WASM_TARGET_DIRECTORY";

/// Environment variable to disable color output of the wasm build.
const WASM_BUILD_NO_COLOR: &str = "WASM_BUILD_NO_COLOR";

/// Environment variable to set the toolchain used to compile the wasm binary.
const WASM_BUILD_TOOLCHAIN: &str = "WASM_BUILD_TOOLCHAIN";

/// Environment variable that makes sure the WASM build is triggered.
const FORCE_WASM_BUILD_ENV: &str = "FORCE_WASM_BUILD";

/// Environment variable that hints the workspace we are building.
const WASM_BUILD_WORKSPACE_HINT: &str = "WASM_BUILD_WORKSPACE_HINT";

/// Environment variable to set whether we'll build `core`/`std`.
const WASM_BUILD_STD: &str = "WASM_BUILD_STD";

/// Write to the given `file` if the `content` is different.
fn write_file_if_changed(file: impl AsRef<Path>, content: impl AsRef<str>) {
	if fs::read_to_string(file.as_ref()).ok().as_deref() != Some(content.as_ref()) {
		fs::write(file.as_ref(), content.as_ref())
			.unwrap_or_else(|_| panic!("Writing `{}` can not fail!", file.as_ref().display()));
	}
}

/// Copy `src` to `dst` if the `dst` does not exist or is different.
fn copy_file_if_changed(src: PathBuf, dst: PathBuf) {
	let src_file = fs::read_to_string(&src).ok();
	let dst_file = fs::read_to_string(&dst).ok();

	if src_file != dst_file {
		fs::copy(&src, &dst).unwrap_or_else(|_| {
			panic!("Copying `{}` to `{}` can not fail; qed", src.display(), dst.display())
		});
	}
}

/// Get a cargo command that should be used to invoke the compilation.
fn get_cargo_command() -> CargoCommand {
	let env_cargo =
		CargoCommand::new(&env::var("CARGO").expect("`CARGO` env variable is always set by cargo"));
	let default_cargo = CargoCommand::new("cargo");
	let wasm_toolchain = env::var(WASM_BUILD_TOOLCHAIN).ok();

	// First check if the user requested a specific toolchain
	if let Some(cmd) =
		wasm_toolchain.map(|t| CargoCommand::new_with_args("rustup", &["run", &t, "cargo"]))
	{
		cmd
	} else if env_cargo.supports_substrate_wasm_env() {
		env_cargo
	} else if default_cargo.supports_substrate_wasm_env() {
		default_cargo
	} else {
		// If no command before provided us with a cargo that supports our Substrate wasm env, we
		// try to search one with rustup. If that fails as well, we return the default cargo and let
		// the prequisities check fail.
		get_rustup_command().unwrap_or(default_cargo)
	}
}

/// Get the newest rustup command that supports our Substrate wasm env.
///
/// Stable versions are always favored over nightly versions even if the nightly versions are
/// newer.
fn get_rustup_command() -> Option<CargoCommand> {
	let host = format!("-{}", env::var("HOST").expect("`HOST` is always set by cargo"));

	let output = Command::new("rustup").args(&["toolchain", "list"]).output().ok()?.stdout;
	let lines = output.as_slice().lines();

	let mut versions = Vec::new();
	for line in lines.filter_map(|l| l.ok()) {
		let rustup_version = line.trim_end_matches(&host);

		let cmd = CargoCommand::new_with_args("rustup", &["run", &rustup_version, "cargo"]);

		if !cmd.supports_substrate_wasm_env() {
			continue
		}

		let Some(cargo_version) = cmd.version() else { continue };

		versions.push((cargo_version, rustup_version.to_string()));
	}

	// Sort by the parsed version to get the latest version (greatest version) at the end of the
	// vec.
	versions.sort_by_key(|v| v.0);
	let version = &versions.last()?.1;

	Some(CargoCommand::new_with_args("rustup", &["run", &version, "cargo"]))
}

/// Wraps a specific command which represents a cargo invocation.
#[derive(Debug)]
struct CargoCommand {
	program: String,
	args: Vec<String>,
	version: Option<Version>,
}

impl CargoCommand {
	fn new(program: &str) -> Self {
		let version = Self::extract_version(program, &[]);

		CargoCommand { program: program.into(), args: Vec::new(), version }
	}

	fn new_with_args(program: &str, args: &[&str]) -> Self {
		let version = Self::extract_version(program, args);

		CargoCommand {
			program: program.into(),
			args: args.iter().map(ToString::to_string).collect(),
			version,
		}
	}

	fn command(&self) -> Command {
		let mut cmd = Command::new(&self.program);
		cmd.args(&self.args);
		cmd
	}

	fn extract_version(program: &str, args: &[&str]) -> Option<Version> {
		let version = Command::new(program)
			.args(args)
			.arg("--version")
			.output()
			.ok()
			.and_then(|o| String::from_utf8(o.stdout).ok())?;

		Version::extract(&version)
	}

	/// Returns the version of this cargo command or `None` if it failed to extract the version.
	fn version(&self) -> Option<Version> {
		self.version
	}

	/// Returns whether this version of the toolchain supports nightly features.
	fn supports_nightly_features(&self) -> bool {
		self.version.map_or(false, |version| version.is_nightly) ||
			env::var("RUSTC_BOOTSTRAP").is_ok()
	}

	/// Check if the supplied cargo command supports our Substrate wasm environment.
	///
	/// This means that either the cargo version is at minimum 1.68.0 or this is a nightly cargo.
	///
	/// Assumes that cargo version matches the rustc version.
	fn supports_substrate_wasm_env(&self) -> bool {
		// `RUSTC_BOOTSTRAP` tells a stable compiler to behave like a nightly. So, when this env
		// variable is set, we can assume that whatever rust compiler we have, it is a nightly
		// compiler. For "more" information, see:
		// https://github.com/rust-lang/rust/blob/fa0f7d0080d8e7e9eb20aa9cbf8013f96c81287f/src/libsyntax/feature_gate/check.rs#L891
		if env::var("RUSTC_BOOTSTRAP").is_ok() {
			return true
		}

		let Some(version) = self.version() else { return false };

		// Check if major and minor are greater or equal than 1.68 or this is a nightly.
		version.major > 1 || (version.major == 1 && version.minor >= 68) || version.is_nightly
	}
}

/// Wraps a [`CargoCommand`] and the version of `rustc` the cargo command uses.
struct CargoCommandVersioned {
	command: CargoCommand,
	version: String,
}

impl CargoCommandVersioned {
	fn new(command: CargoCommand, version: String) -> Self {
		Self { command, version }
	}

	/// Returns the `rustc` version.
	fn rustc_version(&self) -> &str {
		&self.version
	}
}

impl std::ops::Deref for CargoCommandVersioned {
	type Target = CargoCommand;

	fn deref(&self) -> &CargoCommand {
		&self.command
	}
}

/// Returns `true` when color output is enabled.
fn color_output_enabled() -> bool {
	env::var(crate::WASM_BUILD_NO_COLOR).is_err()
}

/// Fetches a boolean environment variable. Will exit the process if the value is invalid.
fn get_bool_environment_variable(name: &str) -> Option<bool> {
	let value = env::var_os(name)?;

	// We're comparing `OsString`s here so we can't use a `match`.
	if value == "1" {
		Some(true)
	} else if value == "0" {
		Some(false)
	} else {
		build_helper::warning!(
			"the '{}' environment variable has an invalid value; it must be either '1' or '0'",
			name
		);
		std::process::exit(1);
	}
}

/// Returns whether we need to also compile the standard library when compiling the runtime.
fn build_std_required() -> bool {
	crate::get_bool_environment_variable(crate::WASM_BUILD_STD).unwrap_or(true)
}
