// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use crate::utils::{
	env_or_default, initialize_network, log_line_absent, log_line_at_least_once,
	log_line_exactly_once, resolve_db_snapshot_height, wait_for_warp_sync_logs,
	BEEFY_BEST_BLOCK_METRIC, BEST_BLOCK_METRIC, CHAIN_SPEC_ENV, DB_SNAPSHOT_ENV,
	DEFAULT_CHAIN_SPEC, DEFAULT_DB_SNAPSHOT_URL, DEFAULT_SUBSTRATE_IMAGE, FINALIZED_BLOCK_METRIC,
	FULLNODE_ROLE_VALUE, INTEGRATION_IMAGE_ENV, VALIDATOR_ROLE_VALUE,
};
use anyhow::{anyhow, Result};
use env_logger::Env;
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

const ROLE_TIMEOUT_SECS: u64 = 60;
const PEER_TIMEOUT_SECS: u64 = 60;
const BOOTSTRAP_TIMEOUT_SECS: u64 = 180;
const METRIC_TIMEOUT_SECS: u64 = 60;
const FINALITY_TIMEOUT_SECS: u64 = 120;
const VALIDATOR_BLOCK_TIMEOUT_SECS: u64 = 10;
const NEW_BLOCK_TIMEOUT_SECS: u64 = 90;
const LOG_TIMEOUT_LONG_SECS: u64 = 60;
const LOG_ERROR_TIMEOUT_SECS: u64 = 10;
const BEEFY_PROGRESS_TIMEOUT_SECS: u64 = 180;

const PEERS_THRESHOLD: f64 = 4.0;
const MIN_BOOTSTRAP_BLOCK: f64 = 1.0;
const BEEFY_TARGET: f64 = 200.0 * 180.0 / 6.0;

const VALIDATORS: [&str; 3] = ["alice", "bob", "other-validator"];
const FULLNODES: [&str; 3] = ["charlie", "dave", "eve"];

#[tokio::test(flavor = "multi_thread")]
async fn validators_warp_sync() -> Result<()> {
	let _ = env_logger::Builder::from_env(Env::default().default_filter_or("info")).try_init();

	log::info!("Spawning network");
	let config = build_network_config()?;
	let network = initialize_network(config).await?;

	network.wait_until_is_up(BOOTSTRAP_TIMEOUT_SECS).await?;

	// Role expectations: validators report 4, followers report 1.
	for validator in VALIDATORS {
		let node = network.get_node(validator)?;
		node.wait_metric_with_timeout(
			"node_roles",
			|role| role == VALIDATOR_ROLE_VALUE,
			ROLE_TIMEOUT_SECS,
		)
		.await?;
	}

	for follower in FULLNODES {
		let node = network.get_node(follower)?;
		node.wait_metric_with_timeout(
			"node_roles",
			|role| role == FULLNODE_ROLE_VALUE,
			ROLE_TIMEOUT_SECS,
		)
		.await?;
	}

	// Peer expectations for all nodes.
	for &node_name in VALIDATORS.iter().chain(FULLNODES.iter()) {
		network
			.get_node(node_name)?
			.wait_metric_with_timeout(
				"substrate_sub_libp2p_peers_count",
				|peers| peers >= PEERS_THRESHOLD,
				PEER_TIMEOUT_SECS,
			)
			.await?;
	}

	// Followers should bootstrap from snapshot shortly after startup.
	for follower in FULLNODES {
		network
			.get_node(follower)?
			.wait_metric_with_timeout(
				BEST_BLOCK_METRIC,
				|height| height >= MIN_BOOTSTRAP_BLOCK,
				BOOTSTRAP_TIMEOUT_SECS,
			)
			.await?;
	}

	let db_snapshot_height = resolve_db_snapshot_height(&network, "charlie").await?;

	for follower in FULLNODES {
		network
			.get_node(follower)?
			.wait_metric_with_timeout(
				BEST_BLOCK_METRIC,
				|height| height >= db_snapshot_height,
				METRIC_TIMEOUT_SECS,
			)
			.await?;
	}

	// Validators should catch up to the snapshot quickly even without a snapshot.
	for validator in VALIDATORS {
		network
			.get_node(validator)?
			.wait_metric_with_timeout(
				BEST_BLOCK_METRIC,
				|height| height >= db_snapshot_height,
				VALIDATOR_BLOCK_TIMEOUT_SECS,
			)
			.await?;
	}

	for validator in VALIDATORS {
		let node = network.get_node(validator)?;
		wait_for_warp_sync_logs(node).await?;
	}
	check_error_logs(&network).await?;

	// Finality must catch up to the snapshot height on validators.
	for validator in VALIDATORS {
		network
			.get_node(validator)?
			.wait_metric_with_timeout(
				FINALIZED_BLOCK_METRIC,
				|height| height >= db_snapshot_height,
				FINALITY_TIMEOUT_SECS,
			)
			.await?;
	}

	// Ensure BEEFY voting starts and progresses.
	for validator in VALIDATORS {
		let node = network.get_node(validator)?;
		node.wait_metric_with_timeout(
			BEEFY_BEST_BLOCK_METRIC,
			|height| height >= 1.0,
			LOG_TIMEOUT_LONG_SECS,
		)
		.await?;
		node.wait_metric_with_timeout(
			BEEFY_BEST_BLOCK_METRIC,
			|height| height >= BEEFY_TARGET,
			BEEFY_PROGRESS_TIMEOUT_SECS,
		)
		.await?;
	}

	// Validators should produce new blocks beyond the snapshot height.
	for validator in VALIDATORS {
		network
			.get_node(validator)?
			.wait_metric_with_timeout(
				BEST_BLOCK_METRIC,
				|height| height > db_snapshot_height,
				NEW_BLOCK_TIMEOUT_SECS,
			)
			.await?;
	}

	network.destroy().await?;

	Ok(())
}

