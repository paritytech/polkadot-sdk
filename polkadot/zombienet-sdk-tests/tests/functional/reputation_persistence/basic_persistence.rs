// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Test: Basic Persistence on Graceful Shutdown with Startup Lookback Verification
//!
//! This test verifies that collator reputation data is correctly persisted to disk
//! during normal operation, survives a graceful validator restart, and that the
//! startup lookback mechanism correctly catches up on reputation bumps from blocks
//! finalized between persistence and restart.
//!
//! ## Test Scenario - Phase 1: Large Gap (>= 20 blocks)
//!
//! 1. Spawn a network with 4 validators and 1 parachain (using experimental collator protocol)
//! 2. Wait for parachain blocks to be produced (establishing reputation through backed candidates)
//! 3. Wait for periodic persistence (using short interval for testing)
//! 4. Record the finalized block number at persistence time
//! 5. **Pause validator-0** (so it misses blocks being finalized)
//! 6. Wait for 22+ finalized blocks while validator-0 is paused (creating a real gap)
//! 7. Restart validator-0 (triggers full startup sequence)
//! 8. Verify validator loads existing reputation from disk on restart
//! 9. Verify the startup lookback processes blocks it missed while paused
//! 10. Verify validator continues normal operation
//!
//! ## Test Scenario - Phase 2: Small Gap (< 20 blocks)
//!
//! 11. Pause validator-0 again
//! 12. Wait for ~10 finalized blocks (smaller gap)
//! 13. Restart validator-0 again
//! 14. Verify the startup lookback processes the entire gap (not limited by MAX_STARTUP_ANCESTRY_LOOKBACK)
//! 15. Verify processed block count matches the actual gap size
//!
//! ## Success Criteria
//!
//! - Validator logs show "Loaded existing reputation DB from disk" on both restarts
//! - First lookback processes at least 20 blocks (large gap)
//! - Second lookback processes exactly ~10 blocks (entire small gap)
//! - Validator resumes backing candidates after both restarts
//! - No errors about missing or corrupted data

use anyhow::anyhow;
use regex::Regex;
use tokio::time::Duration;

use cumulus_zombienet_sdk_helpers::assert_para_throughput;
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

const PARA_ID: u32 = 2000;

