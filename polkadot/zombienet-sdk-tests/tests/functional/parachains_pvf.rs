// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Parachains PVF Preparation & Execution Time Test
//!
//! This test verifies that PVF (Parachain Validation Function) preparation and
//! execution times stay within acceptable bounds. It sets up a network with 8
//! validators and 8 parachains with varying PoV sizes and PVF complexity levels.
//!
//! The test validates:
//! - All validators are running as authorities
//! - All parachains are registered and producing blocks
//! - PVF preparation time stays under 10 seconds
//! - PVF execution time stays under 2 seconds
//! - No samples appear in high-latency buckets

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_para_is_registered, assert_para_throughput};
use polkadot_primitives::Id as ParaId;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder, NetworkNode, RegistrationStrategy,
};

use crate::utils::initialize_network;

// Test configuration constants
const NUM_VALIDATORS: usize = 8;
const FIRST_PARA_ID: u32 = 2000;
const NUM_PARACHAINS: u32 = 8;

// Timeout and threshold constants
const PARA_REGISTRATION_TIMEOUT_BLOCKS: u32 = 5;
const THROUGHPUT_TIMEOUT_BLOCKS: u32 = 30;
const MIN_PARA_BLOCKS: u32 = 10;
const MAX_PARA_BLOCKS: u32 = 50;
const MIN_FINALIZED_HEIGHT: f64 = 30.0;

// PVF metric constants
const MAX_PVF_PREP_TIME_SECS: f64 = 10.0;
const MAX_PVF_EXEC_TIME_SECS: f64 = 2.0;

// Metric bucket definitions
const PVF_PREP_BUCKETS_ACCEPTABLE: &[&str] = &["0.1", "0.5", "1", "2", "3", "10"];
const PVF_PREP_BUCKETS_UNACCEPTABLE: &[&str] = &["20", "30", "60", "120", "+Inf"];
const PVF_EXEC_BUCKETS_ACCEPTABLE: &[&str] = &["0.1", "0.5", "1", "2"];
const PVF_EXEC_BUCKETS_UNACCEPTABLE: &[&str] = &["4", "5", "6", "+Inf"];

// Validator node names
const VALIDATOR_NAMES: [&str; NUM_VALIDATORS] =
	["alice", "bob", "charlie", "dave", "eve", "ferdie", "one", "two"];

// Parachain configurations: (id, pov_size, pvf_complexity, collator_name)
const PARACHAIN_CONFIGS: [(u32, u32, u32, &str); 8] = [
	(2000, 100000, 1, "collator01"),
	(2001, 100000, 10, "collator02"),
	(2002, 100000, 100, "collator03"),
	(2003, 20000, 300, "collator04"),
	(2004, 100000, 300, "collator05"),
	(2005, 20000, 400, "collator06"),
	(2006, 100000, 300, "collator07"),
	(2007, 100000, 300, "collator08"),
];

/// Test PVF preparation & execution time
#[tokio::test(flavor = "multi_thread")]
async fn parachains_pvf_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();
	let col_image = std::env::var("COL_IMAGE")
		.unwrap_or_else(|_| "docker.io/paritypr/colander:latest".to_string());

	let builder = NetworkConfigBuilder::new().with_relaychain(|r| {
		r.with_chain("rococo-local")
			.with_default_command("polkadot")
			.with_default_image(images.polkadot.as_str())
			.with_default_args(vec!["-lparachain=debug,runtime=debug".into()])
			.with_default_resources(|resources| {
				resources
					.with_limit_memory("4G")
					.with_limit_cpu("2")
					.with_request_memory("2G")
					.with_request_cpu("1")
			})
			.with_node(|node| node.with_name("alice").validator(true))
			.with_node(|node| node.with_name("bob").validator(true))
			.with_node(|node| node.with_name("charlie").validator(true))
			.with_node(|node| node.with_name("dave").validator(true))
			.with_node(|node| node.with_name("ferdie").validator(true))
			.with_node(|node| node.with_name("eve").validator(true))
			.with_node(|node| node.with_name("one").validator(true))
			.with_node(|node| node.with_name("two").validator(true))
	});

	// Add parachains with varying configurations
	let mut builder =
		PARACHAIN_CONFIGS
			.iter()
			.fold(builder, |builder, &(id, pov_size, complexity, collator)| {
				let genesis_cmd = format!(
					"undying-collator export-genesis-state --pov-size={} --pvf-complexity={}",
					pov_size, complexity
				);
				builder.with_parachain(|p| {
					p.with_id(id)
						.with_genesis_state_generator(genesis_cmd.as_str())
						.with_default_command("undying-collator")
						.with_default_image(col_image.as_str())
						.cumulus_based(false)
						.with_default_args(vec!["-lparachain=debug".into()])
						.with_registration_strategy(RegistrationStrategy::InGenesis)
						.with_collator(|n| n.with_name(collator))
				})
			});

	builder = builder.with_global_settings(|global_settings| {
		match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => global_settings.with_base_dir(val),
			_ => global_settings,
		}
	});

	let config = builder.build().map_err(|e| {
		let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
		anyhow!("config errs: {errs}")
	})?;

	let network = initialize_network(config).await?;

	// Check authority status (node_roles = 4 means authority)
	log::info!("Checking validator node roles");
	for &name in &VALIDATOR_NAMES {
		let node = network.get_node(name)?;
		let node_roles = node.reports("node_roles").await?;
		assert_eq!(node_roles, 4.0, "Node {} should have node_roles = 4 (authority)", name);
	}

	// Get relay client
	let alice = network.get_node("alice")?;
	let relay_client: OnlineClient<PolkadotConfig> = alice.wait_client().await?;

	// Ensure parachains are registered on the relay chain before waiting for throughput
	log::info!("Checking parachain registration");
	for id in FIRST_PARA_ID..(FIRST_PARA_ID + NUM_PARACHAINS) {
		assert_para_is_registered(
			&relay_client,
			ParaId::from(id),
			PARA_REGISTRATION_TIMEOUT_BLOCKS,
		)
		.await?;
	}
	log::info!("All parachains registered");

	// Wait for parachains to make progress (at least MIN_PARA_BLOCKS blocks each)
	log::info!("Waiting for parachains to make progress");
	let para_throughput_map = (FIRST_PARA_ID..(FIRST_PARA_ID + NUM_PARACHAINS))
		.map(|id| (ParaId::from(id), MIN_PARA_BLOCKS..MAX_PARA_BLOCKS))
		.collect::<std::collections::HashMap<ParaId, std::ops::Range<u32>>>();

	assert_para_throughput(&relay_client, THROUGHPUT_TIMEOUT_BLOCKS, para_throughput_map).await?;
	log::info!("All parachains producing blocks");

	// Check finalized block height is at least MIN_FINALIZED_HEIGHT
	log::info!("Checking finalized block height");
	let finalized_height = alice.reports("substrate_block_height{status=\"finalized\"}").await?;
	assert!(
		finalized_height >= MIN_FINALIZED_HEIGHT,
		"Finalized block height should be at least {}, got {}",
		MIN_FINALIZED_HEIGHT,
		finalized_height
	);
	log::info!("Finalized height OK: {}", finalized_height);

	// Check PVF preparation and execution times for all validators
	log::info!("Checking PVF preparation and execution times");
	for &name in &VALIDATOR_NAMES {
		let node = network.get_node(name)?;
		check_pvf_preparation_time(&node, name).await?;
		check_pvf_execution_time(&node, name).await?;
	}
	log::info!("All PVF timing checks passed");

	log::info!("Test finished successfully");

	Ok(())
}

