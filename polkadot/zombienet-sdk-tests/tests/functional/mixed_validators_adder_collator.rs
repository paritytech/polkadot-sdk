// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test network with 3 validators from current branch and 1 validator from old polkadot image.
// The network should work properly with adder-collator parachain and finalize blocks.
// This test may trigger disputes due to erasure-coding differences between versions during
// approval voting (if the old validator is assigned as approval checker).
// The test verifies that finality continues regardless of whether disputes occur.

use anyhow::anyhow;

use cumulus_zombienet_sdk_helpers::{assert_finality_lag, assert_para_throughput};
use polkadot_primitives::Id as ParaId;
use serde_json::json;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

#[tokio::test(flavor = "multi_thread")]
async fn mixed_validators_adder_collator_test() -> Result<(), anyhow::Error> {
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
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"relay_vrf_modulo_samples": 2,
							"scheduler_params": {
								"group_rotation_frequency": 4,
								"max_validators_per_core": 5,
							}
						}
					}
				}))
				.with_node(|node| node.with_name("validator-0"));

			// Add validators 1-6 with the current branch image (total 7 validators)
			let r = (1..3)
				.fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))));

			// Add validator 7 with old polkadot image
			r.with_node(|node| {
				node.with_name("old-validator-7")
					.with_image(
						std::env::var("OLD_POLKADOT_IMAGE")
							.unwrap_or("docker.io/paritypr/polkadot:latest".to_string())
							.as_str(),
					)
					.with_command("polkadot")
			})
		})
		// Parachain 2000 with adder-collator
		.with_parachain(|p| {
			p.with_id(2000)
				.with_default_command("adder-collator")
				.with_default_image(
					std::env::var("COL_IMAGE")
						.unwrap_or("docker.io/paritypr/colander:latest".to_string())
						.as_str(),
				)
				.cumulus_based(false)
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_collator(|n| n.with_name("collator-adder-2000"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let old_relay_node = network.get_node("old-validator-7")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// Check that old validator is working and responsive
	log::info!("Checking old validator node is responsive");
	let _old_client: OnlineClient<PolkadotConfig> = old_relay_node.wait_client().await?;

	// Assert that parachain has good throughput
	log::info!("Checking parachain throughput");
	assert_para_throughput(
		&relay_client,
		20,
		[(ParaId::from(2000), 10..21)]
			.into_iter()
			.collect(),
	)
	.await?;

	// Check if disputes are raised due to erasure-coding differences during approval voting
	// Note: Disputes are only initiated when the old validator is assigned as approval checker
	// for a backed candidate. With 4 validators, this might not happen for all candidates.
	log::info!("Checking for disputes during approval voting (may not occur if old validator is not assigned as approval checker)");
	let disputes_result = relay_node
		.wait_metric_with_timeout("polkadot_parachain_candidate_disputes_total", |v| v > 0.0, 90u64)
		.await;
	
	if disputes_result.is_ok() {
		log::info!("✅ Disputes detected due to erasure-coding differences during approval voting");
	} else {
		log::info!("ℹ️  No disputes detected - old validator was not assigned as approval checker for backed candidates");
	}

	// Finality should continue regardless of whether disputes occurred
	// We check that finality lag is bounded
	log::info!("Checking that finality continues normally");
	
	// Check approval checking finality lag is bounded
	relay_node
		.wait_metric_with_timeout(
			"polkadot_parachain_approval_checking_finality_lag",
			|lag| lag < 30.0,
			60u64
		)
		.await?;
	log::info!("✅ Approval checking finality lag is within acceptable bounds");
	
	// Check disputes finality lag is bounded
	relay_node
		.wait_metric_with_timeout(
			"polkadot_parachain_disputes_finality_lag",
			|lag| lag < 30.0,
			60u64
		)
		.await?;
	log::info!("✅ Disputes finality lag is within acceptable bounds");

	// Final check: relay chain finality should still work
	log::info!("Final finality check");
	assert_finality_lag(&relay_node.wait_client().await?, 10).await?;

	log::info!("Test finished successfully");

	Ok(())
}

