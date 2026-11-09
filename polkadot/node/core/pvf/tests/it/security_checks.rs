// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Tests for the `--check-security-features` flag, which is used by `polkadot-cli` to verify
//! worker binaries before the node starts.

use polkadot_node_core_pvf::testing::build_workers_and_get_paths;
use std::{path::Path, process::Command};

// The exact argument order used by `polkadot-cli`'s startup check: the flag must come first, with
// `--worker-dir-path` following it.
fn check_security_features(worker_path: &Path, worker_dir_path: &Path) -> bool {
	let status = Command::new(worker_path)
		.arg("--check-security-features")
		.arg("--worker-dir-path")
		.arg(worker_dir_path)
		.status()
		.expect("failed to spawn worker binary");

	status.success()
}

#[test]
fn execute_worker_check_security_features_succeeds_for_valid_worker_dir() {
	let (_, execute_worker_path) = build_workers_and_get_paths();
	let worker_dir = tempfile::tempdir().unwrap();

	assert!(check_security_features(&execute_worker_path, worker_dir.path()));
}

#[test]
fn prepare_worker_check_security_features_succeeds_for_valid_worker_dir() {
	let (prepare_worker_path, _) = build_workers_and_get_paths();
	let worker_dir = tempfile::tempdir().unwrap();

	assert!(check_security_features(&prepare_worker_path, worker_dir.path()));
}

#[test]
fn check_security_features_fails_for_missing_worker_dir() {
	let (_, execute_worker_path) = build_workers_and_get_paths();
	let worker_dir = tempfile::tempdir().unwrap();
	let missing_dir = worker_dir.path().join("does-not-exist");

	assert!(!check_security_features(&execute_worker_path, &missing_dir));
}
