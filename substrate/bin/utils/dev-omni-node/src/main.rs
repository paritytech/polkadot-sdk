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

//! A minimal omni-node capable of running any polkadot-sdk-based runtime so long as it adheres
//! [`standards`]. See this module for more information about the assumptions of this node.
//!
//! See [`cli::Cli`] for usage info.

#![warn(missing_docs)]

pub mod cli;
mod command;
mod fake_runtime_api;
mod rpc;
mod service;
pub mod standards;

fn main() -> sc_cli::Result<()> {
	command::run()
}

#[cfg(test)]
mod tests {
	use nix::{
		sys::signal::{kill, Signal::SIGINT},
		unistd::Pid,
	};
	use std::{
		io::{BufRead, BufReader},
		process,
	};

	const RUNTIME_PATH: &'static str =
		"../../../../target/release/wbuild/minimal-template-runtime/minimal_template_runtime.wasm";

	fn maybe_build_runtime() {
		if !std::path::Path::new(RUNTIME_PATH).exists() {
			assert_cmd::Command::new("cargo")
				.arg("build")
				.arg("--release")
				.arg("-p")
				.arg("minimal-template-runtime")
				.assert()
				.success();
		}
	}

	fn ensure_node_process_works(node_cmd: &mut process::Command) {
		let mut node_process = node_cmd.spawn().unwrap();

		std::thread::sleep(std::time::Duration::from_secs(15));
		let stderr = node_process.stderr.take().unwrap();

		kill(Pid::from_raw(node_process.id().try_into().unwrap()), SIGINT).unwrap();
		let exit_status = node_process.wait().unwrap();
		println!("Exit status: {:?}", exit_status);

		// ensure in stderr there is at least one line containing: "Imported #10"
		assert!(
			BufReader::new(stderr).lines().any(|l| { l.unwrap().contains("Imported #10") }),
			"failed to find 10 imported blocks in the output.",
		);
	}

	#[test]
	// #[ignore = "ignore for now"]
	fn run_omni_node_with_chain_spec() {
		maybe_build_runtime();

		process::Command::new(assert_cmd::cargo::cargo_bin("chain-spec-builder"))
			.arg("create")
			.args(["-r", RUNTIME_PATH])
			.arg("default")
			.stderr(process::Stdio::piped())
			.spawn()
			.unwrap();

		// join current dir and chain_spec.json
		let chain_spec_file = std::env::current_dir().unwrap().join("chain_spec.json");

		let mut binding =
			process::Command::new(assert_cmd::cargo::cargo_bin(env!("CARGO_PKG_NAME")));
		let node_cmd = &mut binding
			.arg("--tmp")
			.args(["--chain", chain_spec_file.to_str().unwrap()])
			.args(["-l", "runtime=debug"])
			.args(["--consensus", "manual-seal-1000"])
			.stderr(process::Stdio::piped());

		ensure_node_process_works(node_cmd);

		// delete chain_spec.json
		std::fs::remove_file(chain_spec_file).unwrap();
	}

	#[test]
	#[ignore = "ignore for now"]
	fn run_omni_node_with_runtime() {
		maybe_build_runtime();
		let mut binding =
			process::Command::new(assert_cmd::cargo::cargo_bin(env!("CARGO_PKG_NAME")));
		let node_cmd = binding
			.arg("--tmp")
			.args(["--chain", RUNTIME_PATH])
			.args(["-l", "runtime=debug"])
			.args(["--consensus", "manual-seal-1000"])
			.stderr(process::Stdio::piped());

		ensure_node_process_works(node_cmd);
	}
}
