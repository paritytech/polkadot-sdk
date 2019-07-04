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

//! # WASM builder runner
//!
//! Since cargo contains many bugs when it comes to correct dependency and feature
//! resolution, we need this little tool. See <https://github.com/rust-lang/cargo/issues/5730> for
//! more information.
//!
//! It will create a project that will call `substrate-wasm-builder` to prevent any dependencies
//! from `substrate-wasm-builder` influencing the main project's dependencies.
//!
//! For more information see <https://crates.io/substrate-wasm-builder>

use std::{env, process::Command, fs, path::{PathBuf, Path}};

/// Environment variable that tells us to skip building the WASM binary.
const SKIP_BUILD_ENV: &str = "SKIP_WASM_BUILD";

/// Environment variable that tells us to create a dummy WASM binary.
///
/// This is useful for `cargo check` to speed-up the compilation.
///
/// # Caution
///
/// Enabling this option will just provide `&[]` as WASM binary.
const DUMMY_WASM_BINARY_ENV: &str = "BUILD_DUMMY_WASM_BINARY";

/// Environment variable that makes sure the WASM build is triggered.
const TRIGGER_WASM_BUILD_ENV: &str = "TRIGGER_WASM_BUILD";

/// Replace all backslashes with slashes.
fn replace_back_slashes<T: ToString>(path: T) -> String {
	path.to_string().replace("\\", "/")
}

/// The `wasm-builder` dependency source.
pub enum WasmBuilderSource {
	/// The relative path to the source code from the current manifest dir.
	Path(&'static str),
	/// The git repository that contains the source code.
	Git {
		repo: &'static str,
		rev: &'static str,
	},
	/// Use the given version released on crates.io
	Crates(&'static str),
}

impl WasmBuilderSource {
	/// Convert to a valid cargo source declaration.
	///
	/// `absolute_path` - The manifest dir.
	fn to_cargo_source(&self, manifest_dir: &Path) -> String {
		match self {
			WasmBuilderSource::Path(path) => {
				replace_back_slashes(format!("path = \"{}\"", manifest_dir.join(path).display()))
			}
			WasmBuilderSource::Git { repo, rev } => {
				format!("git = \"{}\", rev=\"{}\"", repo, rev)
			}
			WasmBuilderSource::Crates(version) => {
				format!("version = \"{}\"", version)
			}
		}
	}
}

/// Build the currently built project as WASM binary.
///
/// The current project is determined using the `CARGO_MANIFEST_DIR` environment variable.
///
/// `file_name` - The name of the file being generated in the `OUT_DIR`. The file contains the
///               constant `WASM_BINARY` which contains the build wasm binary.
/// `wasm_builder_path` - Path to the wasm-builder project, relative to `CARGO_MANIFEST_DIR`.
pub fn build_current_project(file_name: &str, wasm_builder_source: WasmBuilderSource) {
	generate_rerun_if_changed_instructions();

	if check_skip_build() {
		return;
	}

	let manifest_dir = PathBuf::from(
		env::var("CARGO_MANIFEST_DIR").expect(
			"`CARGO_MANIFEST_DIR` is always set for `build.rs` files; qed"
		)
	);

	let cargo_toml_path = manifest_dir.join("Cargo.toml");
	let out_dir = PathBuf::from(env::var("OUT_DIR").expect("`OUT_DIR` is set by cargo!"));
	let file_path = out_dir.join(file_name);
	let project_folder = out_dir.join("wasm_build_runner");

	if check_provide_dummy_wasm_binary() {
		provide_dummy_wasm_binary(&file_path);
	} else {
		create_project(&project_folder, &file_path, &manifest_dir, wasm_builder_source, &cargo_toml_path);
		run_project(&project_folder);
	}
}

fn create_project(
	project_folder: &Path,
	file_path: &Path,
	manifest_dir: &Path,
	wasm_builder_source: WasmBuilderSource,
	cargo_toml_path: &Path,
) {
	fs::create_dir_all(project_folder.join("src"))
		.expect("WASM build runner dir create can not fail; qed");

	fs::write(
		project_folder.join("Cargo.toml"),
		format!(
			r#"
				[package]
				name = "wasm-build-runner-impl"
				version = "1.0.0"
				edition = "2018"

				[dependencies]
				substrate-wasm-builder = {{ {wasm_builder_source} }}

				[workspace]
			"#,
			wasm_builder_source = wasm_builder_source.to_cargo_source(manifest_dir),
		)
	).expect("WASM build runner `Cargo.toml` writing can not fail; qed");

	fs::write(
		project_folder.join("src/main.rs"),
		format!(
			r#"
				fn main() {{
					substrate_wasm_builder::build_project("{file_path}", "{cargo_toml_path}")
				}}
			"#,
			file_path = replace_back_slashes(file_path.display()),
			cargo_toml_path = replace_back_slashes(cargo_toml_path.display()),
		)
	).expect("WASM build runner `main.rs` writing can not fail; qed");
}

fn run_project(project_folder: &Path) {
	let cargo = env::var("CARGO").expect("`CARGO` env variable is always set when executing `build.rs`.");
	let mut cmd = Command::new(cargo);
	cmd.arg("run").arg(format!("--manifest-path={}", project_folder.join("Cargo.toml").display()));

	if env::var("DEBUG") != Ok(String::from("true")) {
		cmd.arg("--release");
	}

	if !cmd.status().map(|s| s.success()).unwrap_or(false) {
		panic!("Running WASM build runner failed!");
	}
}

/// Generate the name of the skip build environment variable for the current crate.
fn generate_crate_skip_build_env_name() -> String {
	format!(
		"SKIP_{}_WASM_BUILD",
		env::var("CARGO_PKG_NAME").expect("Package name is set").to_uppercase().replace('-', "_"),
	)
}

/// Checks if the build of the WASM binary should be skipped.
fn check_skip_build() -> bool {
	env::var(SKIP_BUILD_ENV).is_ok() || env::var(generate_crate_skip_build_env_name()).is_ok()
}

/// Check if we should provide a dummy WASM binary.
fn check_provide_dummy_wasm_binary() -> bool {
	env::var(DUMMY_WASM_BINARY_ENV).is_ok()
}

/// Provide the dummy WASM binary
fn provide_dummy_wasm_binary(file_path: &Path) {
	fs::write(
		file_path,
		"pub const WASM_BINARY: &[u8] = &[]; pub const WASM_BINARY_BLOATY: &[u8] = &[];"
	).expect("Writing dummy WASM binary should not fail");
}

/// Generate the `rerun-if-changed` instructions for cargo to make sure that the WASM binary is
/// rebuilt when needed.
fn generate_rerun_if_changed_instructions() {
	// Make sure that the `build.rs` is called again if one of the following env variables changes.
	println!("cargo:rerun-if-env-changed={}", SKIP_BUILD_ENV);
	println!("cargo:rerun-if-env-changed={}", DUMMY_WASM_BINARY_ENV);
	println!("cargo:rerun-if-env-changed={}", TRIGGER_WASM_BUILD_ENV);
	println!("cargo:rerun-if-env-changed={}", generate_crate_skip_build_env_name());
}
