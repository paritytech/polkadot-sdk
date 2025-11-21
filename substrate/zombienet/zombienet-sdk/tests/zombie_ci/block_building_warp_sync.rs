// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use crate::utils::{
	initialize_network, BEEFY_BEST_BLOCK_METRIC, BEST_BLOCK_METRIC, DEFAULT_CHAIN_SPEC,
	DEFAULT_DB_SNAPSHOT_URL, DEFAULT_SUBSTRATE_IMAGE,
};
use anyhow::{anyhow, Result};
use env_logger::Env;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder, NetworkNode};

const INTEGRATION_IMAGE_ENV: &str = "ZOMBIENET_INTEGRATION_TEST_IMAGE";
const DB_SNAPSHOT_ENV: &str = "DB_SNAPSHOT";
const CHAIN_SPEC_ENV: &str = "WARP_CHAIN_SPEC_PATH";
const DB_BLOCK_HEIGHT_ENV: &str = "DB_BLOCK_HEIGHT";

const ROLE_TIMEOUT_SECS: u64 = 60;
const PEER_TIMEOUT_SECS: u64 = 60;
const BOOTSTRAP_TIMEOUT_SECS: u64 = 180;
const METRIC_TIMEOUT_SECS: u64 = 60;
const NEW_BLOCK_TIMEOUT_SECS: u64 = 120;
const LOG_TIMEOUT_LONG_SECS: u64 = 60;
const LOG_TIMEOUT_SHORT_SECS: u64 = 10;
const LOG_ERROR_TIMEOUT_SECS: u64 = 10;
const BEEFY_SYNC_TIMEOUT_SECS: u64 = 180;
const BEEFY_PROGRESS_TIMEOUT_SECS: u64 = 60;

const PEERS_THRESHOLD: f64 = 2.0;
const MIN_BOOTSTRAP_BLOCK: f64 = 1.0;

const VALIDATORS: [&str; 2] = ["alice", "bob"];
const FOLLOWERS: [&str; 2] = ["charlie", "dave"];

