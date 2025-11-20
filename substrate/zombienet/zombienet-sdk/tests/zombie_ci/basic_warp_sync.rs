// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use crate::utils::{
	initialize_network, BEST_BLOCK_METRIC, DEFAULT_CHAIN_SPEC, DEFAULT_DB_SNAPSHOT_URL,
};
use anyhow::{anyhow, Result};
use env_logger::Env;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder, NetworkNode};

const BEST_BLOCK_THRESHOLD: f64 = 1.0;
const PEERS_THRESHOLD: f64 = 3.0;
const VALIDATOR_ROLE_VALUE: f64 = 1.0;

const NETWORK_READY_TIMEOUT_SECS: u64 = 180;
const ROLE_TIMEOUT_SECS: u64 = 60;
const PEERS_TIMEOUT_SECS: u64 = 60;
const METRIC_TIMEOUT_SECS: u64 = 60;
const LOG_TIMEOUT_LONG_SECS: u64 = 60;
const LOG_TIMEOUT_SHORT_SECS: u64 = 10;
const LOG_ERROR_TIMEOUT_SECS: u64 = 10;
const NODE_ROLE_METRIC: &str = "node_roles";
const PEER_COUNT_METRIC: &str = "substrate_sub_libp2p_peers_count";
const NODE_NAMES: [&str; 4] = ["alice", "bob", "charlie", "dave"];
const SNAPSHOT_NODES: [&str; 3] = ["alice", "bob", "charlie"];
const INTEGRATION_IMAGE_ENV: &str = "ZOMBIENET_INTEGRATION_TEST_IMAGE";
const DB_SNAPSHOT_ENV: &str = "DB_SNAPSHOT";
const CHAIN_SPEC_ENV: &str = "WARP_CHAIN_SPEC_PATH";
const DB_BLOCK_HEIGHT_ENV: &str = "DB_BLOCK_HEIGHT";
const DEFAULT_SUBSTRATE_IMAGE: &str = "docker.io/paritypr/substrate:latest";

#[tokio::test(flavor = "multi_thread")]
async fn basic_warp_sync() -> Result<()> {
	let _ = env_logger::Builder::from_env(Env::default().default_filter_or("info")).try_init();

	ensure_env_defaults();

	log::info!("Spawning network");
	let config = build_network_config()?;
	let network = initialize_network(config).await?;

	network.wait_until_is_up(NETWORK_READY_TIMEOUT_SECS).await?;

	for node_name in NODE_NAMES {
		let node = network.get_node(node_name)?;
		assert_node_roles(node).await?;
		assert_peers_count(node).await?;
	}

	let db_snapshot_height = resolve_db_snapshot_height(&network).await?;

	for node_name in SNAPSHOT_NODES {
		network
			.get_node(node_name)?
			.wait_metric_with_timeout(
				BEST_BLOCK_METRIC,
				|height| height >= db_snapshot_height,
				METRIC_TIMEOUT_SECS,
			)
			.await?;
	}

	let dave = network.get_node("dave")?;
	dave.wait_metric_with_timeout(
		BEST_BLOCK_METRIC,
		|x| x >= BEST_BLOCK_THRESHOLD,
		METRIC_TIMEOUT_SECS,
	)
	.await?;
	dave.wait_metric_with_timeout(
		BEST_BLOCK_METRIC,
		|x| x >= db_snapshot_height,
		METRIC_TIMEOUT_SECS,
	)
	.await?;

	wait_for_warp_logs(dave).await?;
	wait_for_absence_of_errors(dave).await?;

	network.destroy().await?;

	Ok(())
}

fn ensure_env_defaults() {
	if std::env::var(INTEGRATION_IMAGE_ENV).is_err() {
		std::env::set_var(INTEGRATION_IMAGE_ENV, DEFAULT_SUBSTRATE_IMAGE);
	}
	if std::env::var(DB_SNAPSHOT_ENV).is_err() {
		std::env::set_var(DB_SNAPSHOT_ENV, DEFAULT_DB_SNAPSHOT_URL);
	}
	if std::env::var(CHAIN_SPEC_ENV).is_err() {
		std::env::set_var(CHAIN_SPEC_ENV, DEFAULT_CHAIN_SPEC);
	}
}

