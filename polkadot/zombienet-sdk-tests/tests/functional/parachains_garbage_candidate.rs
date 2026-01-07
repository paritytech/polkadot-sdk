// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Parachains Garbage Candidate Test
//!
//! It spawns 4 validators (3 honest + 1 malus)
//! and 3 parachains (2000..2003) and verifies that disputes are initiated and concluded
//! as invalid when a malicious validator attempts to include garbage candidates.

use crate::utils::{
	env_or_default, initialize_network, COL_IMAGE_ENV, INTEGRATION_IMAGE_ENV, MALUS_IMAGE_ENV,
};
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_para_is_registered, assert_para_throughput};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use std::{collections::HashMap, ops::Range, time::Duration};
use zombienet_orchestrator::network::node::LogLineCountOptions;
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

const PARA_FIRST: u32 = 2000;
const NUM_PARAS: u32 = 3;

const HONEST_VALIDATORS: [&str; 3] =
	["honest-validator-0", "honest-validator-1", "honest-validator-2"];
const MALUS_VALIDATOR: &str = "malus-validator-0";

// Metric thresholds from the .zndsl
const DISPUTES_MIN: f64 = 2.0;
const METRIC_WAIT_SECS: u64 = 15;
const LOG_OFFENCE_WAIT_SECS: u64 = 180;
const LOG_REVERSION_WAIT_SECS: u64 = 180;
const LOG_VOTED_WAIT_SECS: u64 = 180;
const FINALITY_LAG_MAX: f64 = 2.0;

