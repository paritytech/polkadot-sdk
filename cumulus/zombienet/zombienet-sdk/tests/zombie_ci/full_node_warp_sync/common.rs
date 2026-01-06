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

use anyhow::anyhow;
use zombienet_sdk::{
	AddCollatorOptions, AddNodeOptions, LocalFileSystem, Network, NetworkConfig,
	NetworkConfigBuilder,
};

pub const PARA_ID: u32 = 2000;

pub const PARA_BEST_BLOCK_TO_WAIT_FOR: f64 = 930.0;
pub const RELAY_BEST_BLOCK_TO_WAIT_FOR: f64 = 70.0;

const DB_SNAPSHOT_RELAYCHAIN: &str = "https://storage.googleapis.com/zombienet-db-snaps/zombienet/full_node_warp_sync_db/relaychain-db.tgz";
const DB_SNAPSHOT_PARACHAIN: &str =
	"https://storage.googleapis.com/zombienet-db-snaps/zombienet/full_node_warp_sync_db/parachain-db.tgz";

// Helper to support local snapshot testing via environment variables
fn get_snapshot_url(default: &str, env_var: &str) -> String {
	std::env::var(env_var).unwrap_or_else(|_| default.to_string())
}

pub(crate) async fn build_network_config(
	with_snapshot: bool,
) -> Result<NetworkConfig, anyhow::Error> {
	// images are not relevant for `native`, but we leave it here in case we use `k8s` some day
	let images = zombienet_sdk::environment::get_images_from_env();
	log::info!("Using images: {images:?}");

	// Get snapshot URLs (with optional local override via env vars)
	let relaychain_snapshot =
		get_snapshot_url(DB_SNAPSHOT_RELAYCHAIN, "DB_SNAPSHOT_RELAYCHAIN_OVERRIDE");
	let parachain_snapshot =
		get_snapshot_url(DB_SNAPSHOT_PARACHAIN, "DB_SNAPSHOT_PARACHAIN_OVERRIDE");

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
				.with_chain_spec_path(
					"tests/zombie_ci/full_node_warp_sync/warp-sync-relaychain-spec.json",
				)
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_node(|node| {
					let n = node.with_name("alice");
					if with_snapshot {
						n.with_db_snapshot(relaychain_snapshot.as_str())
					} else {
						n.with_args(vec![
							("-lparachain=debug").into(),
							("--state-pruning", "archive").into(),
						])
					}
				})
				.with_node(|node| {
					let n = node.with_name("bob");
					if with_snapshot {
						n.with_db_snapshot(relaychain_snapshot.as_str())
					} else {
						n
					}
				})
				.with_node(|node| {
					let n = node.with_name("charlie");
					if with_snapshot {
						n.with_db_snapshot(relaychain_snapshot.as_str())
					} else {
						n
					}
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
				.with_chain_spec_path(
					"tests/zombie_ci/full_node_warp_sync/warp-sync-parachain-spec.json",
				)
				.with_default_args(vec![("-lparachain=debug").into(), ("--").into()])
				.with_collator(|n| {
					let n = n.with_name("one");

					if with_snapshot {
						n.with_db_snapshot(parachain_snapshot.as_str())
					} else {
						n.with_args(vec![
							("-lparachain=debug").into(),
							("--state-pruning", "archive").into(),
							("--").into(),
						])
					}
				})
				.with_collator(|n| {
					let n = n.with_name("two");
					if with_snapshot {
						n.with_db_snapshot(parachain_snapshot.as_str())
					} else {
						n
					}
				})
				.with_collator(|n| {
					let n = n.with_name("three");
					if with_snapshot {
						n.with_db_snapshot(parachain_snapshot.as_str())
					} else {
						n
					}
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

/// Add a relaychain node to the network and wait until it is up.
///
/// # Arguments
/// * `network` - The zombienet network
/// * `name` - Name of the node to add
/// * `is_validator` - Whether the node is a validator
///
/// # Returns
/// Reference to the added node
pub async fn add_relaychain_node_and_wait<'a>(
	network: &mut Network<LocalFileSystem>,
	name: &str,
	is_validator: bool,
) -> Result<(), anyhow::Error> {
	log::info!("Adding {} node to the network", name);
	let images = zombienet_sdk::environment::get_images_from_env();
	let base_dir = network.base_dir().ok_or(anyhow!("failed to get base dir"))?;

	let options = AddNodeOptions {
		image: Some(images.polkadot.as_str().try_into()?),
		command: Some("polkadot".try_into()?),
		subcommand: None,
		args: vec![
			"-lparachain=debug,sync=trace".into(),
			"--no-beefy".into(),
			("--sync", "warp").into(),
		],
		env: vec![],
		is_validator,
		rpc_port: None,
		prometheus_port: None,
		p2p_port: None,
		chain_spec: Some(format!("{base_dir}/rococo-local.json").into()),
	};

	network.add_node(name, options).await?;
	let node = network.get_node(name)?;
	node.wait_until_is_up(20u64).await?;

	Ok(())
}

/// Add a parachain collator to the network and wait until it is up.
///
/// # Arguments
/// * `network` - The zombienet network
/// * `name` - Name of the node to add
/// * `is_validator` - Whether the node is a validator
///
/// # Returns
/// Reference to the added node
pub async fn add_parachain_collator_and_wait<'a>(
	network: &mut Network<LocalFileSystem>,
	name: &str,
	is_validator: bool,
) -> Result<(), anyhow::Error> {
	log::info!("Adding {} collator to the network", name);
	let images = zombienet_sdk::environment::get_images_from_env();
	let base_dir = network.base_dir().ok_or(anyhow!("failed to get base dir"))?;

	let options = AddCollatorOptions {
		image: Some(images.polkadot.as_str().try_into()?),
		command: Some("test-parachain".try_into()?),
		subcommand: None,
		args: vec![
			("-lsync=trace").into(),
			("--sync", "warp").into(),
			("--").into(),
			("--sync", "warp").into(),
		],
		env: vec![],
		is_validator,
		rpc_port: None,
		prometheus_port: None,
		p2p_port: None,
		chain_spec: Some(format!("{base_dir}/{PARA_ID}.json").into()),
		chain_spec_relay: Some(format!("{base_dir}/rococo-local.json").into()),
	};

	network.add_collator(name, options, PARA_ID).await?;
	let node = network.get_node(name)?;
	node.wait_until_is_up(20u64).await?;

	Ok(())
}