fn db_snapshot_height_override() -> Option<f64> {
	std::env::var(DB_BLOCK_HEIGHT_ENV)
		.ok()
		.and_then(|value| value.parse::<f64>().ok())
}

async fn resolve_db_snapshot_height(
	network: &zombienet_sdk::Network<zombienet_sdk::LocalFileSystem>,
) -> Result<f64> {
	if let Some(override_height) = db_snapshot_height_override() {
		return Ok(override_height);
	}

	let alice = network.get_node("alice")?;
	let height = alice.reports(BEST_BLOCK_METRIC).await?;
	Ok(height)
}

fn build_network_config() -> Result<NetworkConfig> {
	let integration_image = std::env::var(INTEGRATION_IMAGE_ENV)
		.unwrap_or_else(|_| DEFAULT_SUBSTRATE_IMAGE.to_string());
	let db_snapshot =
		std::env::var(DB_SNAPSHOT_ENV).map_err(|_| anyhow!("db snapshot env var not set"))?;
	let chain_spec =
		std::env::var(CHAIN_SPEC_ENV).map_err(|_| anyhow!("chain spec env var not set"))?;

	NetworkConfigBuilder::new()
		.with_relaychain(|relaychain| {
			relaychain
				.with_chain("local")
				.with_default_command("substrate")
				.with_default_image(integration_image.as_str())
				.with_chain_spec_path(chain_spec.as_str())
				.with_node(|node| {
					node.with_name("alice").validator(false).with_db_snapshot(db_snapshot.as_str())
				})
				.with_node(|node| {
					node.with_name("bob").validator(false).with_db_snapshot(db_snapshot.as_str())
				})
				.with_node(|node| {
					node.with_name("charlie")
						.validator(false)
						.with_db_snapshot(db_snapshot.as_str())
				})
				.with_node(|node| {
					node.with_name("dave")
						.validator(false)
						.with_args(vec!["--sync=warp".into(), "-ldb::blockchain".into()])
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

async fn assert_node_roles(node: &NetworkNode) -> Result<()> {
	node.wait_metric_with_timeout(
		NODE_ROLE_METRIC,
		|role| (role - VALIDATOR_ROLE_VALUE).abs() < f64::EPSILON,
		ROLE_TIMEOUT_SECS,
	)
	.await?;

	Ok(())
}

async fn assert_peers_count(node: &NetworkNode) -> Result<()> {
	node.wait_metric_with_timeout(
		PEER_COUNT_METRIC,
		|peers| peers >= PEERS_THRESHOLD,
		PEERS_TIMEOUT_SECS,
	)
	.await?;

	Ok(())
}

async fn wait_for_warp_logs(node: &NetworkNode) -> Result<()> {
	let at_least_once = |timeout_secs| {
		LogLineCountOptions::new(|count| count >= 1, Duration::from_secs(timeout_secs), false)
	};

	node.wait_log_line_count_with_timeout(
		"Warp sync is complete",
		false,
		at_least_once(LOG_TIMEOUT_LONG_SECS),
	)
	.await?;
	node.wait_log_line_count_with_timeout(
		r"Checking for displaced leaves after finalization\. leaves=\[0xc5e7b4cfd23932bb930e859865430a35f6741b4732d677822d492ca64cc8d059\]",
		false,
		at_least_once(LOG_TIMEOUT_SHORT_SECS),
	)
	.await?;
	node.wait_log_line_count_with_timeout(
		"State sync is complete",
		false,
		at_least_once(LOG_TIMEOUT_LONG_SECS),
	)
	.await?;
	node.wait_log_line_count_with_timeout(
		"Block history download is complete",
		false,
		at_least_once(LOG_TIMEOUT_SHORT_SECS),
	)
	.await?;

	Ok(())
}

async fn wait_for_absence_of_errors(node: &NetworkNode) -> Result<()> {
	node.wait_log_line_count_with_timeout(
		"error",
		false,
		LogLineCountOptions::no_occurences_within_timeout(Duration::from_secs(
			LOG_ERROR_TIMEOUT_SECS,
		)),
	)
	.await?;
	node.wait_log_line_count_with_timeout(
		"verification failed",
		false,
		LogLineCountOptions::no_occurences_within_timeout(Duration::from_secs(
			LOG_ERROR_TIMEOUT_SECS,
		)),
	)
	.await?;

	Ok(())
}