fn build_network_config() -> Result<NetworkConfig> {
	let integration_image = env_or_default(INTEGRATION_IMAGE_ENV, DEFAULT_SUBSTRATE_IMAGE);
	let db_snapshot = env_or_default(DB_SNAPSHOT_ENV, DEFAULT_DB_SNAPSHOT_URL);
	let chain_spec = env_or_default(CHAIN_SPEC_ENV, DEFAULT_CHAIN_SPEC);

	NetworkConfigBuilder::new()
		.with_relaychain(|relaychain| {
			relaychain
				.with_chain("local")
				.with_default_command("substrate")
				.with_default_image(integration_image.as_str())
				.with_chain_spec_path(chain_spec.as_str())
				.with_node(|node| {
					node.with_name("alice")
						.validator(true)
						.with_args(vec!["--sync=warp".into(), "--log=beefy=debug".into()])
				})
				.with_node(|node| {
					node.with_name("bob")
						.validator(true)
						.with_args(vec!["--sync=warp".into(), "--log=beefy=debug".into()])
				})
				.with_node(|node| {
					node.with_name("other-validator")
						.validator(true)
						.with_args(vec!["--sync=warp".into(), "--log=beefy=debug".into()])
				})
				.with_node(|node| {
					node.with_name("charlie")
						.validator(false)
						.with_db_snapshot(db_snapshot.as_str())
				})
				.with_node(|node| {
					node.with_name("dave").validator(false).with_db_snapshot(db_snapshot.as_str())
				})
				.with_node(|node| {
					node.with_name("eve").validator(false).with_db_snapshot(db_snapshot.as_str())
				})
		})
		.with_global_settings(|global_settings| match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		})
		.build()
		.map_err(|errs| {
			let message =
				errs.into_iter().map(|err| err.to_string()).collect::<Vec<_>>().join(", ");
			anyhow!("config errs: {message}")
		})
}

async fn check_error_logs(
	network: &zombienet_sdk::Network<zombienet_sdk::LocalFileSystem>,
) -> Result<()> {
	let alice = network.get_node("alice")?;
	alice
		.wait_log_line_count_with_timeout(
			"No public addresses configured and no global listen addresses found",
			false,
			log_line_at_least_once(LOG_TIMEOUT_LONG_SECS),
		)
		.await?;
	alice
		.wait_log_line_count_with_timeout(
			"error",
			false,
			log_line_exactly_once(LOG_ERROR_TIMEOUT_SECS),
		)
		.await?;

	let bob = network.get_node("bob")?;
	bob.wait_log_line_count_with_timeout(
		"verification failed",
		false,
		log_line_absent(LOG_ERROR_TIMEOUT_SECS),
	)
	.await?;

	Ok(())
}
