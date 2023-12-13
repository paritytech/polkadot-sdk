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

use crate::{write_file_if_changed, CargoCommand, CargoCommandVersioned};

use std::{fs, path::Path};

use ansi_term::Color;
use tempfile::tempdir;

/// Print an error message.
fn print_error_message(message: &str) -> String {
	if super::color_output_enabled() {
		Color::Red.bold().paint(message).to_string()
	} else {
		message.into()
	}
}

/// Checks that all prerequisites are installed.
///
/// Returns the versioned cargo command on success.
pub(crate) fn check() -> Result<CargoCommandVersioned, String> {
	let cargo_command = crate::get_cargo_command();

	if !cargo_command.supports_substrate_wasm_env() {
		return Err(print_error_message(
			"Cannot compile the WASM runtime: no compatible Rust compiler found!\n\
			 Install at least Rust 1.68.0 or a recent nightly version.",
		))
	}

	check_wasm_toolchain_installed(cargo_command)
}

/// Creates a minimal dummy crate at the given path and returns the manifest path.
fn create_minimal_crate(project_dir: &Path) -> std::path::PathBuf {
	fs::create_dir_all(project_dir.join("src")).expect("Creating src dir does not fail; qed");

	let manifest_path = project_dir.join("Cargo.toml");
	write_file_if_changed(
		&manifest_path,
		r#"
			[package]
			name = "wasm-test"
			version = "1.0.0"
			edition = "2021"

			[workspace]
		"#,
	);

	write_file_if_changed(project_dir.join("src/main.rs"), "fn main() {}");
	manifest_path
}

fn check_wasm_toolchain_installed(
	cargo_command: CargoCommand,
) -> Result<CargoCommandVersioned, String> {
	let temp = tempdir().expect("Creating temp dir does not fail; qed");
	let manifest_path = create_minimal_crate(temp.path()).display().to_string();

	let prepare_command = |subcommand| {
		let mut cmd = cargo_command.command();
		// Chdir to temp to avoid including project's .cargo/config.toml
		// by accident - it can happen in some CI environments.
		cmd.current_dir(&temp);
		cmd.args(&[
			subcommand,
			"--target=wasm32-unknown-unknown",
			"--manifest-path",
			&manifest_path,
		]);

		if super::color_output_enabled() {
			cmd.arg("--color=always");
		}

		// manually set the `CARGO_TARGET_DIR` to prevent a cargo deadlock
		let target_dir = temp.path().join("target").display().to_string();
		cmd.env("CARGO_TARGET_DIR", &target_dir);

		// Make sure the host's flags aren't used here, e.g. if an alternative linker is specified
		// in the RUSTFLAGS then the check we do here will break unless we clear these.
		cmd.env_remove("CARGO_ENCODED_RUSTFLAGS");
		cmd.env_remove("RUSTFLAGS");
		cmd
	};

	let err_msg =
		print_error_message("Rust WASM toolchain is not properly installed; please install it!");
	let build_result = prepare_command("build").output().map_err(|_| err_msg.clone())?;
	if !build_result.status.success() {
		return match String::from_utf8(build_result.stderr) {
			Ok(ref err) if err.contains("the `wasm32-unknown-unknown` target may not be installed") =>
				Err(print_error_message("Cannot compile the WASM runtime: the `wasm32-unknown-unknown` target is not installed!\n\
				                         You can install it with `rustup target add wasm32-unknown-unknown` if you're using `rustup`.")),

			// Apparently this can happen when we're running on a non Tier 1 platform.
			Ok(ref err) if err.contains("linker `rust-lld` not found") =>
				Err(print_error_message("Cannot compile the WASM runtime: `rust-lld` not found!")),

			Ok(ref err) => Err(format!(
				"{}\n\n{}\n{}\n{}{}\n",
				err_msg,
				Color::Yellow.bold().paint("Further error information:"),
				Color::Yellow.bold().paint("-".repeat(60)),
				err,
				Color::Yellow.bold().paint("-".repeat(60)),
			)),

			Err(_) => Err(err_msg),
		};
	}

	let mut run_cmd = prepare_command("rustc");
	run_cmd.args(&["-q", "--", "--version"]);

	let version = run_cmd
		.output()
		.ok()
		.and_then(|o| String::from_utf8(o.stdout).ok())
		.unwrap_or_else(|| "unknown rustc version".into());

	if crate::build_std_required() {
		let mut sysroot_cmd = prepare_command("rustc");
		sysroot_cmd.args(&["-q", "--", "--print", "sysroot"]);
		if let Some(sysroot) =
			sysroot_cmd.output().ok().and_then(|o| String::from_utf8(o.stdout).ok())
		{
			let src_path =
				Path::new(sysroot.trim()).join("lib").join("rustlib").join("src").join("rust");
			if !src_path.exists() {
				return Err(print_error_message(
					"Cannot compile the WASM runtime: no standard library sources found!\n\
					 You can install them with `rustup component add rust-src` if you're using `rustup`.",
				))
			}
		}
	}

	Ok(CargoCommandVersioned::new(cargo_command, version))
}