#[tokio::test(flavor = "multi_thread")]
async fn parachains_garbage_candidate_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let config = build_network_config()?;
	let network = initialize_network(config).await?;

	// Check authority status
	log::info!("Checking validator node roles");
	for &name in HONEST_VALIDATORS.iter().chain(std::iter::once(&MALUS_VALIDATOR)) {
		let node = network.get_node(name)?;
		node.wait_metric_with_timeout("node_roles", |v| v == 4.0, 30u64)
			.await
			.map_err(|e| anyhow!("Node {} role check failed: {}", name, e))?;
	}

	// Get relay client
	let honest_0 = network.get_node("honest-validator-0")?;
	let relay_client = honest_0.wait_client().await?;

	// Ensure parachains are registered
	log::info!("Checking parachain registration");
	for id in PARA_FIRST..(PARA_FIRST + NUM_PARAS) {
		assert_para_is_registered(&relay_client, ParaId::from(id), 30).await?;
	}

	// Ensure parachains have made progress (at least 2 blocks each)
	log::info!("Waiting for parachains to make progress");
	let para_throughput_map = (PARA_FIRST..(PARA_FIRST + NUM_PARAS))
		.map(|id| (ParaId::from(id), 2..100u32))
		.collect::<HashMap<ParaId, Range<u32>>>();
	assert_para_throughput(&relay_client, 30, para_throughput_map).await?;

	// Check system event for offence reported on all honest validators within 180s
	log::info!("Checking honest validators logs for offence reported");
	for &name in &HONEST_VALIDATORS {
		let node = network.get_node(name)?;
		node.wait_log_line_count_with_timeout(
			"There is an offence reported",
			true,
			LogLineCountOptions::new(|n| n >= 1, Duration::from_secs(LOG_OFFENCE_WAIT_SECS), false),
		)
		.await
		.map_err(|e| anyhow!("Offence not reported on {}: {}", name, e))?;
	}

	// Check for chain reversion after dispute conclusion
	log::info!("Checking for chain reversion logs");
	for &name in &HONEST_VALIDATORS {
		let node = network.get_node(name)?;
		node.wait_log_line_count_with_timeout(
			"reverted due to a bad parachain block",
			true,
			LogLineCountOptions::new(
				|n| n >= 1,
				Duration::from_secs(LOG_REVERSION_WAIT_SECS),
				false,
			),
		)
		.await
		.map_err(|e| anyhow!("Chain reversion not logged on {}: {}", name, e))?;
	}

	// Check if disputes are concluded in less than 2 blocks
	log::info!("Checking disputes finality lag is less than 2");
	for &name in &HONEST_VALIDATORS {
		let node = network.get_node(name)?;
		node.wait_metric_with_timeout(
			"polkadot_parachain_disputes_finality_lag",
			|v| v < FINALITY_LAG_MAX,
			120u64,
		)
		.await
		.map_err(|e| anyhow!("Dispute finality lag not < 2 on {}: {}", name, e))?;
	}

	// Allow more time for malicious validator activity
	log::info!("Waiting 30 seconds for malicious validator activity");
	tokio::time::sleep(Duration::from_secs(30)).await;

	// Check that garbage parachain blocks included by malicious validators are being disputed
	log::info!("Checking disputes are initiated");
	for &name in &HONEST_VALIDATORS {
		let node = network.get_node(name)?;
		node.wait_metric_with_timeout(
			"polkadot_parachain_candidate_disputes_total",
			|v| v >= DISPUTES_MIN,
			METRIC_WAIT_SECS,
		)
		.await
		.map_err(|e| anyhow!("Disputes not initiated on {}: {}", name, e))?;
	}

	// Disputes should always end as "invalid"
	log::info!("Checking disputes concluded as invalid");
	let honest_0 = network.get_node("honest-validator-0")?;
	honest_0
		.wait_metric_with_timeout(
			"polkadot_parachain_candidate_dispute_concluded{validity=\"invalid\"}",
			|v| v >= DISPUTES_MIN,
			METRIC_WAIT_SECS,
		)
		.await
		.map_err(|e| anyhow!("Invalid disputes not concluded: {}", e))?;

	let honest_1 = network.get_node("honest-validator-1")?;
	honest_1
		.wait_metric_with_timeout(
			"polkadot_parachain_candidate_dispute_concluded{validity=\"valid\"}",
			|v| v == 0.0,
			METRIC_WAIT_SECS,
		)
		.await
		.map_err(|e| anyhow!("Unexpected valid disputes: {}", e))?;

	// Check participating in the losing side of a dispute logged
	log::info!("Checking malus validator voted-invalid log line");
	let malus = network.get_node(MALUS_VALIDATOR)?;
	malus
		.wait_log_line_count_with_timeout(
			"Voted for a candidate that was concluded invalid.",
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
	let col_image = env_or_default(COL_IMAGE_ENV, "docker.io/paritypr/colander:latest");
	let malus_image = env_or_default(MALUS_IMAGE_ENV, "docker.io/paritypr/malus:latest");

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
							"needed_approvals": 2,
							"scheduler_params": {"max_validators_per_core": 1}
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

		// Add honest validators
		let r = r
			.with_node(|node| {
				node.with_name("honest-validator-0")
					.with_args(vec!["-lparachain=debug,runtime=debug".into()])
			})
			.with_node(|node| {
				node.with_name("honest-validator-1")
					.with_args(vec!["-lparachain=debug,runtime=debug".into()])
			})
			.with_node(|node| {
				node.with_name("honest-validator-2")
					.with_args(vec!["-lparachain=debug,runtime=debug".into()])
			});

		// Add malus validator with suggest-garbage-candidate command
		let r = r.with_node(|node| {
			node.with_name(MALUS_VALIDATOR)
				.with_image(malus_image.as_str())
				.with_command("malus")
				.with_subcommand("suggest-garbage-candidate")
				.with_args(vec![
					"--insecure-validator-i-know-what-i-do".into(),
					"-lparachain=debug,MALUS=trace".into(),
				])
		});

		r
	});

	// Add parachains 2000..2002
	for id in PARA_FIRST..(PARA_FIRST + NUM_PARAS) {
		let pov = 10_000 * (id - 1999);
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
				.with_collator(|n| n.with_name(&format!("collator-{}", id)))
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
