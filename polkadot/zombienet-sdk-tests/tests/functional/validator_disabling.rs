// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test checks that misbehaving validators disabled.

use anyhow::anyhow;

use cumulus_zombienet_sdk_helpers::assert_finalized_para_throughput;
use polkadot_primitives::{BlockNumber, CandidateHash, DisputeState, SessionIndex};
use serde_json::json;
use tokio::time::Duration;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

#[tokio::test(flavor = "multi_thread")]
async fn validator_disabling_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	let images = zombienet_sdk::environment::get_images_from_env();
	let config_builder = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("westend-local") // Use westend-local so the disabling can take effect.
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"group_rotation_frequency": 10,
								"max_validators_per_core": 1
							},
							"needed_approvals": 2,
						}
					}
				}))
				// Adding malicious validator.
				.with_node(|node| {
					node.with_name("malus-validator")
						.with_image(
							std::env::var("MALUS_IMAGE")
								.unwrap_or("docker.io/paritypr/malus".to_string())
								.as_str(),
						)
						.with_command("malus")
						.with_subcommand("suggest-garbage-candidate")
						.with_args(vec![
							"-lMALUS=trace".into(),
							// Without this the malus validator won't run on macOS.
							"--insecure-validator-i-know-what-i-do".into(),
						])
						// Make it vulenrable so disabling really happens
						.invulnerable(false)
				});
			// Also honest validators.
			let r = (0..3).fold(r, |acc, i| {
				acc.with_node(|node| {
					node.with_name(&format!("honest-validator-{i}"))
						.with_args(vec![("-lparachain=debug,runtime::staking=debug".into())])
				})
			});
			r
		})
		.with_parachain(|p| {
			p.with_id(1000)
				.with_default_command("adder-collator")
				.cumulus_based(false)
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec!["-lparachain=debug".into()])
				.with_collator(|n| n.with_name("alice"))
		})
		.build()
		.map_err(|e| {
			let errors = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errors: {errors}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	log::info!("Spawning network");
	let network = spawn_fn(config_builder).await?;

	log::info!("Waiting for parablocks to be produced");
	let honest_validator = network.get_node("honest-validator-0")?;
	let relay_client: OnlineClient<PolkadotConfig> = honest_validator.wait_client().await?;

	assert_finalized_para_throughput(
		&relay_client,
		20,
		[(polkadot_primitives::Id::from(1000), 10..30)].into_iter().collect(),
	)
	.await?;

	log::info!("Wait for a dispute to be initialized.");
	let mut best_blocks = relay_client.blocks().subscribe_best().await?;
	let mut dispute_session: u32 = u32::MAX;
	// Check next new block from the current best fork
	while let Some(block) = best_blocks.next().await {
		let disputes = relay_client
			.runtime_api()
			.at(block?.hash())
			.call_raw::<Vec<(SessionIndex, CandidateHash, DisputeState<BlockNumber>)>>(
				"ParachainHost_disputes",
				None,
			)
			.await?;
		if let Some((session, _, _)) = disputes.first() {
			dispute_session = *session;
			break;
		}
	}
	assert_ne!(dispute_session, u32::MAX);
	log::info!("Dispute initiated.");

	let concluded_dispute_metric =
		"polkadot_parachain_candidate_dispute_concluded{validity=\"invalid\"}";
	let parachain_candidate_dispute_metric = "parachain_candidate_disputes_total";
	// honest-validator-1: reports parachain_candidate_disputes_total is at least 1 within 600
	// seconds
	honest_validator
		.wait_metric_with_timeout(parachain_candidate_dispute_metric, |d| d >= 1.0, 600_u64)
		.await?;
	// honest-validator: reports polkadot_parachain_candidate_dispute_concluded{validity="invalid"}
	// is at least 1 within 200 seconds
	honest_validator
		.wait_metric_with_timeout(concluded_dispute_metric, |d| d >= 1.0, 200_u64)
		.await?;
	// honest-validator: log line contains "Disabled validators detected" within 180 seconds
	let result = honest_validator
		.wait_log_line_count_with_timeout(
			"*Disabled validators detected*",
			true,
			LogLineCountOptions::new(|n| n == 1, Duration::from_secs(180_u64), false),
		)
		.await?;
	assert!(result.success());
	Ok(())
}
