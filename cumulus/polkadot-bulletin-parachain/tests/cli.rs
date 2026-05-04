// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Smoke tests for the Bulletin parachain PoC binary.
//!
//! These confirm:
//!   1. The Bulletin-owned `Cli` (which augments `polkadot_omni_node_lib::Cli`
//!      with a flattened `sc_hop::HopParams`) exposes the HOP CLI group in
//!      `--help`.
//!   2. The binary identifies itself as `polkadot-bulletin-parachain` (not
//!      as `polkadot-omni-node`).

use assert_cmd::Command;

fn run(args: &[&str]) -> String {
	let output = Command::cargo_bin("polkadot-bulletin-parachain")
		.expect("binary `polkadot-bulletin-parachain` should be built by the workspace")
		.args(args)
		.assert()
		.success()
		.get_output()
		.stdout
		.clone();
	String::from_utf8_lossy(&output).into_owned()
}

#[test]
fn help_inherits_hop_cli_group() {
	let help = run(&["--help"]);
	for flag in ["--enable-hop", "--hop-max-pool-size", "--hop-retention-blocks"] {
		assert!(
			help.contains(flag),
			"`--help` should list `{flag}` (HopParams flattened by the Bulletin Cli)"
		);
	}
}

#[test]
fn version_carries_bulletin_name() {
	let version = run(&["--version"]);
	assert!(
		version.starts_with("polkadot-bulletin-parachain "),
		"`--version` must identify as polkadot-bulletin-parachain, got: {version:?}"
	);
}
