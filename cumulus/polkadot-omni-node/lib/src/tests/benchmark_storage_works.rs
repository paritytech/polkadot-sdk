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

#![cfg(feature = "runtime-benchmarks")]

use assert_cmd::cargo::cargo_bin;
use std::{
	path::Path,
	process::{Command, ExitStatus},
};
use tempfile::tempdir;

/// The runtimes that this command supports.
static RUNTIMES: [&str; 1] = ["asset-hub-westend"];

/// The `benchmark storage` command works for the dev runtimes.
#[test]
#[ignore]
fn benchmark_storage_works() {
	for runtime in RUNTIMES {
		let tmp_dir = tempdir().expect("could not create a temp dir");
		let base_path = tmp_dir.path();
		let runtime = format!("{}-dev", runtime);

		// Benchmarking the storage works and creates the weight file.
		assert!(benchmark_storage("rocksdb", &runtime, base_path).success());
		assert!(base_path.join("rocksdb_weights.rs").exists());

		assert!(benchmark_storage("paritydb", &runtime, base_path).success());
		assert!(base_path.join("paritydb_weights.rs").exists());
	}
}

/// Invoke the `benchmark storage` sub-command for the given database and runtime.
fn benchmark_storage(db: &str, runtime: &str, base_path: &Path) -> ExitStatus {
	Command::new(cargo_bin("polkadot-parachain"))
		.args(["benchmark", "storage", "--chain", runtime])
		.arg("--db")
		.arg(db)
		.arg("--weight-path")
		.arg(base_path)
		.args(["--state-version", "0"])
		.args(["--batch-size", "1"])
		.args(["--warmups", "0"])
		.args(["--add", "100", "--mul", "1.2", "--metric", "p75"])
		.status()
		.unwrap()
}
