// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

/// Integration tests that spawn the actual binary `polkadot-parachain`
/// using `assert_cmd`. We verify that the help text
/// includes the `export-chain-spec` sub‑command exactly as intended
/// and that invoking the sub‑command executes successfully.
use assert_cmd::Command;

#[test]
fn polkadot_parachain_help_includes_export_chain_spec_and_command_runs() {
	// 1) Check that help text lists the extra command.
	let help_output = Command::cargo_bin("polkadot-parachain")
		.expect("binary `polkadot-pFarachain` should be built by the workspace")
		.arg("--help")
		.assert()
		.success()
		.get_output()
		.stdout
		.clone();

	let help_text = String::from_utf8_lossy(&help_output);
	assert!(
		help_text.contains("export-chain-spec"),
		"`polkadot-parachain --help` must list the \"export-chain-spec\" subcommand"
	);

	// 2) Call the sub‑command with `--help` to ensure it dispatches correctly.
	Command::cargo_bin("polkadot-parachain")
		.expect("binary `polkadot-parachain` should be built by the workspace")
		.args(&["export-chain-spec", "--help"])
		.assert()
		.success();
}
