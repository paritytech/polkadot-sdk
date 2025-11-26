// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use crate::utils::{
	ensure_env_defaults, initialize_network, log_line_absent, log_line_at_least_once,
	resolve_db_snapshot_height, BEST_BLOCK_METRIC, CHAIN_SPEC_ENV, DB_SNAPSHOT_ENV,
	DEFAULT_CHAIN_SPEC, DEFAULT_DB_SNAPSHOT_URL, DEFAULT_SUBSTRATE_IMAGE, FULLNODE_ROLE_VALUE,
	INTEGRATION_IMAGE_ENV, NODE_ROLE_METRIC, PEER_COUNT_METRIC,
};
use anyhow::{anyhow, Context, Result};
use env_logger::Env;
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder, NetworkNode};

const BEST_BLOCK_THRESHOLD: f64 = 1.0;
const PEERS_THRESHOLD: f64 = 3.0;

const NETWORK_READY_TIMEOUT_SECS: u64 = 180;
const ROLE_TIMEOUT_SECS: u64 = 60;
const PEERS_TIMEOUT_SECS: u64 = 60;
const METRIC_TIMEOUT_SECS: u64 = 60;
const LOG_TIMEOUT_LONG_SECS: u64 = 60;
const LOG_TIMEOUT_SHORT_SECS: u64 = 10;
const LOG_ERROR_TIMEOUT_SECS: u64 = 10;
const SNAPSHOT_NODES: [&str; 3] = ["alice", "bob", "charlie"];
const NODE_ROLE_EXPECTATIONS: [(&str, f64); 4] = [
	("alice", FULLNODE_ROLE_VALUE),
	("bob", FULLNODE_ROLE_VALUE),
	("charlie", FULLNODE_ROLE_VALUE),
	("dave", FULLNODE_ROLE_VALUE),
];
#[tokio::test(flavor = "multi_thread")]
async fn basic_warp_sync() -> Result<()> {
	let _ = env_logger::Builder::from_env(Env::default().default_filter_or("info")).try_init();

	ensure_env_defaults(&[
		(INTEGRATION_IMAGE_ENV, DEFAULT_SUBSTRATE_IMAGE),
		(DB_SNAPSHOT_ENV, DEFAULT_DB_SNAPSHOT_URL),
		(CHAIN_SPEC_ENV, DEFAULT_CHAIN_SPEC),
	]);

	log::info!("Spawning network");
	let config = build_network_config()?;
	let network = initialize_network(config).await?;

	network.wait_until_is_up(NETWORK_READY_TIMEOUT_SECS).await?;

	for &(node_name, expected_role) in NODE_ROLE_EXPECTATIONS.iter() {
		let node = network.get_node(node_name)?;
		assert_node_roles(expected_role, node).await?;
		assert_peers_count(node).await?;
	}

	let db_snapshot_height = resolve_db_snapshot_height(&network, "alice").await?;

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

async fn assert_node_roles(expected_role: f64, node: &NetworkNode) -> Result<()> {
	let node_name = node.name();

	node.wait_metric_with_timeout(
		NODE_ROLE_METRIC,
		|role| role == expected_role,
		ROLE_TIMEOUT_SECS,
	)
	.await
	.with_context(|| {
		format!(
			"node {node_name} did not expose expected role {expected_role} on metric {NODE_ROLE_METRIC}"
		)
	})?;

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
	node.wait_log_line_count_with_timeout(
		"Warp sync is complete",
		false,
		log_line_at_least_once(LOG_TIMEOUT_LONG_SECS),
	)
	.await?;
	node.wait_log_line_count_with_timeout(
		r"Checking for displaced leaves after finalization\. leaves=\[0xc5e7b4cfd23932bb930e859865430a35f6741b4732d677822d492ca64cc8d059\]",
		false,
		log_line_at_least_once(LOG_TIMEOUT_SHORT_SECS),
	)
	.await?;
	node.wait_log_line_count_with_timeout(
		"State sync is complete",
		false,
		log_line_at_least_once(LOG_TIMEOUT_LONG_SECS),
	)
	.await?;
	node.wait_log_line_count_with_timeout(
		"Block history download is complete",
		false,
		log_line_at_least_once(LOG_TIMEOUT_SHORT_SECS),
	)
	.await?;

	Ok(())
}

async fn wait_for_absence_of_errors(node: &NetworkNode) -> Result<()> {
	node.wait_log_line_count_with_timeout("error", false, log_line_absent(LOG_ERROR_TIMEOUT_SECS))
		.await?;
	node.wait_log_line_count_with_timeout(
		"verification failed",
		false,
		log_line_absent(LOG_ERROR_TIMEOUT_SECS),
	)
	.await?;

	Ok(())
}
