// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Smoke tests for the Bulletin parachain PoC binary.
//!
//! After Pass 1 (lib revert), the lib no longer exposes HOP CLI flags. The
//! Pass 2 follow-up will reintroduce them via a Bulletin-owned `Cli` that
//! flattens `sc_hop::HopParams` alongside `polkadot_omni_node_lib::Cli` and
//! wires them through a `NodeExtension` trait. Until then we only assert the
//! Bulletin identity surface.

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
fn version_carries_bulletin_name() {
	let version = run(&["--version"]);
	assert!(
		version.starts_with("polkadot-bulletin-parachain "),
		"`--version` must identify as polkadot-bulletin-parachain, got: {version:?}"
	);
}