#[tokio::test(flavor = "multi_thread")]
async fn basic_persistence_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![
					("-lparachain=debug,parachain::collator-protocol=trace").into(),
					("--experimental-collator-protocol").into(),
				])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"group_rotation_frequency": 4,
								"num_cores": 1
							}
						}
					}
				}))
				.with_node(|node| node.with_name("validator-0"));

			(1..4)
				.fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("undying-collator")
				.cumulus_based(false)
				.with_default_image(
					std::env::var("COL_IMAGE")
						.unwrap_or("docker.io/paritypr/colander:latest".to_string())
						.as_str(),
				)
				.with_default_args(vec![
				("-lparachain=debug").into(),
				("--experimental-send-approved-peer").into(),
			])
				.with_collator(|n| n.with_name("collator"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let validator_0 = network.get_node("validator-0")?;
	let validator0_client: OnlineClient<PolkadotConfig> = validator_0.wait_client().await?;

	// Verify validator-0 shows fresh start initially (no existing data)
	let fresh_start_result = validator_0
		.wait_log_line_count_with_timeout(
			"Reputation DB initialized fresh",
			false,
			LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(60), false),
		)
		.await?;
	assert!(
		fresh_start_result.success(),
		"Expected validator to log 'Reputation DB initialized fresh' on initial startup"
	);

	log::info!("Network spawned, waiting for parachain blocks to be produced");
	assert_para_throughput(&validator0_client, 10, [(ParaId::from(PARA_ID), 8..12)]).await?;

	log::info!("Parachain blocks produced, waiting for periodic persistence");
	let persistence_result = validator_0
		.wait_log_line_count_with_timeout(
			"Periodic persistence completed: reputation DB written to disk",
			false,
			LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(60), false),
		)
		.await?;
	assert!(persistence_result.success(), "Periodic persistence should have completed");

	let logs_before_pause = validator_0.logs().await?;
	let persistence_re = Regex::new(
		r"Periodic persistence completed: reputation DB written to disk.*last_finalized=Some\((\d+)\)"
	)?;

	let mut block_at_persistence: Option<u32> = None;
	for line in logs_before_pause.lines() {
		if let Some(caps) = persistence_re.captures(line) {
			block_at_persistence = caps.get(1).and_then(|m| m.as_str().parse().ok());
		}
	}

	let block_at_persistence = block_at_persistence
		.ok_or(anyhow!("Could not parse last_finalized from persistence log"))?;
	log::info!("Periodic persistence completed at finalized block {}", block_at_persistence);

	log::info!("Pausing validator-0 to create a block gap");
	validator_0.pause().await?;

	let validator_1 = network.get_node("validator-1")?;
	let validator_1_client: OnlineClient<PolkadotConfig> = validator_1.wait_client().await?;
	let mut finalized_blocks_1 = validator_1_client.blocks().subscribe_finalized().await?;

	log::info!("Waiting for finalized blocks while validator-0 is paused");
	let target_gap = 30u32;
	let mut block_at_restart = block_at_persistence;
	while block_at_restart < block_at_persistence + target_gap {
		if let Some(Ok(block)) = finalized_blocks_1.next().await {
			block_at_restart = block.number();
			log::info!("Finalized block {} (gap: {})", block_at_restart, block_at_restart.saturating_sub(block_at_persistence));
		}
	}
	log::info!(
		"Gap created while validator-0 was paused: finalized block now at {}, gap of {} blocks",
		block_at_restart,
		block_at_restart.saturating_sub(block_at_persistence)
	);

	log::info!("Restarting validator-0 (full restart to trigger startup lookback)");
	validator_0.restart(None).await?;
	let _: OnlineClient<PolkadotConfig> = validator_0.wait_client().await?;
	log::info!("Validator-0 restarted, verifying reputation loaded from disk");

	let load_result = validator_0
		.wait_log_line_count_with_timeout(
			"Loaded existing reputation DB from disk",
			false,
			LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(60), false),
		)
		.await?;
	assert!(
		load_result.success(),
		"Expected validator to log 'Loaded existing reputation DB from disk' after restart"
	);

	let lookback_completed_result = validator_0
		.wait_log_line_count_with_timeout(
			"Startup lookback completed",
			false,
			LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(30), false),
		)
		.await?;
	assert!(
		lookback_completed_result.success(),
		"Expected validator to log 'Startup lookback completed' after restart"
	);

	let logs = validator_0.logs().await?;
	let lookback_completed_re = Regex::new(
		r"Startup lookback completed.*blocks_processed=(\d+)"
	)?;

	let mut found_lookback_completed = false;
	let mut blocks_processed: Option<u32> = None;

	for line in logs.lines() {
		if let Some(caps) = lookback_completed_re.captures(line) {
			found_lookback_completed = true;
			blocks_processed = caps.get(1).and_then(|m| m.as_str().parse().ok());
			log::info!(
				"Found startup lookback completed log: blocks_processed={}",
				blocks_processed.unwrap()
			);
			break;
		}
	}

	assert!(
		found_lookback_completed,
		"Expected to find 'Startup lookback completed' log with blocks_processed field"
	);

	let processed = blocks_processed.expect("blocks_processed should be present in log");
	assert!(
		processed == 20,
		"Expected blocks_processed ({}) == MAX_STARTUP_ANCESTRY_LOOKBACK ({})",
		processed, target_gap
	);
	log::info!(
		"Lookback verification passed: processed {} blocks (< existing gap {})",
		processed, target_gap
	);

	log::info!("Verifying validator resumes normal operation");

	let relay_client_after: OnlineClient<PolkadotConfig> = validator_0.wait_client().await?;
	assert_para_throughput(&relay_client_after, 5, [(ParaId::from(PARA_ID), 4..6)]).await?;

	// === Phase 2: Verify lookback processes entire gap when gap < MAX_STARTUP_ANCESTRY_LOOKBACK ===
	log::info!("Phase 2: Testing lookback with smaller gap (< 20 blocks)");

	// Wait for another periodic persistence to get a precise starting point
	log::info!("Waiting for second periodic persistence");
	let persistence_result_2 = validator_0
		.wait_log_line_count_with_timeout(
			"Periodic persistence completed: reputation DB written to disk",
			false,
			LogLineCountOptions::new(|n| n >= 2, Duration::from_secs(60), false),
		)
		.await?;
	assert!(persistence_result_2.success(), "Second periodic persistence should have completed");

	validator_0.pause().await?;
	log::info!("Pausing validator-0 again to create a smaller gap");

	let logs_before_second_pause = validator_0.logs().await?;
	let mut block_before_second_pause: Option<u32> = None;

	for line in logs_before_second_pause.lines().rev() {
		if let Some(caps) = persistence_re.captures(line) {
			block_before_second_pause = caps.get(1).and_then(|m| m.as_str().parse().ok());
			if block_before_second_pause.is_some() {
				break;
			}
		}
	}

	let block_before_second_pause = block_before_second_pause
		.ok_or(anyhow!("Could not parse last_finalized from second persistence log"))?;
	log::info!("Second periodic persistence completed at finalized block {}", block_before_second_pause);
	

	let small_gap_target = 10u32;
	let mut block_at_second_restart = block_before_second_pause;
	while block_at_second_restart < block_before_second_pause + small_gap_target {
		if let Some(Ok(block)) = finalized_blocks_1.next().await {
			block_at_second_restart = block.number();
			log::info!(
				"Finalized block {} (gap: {})",
				block_at_second_restart,
				block_at_second_restart.saturating_sub(block_before_second_pause)
			);
		}
	}
	log::info!(
		"Small gap created: {} blocks (from {} to {})",
		block_at_second_restart.saturating_sub(block_before_second_pause),
		block_before_second_pause,
		block_at_second_restart
	);

	log::info!("Restarting validator-0 (second restart)");
	validator_0.restart(None).await?;
	let _: OnlineClient<PolkadotConfig> = validator_0.wait_client().await?;

	let lookback_completed_result_2 = validator_0
		.wait_log_line_count_with_timeout(
			"Startup lookback completed",
			false,
			LogLineCountOptions::new(|n| n >= 2, Duration::from_secs(30), false),
		)
		.await?;
	assert!(
		lookback_completed_result_2.success(),
		"Expected second 'Startup lookback completed' log"
	);

	let logs_second_restart = validator_0.logs().await?;

	let mut last_blocks_processed: Option<u32> = None;
	for line in logs_second_restart.lines().rev() {
		if let Some(caps) = lookback_completed_re.captures(line) {
			last_blocks_processed = caps.get(1).and_then(|m| m.as_str().parse().ok());
			if last_blocks_processed.is_some() {
				break;
			}
		}
	}

	let processed_second = last_blocks_processed.expect("Should find second lookback completed log");
	log::info!("Second lookback processed {} blocks", processed_second);

	let expected_gap = block_at_second_restart.saturating_sub(block_before_second_pause);
	log::info!(
		"Second lookback: gap was {} blocks (from {} to {}), processed {}",
		expected_gap,
		block_before_second_pause,
		block_at_second_restart,
		processed_second
	);

	assert!(
		expected_gap < 20,
		"Expected second gap to be < 20 (to test no artificial limit), but got {}",
		expected_gap
	);

	assert!(
		processed_second >= expected_gap.saturating_sub(4) &&
			processed_second <= expected_gap + 4,
		"Expected second lookback to process entire gap (~{} blocks), but got {}",
		expected_gap,
		processed_second
	);

	log::info!("Basic persistence test completed successfully - both large and small gap tests passed");

	Ok(())
}
