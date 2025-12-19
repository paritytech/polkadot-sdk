// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Test for warp syncing nodes.
//!
//! ## How to update this test?
//!
//! Usually, this action is required after changes suffered by `cumulus-test-runtime` or
//! `rococo-local`. The test starts a relaychain + parachain network, where a few nodes are started
//! based on existing db snapshots, while the rest of the nodes are warp synced to the latest
//! state. Updating the test means updating the chain specs used to start both relaychain and
//! parachain nodes, but also the snapshots.
//!
//! ### Updating chain specs
//!
//! Existing chain specs are found under [./warp-sync-parachain-spec.json] and
//! [./warp-sync-relaychain-spec.json]. We need to replace them with the updated chain specs.
//!
//! #### For parachain
//!
//! 1. We need to rebuild `cumulus-test-runtime`:
//!
//! ```bash
//! cargo build -p cumulus-test-runtime --release
//! ```
//!
//! 2. Build `chain-spec-builder`:
//!
//! ```bash
//! cargo build -p staging-chain-spec-builder --release
//! ```
//!
//! 3. Generate the chain spec:
//! ```bash
//! target/release/chain-spec-builder create -r target/release/wbuild/cumulus-test-runtime/cumulus_test_runtime.wasm named-preset development
//! ```
//!
//! 4. Replace the chain spec:
//! ```bash
//! mv chain_spec.json cumulus/zombienet/zombienet-sdk/tests/zombie_ci/warp-sync-parachain-spec.json
//! ```
//!
//! #### For relaychain
//!
//! 1. Build the `polkadot` binary
//! ```bash
//! cargo build -p polkadot --release
//! ```
//!
//! 2. Export `rococo-local` chainspec:
//! ```bash
//! polkadot export-chain-spec --chain rococo-local > chain_spec.json
//! ```
//!
//! 3. Replace the chain spec:
//! ```bash
//! mv chain_spec.json cumulus/zombienet/zombienet-sdk/tests/zombie_ci/warp-sync-relaychain-spec.json
//! ```
//!
//! ### Update snapshots
//!
//! **NOTE**: This process is automated by the `update-warp-sync-test.sh` script. See the script
//! for usage instructions. Manual steps are documented below for reference.
//!
//! For this we need to have the zombienet network running from genesis for a while, with same
//! nodes, and archive final db states of `alice` and `one`. Actual steps below:
//!
//! #### Using the automation script (recommended)
//!
//! ```bash
//! # Generate snapshots
//! export ZOMBIENET_SDK_BASE_DIR=<absolute-existing-dir-path>
//! ./update-warp-sync-test.sh snapshots-run
//! ./update-warp-sync-test.sh snapshots-archive
//!
//! # Test locally before uploading
//! ./update-warp-sync-test.sh snapshots-test-local
//!
//! # Upload to GCS (requires credentials) and update constants
//! ```
//!
//! #### Manual process
//!
//! 1. Run the test with `snapshot-update-mode` feature:
//! ```bash
//! ZOMBIENET_SDK_BASE_DIR=<absolute-existing-dir-path> ZOMBIE_PROVIDER=native cargo nextest run --release \
//! -p cumulus-zombienet-sdk-tests --features zombie-ci,snapshot-update-mode --no-capture -- full_node_warp_sync
//! ```
//!
//! 2. Archive/compress the databases:
//!
//! For relaychain:
//! ```bash
//! cd $ZOMBIENET_SDK_BASE_DIR/alice
//! tar -czf alice-db.tgz data/
//! ```
//!
//! For parachain:
//! ```bash
//! cd $ZOMBIENET_SDK_BASE_DIR/one
//! tar -czf one-db.tgz data/ relay-data/
//! ```
//!
//! 3. Test locally before uploading:
//! ```bash
//! export DB_SNAPSHOT_RELAYCHAIN_LOCAL="file://$PWD/alice-db.tgz"
//! export DB_SNAPSHOT_PARACHAIN_LOCAL="file://$PWD/one-db.tgz"
//! cargo nextest run --release -p cumulus-zombienet-sdk-tests --features zombie-ci --no-capture -- full_node_warp_sync
//! ```
//!
//! 4. Upload the archives to public URL (CI/CD team can help), and update the const's in this file
//!    to point to them.

