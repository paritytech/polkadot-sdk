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
//! For this we need to have the zombienet network running from genesis for a while, with same
//! nodes, and archive final db states of `alice` and `eve`. Actual steps below:
//!
//! #### Modify the test
//!
//! 1. Comment the `with_db_snapshot` setters.
//! 2. make `alice` and `eve` archive nodes by adding:
//! ```ignore
//! .with_args(vec![("--state-pruning", "archive")])
//! ```
//! 3. Increase the `wait_metric_with_timeout(.., .., 225u64)` timeout parameter to something like
//!    `86400u64` (a day worth of running, which should be sufficient time for the node to reach the
//!    930th best block on `eve`).
//!
//! #### Run the test
//! ```bash
//! ZOMBIENET_SDK_BASE_DIR=<absolute-existing-dir-path> ZOMBIE_PROVIDER=native cargo nextest run --release \
//! -p cumulus-zombienet-sdk-tests --features zombie-ci --no-capture -- full_node_warp_sync
//! ```
//!
//! #### Archive/compress the databases
//!
//! 1. For relaychain:
//!
//! ```bash
//! cd $ZOMBIENET_SDK_BASE_DIR/alice
//! tar -czf alice-db.tgz data/
//! ```
//!
//! 2. For parachain:
//!
//! ```bash
//! cd $ZOMBIENET_SDK_BASE_DIR/eve
//! tar -czf eve-db.tgz data/ relay-data/
//! ```
//!
//! 3. Upload the archives to public URL (CI/CD team can help), and update the const's in this file
//!    to point to them.

use anyhow::anyhow;

use polkadot_primitives::Id as ParaId;

use crate::utils::{initialize_network, BEST_BLOCK_METRIC};
use cumulus_zombienet_sdk_helpers::assert_para_is_registered;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfig, NetworkConfigBuilder,
};

const PARA_ID: u32 = 2000;

const DB_SNAPSHOT_RELAYCHAIN: &str = "https://storage.googleapis.com/zombienet-db-snaps/zombienet/0007-full_node_warp_sync_db/alice-db.tgz";
const DB_SNAPSHOT_PARACHAIN: &str = "https://storage.googleapis.com/zombienet-db-snaps/zombienet/0007-full_node_warp_sync_db/eve-db.tgz";

#[tokio::test(flavor = "multi_thread")]
async fn full_node_warp_sync() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Spawning network");
	let config = build_network_config().await?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let alice_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;

	log::info!("Ensuring parachain is registered");
	assert_para_is_registered(&alice_client, ParaId::from(PARA_ID), 10).await?;

	for name in ["two", "three", "four"] {
		log::info!("Checking full node {name} is syncing");
		assert!(network
			.get_node(name)?
			.wait_metric_with_timeout(BEST_BLOCK_METRIC, |b| b >= 930.0, 225u64)
			.await
			.is_ok());
	}

	Ok(())
}

async fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	// Network setup:
	// - relaychain nodes:
	//   - alice    - validator
	//   - bob      - validator
	//   - charlie  - validator
	//   - dave     - validator
	// - parachain nodes
	//   - eve      - collator
	//   - ferdie   - collator
	//   - one      - collator
	//   - two      - full node
	//   - three    - full node
	//   - four     - full node
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_chain_spec_path("tests/zombie_ci/warp-sync-relaychain-spec.json")
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_node(|node| node.with_name("alice").with_db_snapshot(DB_SNAPSHOT_RELAYCHAIN))
				.with_node(|node| node.with_name("bob").with_db_snapshot(DB_SNAPSHOT_RELAYCHAIN))
				.with_node(|node| {
					node.with_name("charlie").with_db_snapshot(DB_SNAPSHOT_RELAYCHAIN)
				})
				.with_node(|node| {
					node.with_name("dave").with_args(vec![
						("-lparachain=debug").into(),
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
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain_spec_path("tests/zombie_ci/warp-sync-parachain-spec.json")
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_collator(|n| n.with_name("eve").with_db_snapshot(DB_SNAPSHOT_PARACHAIN))
				.with_collator(|n| n.with_name("ferdie").with_db_snapshot(DB_SNAPSHOT_PARACHAIN))
				.with_collator(|n| n.with_name("one").with_db_snapshot(DB_SNAPSHOT_PARACHAIN))
				.with_collator(|n| {
					n.with_name("two").validator(false).with_args(vec![
						("-lsync=debug").into(),
						("--sync", "warp").into(),
						("--").into(),
						("--sync", "warp").into(),
					])
				})
				.with_collator(|n| {
					n.with_name("three").validator(false).with_args(vec![
						("-lsync=debug").into(),
						("--sync", "warp").into(),
						("--relay-chain-rpc-urls", "{{ZOMBIE:alice:ws_uri}}").into(),
					])
				})
				.with_collator(|n| {
					n.with_name("four").validator(false).with_args(vec![
						("-lsync=debug").into(),
						("--sync", "warp").into(),
						("--relay-chain-rpc-urls", "{{ZOMBIE:dave:ws_uri}}").into(),
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
