// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Test: Reputation Pruning on Parachain Deregistration
//!
//! This test verifies that the reputation pruning mechanism correctly:
//! 1. Builds reputation for multiple parachains
//! 2. Detects when a parachain is deregistered
//! 3. Prunes reputation data for the deregistered parachain
//! 4. Persists the pruned state to disk
//! 5. Correctly loads only the non-pruned data after restart
//!
//! ## Test Scenario
//!
//! 1. Spawn a network with 4 validators and 2 parachains
//! 2. Wait for both parachains to produce blocks (establishing reputation for both)
//! 3. Wait for periodic persistence (both paras' reputation on disk)
//! 4. Record reputation entries for both parachains
//! 5. **Deregister parachain 2001 using sudo**
//! 6. Wait for session boundary (triggers pruning check)
//! 7. Verify pruning logs show para 2001 was pruned
//! 8. Wait for periodic persistence (pruned state written to disk)
//! 9. Restart validator-0
//! 10. Verify only para 2000's reputation was loaded (para 2001 pruned)
//! 11. Verify validator continues normal operation with para 2000
//!
//! ## Success Criteria
//!
//! - Both parachains build reputation initially
//! - After deregistration, pruning logs show para 2001 removed
//! - After restart, only para 2000's reputation is loaded from disk
//! - Para 2000 continues producing blocks normally

use anyhow::anyhow;
use regex::Regex;
use tokio::time::Duration;

use cumulus_zombienet_sdk_helpers::{assert_para_throughput, wait_for_first_session_change};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{ext::scale_value::value, OnlineClient, PolkadotConfig},
	subxt_signer::sr25519::dev,
	NetworkConfigBuilder,
};

const PARA_ID_1: u32 = 2000;
const PARA_ID_2: u32 = 2001;

