// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Parachains Disputes Test
//!
//! Spawns 8 validators (alice,
//! bob are malus nodes) and 4 parachains (2000..2003) and verifies disputes are
//! initiated and concluded as expected, finality/lag metrics are low and
//! specific log lines are observed.

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_para_is_registered, assert_para_throughput};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use std::time::Duration;
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

use crate::utils::{
	env_or_default, initialize_network, COL_IMAGE_ENV, INTEGRATION_IMAGE_ENV, MALUS_IMAGE_ENV,
};

const PARA_FIRST: u32 = 2000;
const NUM_PARAS: u32 = 4;

const VALIDATOR_NAMES: [&str; 8] =
	["alice", "bob", "charlie", "dave", "ferdie", "eve", "one", "two"];

const DISPUTES_MIN: f64 = 10.0;
const METRIC_WAIT_SECS: u64 = 15; // for dispute counts
const LOG_OFFENCE_WAIT_SECS: u64 = 60;
const LOG_VOTED_WAIT_SECS: u64 = 180;
const LAG_WAIT_SECS: u64 = 120;

#[tokio::test(flavor = "multi_thread")]
async fn parachains_disputes_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config()?;
	let network = initialize_network(config).await?;

	// Check authority status
	log::info!("Checking validator node roles");
	for &name in &VALIDATOR_NAMES {
		let node = network.get_node(name)?;
		node.wait_metric_with_timeout("node_roles", |v| v == 4.0, 30u64)
			.await
			.map_err(|e| anyhow!("Node {} role check failed: {}", name, e))?;
	}

	// Get relay client
	let alice = network.get_node("alice")?;
	let relay_client = alice.wait_client().await?;

	// Ensure parachains are registered
	log::info!("Checking parachain registration");
	for id in PARA_FIRST..(PARA_FIRST + NUM_PARAS) {
		assert_para_is_registered(&relay_client, ParaId::from(id), 30).await?;
	}

	// Ensure parachains have made progress (at least 10 blocks each within 200s)
	log::info!("Waiting for parachains to make progress");
	// TODO: verify throughput range
	let para_throughput_map = (PARA_FIRST..(PARA_FIRST + NUM_PARAS))
		.map(|id| (ParaId::from(id), 10..100u32))
		.collect::<std::collections::HashMap<ParaId, std::ops::Range<u32>>>();
	assert_para_throughput(&relay_client, 30, para_throughput_map).await?;

	// Check disputes metrics
	log::info!("Checking disputes are initiated");
	// Use one honest node (eve) to read metrics
	let eve = network.get_node("eve")?;
	eve.wait_metric_with_timeout(
		"polkadot_parachain_candidate_disputes_total",
		|v| v >= DISPUTES_MIN,
		METRIC_WAIT_SECS,
	)
	.await
	.map_err(|e| anyhow!("Disputes not initiated: {}", e))?;

	log::info!("Checking disputes concluded as valid");
	eve.wait_metric_with_timeout(
		"polkadot_parachain_candidate_dispute_concluded{validity=\"valid\"}",
		|v| v >= DISPUTES_MIN,
		METRIC_WAIT_SECS,
	)
	.await
	.map_err(|e| anyhow!("Valid disputes not concluded: {}", e))?;

	// Ensure no invalid conclusions
	eve.wait_metric_with_timeout(
		"polkadot_parachain_candidate_dispute_concluded{validity=\"invalid\"}",
		|v| v == 0.0,
		METRIC_WAIT_SECS,
	)
	.await
	.map_err(|e| anyhow!("Unexpected invalid disputes: {}", e))?;

	// Check system event for offence reported on alice within 60s
	log::info!("Checking alice logs for offence reported");
	alice
		.wait_log_line_count_with_timeout(
			"There is an offence reported",
			true,
			LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(LOG_OFFENCE_WAIT_SECS), false),
		)
		.await?;

	// Check approval and dispute finality lag metrics are 0 for all validators
	log::info!("Checking approval and dispute finality lag metrics");
	for &name in &VALIDATOR_NAMES {
		let node = network.get_node(name)?;
		node.wait_metric_with_timeout(
			"polkadot_parachain_approval_checking_finality_lag",
			|v| v == 0.0,
			LAG_WAIT_SECS,
		)
		.await
		.map_err(|e| anyhow!("Approval finality lag not zero on {}: {}", name, e))?;

		node.wait_metric_with_timeout(
			"polkadot_parachain_disputes_finality_lag",
			|v| v == 0.0,
			LAG_WAIT_SECS,
		)
		.await
		.map_err(|e| anyhow!("Dispute finality lag not zero on {}: {}", name, e))?;
	}

	// Check that alice logged that it "Voted against a candidate that was concluded valid." within
	// 180s
	log::info!("Checking alice voted-against log line");
	alice
		.wait_log_line_count_with_timeout(
			"Voted against a candidate that was concluded valid.",
			true,
			LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(LOG_VOTED_WAIT_SECS), false),
		)
		.await?;

	log::info!("Test finished successfully");

	Ok(())
}