use anyhow::anyhow;
use std::time::Duration;

use polkadot_primitives::Id as ParaId;

use crate::utils::{initialize_network, BEST_BLOCK_METRIC};
use cumulus_zombienet_sdk_helpers::assert_para_is_registered;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	AddNodeOptions, NetworkConfig, NetworkConfigBuilder, NetworkNode,
};

const PARA_ID: u32 = 2000;

const DB_SNAPSHOT_RELAYCHAIN: &str = "https://storage.googleapis.com/zombienet-db-snaps/zombienet/0007-full_node_warp_sync_db/alice-db.tgz";
const DB_SNAPSHOT_PARACHAIN: &str = "https://storage.googleapis.com/zombienet-db-snaps/zombienet/0007-full_node_warp_sync_db/one-db.tgz";

#[cfg(feature = "snapshot-update-mode")]
const SYNC_TIMEOUT: u64 = 225;
#[cfg(not(feature = "snapshot-update-mode"))]
const SYNC_TIMEOUT: u64 = 86400; // 24 hours for snapshot generation

// Helper to support local snapshot testing via environment variables
fn get_snapshot_url(default: &str, env_var: &str) -> String {
	std::env::var(env_var).unwrap_or_else(|_| default.to_string())
}

// Asserting Warp sync requires at least sync=debug level
async fn assert_warp_sync(node: &NetworkNode) -> Result<(), anyhow::Error> {
	let option_1_line = LogLineCountOptions::new(|n| n == 1, Duration::from_secs(5), false);

	log::info!("Asserting Warp sync for node {}", node.name());
	// We are interested only in Relaychain Warp sync (relaychain and parachain nodes),
	// thus exclude exclude lines containing "[Parachain]"
	let result = node
		.wait_log_line_count_with_timeout(
			r"(?<!\[Parachain\] )Started warp sync with [0-9]+ peers",
			false,
			option_1_line.clone(),
		)
		.await?;
	if !result.success() {
		return Err(anyhow!("Warp sync is not started"));
	}
	let result = node
		.wait_log_line_count_with_timeout(
			r"(?<!\[Parachain\] )Starting import of [0-9]+ blocks.*\(origin: WarpSync\)",
			false,
			option_1_line.clone(),
		)
		.await?;
	if !result.success() {
		return Err(anyhow!("Warp sync block import is not started"));
	}
	let result = node
		.wait_log_line_count_with_timeout(
			r"(?<!\[Parachain\] )Imported [0-9]+ out of [0-9]+ blocks.*\(origin: WarpSync\)",
			false,
			option_1_line.clone(),
		)
		.await?;
	if !result.success() {
		return Err(anyhow!("Warp sync block import is not progressing"));
	}

	let result = node
		.wait_log_line_count_with_timeout(
			r"(?<!\[Parachain\] )Warp sync is complete",
			false,
			option_1_line.clone(),
		)
		.await?;
	if !result.success() {
		return Err(anyhow!("Warp sync is not complete"));
	}

	Ok(())
}

