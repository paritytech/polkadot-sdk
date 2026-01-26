// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that when a validator starts after the chain has been running, it correctly processes
// finalized blocks to update collator reputation. This ensures that collators with good
// reputation (from successfully included candidates) are fetched without delay.
//
// The test verifies: the new validator processes finalized blocks during startup to catch 
// up on reputation

use anyhow::anyhow;
use tokio::time::Duration;

use cumulus_zombienet_sdk_helpers::{assert_para_throughput, wait_for_first_session_change};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use zombienet_orchestrator::{network::node::LogLineCountOptions, AddNodeOptions};
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

#[tokio::test(flavor = "multi_thread")]
async fn collator_protocol_reputation_lookback_test() -> Result<(), anyhow::Error> {
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
					"-lparachain=debug,parachain::collator-protocol=trace".into(),
				])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 1
							},
							"async_backing_params": {
								"max_candidate_depth": 3,
								"allowed_ancestry_len": 2
							}
						}
					}
				}))
				.with_node(|node| node.with_name("validator-0"));
			(1..3).fold(r, |acc, i| {
				acc.with_node(|node| {
					node.with_name(&format!("validator-{i}"))
				})
			})
		})
		.with_parachain(|p| {
			p.with_id(2000)
				.with_default_command("undying-collator")
				.with_default_image(
					std::env::var("COL_IMAGE")
						.unwrap_or("docker.io/paritypr/colander:latest".to_string())
						.as_str(),
				)
				.cumulus_based(false)
				.with_collator(|n| {
					n.with_name("collator")
				})
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let mut network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	let mut blocks_sub = relay_client.blocks().subscribe_finalized().await?;
	wait_for_first_session_change(&mut blocks_sub).await?;
	log::info!("First session started");


	assert_para_throughput(&relay_client, 10, [(ParaId::from(2000), 8..11)]).await?;

	// Now add a new validator that will start from scratch and sync
	log::info!("Adding new validator that will sync from genesis...");

	let opts = AddNodeOptions {
		is_validator: true,
		args: vec![                                                                                                                                                                                                                                                                      
          "-lparachain=debug,parachain::collator-protocol=trace".into(),                                                                                                                                                                                                                      
      ],                                                                                                                                                                                                                                                                               
		..Default::default()
	};

	network.add_node("validator-new", opts).await?;

	log::info!("New validator added, waiting for it to sync...");
	let new_validator = network.get_node("validator-new")?;
	let new_client: OnlineClient<PolkadotConfig> = new_validator.wait_client().await?;

	// Verify it started from block 0 and advanced to any higher block with updates"
	let reputation_catchup_options = LogLineCountOptions::new(
		|n| n >= 1,
		Duration::from_secs(60), // Give it time to sync and process
		false,
	);

	let reputation_result = new_validator
		.wait_log_line_count_with_timeout(
			"*Reputation DB advanced from block 0 to block * with * updates*",
			true, // Enable glob matching
			reputation_catchup_options,
		)
		.await?;

	assert!(
		reputation_result.success(),
		"New validator did not advance reputation DB from block 0 with updates"
	);

	log::info!("✓ New validator advanced reputation DB from block 0 during startup (catch-up mechanism working)");

	Ok(())
}
