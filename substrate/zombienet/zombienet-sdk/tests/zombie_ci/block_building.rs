// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use crate::utils::{initialize_network, DEFAULT_CHAIN_SPEC, DEFAULT_SUBSTRATE_IMAGE};
use anyhow::{anyhow, Result};
use subxt::{config::substrate::SubstrateConfig, dynamic::tx, OnlineClient};
use subxt_signer::sr25519::dev;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder, NetworkNode};

const NODE_NAMES: [&str; 2] = ["alice", "bob"];

const NODE_ROLE_METRIC: &str = "node_roles";
const PEER_COUNT_METRIC: &str = "substrate_sub_libp2p_peers_count";
const BEST_BLOCK_METRIC: &str = "block_height{status=\"best\"}";

const ROLE_VALIDATOR_VALUE: f64 = 4.0;
const PEER_MIN_THRESHOLD: f64 = 1.0;
const BLOCK_TARGET: f64 = 5.0;

const NETWORK_READY_TIMEOUT_SECS: u64 = 60;
const METRIC_TIMEOUT_SECS: u64 = 20;
const LOG_TIMEOUT_SECS: u64 = 2;
const SCRIPT_TIMEOUT_SECS: u64 = 30;

const REMARK_PAYLOAD: &[u8] = b"block-building-test";
const INTEGRATION_IMAGE_ENV: &str = "ZOMBIENET_INTEGRATION_TEST_IMAGE";
const CHAIN_SPEC_ENV: &str = "WARP_CHAIN_SPEC_PATH";

#[tokio::test(flavor = "multi_thread")]
async fn block_building_test() -> Result<()> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	ensure_env_defaults();

	log::info!("Spawning network");
	let config = build_network_config()?;
	let network = initialize_network(config).await?;

	network.wait_until_is_up(NETWORK_READY_TIMEOUT_SECS).await?;

	for node_name in NODE_NAMES {
		let node = network.get_node(node_name)?;
		assert_node_health(node).await?;
	}

	let alice = network.get_node("alice")?;
	submit_transaction_and_wait_finalization(alice).await?;

	network.destroy().await?;

	Ok(())
}

fn ensure_env_defaults() {
	if std::env::var(INTEGRATION_IMAGE_ENV).is_err() {
		std::env::set_var(INTEGRATION_IMAGE_ENV, DEFAULT_SUBSTRATE_IMAGE);
	}
	if std::env::var(CHAIN_SPEC_ENV).is_err() {
		std::env::set_var(CHAIN_SPEC_ENV, DEFAULT_CHAIN_SPEC);
	}
}

fn build_network_config() -> Result<NetworkConfig> {
	let integration_image = std::env::var(INTEGRATION_IMAGE_ENV)
		.unwrap_or_else(|_| DEFAULT_SUBSTRATE_IMAGE.to_string());
	let chain_spec =
		std::env::var(CHAIN_SPEC_ENV).unwrap_or_else(|_| DEFAULT_CHAIN_SPEC.to_string());

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|relaychain| {
			relaychain
				.with_chain("local")
				.with_default_command("substrate")
				.with_default_image(integration_image.as_str())
				.with_chain_spec_path(chain_spec.as_str())
				.with_default_args(vec!["-lparachain=debug".into()])
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
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
		})?;

	Ok(config)
}

async fn assert_node_health(node: &NetworkNode) -> Result<()> {
	node.wait_until_is_up(METRIC_TIMEOUT_SECS).await?;

	node.wait_metric_with_timeout(
		NODE_ROLE_METRIC,
		|role| (role - ROLE_VALIDATOR_VALUE).abs() < f64::EPSILON,
		METRIC_TIMEOUT_SECS,
	)
	.await?;

	node.wait_metric_with_timeout(
		PEER_COUNT_METRIC,
		|peers| peers >= PEER_MIN_THRESHOLD,
		METRIC_TIMEOUT_SECS,
	)
	.await?;

	node.wait_metric_with_timeout(
		BEST_BLOCK_METRIC,
		|height| height >= BLOCK_TARGET,
		METRIC_TIMEOUT_SECS,
	)
	.await?;

	node.wait_log_line_count_with_timeout(
		"error",
		false,
		LogLineCountOptions::no_occurences_within_timeout(Duration::from_secs(LOG_TIMEOUT_SECS)),
	)
	.await?;

	Ok(())
}

async fn submit_transaction_and_wait_finalization(node: &NetworkNode) -> Result<()> {
	let client: OnlineClient<SubstrateConfig> = node.wait_client::<SubstrateConfig>().await?;
	let signer = dev::alice();

	let remark_call =
		tx("System", "remark", vec![subxt::dynamic::Value::from_bytes(REMARK_PAYLOAD)]);

	tokio::time::timeout(Duration::from_secs(SCRIPT_TIMEOUT_SECS), async {
		client
			.tx()
			.sign_and_submit_then_watch_default(&remark_call, &signer)
			.await?
			.wait_for_finalized_success()
			.await
	})
	.await
	.map_err(|_| anyhow!("transaction timed out"))??;

	Ok(())
}
