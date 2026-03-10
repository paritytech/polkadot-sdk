// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Test that V3 candidate descriptor is enabled on the relay chain but parachains still produce
//! blocks using V2 descriptors (the current collator always uses V2).
//!
//! Verifies that:
//! - The relay chain finalizes blocks after the first session change.
//! - Parachains can produce and back candidates reported as V2 even when V3 is enabled.
//! - Parachain finality progresses with acceptable lag.

use super::assert_candidates_version;
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::assert_finality_lag;
use polkadot_primitives::{CandidateDescriptorVersion, Id as ParaId};
use serde_json::json;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

/// Test: V3 descriptor enabled on relay chain, collator reports V2.
///
/// Enables both `CandidateReceiptV2` (bit 3) and `CandidateReceiptV3` (bit 4) in node_features.
/// The collator still uses V2 descriptors. Verifies that the relay chain accepts these candidates,
/// blocks are finalized, and the parachain progresses.
#[tokio::test(flavor = "multi_thread")]
async fn v3_enabled_collator_reports_v2() -> Result<(), anyhow::Error> {
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
								"group_rotation_frequency": 4
							},
							"node_features": node_features_with_v3,
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));

			(1..4).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			})
		})
		.with_parachain(|p| {
			// The default test-parachain collator uses V2 descriptors.
			p.with_id(2500)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug,aura=debug").into()])
				.with_collator(|n| n.with_name("collator-2500"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node = network.get_node("collator-2500")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// V3 is enabled on the relay chain but the collator sends V2 descriptors.
	// Verify at least 15 V2 candidates are backed within 20 relay chain blocks.
	assert_candidates_version(
		&relay_client,
		ParaId::from(2500),
		CandidateDescriptorVersion::V2,
		true, // v3 enabled on relay
		15,
		20,
	)
	.await?;

	// Verify the parachain is finalizing blocks with acceptable lag.
	assert_finality_lag(&para_node.wait_client().await?, 5).await?;

	log::info!("V3 enabled / collator V2 test finished successfully");
	Ok(())
}