fn build_network_config() -> Result<NetworkConfig, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();
	let polkadot_image = env_or_default(INTEGRATION_IMAGE_ENV, images.polkadot.as_str());
	let col_image = env_or_default(COL_IMAGE_ENV, "docker.io/paritypr/colander:10656-27c42bae");
	let malus_image = env_or_default(MALUS_IMAGE_ENV, "docker.io/paritypr/malus:10666-e5b2ef85");

	let mut builder = NetworkConfigBuilder::new().with_relaychain(|r| {
		let r = r
			.with_chain("rococo-local")
			.with_default_command("polkadot")
			.with_default_image(polkadot_image.as_str())
			.with_default_args(vec!["-lparachain=debug".into()])
			.with_genesis_overrides(json!({
				"patch": {
					"configuration": {
						"config": {
							"needed_approvals": 8,
							"scheduler_params": {"max_validators_per_core": 5},
							"approval_voting_params": {"max_approval_coalesce_count": 5}
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

		// Add nodes: alice and bob are malus with dispute-ancestor command
		let r = r.with_node(|node| {
			node.with_name("alice")
				.with_image(malus_image.as_str())
				.with_command("malus")
				.with_subcommand("dispute-ancestor")
				.with_args(vec![
					"--fake-validation".into(),
					"approval-invalid".into(),
					"--bob".into(),
					"--insecure-validator-i-know-what-i-do".into(),
					"-lparachain=debug,MALUS=trace".into(),
				])
		});

		let r = r.with_node(|node| {
			node.with_name("bob")
				.with_image(malus_image.as_str())
				.with_command("malus")
				.with_subcommand("dispute-ancestor")
				.with_args(vec![
					"--fake-validation".into(),
					"approval-invalid".into(),
					"--bob".into(),
					"--insecure-validator-i-know-what-i-do".into(),
					"-lparachain=debug,MALUS=trace".into(),
				])
		});

		// Add remaining honest validators
		let r = (0..6).fold(r, |acc, i| {
			let name = ["charlie", "dave", "ferdie", "eve", "one", "two"][i as usize];
			acc.with_node(|node| node.with_name(name).with_args(vec!["-lparachain=debug".into()]))
		});

		r
	});

	// Add parachains 2000..2003
	for id in PARA_FIRST..(PARA_FIRST + NUM_PARAS) {
		let pov = 25_000 * (id - 1999);
		let complexity = id - 1999;
		let genesis_cmd = format!(
			"undying-collator export-genesis-state --pov-size={} --pvf-complexity={}",
			pov, complexity
		);

		builder = builder.with_parachain(|p| {
			p.with_id(id)
				.with_genesis_state_generator(genesis_cmd.as_str())
				.with_default_command("undying-collator")
				.with_default_image(col_image.as_str())
				.cumulus_based(false)
				.with_default_args(vec![
					"-lparachain=debug".into(),
					format!("--pov-size={}", pov).as_str().into(),
					format!("--pvf-complexity={}", complexity).as_str().into(),
				])
				.with_collator(|n| n.with_name("collator"))
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
