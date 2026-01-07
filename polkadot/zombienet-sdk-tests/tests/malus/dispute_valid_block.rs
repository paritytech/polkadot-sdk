// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Dispute Valid Block Test
//!
//! This test verifies that disputes are correctly triggered and resolved when a malicious
//! validator attempts to dispute valid blocks.
//! The test ensures:
//! - Disputes are initiated by the malicious validator
//! - Honest validators correctly vote that the candidate is valid
//! - Disputes conclude with the candidate being marked as valid (not invalid)

use crate::utils::{
	env_or_default, initialize_network, COL_IMAGE_ENV, DISPUTES_TOTAL_METRIC,
	DISPUTE_CONCLUDED_INVALID_METRIC, DISPUTE_CONCLUDED_VALID_METRIC, DISPUTE_VOTES_VALID_METRIC,
	INTEGRATION_IMAGE_ENV, IS_MAJOR_SYNCING_METRIC, MALUS_IMAGE_ENV, NODE_ROLES_METRIC,
	PEERS_COUNT_METRIC, SUBSTRATE_BLOCK_HEIGHT_METRIC,
};

use anyhow::anyhow;
use serde_json::json;
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

const PARA_ID: u32 = 100;

/// Test that disputes triggered by a malicious validator are correctly resolved.
///
/// This test:
/// - Spawns 3 honest validators (alice, bob, charlie)
/// - Spawns 1 malicious validator (dave) running `malus dispute-ancestor`
/// - Spawns a parachain with adder-collator
/// - Verifies disputes are triggered and concluded as valid
#[tokio::test(flavor = "multi_thread")]
async fn dispute_valid_block_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config()?;
	let network = initialize_network(config).await?;

	let alice = network.get_node("alice")?;
	let bob = network.get_node("bob")?;
	let charlie = network.get_node("charlie")?;
	let _dave = network.get_node("dave")?;

	// Check node roles (4 = authority).
	log::info!("Checking node roles");
	alice
		.wait_metric_with_timeout(NODE_ROLES_METRIC, |v| v == 4.0, 30u64)
		.await
		.map_err(|e| anyhow!("Alice node role check failed: {}", e))?;
	bob.wait_metric_with_timeout(NODE_ROLES_METRIC, |v| v == 4.0, 30u64)
		.await
		.map_err(|e| anyhow!("Bob node role check failed: {}", e))?;

	// Check alice is not major syncing.
	log::info!("Checking alice is not major syncing");
	alice
		.wait_metric_with_timeout(IS_MAJOR_SYNCING_METRIC, |v| v == 0.0, 30u64)
		.await
		.map_err(|e| anyhow!("Alice is still syncing: {}", e))?;

	// Check block height reaches at least 2.
	log::info!("Waiting for block height to reach at least 2");
	alice
		.wait_metric_with_timeout(SUBSTRATE_BLOCK_HEIGHT_METRIC, |v| v >= 2.0, 15u64)
		.await
		.map_err(|e| anyhow!("Alice block height too low: {}", e))?;
	bob.wait_metric_with_timeout(SUBSTRATE_BLOCK_HEIGHT_METRIC, |v| v >= 2.0, 30u64)
		.await
		.map_err(|e| anyhow!("Bob block height too low: {}", e))?;
	charlie
		.wait_metric_with_timeout(SUBSTRATE_BLOCK_HEIGHT_METRIC, |v| v >= 2.0, 30u64)
		.await
		.map_err(|e| anyhow!("Charlie block height too low: {}", e))?;

	// Check peers count is at least 2.
	log::info!("Checking peer counts");
	alice
		.wait_metric_with_timeout(PEERS_COUNT_METRIC, |v| v >= 2.0, 30u64)
		.await
		.map_err(|e| anyhow!("Alice peers count too low: {}", e))?;
	bob.wait_metric_with_timeout(PEERS_COUNT_METRIC, |v| v >= 2.0, 30u64)
		.await
		.map_err(|e| anyhow!("Bob peers count too low: {}", e))?;
	charlie
		.wait_metric_with_timeout(PEERS_COUNT_METRIC, |v| v >= 2.0, 30u64)
		.await
		.map_err(|e| anyhow!("Charlie peers count too low: {}", e))?;

	// Wait for at least 1 dispute to be triggered.
	log::info!("Waiting for disputes to be triggered");
	alice
		.wait_metric_with_timeout(DISPUTES_TOTAL_METRIC, |v| v >= 1.0, 250u64)
		.await
		.map_err(|e| anyhow!("Alice disputes not triggered: {}", e))?;
	bob.wait_metric_with_timeout(DISPUTES_TOTAL_METRIC, |v| v >= 1.0, 90u64)
		.await
		.map_err(|e| anyhow!("Bob disputes not triggered: {}", e))?;
	charlie
		.wait_metric_with_timeout(DISPUTES_TOTAL_METRIC, |v| v >= 1.0, 90u64)
		.await
		.map_err(|e| anyhow!("Charlie disputes not triggered: {}", e))?;

	// Check valid dispute votes are recorded.
	log::info!("Checking valid dispute votes");
	alice
		.wait_metric_with_timeout(DISPUTE_VOTES_VALID_METRIC, |v| v >= 1.0, 90u64)
		.await
		.map_err(|e| anyhow!("Alice valid votes not recorded: {}", e))?;
	bob.wait_metric_with_timeout(DISPUTE_VOTES_VALID_METRIC, |v| v >= 2.0, 90u64)
		.await
		.map_err(|e| anyhow!("Bob valid votes not recorded: {}", e))?;
	charlie
		.wait_metric_with_timeout(DISPUTE_VOTES_VALID_METRIC, |v| v >= 2.0, 90u64)
		.await
		.map_err(|e| anyhow!("Charlie valid votes not recorded: {}", e))?;

	// Check disputes concluded as valid.
	log::info!("Checking disputes concluded as valid");
	alice
		.wait_metric_with_timeout(DISPUTE_CONCLUDED_VALID_METRIC, |v| v >= 1.0, 90u64)
		.await
		.map_err(|e| anyhow!("Alice dispute not concluded as valid: {}", e))?;
	bob.wait_metric_with_timeout(DISPUTE_CONCLUDED_VALID_METRIC, |v| v >= 1.0, 90u64)
		.await
		.map_err(|e| anyhow!("Bob dispute not concluded as valid: {}", e))?;
	charlie
		.wait_metric_with_timeout(DISPUTE_CONCLUDED_VALID_METRIC, |v| v >= 1.0, 90u64)
		.await
		.map_err(|e| anyhow!("Charlie dispute not concluded as valid: {}", e))?;

	log::info!("Verifying no disputes concluded as invalid");
	alice
		.wait_metric_with_timeout(DISPUTE_CONCLUDED_INVALID_METRIC, |v| v == 0.0, 90u64)
		.await
		.map_err(|e| anyhow!("Alice has invalid dispute conclusions: {}", e))?;

	log::info!("Test finished successfully");
	Ok(())
}

fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let polkadot_image = env_or_default(INTEGRATION_IMAGE_ENV, images.polkadot.as_str());
	let malus_image = env_or_default(MALUS_IMAGE_ENV, images.cumulus.as_str());
	let col_image = env_or_default(COL_IMAGE_ENV, images.cumulus.as_str());

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(polkadot_image.as_str())
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"max_validators_per_core": 1
							}
						}
					}
				}))
				.with_node(|node| {
					node.with_name("alice")
						.validator(true)
						.with_args(vec!["--alice".into(), "-lparachain=debug".into()])
				})
				.with_node(|node| {
					node.with_name("bob")
						.validator(true)
						.with_args(vec!["--bob".into(), "-lparachain=debug".into()])
				})
				.with_node(|node| {
					node.with_name("charlie")
						.validator(true)
						.with_args(vec!["--charlie".into(), "-lparachain=debug".into()])
				})
				// Malicious validator running dispute-ancestor.
				.with_node(|node| {
					node.with_name("dave")
						.validator(true)
						.with_image(malus_image.as_str())
						.with_command("malus")
						.with_subcommand("dispute-ancestor")
						.with_args(vec![
							"--dave".into(),
							"--insecure-validator-i-know-what-i-do".into(),
							"-lparachain=debug".into(),
						])
				})
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("adder-collator")
				.cumulus_based(false)
				.with_default_image(col_image.as_str())
				.with_default_args(vec!["-lparachain=debug".into()])
				.with_collator(|n| n.with_name("collator01"))
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
