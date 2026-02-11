// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Test warp sync over a chain that contains a historical BEEFY ConsensusReset.
//!
//! This test verifies that a node can successfully warp sync over a chain where
//! a ConsensusReset has already occurred in the past.

use anyhow::anyhow;
use std::time::Duration;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	AddNodeOptions, NetworkConfig, NetworkConfigBuilder,
};

use super::utils::create_set_new_genesis_call;

const POLKADOT_IMAGE_ENV: &str = "POLKADOT_IMAGE";
const VALIDATOR_NAMES: [&str; 3] = ["validator-0", "validator-1", "validator-2"];
const SYNCING_NODE: &str = "warp-syncing-node";

#[tokio::test(flavor = "multi_thread")]
async fn warp_sync_with_historical_consensus_reset() -> Result<(), anyhow::Error> {
	env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	)
	.ok();

	log::info!("Starting warp sync with historical ConsensusReset test");

	let config = build_network_config()?;
	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let mut network = spawn_fn(config).await?;

	log::info!("Waiting for validators to be ready");
	for name in VALIDATOR_NAMES {
		let node = network.get_node(name)?;
		node.wait_metric_with_timeout("node_roles", |v| v == 4.0, 30u64)
			.await
			.map_err(|e| anyhow!("Node {name} role check failed: {e}"))?;
	}

	log::info!("Waiting for initial BEEFY voting");
	for name in VALIDATOR_NAMES {
		let node = network.get_node(name)?;
		node.wait_metric_with_timeout("substrate_beefy_best_block", |v| v >= 5.0, 120u64)
			.await
			.map_err(|e| anyhow!("BEEFY voting not started on {name}: {e}"))?;
	}

	log::info!("Triggering ConsensusReset via set_new_genesis");
	let validator_node = network.get_node("validator-0")?;
	let client: OnlineClient<PolkadotConfig> = validator_node.wait_client().await?;

	let set_genesis_call = create_set_new_genesis_call(5);
	let alice = dev::alice();
	let result = client
		.tx()
		.sign_and_submit_then_watch_default(&set_genesis_call, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;

	log::info!("ConsensusReset scheduled, tx hash: {:?}", result.extrinsic_hash());

	log::info!("Waiting for ConsensusReset detection in logs");
	validator_node
		.wait_log_line_count_with_timeout(
			"ConsensusReset",
			true,
			LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(60), false),
		)
		.await
		.map_err(|e| anyhow!("ConsensusReset not detected in logs: {e}"))?;

	validator_node
		.wait_metric_with_timeout("substrate_beefy_best_block", |v| v >= 15.0, 180u64)
		.await
		.map_err(|e| anyhow!("BEEFY did not continue after reset: {e}"))?;

	log::info!("Spawning warp-syncing node to sync over chain with historical ConsensusReset");
	network
		.add_node(
			SYNCING_NODE,
			AddNodeOptions {
				is_validator: false,
				args: vec!["--sync=warp".into()],
				..Default::default()
			},
		)
		.await?;

	let syncing_node = network.get_node(SYNCING_NODE)?;

	syncing_node
		.wait_metric_with_timeout("substrate_sub_libp2p_is_major_syncing", |v| v == 1.0, 60u64)
		.await
		.map_err(|e| anyhow!("Warp sync did not start: {e}"))?;

	syncing_node
		.wait_metric_with_timeout("substrate_sub_libp2p_is_major_syncing", |v| v == 0.0, 300u64)
		.await
		.map_err(|e| anyhow!("Warp sync did not complete: {e}"))?;

	syncing_node
		.wait_metric_with_timeout("substrate_beefy_best_block", |v| v >= 5.0, 120u64)
		.await
		.map_err(|e| anyhow!("Syncing node BEEFY state incorrect after warp sync: {e}"))?;

	log::info!("Warp-syncing node successfully synced over chain with historical ConsensusReset");

	Ok(())
}

fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let polkadot_image =
		std::env::var(POLKADOT_IMAGE_ENV).unwrap_or_else(|_| images.polkadot.clone());

	let builder = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(polkadot_image.as_str())
				.with_default_args(vec!["--log=beefy=debug,sync=debug".into()])
				.with_node(|node| node.with_name("validator-0"))
				.with_node(|node| node.with_name("validator-1"))
				.with_node(|node| node.with_name("validator-2"))
		})
		.with_global_settings(|gs| {
			if let Ok(base_dir) = std::env::var("ZOMBIENET_SDK_BASE_DIR") {
				gs.with_base_dir(base_dir)
			} else {
				gs
			}
		});

	builder.build().map_err(|e| {
		let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
		anyhow!("config errs: {errs}")
	})
}