// Asserting Gap sync requires at least sync=debug level
async fn assert_gap_sync(node: &NetworkNode) -> Result<(), anyhow::Error> {
	let option_1_line = LogLineCountOptions::new(|n| n == 1, Duration::from_secs(5), false);
	let option_at_least_5_lines =
		LogLineCountOptions::new(|n| n > 1, Duration::from_secs(5), false);

	log::info!("Asserting Gap sync for node {}", node.name());
	// We are interested only in Relaychain Gap sync (relaychain and parachain nodes),
	// thus exclude exclude lines containing "[Parachain]"
	let result = node
		.wait_log_line_count_with_timeout(
			r"(?<!\[Parachain\] )Starting gap sync",
			false,
			option_1_line.clone(),
		)
		.await?;
	if !result.success() {
		return Err(anyhow!("Gap sync not started"));
	}

	let result = node
		.wait_log_line_count_with_timeout(
			r"(?<!\[Parachain\] )Starting import of [0-9]+ blocks.*\(origin: GapSync\)",
			false,
			option_at_least_5_lines.clone(),
		)
		.await?;
	if !result.success() {
		return Err(anyhow!("Gap sync block imports are not started"));
	}

	let result = node
		.wait_log_line_count_with_timeout(
			r"(?<!\[Parachain\] )Imported [0-9]+ out of [0-9]+ blocks.*\(origin: GapSync\)",
			false,
			option_at_least_5_lines,
		)
		.await?;
	if !result.success() {
		return Err(anyhow!("Gap sync block imports are not progressing"));
	}

	let result = node
		.wait_log_line_count_with_timeout(
			r"(?<!\[Parachain\] )Block history download is complete",
			false,
			option_1_line.clone(),
		)
		.await?;
	if !result.success() {
		return Err(anyhow!("Gap sync is not complete"));
	}

	Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn full_node_warp_sync() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let config = build_network_config().await?;
	let mut network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let alice_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;

	log::info!("Ensuring parachain is registered");
	assert_para_is_registered(&alice_client, ParaId::from(PARA_ID), 10).await?;

	#[cfg(not(feature = "snapshot-update-mode"))]
	{
		for name in ["dave", "eve", "four", "five"] {
			assert_warp_sync(network.get_node(name)?).await?;
			assert_gap_sync(network.get_node(name)?).await?;
		}

		// check progress
		for name in ["dave", "eve", "four", "five"] {
			log::info!("Checking full node {name} is syncing");
			network
				.get_node(name)?
				.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 930.0, SYNC_TIMEOUT)
				.await?;
		}

		for name in ["alice", "bob", "one", "two"] {
			log::info!("Pausing node {name}");
			network.get_node(name)?.pause().await?;
		}

		// Add ferdie node dynamically
		log::info!("Adding ferdie node to the network");
		let images = zombienet_sdk::environment::get_images_from_env();
		let ferdie_options = AddNodeOptions {
			image: Some(images.polkadot.as_str().try_into()?),
			command: Some("polkadot".try_into()?),
			subcommand: None,
			args: vec![
				"-lparachain=debug,sync=trace".into(),
				"--no-beefy".into(),
				("--sync", "warp").into(),
			],
			env: vec![],
			is_validator: true,
			rpc_port: None,
			prometheus_port: None,
			p2p_port: None,
			chain_spec: Some("tests/zombie_ci/warp-sync-relaychain-spec.json".into()),
		};
		network.add_node("ferdie", ferdie_options).await?;

		// Assert warp and gap sync for ferdie
		let ferdie = network.get_node("ferdie")?;
		assert_warp_sync(ferdie).await?;
		assert_gap_sync(ferdie).await?;

		// Check progress for ferdie
		log::info!("Checking full node ferdie is syncing");
		ferdie
			.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 930.0, SYNC_TIMEOUT)
			.await?;
	}

	#[cfg(feature = "snapshot-update-mode")]
	{
		log::info!("Checking progress");
		for name in ["alice", "bob", "one", "two"] {
			// for name in ["alice", "bob"] {
			log::info!("Checking full node {name} is syncing");
			network
				.get_node(name)?
				.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 930.0, SYNC_TIMEOUT)
				.await?;
		}
	}

	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	// Get snapshot URLs (with optional local override via env vars)
	let relaychain_snapshot =
		get_snapshot_url(DB_SNAPSHOT_RELAYCHAIN, "DB_SNAPSHOT_RELAYCHAIN_LOCAL");
	let parachain_snapshot = get_snapshot_url(DB_SNAPSHOT_PARACHAIN, "DB_SNAPSHOT_PARACHAIN_LOCAL");

	// Network setup:
	// - relaychain nodes:
	//   - alice    - validator
	//   - bob      - validator
	//   - charlie  - validator
	//   - dave     - validator
	//   - eve      - full node
	// - parachain nodes
	//   - one      - collator
	//   - two      - collator
	//   - three    - collator
	//   - four     - full node
	//   - five     - full node
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_chain_spec_path("tests/zombie_ci/warp-sync-relaychain-spec.json")
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_node(|node| {
					let n = node.with_name("alice");
					#[cfg(not(feature = "snapshot-update-mode"))]
					let n = n.with_db_snapshot(relaychain_snapshot.as_str());
					#[cfg(feature = "snapshot-update-mode")]
					let n = n.with_args(vec![
						("-lparachain=debug").into(),
						("--state-pruning", "archive").into(),
					]);
					n
				})
				.with_node(|node| {
					let n = node.with_name("bob");
					#[cfg(not(feature = "snapshot-update-mode"))]
					let n = n.with_db_snapshot(relaychain_snapshot.as_str());
					n
				})
				.with_node(|node| {
					let n = node.with_name("charlie");
					#[cfg(not(feature = "snapshot-update-mode"))]
					let n = n.with_db_snapshot(relaychain_snapshot.as_str());
					n
				})
				.with_node(|node| {
					node.with_name("dave").with_args(vec![
						("-lparachain=debug,sync=trace").into(),
						("--no-beefy").into(),
						("--reserved-only").into(),
						(
							"--reserved-nodes",
							vec![
								"{{ZOMBIE:alice:multiaddr}}",
								"{{ZOMBIE:bob:multiaddr}}",
								"{{ZOMBIE:charlie:multiaddr}}",
							],
						)
							.into(),
						("--sync", "warp").into(),
					])
				})
				.with_node(|node| {
					node.with_name("eve").validator(false).with_args(vec![
						("-lparachain=debug,sync=trace").into(),
						("--no-beefy").into(),
						("--sync", "warp").into(),
					])
				})
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain_spec_path("tests/zombie_ci/warp-sync-parachain-spec.json")
				.with_default_args(vec![("-lparachain=debug").into(), ("--").into()])
				.with_collator(|n| {
					let node = n.with_name("one");
					#[cfg(not(feature = "snapshot-update-mode"))]
					let node = node.with_db_snapshot(parachain_snapshot.as_str());
					#[cfg(feature = "snapshot-update-mode")]
					let node = node.with_args(vec![
						("-lparachain=debug").into(),
						("--state-pruning", "archive").into(),
						("--").into(),
					]);
					node
				})
				.with_collator(|n| {
					let node = n.with_name("two");
					#[cfg(not(feature = "snapshot-update-mode"))]
					let node = node.with_db_snapshot(parachain_snapshot.as_str());
					node
				})
				.with_collator(|n| {
					let node = n.with_name("three");
					#[cfg(not(feature = "snapshot-update-mode"))]
					let node = node.with_db_snapshot(parachain_snapshot.as_str());
					node
				})
				.with_collator(|n| {
					n.with_name("four").validator(false).with_args(vec![
						("-lsync=trace").into(),
						("--sync", "warp").into(),
						("--").into(),
						("--sync", "warp").into(),
					])
				})
				.with_collator(|n| {
					n.with_name("five").validator(false).with_args(vec![
						("-lsync=trace").into(),
						("--sync", "warp").into(),
						("--relay-chain-rpc-urls", "{{ZOMBIE:charlie:ws_uri}}").into(),
					])
				})
		})
		.with_global_settings(|global_settings| match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	Ok(config)
}
