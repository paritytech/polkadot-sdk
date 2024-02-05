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

use crate::{write_file_if_changed, CargoCommand, CargoCommandVersioned, RuntimeTarget};

use console::style;
use std::{
	fs,
	path::{Path, PathBuf},
	process::Command,
};

use tempfile::tempdir;

/// Colorizes an error message, if color output is enabled.
fn colorize_error_message(message: &str) -> String {
	if super::color_output_enabled() {
		style(message).red().bold().to_string()
	} else {
		message.into()
	}
}

/// Colorizes an auxiliary message, if color output is enabled.
fn colorize_aux_message(message: &str) -> String {
	if super::color_output_enabled() {
		style(message).yellow().bold().to_string()
	} else {
		message.into()
	}
}

/// Checks that all prerequisites are installed.
///
/// Returns the versioned cargo command on success.
pub(crate) fn check(target: RuntimeTarget) -> Result<CargoCommandVersioned, String> {
	let cargo_command = crate::get_cargo_command(target);
	match target {
		RuntimeTarget::Wasm => {
			if !cargo_command.supports_substrate_runtime_env(target) {
				return Err(colorize_error_message(
					"Cannot compile a WASM runtime: no compatible Rust compiler found!\n\
					 Install at least Rust 1.68.0 or a recent nightly version.",
				));
			}

			check_wasm_toolchain_installed(cargo_command)
		},
		RuntimeTarget::Riscv => {
			if !cargo_command.supports_substrate_runtime_env(target) {
				return Err(colorize_error_message(
					"Cannot compile a RISC-V runtime: no compatible Rust compiler found!\n\
					 Install a toolchain from here and try again: https://github.com/paritytech/rustc-rv32e-toolchain/",
				));
			}

			let dummy_crate = DummyCrate::new(&cargo_command, target);
			let version = dummy_crate.get_rustc_version();
			Ok(CargoCommandVersioned::new(cargo_command, version))
		},
	}
}

struct DummyCrate<'a> {
	cargo_command: &'a CargoCommand,
	temp: tempfile::TempDir,
	manifest_path: PathBuf,
	target: RuntimeTarget,
}

impl<'a> DummyCrate<'a> {
	/// Creates a minimal dummy crate.
	fn new(cargo_command: &'a CargoCommand, target: RuntimeTarget) -> Self {
		let temp = tempdir().expect("Creating temp dir does not fail; qed");
		let project_dir = temp.path();
		fs::create_dir_all(project_dir.join("src")).expect("Creating src dir does not fail; qed");

		let manifest_path = project_dir.join("Cargo.toml");
		write_file_if_changed(
			&manifest_path,
			r#"
				[package]
				name = "dummy-crate"
				version = "1.0.0"
				edition = "2021"

				[workspace]
			"#,
		);

		write_file_if_changed(project_dir.join("src/main.rs"), "fn main() {}");
		DummyCrate { cargo_command, temp, manifest_path, target }
	}

	fn prepare_command(&self, subcommand: &str) -> Command {
		let mut cmd = self.cargo_command.command();
		// Chdir to temp to avoid including project's .cargo/config.toml
		// by accident - it can happen in some CI environments.
		cmd.current_dir(&self.temp);
		cmd.arg(subcommand)
			.arg(format!("--target={}", self.target.rustc_target()))
			.args(&["--manifest-path", &self.manifest_path.display().to_string()]);

		if super::color_output_enabled() {
			cmd.arg("--color=always");
		}

		// manually set the `CARGO_TARGET_DIR` to prevent a cargo deadlock
		let target_dir = self.temp.path().join("target").display().to_string();
		cmd.env("CARGO_TARGET_DIR", &target_dir);

		// Make sure the host's flags aren't used here, e.g. if an alternative linker is specified
		// in the RUSTFLAGS then the check we do here will break unless we clear these.
		cmd.env_remove("CARGO_ENCODED_RUSTFLAGS");
		cmd.env_remove("RUSTFLAGS");
		// Make sure if we're called from within a `build.rs` the host toolchain won't override a
		// rustup toolchain we've picked.
		cmd.env_remove("RUSTC");
		cmd
	}

	fn get_rustc_version(&self) -> String {
		let mut run_cmd = self.prepare_command("rustc");
		run_cmd.args(&["-q", "--", "--version"]);
		run_cmd
			.output()
			.ok()
			.and_then(|o| String::from_utf8(o.stdout).ok())
			.unwrap_or_else(|| "unknown rustc version".into())
	}

	fn get_sysroot(&self) -> Option<String> {
		let mut sysroot_cmd = self.prepare_command("rustc");
		sysroot_cmd.args(&["-q", "--", "--print", "sysroot"]);
		sysroot_cmd.output().ok().and_then(|o| String::from_utf8(o.stdout).ok())
	}

	fn try_build(&self) -> Result<(), Option<String>> {
		let Ok(result) = self.prepare_command("build").output() else { return Err(None) };
		if !result.status.success() {
			return Err(Some(String::from_utf8_lossy(&result.stderr).into()));
		}
		Ok(())
	}
}

fn check_wasm_toolchain_installed(
	cargo_command: CargoCommand,
) -> Result<CargoCommandVersioned, String> {
	let dummy_crate = DummyCrate::new(&cargo_command, RuntimeTarget::Wasm);

	if let Err(error) = dummy_crate.try_build() {
		let basic_error_message = colorize_error_message(
			"Rust WASM toolchain is not properly installed; please install it!",
		);
		return match error {
			None => Err(basic_error_message),
			Some(error) if error.contains("the `wasm32-unknown-unknown` target may not be installed") => {
				Err(colorize_error_message("Cannot compile the WASM runtime: the `wasm32-unknown-unknown` target is not installed!\n\
				                         You can install it with `rustup target add wasm32-unknown-unknown` if you're using `rustup`."))
			},
			// Apparently this can happen when we're running on a non Tier 1 platform.
			Some(ref error) if error.contains("linker `rust-lld` not found") =>
				Err(colorize_error_message("Cannot compile the WASM runtime: `rust-lld` not found!")),
			Some(error) => Err(format!(
				"{}\n\n{}\n{}\n{}{}\n",
				basic_error_message,
				colorize_aux_message("Further error information:"),
				colorize_aux_message(&"-".repeat(60)),
				error,
				colorize_aux_message(&"-".repeat(60)),
			))
		}
	}

	let version = dummy_crate.get_rustc_version();
	if crate::build_std_required() {
		if let Some(sysroot) = dummy_crate.get_sysroot() {
			let src_path =
				Path::new(sysroot.trim()).join("lib").join("rustlib").join("src").join("rust");
			if !src_path.exists() {
				return Err(colorize_error_message(
					"Cannot compile the WASM runtime: no standard library sources found!\n\
					 You can install them with `rustup component add rust-src` if you're using `rustup`.",
				))
			}
		}
	}

	Ok(CargoCommandVersioned::new(cargo_command, version))
}
