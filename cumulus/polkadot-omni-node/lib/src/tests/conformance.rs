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

//! Conformance tests for parachain node binaries.

use std::path::Path;
use tempfile::tempdir;

#[cfg(unix)]
use nix::sys::signal::Signal::{SIGINT, SIGTERM};

#[cfg(unix)]
use super::common;

/// Write a minimal valid parachain chain spec JSON file into `dir` and return its path.
///
/// Uses the real `cumulus-test-runtime` WASM binary so that `:code` and
/// `:state_version` are present in genesis storage.  Without them the node
/// exits immediately with:
///   "Failed to get runtime version: Runtime missing from initial storage"
/// before it ever prints the RPC server address, causing all conformance
/// tests to panic in `find_ws_url_from_output`.
#[cfg(unix)]
fn write_test_chain_spec(dir: &Path) -> std::path::PathBuf {
	use crate::common::chain_spec::{Extensions, GenericChainSpec};
	use sc_chain_spec::ChainType;

	let spec_path = dir.join("test-parachain-spec.json");

	let wasm = cumulus_test_runtime::WASM_BINARY
		.expect("cumulus-test-runtime WASM binary must be available for conformance tests");

	let chain_spec =
		GenericChainSpec::builder(wasm, Extensions::new("rococo-local".to_string(), 1000))
			.with_name("Test Parachain Local")
			.with_id("test_parachain_local")
			.with_chain_type(ChainType::Local)
			.with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
			.build();

	let json = chain_spec.as_json(false).expect("Failed to serialize test chain spec to JSON");

	std::fs::write(&spec_path, json).expect("Failed to write test chain spec file");

	spec_path
}

/// Run the node with the given args and ensure it starts and can be interrupted.
#[cfg(unix)]
pub async fn running_the_node_works_and_can_be_interrupted(binary_path: &Path) {
	let base_dir = tempdir().expect("could not create a temp dir");

	let spec_path = write_test_chain_spec(base_dir.path());
	let spec_str = spec_path.to_string_lossy().into_owned();

	let args = &["--chain", spec_str.as_str(), "--", "--chain=rococo-local"];

	common::run_node_for_a_while(binary_path, base_dir.path(), args, SIGINT).await;
	common::run_node_for_a_while(binary_path, base_dir.path(), args, SIGTERM).await;
}

/// Run the node and verify that mDNS issues are handled correctly.
#[cfg(unix)]
pub async fn interrupt_mdns_issue(binary_path: &Path) {
	let base_dir = tempdir().expect("could not create a temp dir");

	let spec_path = write_test_chain_spec(base_dir.path());
	let spec_str = spec_path.to_string_lossy().into_owned();

	let args = &["--chain", spec_str.as_str(), "--", "--chain=rococo-local"];

	common::run_node_for_a_while(binary_path, base_dir.path(), args, SIGINT).await;
	common::run_node_for_a_while(binary_path, base_dir.path(), args, SIGTERM).await;
}

/// Run the node with Polkadot relay chain argument parsing.
#[cfg(unix)]
pub async fn polkadot_argument_parsing(binary_path: &Path) {
	let base_dir = tempdir().expect("could not create a temp dir");

	let spec_path = write_test_chain_spec(base_dir.path());
	let spec_str = spec_path.to_string_lossy().into_owned();

	let args = &[
		"--chain",
		spec_str.as_str(),
		"--",
		"--chain=rococo-local",
		"--bootnodes",
		"/ip4/127.0.0.1/tcp/30333/p2p/Qmbx43psh7LVkrYTRXisUpzCubbgYojkejzAgj5mteDnxy",
		"--bootnodes",
		"/ip4/127.0.0.1/tcp/50500/p2p/Qma6SpS7tzfCrhtgEVKR9Uhjmuv55ovC3kY6y6rPBxpWde",
	];

	common::run_node_for_a_while(binary_path, base_dir.path(), args, SIGINT).await;
	common::run_node_for_a_while(binary_path, base_dir.path(), args, SIGTERM).await;
}

