// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Chunk Fetching Network Compatibility Test
//!
//! This test verifies that validators preserve backwards compatibility with
//! peers speaking an older version of the /req_chunk protocol. It sets up
//! a mixed network with both old and new validators to ensure chunk fetching
//! works correctly across protocol versions.

use crate::utils::{env_or_default, initialize_network, INTEGRATION_IMAGE_ENV};
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_para_is_registered, assert_para_throughput};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use tokio::time::Duration;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

const PARA_ID_2000: u32 = 2000;
const PARA_ID_2001: u32 = 2001;
const OLD_TAG: &str = "master-bde0bbe5";
const SUBSTRATE_BLOCK_HEIGHT_METRIC: &str = "substrate_block_height{status=\"finalized\"}";
const POLKADOT_PARACHAIN_APPROVAL_CHECKING_FINALITY_LAG_METRIC: &str =
	"polkadot_parachain_approval_checking_finality_lag";
const POLKADOT_PARACHAIN_APPROVALS_NO_SHOWS_TOTAL_METRIC: &str =
	"polkadot_parachain_approvals_no_shows_total";
const POLKADOT_PARACHAIN_AVAILABILITY_RECOVERY_RECOVERIES_FINISHED_METRIC: &str =
	"polkadot_parachain_availability_recovery_recoveries_finished{result=\"failure\"}";
const POLKADOT_PARACHAIN_FETCHED_SUCCESSFUL_CHUNKS_TOTAL_METRIC: &str =
	"polkadot_parachain_fetched_chunks_total{success=\"succeeded\"}";
const POLKADOT_PARACHAIN_FETCHED_FAILED_CHUNKS_TOTAL_METRIC: &str =
	"polkadot_parachain_fetched_chunks_total{success=\"failed\"}";
const POLKADOT_PARACHAIN_FETCHED_NOT_FOUND_CHUNKS_TOTAL_METRIC: &str =
	"polkadot_parachain_fetched_chunks_total{success=\"not-found\"}";
const NODE_ROLES_METRIC: &str = "node_roles";

