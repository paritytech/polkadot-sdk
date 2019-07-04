// Copyright 2019 Parity Technologies (UK) Ltd.
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
//! 2. Add `substrate-wasm-builder-runner` as dependency into `build-dependencies`.
//! 3. Add a feature called `no-std`.
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
//! The `no-std` feature will be enabled by WASM builder while compiling your project to WASM.
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
//! - `SKIP_WASM_BUILD` - Skips building any WASM binary. This is useful when only native should be recompiled.
//! - `BUILD_DUMMY_WASM_BINARY` - Builds dummy WASM binaries. These dummy binaries are empty and useful
//!                              for `cargo check` runs.
//! - `WASM_BUILD_TYPE` - Sets the build type for building WASM binaries. Supported values are `release` or `debug`.
//!                       By default the build type is equal to the build type used by the main build.
//! - `TRIGGER_WASM_BUILD` - Can be set to trigger a WASM build. On subsequent calls the value of the variable
//!                          needs to change. As WASM builder instructs `cargo` to watch for file changes
//!                          this environment variable should only be required in certain circumstances.
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
//! - wasm-gc
//!

use std::{env, fs, path::PathBuf, process::Command};

mod prerequisites;
mod wasm_project;

/// Environment variable that tells us to skip building the WASM binary.
const SKIP_BUILD_ENV: &str = "SKIP_WASM_BUILD";

/// Environment variable to force a certain build type when building the WASM binary.
/// Expects "debug" or "release" as value.
///
/// By default the WASM binary uses the same build type as the main cargo build.
const WASM_BUILD_TYPE_ENV: &str = "WASM_BUILD_TYPE";

/// Build the currently built project as WASM binary.
///
/// The current project is determined by using the `CARGO_MANIFEST_DIR` environment variable.
///
/// `file_name` - The name + path of the file being generated. The file contains the
///               constant `WASM_BINARY`, which contains the built WASM binary.
/// `cargo_manifest` - The path to the `Cargo.toml` of the project that should be built.
pub fn build_project(file_name: &str, cargo_manifest: &str) {
	if check_skip_build() {
		return;
	}

	let cargo_manifest = PathBuf::from(cargo_manifest);

	if !cargo_manifest.exists() {
		create_out_file(
			file_name,
			format!("compile_error!(\"'{}' does not exists!\")", cargo_manifest.display())
		);
		return
	}

	if !cargo_manifest.ends_with("Cargo.toml") {
		create_out_file(
			file_name,
			format!("compile_error!(\"'{}' no valid path to a `Cargo.toml`!\")", cargo_manifest.display())
		);
		return
	}

	if let Some(err_msg) = prerequisites::check() {
		create_out_file(file_name, format!("compile_error!(\"{}\");", err_msg));
		return
	}

	let (wasm_binary, bloaty) = wasm_project::create_and_compile(&cargo_manifest);

	create_out_file(
		file_name,
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

fn create_out_file(file_name: &str, content: String) {
	fs::write(
		file_name,
		content
	).expect("Creating and writing can not fail; qed");
}

/// Get a cargo command that compiles with nightly
fn get_nightly_cargo() -> Command {
	if Command::new("rustup").args(&["run", "nightly", "cargo"])
		.status().map(|s| s.success()).unwrap_or(false)
	{
		let mut cmd = Command::new("rustup");
		cmd.args(&["run", "nightly", "cargo"]);
		cmd
	} else {
		Command::new("cargo")
	}
}