/// Check PVF preparation time metrics for a validator node
async fn check_pvf_preparation_time(node: &NetworkNode, name: &str) -> Result<(), anyhow::Error> {
	// Ensure there is at least one sample in acceptable buckets (<= 10s)
	let mut found_sample = false;
	for bucket in PVF_PREP_BUCKETS_ACCEPTABLE {
		let metric_name = format!("polkadot_pvf_preparation_time_bucket{{le=\"{}\"}}", bucket);
		let count = node.reports(&metric_name).await?;
		if count >= 1.0 {
			found_sample = true;
			break;
		}
	}
	assert!(
		found_sample,
		"Node {} should have at least 1 sample in acceptable preparation buckets (<= 10s)",
		name
	);

	// Check that the sum is under MAX_PVF_PREP_TIME_SECS
	let prep_sum = node.reports("polkadot_pvf_preparation_time_sum").await?;
	assert!(
		prep_sum < MAX_PVF_PREP_TIME_SECS,
		"Node {} polkadot_pvf_preparation_time_sum should be < {}s, got {}",
		name,
		MAX_PVF_PREP_TIME_SECS,
		prep_sum
	);

	// Check that we have 0 samples in unacceptable buckets (>= 20s)
	for bucket in PVF_PREP_BUCKETS_UNACCEPTABLE {
		let metric_name = format!("polkadot_pvf_preparation_time_bucket{{le=\"{}\"}}", bucket);
		let count = node.reports(&metric_name).await?;
		assert_eq!(
			count, 0.0,
			"Node {} should have 0 samples in preparation bucket {}",
			name, bucket
		);
	}

	Ok(())
}

/// Check PVF execution time metrics for a validator node
async fn check_pvf_execution_time(node: &NetworkNode, name: &str) -> Result<(), anyhow::Error> {
	// Ensure there is at least one sample in acceptable execution buckets (<= 2s)
	let mut found_sample = false;
	for bucket in PVF_EXEC_BUCKETS_ACCEPTABLE {
		let metric_name = format!("polkadot_pvf_execution_time_bucket{{le=\"{}\"}}", bucket);
		let count = node.reports(&metric_name).await?;
		if count >= 1.0 {
			found_sample = true;
			break;
		}
	}
	assert!(
		found_sample,
		"Node {} should have at least 1 sample in acceptable execution buckets (<= 2s)",
		name
	);

	// Check that the execution-time sum is under MAX_PVF_EXEC_TIME_SECS
	let exec_sum = node.reports("polkadot_pvf_execution_time_sum").await?;
	assert!(
		exec_sum < MAX_PVF_EXEC_TIME_SECS,
		"Node {} polkadot_pvf_execution_time_sum should be < {}s, got {}",
		name,
		MAX_PVF_EXEC_TIME_SECS,
		exec_sum
	);

	// Check that we have 0 samples in unacceptable execution buckets (> 2s)
	for bucket in PVF_EXEC_BUCKETS_UNACCEPTABLE {
		let metric_name = format!("polkadot_pvf_execution_time_bucket{{le=\"{}\"}}", bucket);
		let count = node.reports(&metric_name).await?;
		assert_eq!(
			count, 0.0,
			"Node {} should have 0 samples in execution bucket {}",
			name, bucket
		);
	}

	Ok(())
}
