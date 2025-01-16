// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Test past-session slashing when a malicious validator backs an invalid
//! candidate and a dispute concluding in a future session. We achieve that by
//! making some of the honest nodes go offline.

use anyhow::anyhow;

use crate::helpers::{assert_blocks_are_being_finalized, assert_para_throughput};
use polkadot_primitives::{BlockNumber, CandidateHash, DisputeState, Id as ParaId, SessionIndex};
use serde_json::json;
use subxt::{OnlineClient, PolkadotConfig};
use zombienet_sdk::{NetworkConfigBuilder, NetworkNode};

#[tokio::test(flavor = "multi_thread")]
async fn dispute_past_session_slashing() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![
					"--no-hardware-benchmarks".into(),
					"-lparachain=debug".into(),
				])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"group_rotation_frequency": 3,
								"max_validators_per_core": 1,
								"lookahead": 2,
								"max_candidate_depth": 3,
								"allowed_ancestry_len": 2
							},
							"needed_approvals": 2
						}
					}
				}))
				.with_node(|node| node.with_name("honest-validator-0"))
				.with_node(|node| node.with_name("honest-validator-1"))
				.with_node(|node| node.with_name("honest-flaky-validator-0"))
				.with_node(|node| {
					node.with_name("malicious-backer")
						.with_command("malus")
						.with_subcommand("suggest-garbage-candidate")
						.with_args(vec![
							"--no-hardware-benchmarks".into(),
							"--insecure-validator-i-know-what-i-do".into(),
							"-lMALUS=trace,parachain=debug".into(),
						])
				})
		})
		.with_parachain(|p| {
			p.with_id(1337)
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![
					"--experimental-use-slot-based".into(),
					"-lparachain=debug".into(),
				])
				.with_collator(|n| n.with_name("collator-1337"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let malus = network.get_node("malicious-backer")?;
	malus.pause().await?;

	let honest = network.get_node("honest-validator-0")?;
	let relay_client: OnlineClient<PolkadotConfig> = honest.wait_client().await?;

	// Wait for some para blocks being produced
	assert_para_throughput(&relay_client, 2, [(ParaId::from(1337), 1..5)].into_iter().collect())
		.await?;

	// Let's initiate a dispute
	malus.resume().await?;
	// Pause flaky nodes, so a dispute doesn't conclude
	let flaky_0 = network.get_node("honest-flaky-validator-0")?;
	flaky_0.pause().await?;

	// wait for a dispute to be initiated
	let dispute_session: u32;
	loop {
		let disputes = relay_client
			.runtime_api()
			.at_latest()
			.await?
			.call_raw::<Vec<(SessionIndex, CandidateHash, DisputeState<BlockNumber>)>>(
				"ParachainHost_disputes",
				None,
			)
			.await?;
		if let Some((session, _, _)) = disputes.iter().next() {
			dispute_session = *session;
			break
		}
		tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;
	}

	log::info!("Dispute initiated, now waiting for a new session");

	loop {
		let current_session = relay_client
			.runtime_api()
			.at_latest()
			.await?
			.call_raw::<SessionIndex>("ParachainHost_session_index_for_child", None)
			.await?;
		if current_session > dispute_session {
			break
		}
		tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;
	}

	// We don't need malus anymore
	malus.pause().await?;

	let concluded_dispute_metric =
		"polkadot_parachain_candidate_dispute_concluded{validity=\"invalid\"}";

	let concluded_disputes = wait_for_metric(&honest, concluded_dispute_metric, 0).await;

	assert_eq!(concluded_disputes, 0, "with one offline honest node, dispute should not conclude");

	// Now resume flaky validators
	log::info!("Resuming flaky nodes - dispute should conclude");
	flaky_0.resume().await?;

	wait_for_metric(&honest, concluded_dispute_metric, 1).await;
	log::info!("A dispute has concluded");

	let timeout_secs: u64 = 360;
	honest
		.wait_log_line_count_with_timeout(
			"*Successfully reported pending slash*",
			true,
			1,
			timeout_secs,
		)
		.await?;

	assert_blocks_are_being_finalized(&relay_client).await?;

	log::info!("Test finished successfully");

	Ok(())
}

pub async fn wait_for_metric(node: &NetworkNode, metric: &str, value: u64) -> u64 {
	log::info!("Waiting for {metric} to reach {value}:");
	loop {
		let current = node.reports(metric).await.unwrap_or(0.0) as u64;
		log::debug!("{metric} = {current}");
		if current >= value {
			return current;
		}
		tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
	}
}
