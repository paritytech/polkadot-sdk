// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! # WASM builder is a utility for building a project as a WASM binary
//!
//! The WASM builder is a tool that integrates the process of building the WASM binary of your project into the main
//! `cargo` build process.
//!
//! ## Project setup
//!
//! A project that should be compiled as a WASM binary needs to:
//!
//! 1. Add a `build.rs` file.
//! 2. Add `substrate-wasm-builder` as dependency into `build-dependencies`.
//!
//! The `build.rs` file needs to contain the following code:
//!
//! ```ignore
//! use wasm_builder_runner::{build_current_project, WasmBuilderSource};
//!
//! fn main() {
//! 	build_current_project(
//! 		// The name of the file being generated in out-dir.
//! 		"wasm_binary.rs",
//! 		// How to include wasm-builder, in this case from crates.io.
//! 		WasmBuilderSource::Crates("1.0.0"),
//! 	);
//! }
//! ```
//!
//! As the final step, you need to add the following to your project:
//!
//! ```ignore
//! include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
//! ```
//!
//! This will include the generated WASM binary as two constants `WASM_BINARY` and `WASM_BINARY_BLOATY`.
//! The former is a compact WASM binary and the latter is not compacted.
//!
//! ## Environment variables
//!
//! By using environment variables, you can configure which WASM binaries are built and how:
//!
//! - `SKIP_WASM_BUILD` - Skips building any wasm binary. This is useful when only native should be recompiled.
//! - `BUILD_DUMMY_WASM_BINARY` - Builds dummy wasm binaries. These dummy binaries are empty and useful
//!                              for `cargo check` runs.
//! - `WASM_BUILD_TYPE` - Sets the build type for building wasm binaries. Supported values are `release` or `debug`.
//!                       By default the build type is equal to the build type used by the main build.
//! - `TRIGGER_WASM_BUILD` - Can be set to trigger a wasm build. On subsequent calls the value of the variable
//!                          needs to change. As WASM builder instructs `cargo` to watch for file changes
//!                          this environment variable should only be required in certain circumstances.
//! - `WASM_BUILD_RUSTFLAGS` - Extend `RUSTFLAGS` given to `cargo build` while building the wasm binary.
//! - `WASM_BUILD_NO_COLOR` - Disable color output of the wasm build.
//! - `WASM_TARGET_DIRECTORY` - Will copy any build wasm binary to the given directory. The path needs
//!                            to be absolute.
//!
//! Each project can be skipped individually by using the environment variable `SKIP_PROJECT_NAME_WASM_BUILD`.
//! Where `PROJECT_NAME` needs to be replaced by the name of the cargo project, e.g. `node-runtime` will
//! be `NODE_RUNTIME`.
//!
//! ## Prerequisites:
//!
//! WASM builder requires the following prerequisities for building the WASM binary:
//!
//! - rust nightly + `wasm32-unknown-unknown` toolchain
//!

use std::{env, fs, path::PathBuf, process::{Command, Stdio, self}};

mod prerequisites;
mod wasm_project;

/// Environment variable that tells us to skip building the wasm binary.
const SKIP_BUILD_ENV: &str = "SKIP_WASM_BUILD";

/// Environment variable to force a certain build type when building the wasm binary.
/// Expects "debug" or "release" as value.
///
/// By default the WASM binary uses the same build type as the main cargo build.
const WASM_BUILD_TYPE_ENV: &str = "WASM_BUILD_TYPE";

/// Environment variable to extend the `RUSTFLAGS` variable given to the wasm build.
const WASM_BUILD_RUSTFLAGS_ENV: &str = "WASM_BUILD_RUSTFLAGS";

/// Environment variable to set the target directory to copy the final wasm binary.
///
/// The directory needs to be an absolute path.
const WASM_TARGET_DIRECTORY: &str = "WASM_TARGET_DIRECTORY";

/// Environment variable to disable color output of the wasm build.
const WASM_BUILD_NO_COLOR: &str = "WASM_BUILD_NO_COLOR";

/// Build the currently built project as wasm binary.
///
/// The current project is determined by using the `CARGO_MANIFEST_DIR` environment variable.
///
/// `file_name` - The name + path of the file being generated. The file contains the
///               constant `WASM_BINARY`, which contains the built WASM binary.
/// `cargo_manifest` - The path to the `Cargo.toml` of the project that should be built.
pub fn build_project(file_name: &str, cargo_manifest: &str) {
	build_project_with_default_rustflags(file_name, cargo_manifest, "");
}

