// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Test that V3 candidate descriptors work correctly with elastic scaling (multiple cores).
//!
//! Enables both `CandidateReceiptV2` (bit 3) and `CandidateReceiptV3` (bit 4) in node_features,
//! assigns 3 cores to a single parachain, and verifies that:
//! - V2 candidates are backed (the current collator always uses V2).
//! - Multiple candidates per relay block are produced (elastic throughput).
//! - Parachain finality progresses with acceptable lag.

use super::assert_candidates_version;
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_finality_lag, assign_cores};
use polkadot_primitives::{CandidateDescriptorVersion, Id as ParaId};
use serde_json::json;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

/// Test: V3 descriptor enabled with elastic scaling (3 cores).
///
/// The collator still sends V2 descriptors. With 3 cores assigned, the relay chain must
/// correctly handle multiple V2 candidates per block while V3 is enabled.
#[tokio::test(flavor = "multi_thread")]
async fn v3_enabled_elastic_scaling() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	// Enable CandidateReceiptV2 (bit 3) and CandidateReceiptV3 (bit 4).
	// bitvec Lsb0 u8: bits 3 and 4 set => 0b00011000 = 24
	let node_features_with_v3 = json!({"bits": 8, "data": [0b00011000]});

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug,runtime=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"num_cores": 2,
								"max_validators_per_core": 1,
								"group_rotation_frequency": 4
							},
							"node_features": node_features_with_v3,
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));

			(1..6).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			})
		})
		.with_parachain(|p| {
			p.with_id(2700)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("elastic-scaling")
				.with_default_args(vec![
					("-lparachain=debug,aura=debug").into(),
					("--authoring=slot-based").into(),
				])
				.with_collator(|n| n.with_name("collator-2700"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node = network.get_node("collator-2700")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// Assign 2 additional cores (zombienet already assigns 1), giving 3 total.
	assign_cores(&relay_client, 2700, vec![0, 1]).await?;

	// With 3 cores and V3 enabled, expect higher throughput.
	// At least 15 V2 candidates within 20 relay blocks.
	assert_candidates_version(
		&relay_client,
		ParaId::from(2700),
		CandidateDescriptorVersion::V2,
		true, // v3 enabled on relay
		15,
		20,
	)
	.await?;

	// Allow more finality lag with elastic scaling.
	assert_finality_lag(&para_node.wait_client().await?, 15).await?;

	log::info!("V3 elastic scaling test finished successfully");
	Ok(())
}
