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

use assert_cmd::cargo::cargo_bin;
use nix::sys::signal::SIGINT;
use std::process::Command;
use tempfile::tempdir;

mod common;

#[tokio::test]
#[cfg(unix)]
#[ignore]
async fn purge_chain_works() {
	// Check that both databases are deleted

	let base_dir = tempdir().expect("could not create a temp dir");
	let base_dir_path = format!("{}/polkadot", base_dir.path().display());

	let args = &["--", "-d", &base_dir_path, "--chain=rococo-local"];

	common::run_node_for_a_while(base_dir.path(), args, SIGINT).await;

	assert!(base_dir.path().join("chains/local_testnet/db/full").exists());
	assert!(base_dir.path().join("polkadot/chains/rococo_local_testnet/db/full").exists());

	let status = Command::new(cargo_bin("polkadot-parachain"))
		.args(["purge-chain", "-d"])
		.arg(base_dir.path())
		.arg("-y")
		.status()
		.unwrap();
	assert!(status.success());

	// Make sure that the `parachain_local_testnet` chain folder exists, but the `db` is deleted.
	assert!(base_dir.path().join("chains/local_testnet").exists());
	assert!(!base_dir.path().join("chains/local_testnet/db/full").exists());
	assert!(base_dir.path().join("polkadot/chains/rococo_local_testnet").exists());
	assert!(!base_dir.path().join("polkadot/chains/rococo_local_testnet/db/full").exists());
}