/// Build the currently built project as wasm binary.
///
/// The current project is determined by using the `CARGO_MANIFEST_DIR` environment variable.
///
/// `file_name` - The name + path of the file being generated. The file contains the
///               constant `WASM_BINARY`, which contains the built WASM binary.
/// `cargo_manifest` - The path to the `Cargo.toml` of the project that should be built.
/// `default_rustflags` - Default `RUSTFLAGS` that will always be set for the build.
pub fn build_project_with_default_rustflags(
	file_name: &str,
	cargo_manifest: &str,
	default_rustflags: &str,
) {
	if check_skip_build() {
		return;
	}

	let cargo_manifest = PathBuf::from(cargo_manifest);

	if !cargo_manifest.exists() {
		panic!("'{}' does not exist!", cargo_manifest.display());
	}

	if !cargo_manifest.ends_with("Cargo.toml") {
		panic!("'{}' no valid path to a `Cargo.toml`!", cargo_manifest.display());
	}

	if let Some(err_msg) = prerequisites::check() {
		eprintln!("{}", err_msg);
		process::exit(1);
	}

	let (wasm_binary, bloaty) = wasm_project::create_and_compile(
		&cargo_manifest,
		default_rustflags,
	);

	write_file_if_changed(
		file_name.into(),
		format!(
			r#"
				pub const WASM_BINARY: &[u8] = include_bytes!("{wasm_binary}");
				pub const WASM_BINARY_BLOATY: &[u8] = include_bytes!("{wasm_binary_bloaty}");
			"#,
			wasm_binary = wasm_binary.wasm_binary_path(),
			wasm_binary_bloaty = bloaty.wasm_binary_bloaty_path(),
		),
	);
}

/// Checks if the build of the WASM binary should be skipped.
fn check_skip_build() -> bool {
	env::var(SKIP_BUILD_ENV).is_ok()
}

/// Write to the given `file` if the `content` is different.
fn write_file_if_changed(file: PathBuf, content: String) {
	if fs::read_to_string(&file).ok().as_ref() != Some(&content) {
		fs::write(&file, content).expect(&format!("Writing `{}` can not fail!", file.display()));
	}
}

/// Get a cargo command that compiles with nightly
fn get_nightly_cargo() -> CargoCommand {
	let default_cargo = CargoCommand::new("cargo");
	let mut rustup_run_nightly = CargoCommand::new("rustup");
	rustup_run_nightly.args(&["run", "nightly", "cargo"]);

	if default_cargo.is_nightly() {
		default_cargo
	} else if rustup_run_nightly.works() {
		rustup_run_nightly
	} else {
		default_cargo
	}
}

/// Builder for cargo commands
#[derive(Debug)]
struct CargoCommand {
	program: String,
	args: Vec<String>,
}

impl CargoCommand {
	fn new(program: &str) -> Self {
		CargoCommand { program: program.into(), args: Vec::new() }
	}

	fn arg(&mut self, arg: &str) -> &mut Self {
		self.args.push(arg.into());
		self
	}

	fn args(&mut self, args: &[&str]) -> &mut Self {
		args.into_iter().for_each(|a| { self.arg(a); });
		self
	}

	fn command(&self) -> Command {
		let mut cmd = Command::new(&self.program);
		cmd.args(&self.args);
		cmd
	}

	fn works(&self) -> bool {
		self.command()
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.status()
			.map(|s| s.success()).unwrap_or(false)
	}

	/// Check if the supplied cargo command is a nightly version
	fn is_nightly(&self) -> bool {
		// `RUSTC_BOOTSTRAP` tells a stable compiler to behave like a nightly. So, when this env
		// variable is set, we can assume that whatever rust compiler we have, it is a nightly compiler.
		// For "more" information, see:
		// https://github.com/rust-lang/rust/blob/fa0f7d0080d8e7e9eb20aa9cbf8013f96c81287f/src/libsyntax/feature_gate/check.rs#L891
		env::var("RUSTC_BOOTSTRAP").is_ok() ||
			self.command()
				.arg("--version")
				.output()
				.map_err(|_| ())
				.and_then(|o| String::from_utf8(o.stdout).map_err(|_| ()))
				.unwrap_or_default()
				.contains("-nightly")
	}
}
