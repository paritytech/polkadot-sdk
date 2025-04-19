// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

/// Integration tests that spawn the actual binaries (`polkadot-omni-node` and
/// `polkadot-parachain`) using `assert_cmd`. We verify that the help text
/// includes or excludes the `export-chain-spec` sub‑command exactly as intended
/// and that invoking the sub‑command executes successfully.

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

#[test]
fn polkadot_parachain_help_includes_export_chain_spec_and_command_runs() {
    // 1) Check that help text lists the extra command.
    let help_output = Command::cargo_bin("polkadot-parachain")
        .expect("binary `polkadot-parachain` should be built by the workspace")
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