#[tokio::test(flavor = "multi_thread")]
async fn pruning_test() -> Result<(), anyhow::Error> {
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
								"num_cores": 2
							}
						}
					}
				}))
				.with_node(|node| node.with_name("validator-0"));

			(1..4)
				.fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID_1)
				.with_default_command("undying-collator")
				.cumulus_based(false)
				.with_default_image(
					std::env::var("COL_IMAGE")
						.unwrap_or("docker.io/paritypr/colander:latest".to_string())
						.as_str(),
				)
				.with_default_args(vec![("-lparachain=debug").into(), ("--experimental-send-approved-peer").into()])
				.with_collator(|n| n.with_name("collator-1"))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID_2)
				.with_default_command("undying-collator")
				.cumulus_based(false)
				.with_default_image(
					std::env::var("COL_IMAGE")
						.unwrap_or("docker.io/paritypr/colander:latest".to_string())
						.as_str(),
				)
				.with_default_args(vec![("-lparachain=debug").into(), ("--experimental-send-approved-peer").into()])
				.with_collator(|n| n.with_name("collator-2"))
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

	log::info!("Network spawned, waiting for both parachains to produce blocks");
	assert_para_throughput(
		&validator0_client,
		10,
		[(ParaId::from(PARA_ID_1), 8..12), (ParaId::from(PARA_ID_2), 8..12)],
	)
	.await?;
	log::info!("Both parachains producing blocks, waiting for initial periodic persistence");

	let persistence_result = validator_0
		.wait_log_line_count_with_timeout(
			"Periodic persistence completed: reputation DB written to disk",
			false,
			LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(60), false),
		)
		.await?;
	assert!(persistence_result.success(), "Initial periodic persistence should have completed");
	log::info!("Initial persistence completed - both paras' reputation on disk");
	// Parse logs to verify both paras have reputation entries before pruning
	let logs_before_pruning = validator_0.logs().await?;
	let persistence_para_count_re = Regex::new(
		r"Periodic persistence completed: reputation DB written to disk.*para_count=(\d+)"
	)?;
	let mut para_count_before_pruning: Option<u32> = None;
	for line in logs_before_pruning.lines() {
		if let Some(caps) = persistence_para_count_re.captures(line) {
			para_count_before_pruning = caps.get(1).and_then(|m| m.as_str().parse().ok());
		}
	}

	let para_count = para_count_before_pruning
		.ok_or(anyhow!("Could not parse para_count from persistence log"))?;
	log::info!("Before pruning: para_count={}", para_count);
	assert_eq!(
		para_count, 2,
		"Expected 2 paras with reputation before pruning (2000 and 2001), but found {}",
		para_count
	);


	log::info!("Cleaning up parachain 2001 using ParasSudoWrapper::sudo_schedule_para_cleanup + Paras::force_queue_action");
	// Get Alice's signer
	let alice = dev::alice();
	let cleanup_calls = vec![
		value! {
			ParasSudoWrapper(sudo_schedule_para_cleanup { id: PARA_ID_2 })
		},
		value! {
			Paras(force_queue_action { para: PARA_ID_2 })
		},
	];
	let sudo_batch_call = zombienet_sdk::subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			Utility(batch_all { calls: cleanup_calls })
		}],
	);
	// Submit the transaction
	let tx_progress = validator0_client
		.tx()
		.sign_and_submit_then_watch_default(&sudo_batch_call, &alice)
		.await?;
	// Wait for finalization
	let _finalized = tx_progress.wait_for_finalized_success().await?;
	log::info!("Para cleanup scheduled and force_queue_action submitted successfully");
	// Stop the collator for para 2001 since it's now being cleaned up
	log::info!("Stopping collator-2 for the cleaned-up parachain 2001");
	let collator_2 = network.get_node("collator-2")?;
	collator_2.pause().await?;
	log::info!("Parachain 2001 cleanup scheduled, waiting for session change");
	// The cleanup is scheduled for the next session. We need to wait for at least one
	// session change for the para to be fully offboarded.
	let mut best_blocks = validator0_client.blocks().subscribe_best().await?;
	wait_for_first_session_change(&mut best_blocks).await?;
	log::info!("Session change detected, para 2001 should now be offboarded");

	log::info!("Waiting for pruning logs to confirm para 2001 was pruned");
	let pruning_result = validator_0
		.wait_log_line_count_with_timeout(
			"Prune paras persisted to disk immediately pruned_para_count=1 remaining_para_count=1 registered_para_count=1",
			false,
			LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(90), false),
		)
		.await?;
	assert!(
		pruning_result.success(),
		"Expected validator to log 'Prune paras persisted to disk immediately' with pruned=1, remaining=1, registered=1"
	);
	log::info!("Pruning verified: pruned 1 para, 1 remaining, 1 registered");

	log::info!("Restarting validator-0 to verify only para 2000's reputation loads");
	validator_0.restart(None).await?;
	let validator0_client_after: OnlineClient<PolkadotConfig> = validator_0.wait_client().await?;
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
	// Parse logs after restart to verify only para 2000 was loaded
	let logs_after_restart = validator_0.logs().await?;
	let load_re = Regex::new(r"Loaded existing reputation DB from disk.*para_count=(\d+)")?;
	let mut para_count: Option<u32> = None;
	for line in logs_after_restart.lines() {
		if let Some(caps) = load_re.captures(line) {
			para_count = caps.get(1).and_then(|m| m.as_str().parse().ok());
			if para_count.is_some() {
				break;
			}
		}
	}
	let count = para_count.unwrap();
	log::info!("After restart: para_count={}", count);
	assert!(
		count <= 1,
		"Expected at most 1 para after pruning, but found {}",
		count
	);

	log::info!("Verifying para 2000 continues normal operation (para 2001 is deregistered)");
	assert_para_throughput(&validator0_client_after, 5, [(ParaId::from(PARA_ID_1), 4..6)])
		.await?;

	log::info!("Pruning test completed successfully");
	Ok(())
}