/// Run the node and then purge the chain, verifying the DB is cleaned up.
#[cfg(unix)]
pub async fn purge_chain_works(binary_path: &Path) {
	use std::process::Command;

	let base_dir = tempdir().expect("could not create a temp dir");

	let relay_base_dir = format!("{}/polkadot", base_dir.path().display());

	let spec_path = write_test_chain_spec(base_dir.path());
	let spec_str = spec_path.to_string_lossy().into_owned();

	let args = &[
		"--chain",
		spec_str.as_str(),
		"--",
		"-d",
		relay_base_dir.as_str(),
		"--chain=rococo-local",
	];

	common::run_node_for_a_while(binary_path, base_dir.path(), args, SIGINT).await;

	assert!(
		base_dir.path().join("chains/test_parachain_local/db/full").exists(),
		"parachain DB should exist after running the node"
	);
	assert!(
		base_dir.path().join("polkadot/chains/rococo_local_testnet/db/full").exists(),
		"relay chain DB should exist after running the node"
	);

	// Must pass --chain so purge-chain knows which chain spec to use
	let status = Command::new(binary_path)
		.args(["purge-chain", "--chain"])
		.arg(&spec_path)
		.arg("-d")
		.arg(base_dir.path())
		.arg("-y")
		.status()
		.unwrap();
	assert!(status.success(), "purge-chain should exit successfully");

	assert!(
		base_dir.path().join("chains/test_parachain_local").exists(),
		"parachain chain dir should still exist after purge"
	);
	assert!(
		!base_dir.path().join("chains/test_parachain_local/db/full").exists(),
		"parachain DB should be removed after purge"
	);
	assert!(
		base_dir.path().join("polkadot/chains/rococo_local_testnet").exists(),
		"relay chain dir should still exist after purge"
	);
	assert!(
		!base_dir.path().join("polkadot/chains/rococo_local_testnet/db/full").exists(),
		"relay chain DB should be removed after purge"
	);
}

/// Benchmark storage conformance test.
#[cfg(feature = "runtime-benchmarks")]
pub fn benchmark_storage_works(binary_path: &Path) {
	static RUNTIMES: [&str; 1] = ["asset-hub-westend"];

	for runtime in RUNTIMES {
		let tmp_dir = tempdir().expect("could not create a temp dir");
		let base_path = tmp_dir.path();
		let runtime = format!("{}-dev", runtime);

		assert!(
			benchmark_storage_with_binary(binary_path, "rocksdb", &runtime, base_path).success()
		);
		assert!(base_path.join("rocksdb_weights.rs").exists());

		assert!(
			benchmark_storage_with_binary(binary_path, "paritydb", &runtime, base_path).success()
		);
		assert!(base_path.join("paritydb_weights.rs").exists());
	}
}

#[cfg(feature = "runtime-benchmarks")]
fn benchmark_storage_with_binary(
	binary_path: &Path,
	db: &str,
	runtime: &str,
	base_path: &Path,
) -> std::process::ExitStatus {
	std::process::Command::new(binary_path)
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

/// Convenience macro to instantiate all conformance tests for a given binary.
#[macro_export]
macro_rules! instantiate_conformance_tests {
    ($binary_expr:expr) => {
        #[cfg(unix)]
        mod conformance {
            use super::*;

            fn binary() -> std::path::PathBuf {
                $binary_expr
            }

            #[tokio::test]
            async fn running_the_node_works_and_can_be_interrupted() {
                polkadot_omni_node_lib::tests::conformance::running_the_node_works_and_can_be_interrupted(&binary()).await;
            }

            #[tokio::test]
            async fn interrupt_mdns_issue() {
                polkadot_omni_node_lib::tests::conformance::interrupt_mdns_issue(&binary()).await;
            }

            #[tokio::test]
            async fn polkadot_argument_parsing() {
                polkadot_omni_node_lib::tests::conformance::polkadot_argument_parsing(&binary()).await;
            }

            #[tokio::test]
            async fn purge_chain_works() {
                polkadot_omni_node_lib::tests::conformance::purge_chain_works(&binary()).await;
            }

            #[cfg(feature = "runtime-benchmarks")]
            #[test]
            fn benchmark_storage_works() {
                polkadot_omni_node_lib::tests::conformance::benchmark_storage_works(&binary());
            }
        }
    };
}
