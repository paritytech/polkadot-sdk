// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Systematic Chunk Recovery Test
//!
//! This test verifies that systematic chunk recovery is used when the chunk mapping
//! feature is enabled. It:
//! 1. Spawns validators and parachains
//! 2. Verifies regular chunk recovery is used initially
//! 3. Enables the chunk mapping feature via sudo
//! 4. Verifies systematic chunk recovery is used after enabling

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_para_is_registered, assert_para_throughput};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use std::time::Duration;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

use crate::utils::{
	enable_node_feature, env_or_default, initialize_network, APPROVALS_NO_SHOWS_TOTAL_METRIC,
	APPROVAL_CHECKING_FINALITY_LAG_METRIC, AVAILABILITY_RECOVERY_RECOVERIES_FINISHED_METRIC,
	CUMULUS_IMAGE_ENV, INTEGRATION_IMAGE_ENV, NODE_ROLES_METRIC,
	SUBSTRATE_BLOCK_HEIGHT_FINALIZED_METRIC,
};

const PARA_IDS: [u32; 2] = [2000, 2001];
const CHUNK_MAPPING_FEATURE_INDEX: u32 = 2;

/// Test that systematic chunk recovery is used when the chunk mapping feature is enabled.
///
/// - Spawns validators and glutton parachains
/// - Verifies regular chunk recovery is used before feature is enabled
/// - Enables the chunk mapping feature via sudo call
/// - Verifies systematic chunk recovery is used after enabling
#[tokio::test(flavor = "multi_thread")]
async fn systematic_chunk_recovery() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	log::info!("Starting systematic chunk recovery test");

	let config = build_network_config()?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let validator = network.get_node("validator-0")?;
	let alice_client = alice.wait_client().await?;
	let validator_client = validator.wait_client().await?;

	// Check authority status
	log::info!("Checking node roles");
	alice
		.wait_metric_with_timeout(NODE_ROLES_METRIC, |v| v == 4.0, 30u64)
		.await
		.map_err(|e| anyhow!("Alice role check failed: {}", e))?;
	validator
		.wait_metric_with_timeout(NODE_ROLES_METRIC, |v| v == 4.0, 30u64)
		.await
		.map_err(|e| anyhow!("Validator role check failed: {}", e))?;

	// Ensure parachains are registered
	log::info!("Checking parachains are registered");
	for para_id in PARA_IDS {
		assert_para_is_registered(&alice_client, ParaId::from(para_id), 60).await?;
		assert_para_is_registered(&validator_client, ParaId::from(para_id), 60).await?;
	}

	// Ensure parachains made progress
	log::info!("Waiting for parachains to produce blocks");
	// Check throughput for both parachains together to avoid receipts from other parachains
	let mut expected = std::collections::HashMap::new();
	expected.insert(ParaId::from(PARA_IDS[0]), 15..200);
	expected.insert(ParaId::from(PARA_IDS[1]), 15..200);
	assert_para_throughput(&alice_client, 30, expected).await?;

	// Check finalized block height
	log::info!("Checking finalized block height");
	validator
		.wait_metric_with_timeout(SUBSTRATE_BLOCK_HEIGHT_FINALIZED_METRIC, |v| v >= 30.0, 400u64)
		.await
		.map_err(|e| anyhow!("Finalized height too low: {}", e))?;

	// Check approval checking works
	log::info!("Checking approval finality lag");
	validator
		.wait_metric_with_timeout(APPROVAL_CHECKING_FINALITY_LAG_METRIC, |v| v < 3.0, 30u64)
		.await
		.map_err(|e| anyhow!("Approval lag too high: {}", e))?;

	log::info!("Checking no-shows");
	validator
		.wait_metric_with_timeout(APPROVALS_NO_SHOWS_TOTAL_METRIC, |v| v < 3.0, 100u64)
		.await
		.map_err(|e| anyhow!("Too many no-shows: {}", e))?;

	// Ensure we used regular chunk recovery initially
	log::info!("Verifying regular chunk recovery is used initially");
	validator
		.wait_log_line_count_with_timeout(
			"Data recovery from chunks complete",
			true,
			LogLineCountOptions::new(|n| n >= 10, Duration::from_secs(300), false),
		)
		.await
		.map_err(|e| anyhow!("Regular chunk recovery not found: {}", e))?;

	validator
		.wait_log_line_count_with_timeout(
			"Data recovery from systematic chunks complete",
			true,
			LogLineCountOptions::new(|n| n == 0, Duration::from_secs(10), false),
		)
		.await
		.map_err(|e| anyhow!("Systematic recovery unexpectedly found before enabling: {}", e))?;

	validator
		.wait_log_line_count_with_timeout(
			"Data recovery from systematic chunks is not possible",
			true,
			LogLineCountOptions::new(|n| n == 0, Duration::from_secs(10), false),
		)
		.await
		.map_err(|e| anyhow!("Systematic recovery errors found: {}", e))?;

	validator
		.wait_log_line_count_with_timeout(
			"Data recovery from chunks is not possible",
			true,
			LogLineCountOptions::new(|n| n == 0, Duration::from_secs(10), false),
		)
		.await
		.map_err(|e| anyhow!("Chunk recovery errors found: {}", e))?;

	// Check no failed recoveries
	log::info!("Checking no failed recoveries");
	let failure_metric =
		format!("{AVAILABILITY_RECOVERY_RECOVERIES_FINISHED_METRIC}{{result=\"failure\"}}");
	validator
		.wait_metric_with_timeout(&failure_metric, |v| v == 0.0, 10u64)
		.await
		.map_err(|e| anyhow!("Failed recoveries detected: {}", e))?;

	// Enable the chunk mapping feature using our helper
	log::info!("Enabling chunk mapping feature (index {})", CHUNK_MAPPING_FEATURE_INDEX);
	enable_node_feature(&network, "alice", CHUNK_MAPPING_FEATURE_INDEX).await?;

	// Wait for more blocks after enabling the feature
	log::info!("Waiting for more finalized blocks after enabling feature");
	validator
		.wait_metric_with_timeout(SUBSTRATE_BLOCK_HEIGHT_FINALIZED_METRIC, |v| v >= 60.0, 400u64)
		.await
		.map_err(|e| anyhow!("Finalized height too low after feature enable: {}", e))?;

	// Check approval checking still works
	log::info!("Checking approval finality lag after feature enable");
	validator
		.wait_metric_with_timeout(APPROVAL_CHECKING_FINALITY_LAG_METRIC, |v| v < 3.0, 30u64)
		.await
		.map_err(|e| anyhow!("Approval lag too high after feature enable: {}", e))?;

	log::info!("Checking no-shows after feature enable");
	validator
		.wait_metric_with_timeout(APPROVALS_NO_SHOWS_TOTAL_METRIC, |v| v < 3.0, 100u64)
		.await
		.map_err(|e| anyhow!("Too many no-shows after feature enable: {}", e))?;

	// Ensure we now use systematic chunk recovery
	log::info!("Verifying systematic chunk recovery is used after enabling");
	validator
		.wait_log_line_count_with_timeout(
			"Data recovery from systematic chunks complete",
			true,
			LogLineCountOptions::new(|n| n >= 10, Duration::from_secs(300), false),
		)
		.await
		.map_err(|e| anyhow!("Systematic chunk recovery not found after enabling: {}", e))?;

	validator
		.wait_log_line_count_with_timeout(
			"Data recovery from systematic chunks is not possible",
			true,
			LogLineCountOptions::new(|n| n == 0, Duration::from_secs(10), false),
		)
		.await
		.map_err(|e| anyhow!("Systematic recovery errors found after enabling: {}", e))?;

	validator
		.wait_log_line_count_with_timeout(
			"Data recovery from chunks is not possible",
			true,
			LogLineCountOptions::new(|n| n == 0, Duration::from_secs(10), false),
		)
		.await
		.map_err(|e| anyhow!("Chunk recovery errors found after enabling: {}", e))?;

	// Check no failed recoveries after feature enable
	log::info!("Checking no failed recoveries after feature enable");
	validator
		.wait_metric_with_timeout(&failure_metric, |v| v == 0.0, 10u64)
		.await
		.map_err(|e| anyhow!("Failed recoveries detected after feature enable: {}", e))?;

	log::info!("Systematic chunk recovery test completed successfully");

	Ok(())
}

fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let polkadot_image = env_or_default(INTEGRATION_IMAGE_ENV, images.polkadot.as_str());
	let cumulus_image = env_or_default(CUMULUS_IMAGE_ENV, images.cumulus.as_str());

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
							"scheduler_params": { "max_validators_per_core": 2 }
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

		// Add alice as a validator
		let r = r.with_node(|node| node.with_name("alice").validator(true));

		// Add 3 validators
		r.with_node(|node| {
			node.with_name("validator-0").with_args(vec!["-lparachain=debug,parachain::availability-recovery=trace,parachain::availability-distribution=trace".into()])
		}).with_node(|node| {
			node.with_name("validator-1").with_args(vec!["-lparachain=debug,parachain::availability-recovery=trace,parachain::availability-distribution=trace".into()])
		}).with_node(|node| {
			node.with_name("validator-2").with_args(vec!["-lparachain=debug,parachain::availability-recovery=trace,parachain::availability-distribution=trace".into()])
		})
	});

	// Add glutton parachains.
	for para_id in PARA_IDS {
		let chain_name = format!("glutton-westend-local-{para_id}");
		let collator_name = format!("collator-{para_id}");

		builder = builder.with_parachain(|p| {
			p.with_id(para_id)
				.with_chain(chain_name.as_str())
				.with_default_image(cumulus_image.as_str())
				.with_default_command("polkadot-parachain")
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
				.with_collator(|n| n.with_name(collator_name.as_str()))
		});
	}

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
