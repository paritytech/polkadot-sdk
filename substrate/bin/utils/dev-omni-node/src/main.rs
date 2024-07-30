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
		path::Path,
		process,
	};

	const RUNTIME_PATH: &'static str =
		"target/release/wbuild/minimal-template-runtime/minimal_template_runtime.wasm";
	const CHAIN_SPEC_BUILDER: &'static str = "target/release/chain-spec-builder";
	const DEV_OMNI_NODE: &'static str = "target/release/dev-omni-node";

	fn maybe_build_runtime() {
		if !Path::new(RUNTIME_PATH).exists() {
			println!("Building runtime...");
			assert_cmd::Command::new("cargo")
				.arg("build")
				.arg("--release")
				.arg("-p")
				.arg("minimal-template-runtime")
				.assert()
				.success();
		}
		assert!(Path::new(RUNTIME_PATH).exists(), "runtime must now exist!");
	}

	fn maybe_build_chain_spec_builder() {
		// build chain-spec-builder if it does not exist
		if !Path::new(CHAIN_SPEC_BUILDER).exists() {
			println!("Building chain-spec-builder...");
			assert_cmd::Command::new("cargo")
				.arg("build")
				.arg("--release")
				.arg("-p")
				.arg("staging-chain-spec-builder")
				.assert()
				.success();
		}
		assert!(Path::new(CHAIN_SPEC_BUILDER).exists(), "chain-spec-builder must now exist!");
	}

	fn maybe_build_dev_omni_node() {
		// build dev-omni-node if it does not exist
		if !Path::new(DEV_OMNI_NODE).exists() {
			println!("Building dev-omni-node...");
			assert_cmd::Command::new("cargo")
				.arg("build")
				.arg("--release")
				.arg("-p")
				.arg("dev-omni-node")
				.assert()
				.success();
		}
		assert!(Path::new(DEV_OMNI_NODE).exists(), "dev-omni-node must now exist!");
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
	#[ignore = "is flaky"]
	fn run_omni_node_with_chain_spec() {
		// set working directory to project root, 4 parents
		std::env::set_current_dir(std::env::current_dir().unwrap().join("../../../..")).unwrap();
		// last segment of cwd must now be `polkadot-sdk`
		assert!(std::env::current_dir().unwrap().ends_with("polkadot-sdk"));

		maybe_build_runtime();
		maybe_build_chain_spec_builder();
		maybe_build_dev_omni_node();

		process::Command::new(CHAIN_SPEC_BUILDER)
			.arg("create")
			.args(["-r", RUNTIME_PATH])
			.arg("default")
			.stderr(process::Stdio::piped())
			.spawn()
			.unwrap();

		// join current dir and chain_spec.json
		let chain_spec_file = std::env::current_dir().unwrap().join("chain_spec.json");

		let mut binding = process::Command::new(DEV_OMNI_NODE);
		let node_cmd = &mut binding
			.arg("--tmp")
			.args(["--chain", chain_spec_file.to_str().unwrap()])
			.stderr(process::Stdio::piped());

		ensure_node_process_works(node_cmd);

		// delete chain_spec.json
		std::fs::remove_file(chain_spec_file).unwrap();
	}

	#[test]
	#[ignore = "is flaky"]
	fn run_omni_node_with_runtime() {
		maybe_build_runtime();
		maybe_build_dev_omni_node();

		let mut binding = process::Command::new(DEV_OMNI_NODE);
		let node_cmd = binding
			.arg("--tmp")
			.args(["--runtime", RUNTIME_PATH])
			.stderr(process::Stdio::piped());

		ensure_node_process_works(node_cmd);
	}
}
