// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use assert_cmd::Command;

fn run(args: &[&str]) -> String {
	let output = Command::cargo_bin("polkadot-bulletin-parachain")
		.expect("workspace builds the binary")
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
		assert!(help.contains(flag), "`--help` missing `{flag}`");
	}
}

#[test]
fn version_carries_bulletin_name() {
	let version = run(&["--version"]);
	assert!(
		version.starts_with("polkadot-bulletin-parachain "),
		"got: {version:?}"
	);
}
