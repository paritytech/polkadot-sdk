// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

#![cfg(unix)]

use assert_cmd::cargo::cargo_bin;
use common::run_with_timeout;
use nix::{
	sys::signal::{kill, Signal::SIGINT},
	unistd::Pid,
};
use std::{
	process::{self, Command},
	time::Duration,
};
use tempfile::tempdir;

pub mod common;

#[tokio::test]
async fn purge_chain_rocksdb_works() {
	run_with_timeout(Duration::from_secs(10 * 60), async move {
		let tmpdir = tempdir().expect("could not create temp dir");

		let mut cmd = Command::new(cargo_bin("polkadot"))
			.stdout(process::Stdio::piped())
			.stderr(process::Stdio::piped())
			.args(["--dev", "-d"])
			.arg(tmpdir.path())
			.arg("--port")
			.arg("33034")
			.arg("--no-hardware-benchmarks")
			.spawn()
			.unwrap();

		let (ws_url, _) = common::find_ws_url_from_output(cmd.stderr.take().unwrap());

		// Let it produce 1 block.
		common::wait_n_finalized_blocks(1, &ws_url).await;

		// Send SIGINT to node.
		kill(Pid::from_raw(cmd.id().try_into().unwrap()), SIGINT).unwrap();
		// Wait for the node to handle it and exit.
		assert!(cmd.wait().unwrap().success());
		assert!(tmpdir.path().join("chains/rococo_dev").exists());
		assert!(tmpdir.path().join("chains/rococo_dev/db/full").exists());

		// Purge chain
		let status = Command::new(cargo_bin("polkadot"))
			.args(["purge-chain", "--dev", "-d"])
			.arg(tmpdir.path())
			.arg("-y")
			.status()
			.unwrap();
		assert!(status.success());

		// Make sure that the chain folder exists, but `db/full` is deleted.
		assert!(tmpdir.path().join("chains/rococo_dev").exists());
		assert!(!tmpdir.path().join("chains/rococo_dev/db/full").exists());
	})
	.await;
}

#[tokio::test]
async fn purge_chain_paritydb_works() {
	run_with_timeout(Duration::from_secs(10 * 60), async move {
		let tmpdir = tempdir().expect("could not create temp dir");

		let mut cmd = Command::new(cargo_bin("polkadot"))
			.stdout(process::Stdio::piped())
			.stderr(process::Stdio::piped())
			.args(["--dev", "-d"])
			.arg(tmpdir.path())
			.arg("--database")
			.arg("paritydb-experimental")
			.arg("--no-hardware-benchmarks")
			.spawn()
			.unwrap();

		let (ws_url, _) = common::find_ws_url_from_output(cmd.stderr.take().unwrap());

		// Let it produce 1 block.
		common::wait_n_finalized_blocks(1, &ws_url).await;

		// Send SIGINT to node.
		kill(Pid::from_raw(cmd.id().try_into().unwrap()), SIGINT).unwrap();
		// Wait for the node to handle it and exit.
		assert!(cmd.wait().unwrap().success());
		assert!(tmpdir.path().join("chains/rococo_dev").exists());
		assert!(tmpdir.path().join("chains/rococo_dev/paritydb/full").exists());

		// Purge chain
		let status = Command::new(cargo_bin("polkadot"))
			.args(["purge-chain", "--dev", "-d"])
			.arg(tmpdir.path())
			.arg("--database")
			.arg("paritydb-experimental")
			.arg("-y")
			.status()
			.unwrap();
		assert!(status.success());

		// Make sure that the chain folder exists, but `db/full` is deleted.
		assert!(tmpdir.path().join("chains/rococo_dev").exists());
		assert!(!tmpdir.path().join("chains/rococo_dev/paritydb/full").exists());
	})
	.await;
}