/// Test that validators preserve backwards compatibility with the old /req_chunk protocol.
///
/// - Spawns 2 "old" validators (using an image without /req_chunk/2)
/// - Spawns 2 "new" validators (with current protocol support)
/// - Adds 2 glutton parachains to generate load
/// - Verifies parachains produce blocks and recovery works
/// - Ensures the fallback protocol is used for chunk fetching
#[tokio::test(flavor = "multi_thread")]
async fn chunk_fetching_network_compatibility() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config()?;
	let network = initialize_network(config).await?;

	let new_node = network.get_node("new-0")?;
	let old_node = network.get_node("old-0")?;
	let new_client = new_node.wait_client().await?;
	let old_client = old_node.wait_client().await?;

	// Check authority status
	log::info!("Checking node roles");
	new_node
		.wait_metric_with_timeout(NODE_ROLES_METRIC, |v| v == 4.0, 30u64)
		.await
		.map_err(|e| anyhow!("New node role check failed: {}", e))?;
	old_node
		.wait_metric_with_timeout(NODE_ROLES_METRIC, |v| v == 4.0, 30u64)
		.await
		.map_err(|e| anyhow!("Old node role check failed: {}", e))?;

	// Ensure parachains are registered
	log::info!("Checking parachains are registered");
	assert_para_is_registered(&new_client, ParaId::from(PARA_ID_2000), 20).await?;
	assert_para_is_registered(&old_client, ParaId::from(PARA_ID_2000), 20).await?;
	assert_para_is_registered(&old_client, ParaId::from(PARA_ID_2001), 20).await?;
	assert_para_is_registered(&new_client, ParaId::from(PARA_ID_2001), 20).await?;
	log::info!("All parachains registered");

	// Ensure parachains made progress and approval checking works
	log::info!("Waiting for parachains to produce blocks");
	assert_para_throughput(&new_client, 200, [(ParaId::from(PARA_ID_2000), 15..200)]).await?;
	assert_para_throughput(&old_client, 200, [(ParaId::from(PARA_ID_2000), 15..200)]).await?;
	assert_para_throughput(&new_client, 200, [(ParaId::from(PARA_ID_2001), 15..200)]).await?;
	assert_para_throughput(&old_client, 200, [(ParaId::from(PARA_ID_2001), 15..200)]).await?;
	log::info!("Parachains producing blocks");

	// Check finalized block height
	log::info!("Checking finalized block height");
	new_node
		.wait_metric_with_timeout(SUBSTRATE_BLOCK_HEIGHT_METRIC, |v| v >= 30.0, 400u64)
		.await
		.map_err(|e| anyhow!("New node finalized height too low: {}", e))?;
	old_node
		.wait_metric_with_timeout(SUBSTRATE_BLOCK_HEIGHT_METRIC, |v| v >= 30.0, 400u64)
		.await
		.map_err(|e| anyhow!("Old node finalized height too low: {}", e))?;

	// Check approval finality lag
	log::info!("Checking approval finality lag");
	new_node
		.wait_metric_with_timeout(
			POLKADOT_PARACHAIN_APPROVAL_CHECKING_FINALITY_LAG_METRIC,
			|v| v < 3.0,
			30u64,
		)
		.await
		.map_err(|e| anyhow!("New node approval lag too high: {}", e))?;
	old_node
		.wait_metric_with_timeout(
			POLKADOT_PARACHAIN_APPROVAL_CHECKING_FINALITY_LAG_METRIC,
			|v| v < 3.0,
			30u64,
		)
		.await
		.map_err(|e| anyhow!("Old node approval lag too high: {}", e))?;

	// Check no-shows are low
	log::info!("Checking no-shows");
	new_node
		.wait_metric_with_timeout(
			POLKADOT_PARACHAIN_APPROVALS_NO_SHOWS_TOTAL_METRIC,
			|v| v < 3.0,
			10u64,
		)
		.await
		.map_err(|e| anyhow!("New node no-shows too high: {}", e))?;
	old_node
		.wait_metric_with_timeout(
			POLKADOT_PARACHAIN_APPROVALS_NO_SHOWS_TOTAL_METRIC,
			|v| v < 3.0,
			10u64,
		)
		.await
		.map_err(|e| anyhow!("Old node no-shows too high: {}", e))?;

	// Ensure there are successful recoveries
	log::info!("Checking successful data recoveries");
	let result = new_node
		.wait_log_line_count_with_timeout(
			"*Data recovery from chunks complete*",
			true,
			LogLineCountOptions::new(|n| n >= 10, Duration::from_secs(300), false),
		)
		.await?;
	assert!(result.success(), "New node should have successful recoveries");

	let result = old_node
		.wait_log_line_count_with_timeout(
			"*Data recovery from chunks complete*",
			true,
			LogLineCountOptions::new(|n| n >= 10, Duration::from_secs(300), false),
		)
		.await?;
	assert!(result.success(), "Old node should have successful recoveries");

	// Ensure there are no failed recoveries
	log::info!("Checking no failed recoveries");
	let result = new_node
		.wait_log_line_count_with_timeout(
			"*Data recovery from chunks is not possible*",
			true,
			LogLineCountOptions::new(|n| n == 0, Duration::from_secs(10), false),
		)
		.await?;
	assert!(result.success(), "New node should have no failed recoveries");

	let result = old_node
		.wait_log_line_count_with_timeout(
			"*Data recovery from chunks is not possible*",
			true,
			LogLineCountOptions::new(|n| n == 0, Duration::from_secs(10), false),
		)
		.await?;
	assert!(result.success(), "Old node should have no failed recoveries");

	// Check recovery failure metrics
	new_node
		.wait_metric_with_timeout(
			POLKADOT_PARACHAIN_AVAILABILITY_RECOVERY_RECOVERIES_FINISHED_METRIC,
			|v| v == 0.0,
			10u64,
		)
		.await
		.map_err(|e| anyhow!("New node has recovery failures: {}", e))?;
	old_node
		.wait_metric_with_timeout(
			POLKADOT_PARACHAIN_AVAILABILITY_RECOVERY_RECOVERIES_FINISHED_METRIC,
			|v| v == 0.0,
			10u64,
		)
		.await
		.map_err(|e| anyhow!("Old node has recovery failures: {}", e))?;

	log::info!("Checking fallback protocol was used");
	let result = new_node
		.wait_log_line_count_with_timeout(
			"*Trying the fallback protocol*",
			true,
			LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(100), false),
		)
		.await?;
	assert!(result.success(), "Fallback protocol not used");

	// Ensure systematic recovery was not used
	log::info!("Checking systematic recovery was not used");
	let result = old_node
		.wait_log_line_count_with_timeout(
			"*Data recovery from systematic chunks complete*",
			true,
			LogLineCountOptions::new(|n| n == 0, Duration::from_secs(10), false),
		)
		.await?;
	assert!(result.success(), "Old node should not use systematic recovery");

	let result = new_node
		.wait_log_line_count_with_timeout(
			"*Data recovery from systematic chunks complete*",
			true,
			LogLineCountOptions::new(|n| n == 0, Duration::from_secs(10), false),
		)
		.await?;
	assert!(result.success(), "New node should not use systematic recovery");

	// Check chunk fetching metrics
	log::info!("Checking chunk fetching metrics");
	new_node
		.wait_metric_with_timeout(
			POLKADOT_PARACHAIN_FETCHED_SUCCESSFUL_CHUNKS_TOTAL_METRIC,
			|v| v >= 10.0,
			400u64,
		)
		.await
		.map_err(|e| anyhow!("New node fetched chunks too low: {}", e))?;
	old_node
		.wait_metric_with_timeout(
			POLKADOT_PARACHAIN_FETCHED_SUCCESSFUL_CHUNKS_TOTAL_METRIC,
			|v| v >= 10.0,
			400u64,
		)
		.await
		.map_err(|e| anyhow!("Old node fetched chunks too low: {}", e))?;

	// No failed chunk fetches
	new_node
		.wait_metric_with_timeout(
			POLKADOT_PARACHAIN_FETCHED_FAILED_CHUNKS_TOTAL_METRIC,
			|v| v == 0.0,
			10u64,
		)
		.await
		.map_err(|e| anyhow!("New node has failed chunk fetches: {}", e))?;
	old_node
		.wait_metric_with_timeout(
			POLKADOT_PARACHAIN_FETCHED_FAILED_CHUNKS_TOTAL_METRIC,
			|v| v == 0.0,
			10u64,
		)
		.await
		.map_err(|e| anyhow!("Old node has failed chunk fetches: {}", e))?;

	// No not-found chunk fetches
	new_node
		.wait_metric_with_timeout(
			POLKADOT_PARACHAIN_FETCHED_NOT_FOUND_CHUNKS_TOTAL_METRIC,
			|v| v == 0.0,
			10u64,
		)
		.await
		.map_err(|e| anyhow!("New node has not-found chunk fetches: {}", e))?;
	old_node
		.wait_metric_with_timeout(
			POLKADOT_PARACHAIN_FETCHED_NOT_FOUND_CHUNKS_TOTAL_METRIC,
			|v| v == 0.0,
			10u64,
		)
		.await
		.map_err(|e| anyhow!("Old node has not-found chunk fetches: {}", e))?;

	log::info!("Test finished successfully");
	Ok(())
}

fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let polkadot_image = env_or_default(INTEGRATION_IMAGE_ENV, images.polkadot.as_str());
	let old_suffix = std::env::var("OLD_SUFFIX").unwrap_or_default();

	// Old image that doesn't speak /req_chunk/2 protocol
	let old_polkadot_image = std::env::var("POLKADOT_IMAGE")
		.map(|img| format!("{}:{}", img.split(':').next().unwrap_or(&img), OLD_TAG))
		.unwrap_or_else(|_| format!("docker.io/parity/polkadot:{OLD_TAG}"));

	let old_polkadot_command = format!("polkadot{old_suffix}");
	let old_collator_image = format!("docker.io/paritypr/polkadot-parachain-debug:{OLD_TAG}");
	let old_collator_command = format!("polkadot-parachain{old_suffix}");

	let mut builder = NetworkConfigBuilder::new().with_relaychain(|r| {
		let r = r
			.with_chain("rococo-local")
			.with_default_command("polkadot")
			.with_default_image(polkadot_image.as_str())
			.with_genesis_overrides(json!({
				"patch": {
					"configuration": {
						"config": {
							"needed_approvals": 4,
							"scheduler_params": {
								"max_validators_per_core": 2
							}
						}
					}
				}
			}))
			.with_default_resources(|r| {
				r.with_limit_memory("4G")
					.with_limit_cpu("2")
					.with_request_memory("2G")
					.with_request_cpu("1")
			});

		// Add first old validator to transition type
		let r = r.with_node(|node| {
			node.with_name("old-0")
				.with_image(old_polkadot_image.as_str())
				.with_command(old_polkadot_command.as_str())
				.with_args(vec![
					"-lparachain=debug,parachain::availability-recovery=trace,parachain::availability-distribution=trace".into(),
				])
		});

		// Add second old validator
		let r = r.with_node(|node| {
			node.with_name("old-1")
				.with_image(old_polkadot_image.as_str())
				.with_command(old_polkadot_command.as_str())
				.with_args(vec![
					"-lparachain=debug,parachain::availability-recovery=trace,parachain::availability-distribution=trace".into(),
				])
		});

		// Add 2 new validators (with /req_chunk/2 support)
		let r = r.with_node(|node| {
			node.with_name("new-0")
				.with_args(vec![
					"-lparachain=debug,parachain::availability-recovery=trace,parachain::availability-distribution=trace,sub-libp2p=trace".into(),
				])
		});

		r.with_node(|node| {
			node.with_name("new-1")
				.with_args(vec![
					"-lparachain=debug,parachain::availability-recovery=trace,parachain::availability-distribution=trace,sub-libp2p=trace".into(),
				])
		})
	});

	// Add glutton parachain 2000
	builder = builder.with_parachain(|p| {
		p.with_id(PARA_ID_2000)
			.cumulus_based(true)
			.with_chain("glutton-westend-local-2000")
			.with_default_image(old_collator_image.as_str())
			.with_default_command(old_collator_command.as_str())
			.with_default_args(vec!["-lparachain=debug".into()])
			.with_genesis_overrides(json!({
				"patch": {
					"glutton": {
						"compute": "50000000",
						"storage": "2500000000",
						"trashDataCount": 5120
					}
				}
			}))
			.with_collator(|n| n.with_name("collator-2000"))
	});

	// Add glutton parachain 2001
	builder = builder.with_parachain(|p| {
		p.with_id(PARA_ID_2001)
			.cumulus_based(true)
			.with_chain("glutton-westend-local-2001")
			.with_default_image(old_collator_image.as_str())
			.with_default_command(old_collator_command.as_str())
			.with_default_args(vec!["-lparachain=debug".into()])
			.with_genesis_overrides(json!({
				"patch": {
					"glutton": {
						"compute": "50000000",
						"storage": "2500000000",
						"trashDataCount": 5120
					}
				}
			}))
			.with_collator(|n| n.with_name("collator-2001"))
	});

	builder = builder.with_global_settings(|global_settings| {
		match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		}
	});

	builder.build().map_err(|e| {
		let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
		anyhow!("config errs: {errs}")
	})
}