#[tokio::test(flavor = "multi_thread")]
async fn block_building_warp_sync() -> Result<()> {
	let _ = env_logger::Builder::from_env(Env::default().default_filter_or("info")).try_init();

	ensure_env_defaults();

	log::info!("Spawning network");
	let config = build_network_config()?;
	let network = initialize_network(config).await?;

	network.wait_until_is_up(BOOTSTRAP_TIMEOUT_SECS).await?;

	check_node_roles(&network).await?;
	check_peers(&network).await?;

	let db_snapshot_height = resolve_db_snapshot_height(&network).await?;

	verify_bootstrap_height(&network, db_snapshot_height).await?;
	verify_new_blocks(&network, db_snapshot_height).await?;

	let dave = network.get_node("dave")?;
	verify_dave_progress(dave, db_snapshot_height).await?;
	verify_dave_logs(dave).await?;
	verify_dave_beefy(dave, db_snapshot_height).await?;
	verify_dave_log_errors_absent(dave).await?;

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
					node.with_name("alice").validator(true).with_db_snapshot(db_snapshot.as_str())
				})
				.with_node(|node| {
					node.with_name("bob").validator(true).with_db_snapshot(db_snapshot.as_str())
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

async fn check_node_roles(
	network: &zombienet_sdk::Network<zombienet_sdk::LocalFileSystem>,
) -> Result<()> {
	for validator in VALIDATORS {
		network
			.get_node(validator)?
			.wait_metric_with_timeout(
				"node_roles",
				|role| (role - 4.0).abs() < f64::EPSILON,
				ROLE_TIMEOUT_SECS,
			)
			.await?;
	}

	for follower in FOLLOWERS {
		network
			.get_node(follower)?
			.wait_metric_with_timeout(
				"node_roles",
				|role| (role - 1.0).abs() < f64::EPSILON,
				ROLE_TIMEOUT_SECS,
			)
			.await?;
	}

	Ok(())
}

async fn check_peers(
	network: &zombienet_sdk::Network<zombienet_sdk::LocalFileSystem>,
) -> Result<()> {
	for &node_name in VALIDATORS.iter().chain(FOLLOWERS.iter()) {
		network
			.get_node(node_name)?
			.wait_metric_with_timeout(
				"substrate_sub_libp2p_peers_count",
				|peers| peers >= PEERS_THRESHOLD,
				PEER_TIMEOUT_SECS,
			)
			.await?;
	}

	Ok(())
}

async fn verify_bootstrap_height(
	network: &zombienet_sdk::Network<zombienet_sdk::LocalFileSystem>,
	db_snapshot_height: f64,
) -> Result<()> {
	for &node_name in VALIDATORS.iter().chain(FOLLOWERS.iter()) {
		network
			.get_node(node_name)?
			.wait_metric_with_timeout(
				BEST_BLOCK_METRIC,
				|height| height >= MIN_BOOTSTRAP_BLOCK,
				BOOTSTRAP_TIMEOUT_SECS,
			)
			.await?;
		network
			.get_node(node_name)?
			.wait_metric_with_timeout(
				BEST_BLOCK_METRIC,
				|height| height >= db_snapshot_height,
				METRIC_TIMEOUT_SECS,
			)
			.await?;
	}

	Ok(())
}

async fn verify_new_blocks(
	network: &zombienet_sdk::Network<zombienet_sdk::LocalFileSystem>,
	db_snapshot_height: f64,
) -> Result<()> {
	for node_name in VALIDATORS {
		network
			.get_node(node_name)?
			.wait_metric_with_timeout(
				BEST_BLOCK_METRIC,
				|height| height > db_snapshot_height,
				NEW_BLOCK_TIMEOUT_SECS,
			)
			.await?;
	}

	let charlie = network.get_node("charlie")?;
	charlie
		.wait_metric_with_timeout(
			BEST_BLOCK_METRIC,
			|height| height > db_snapshot_height,
			NEW_BLOCK_TIMEOUT_SECS,
		)
		.await?;

	Ok(())
}

async fn verify_dave_progress(dave: &NetworkNode, db_snapshot_height: f64) -> Result<()> {
	dave.wait_metric_with_timeout(BEST_BLOCK_METRIC, |height| height >= 1.0, METRIC_TIMEOUT_SECS)
		.await?;
	dave.wait_metric_with_timeout(
		BEST_BLOCK_METRIC,
		|height| height >= db_snapshot_height,
		METRIC_TIMEOUT_SECS,
	)
	.await?;
	dave.wait_metric_with_timeout(
		BEST_BLOCK_METRIC,
		|height| height > db_snapshot_height,
		METRIC_TIMEOUT_SECS,
	)
	.await?;

	Ok(())
}

async fn verify_dave_logs(dave: &NetworkNode) -> Result<()> {
	let at_least_once = |timeout_secs| {
		LogLineCountOptions::new(|count| count >= 1, Duration::from_secs(timeout_secs), false)
	};

	dave.wait_log_line_count_with_timeout(
		"Warp sync is complete",
		false,
		at_least_once(LOG_TIMEOUT_LONG_SECS),
	)
	.await?;
	dave.wait_log_line_count_with_timeout(
		"State sync is complete",
		false,
		at_least_once(LOG_TIMEOUT_LONG_SECS),
	)
	.await?;
	dave.wait_log_line_count_with_timeout(
		"Block history download is complete",
		false,
		at_least_once(LOG_TIMEOUT_SHORT_SECS),
	)
	.await?;

	Ok(())
}

async fn verify_dave_beefy(dave: &NetworkNode, db_snapshot_height: f64) -> Result<()> {
	dave.wait_metric_with_timeout(
		BEEFY_BEST_BLOCK_METRIC,
		|height| height >= db_snapshot_height,
		BEEFY_SYNC_TIMEOUT_SECS,
	)
	.await?;
	dave.wait_metric_with_timeout(
		BEEFY_BEST_BLOCK_METRIC,
		|height| height > db_snapshot_height,
		BEEFY_PROGRESS_TIMEOUT_SECS,
	)
	.await?;

	Ok(())
}

async fn verify_dave_log_errors_absent(dave: &NetworkNode) -> Result<()> {
	dave.wait_log_line_count_with_timeout(
		r"error(?! importing block .*: block has an unknown parent)",
		false,
		LogLineCountOptions::no_occurences_within_timeout(Duration::from_secs(
			LOG_ERROR_TIMEOUT_SECS,
		)),
	)
	.await?;
	dave.wait_log_line_count_with_timeout(
		"verification failed",
		false,
		LogLineCountOptions::no_occurences_within_timeout(Duration::from_secs(
			LOG_ERROR_TIMEOUT_SECS,
		)),
	)
	.await?;

	Ok(())
}
