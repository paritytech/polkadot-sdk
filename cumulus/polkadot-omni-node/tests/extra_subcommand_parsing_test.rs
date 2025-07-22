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

/// Integration tests that spawn the actual binary `polkadot-omni-node`
/// using `assert_cmd`. We verify that the help text
/// excludes the `export-chain-spec` sub‑command exactly as intended
use assert_cmd::Command;

#[test]
fn polkadot_omni_node_help_excludes_export_chain_spec() {
	// Run `polkadot-omni-node --help` and capture stdout.
	let output = Command::cargo_bin("polkadot-omni-node")
		.expect("binary `polkadot-omni-node` should be built by the workspace")
		.arg("--help")
		.assert()
		.success()
		.get_output()
		.stdout
		.clone();

	let help_text = String::from_utf8_lossy(&output);
	assert!(
		!help_text.contains("export-chain-spec"),
		"`polkadot-omni-node --help` must NOT list the \"export-chain-spec\" subcommand"
	);
}
