// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Test that `CandidateReceiptV3` can be enabled dynamically while the network is running.
//!
//! Starts with only `CandidateReceiptV2` (bit 3) set in `node_features`. After verifying that V2
//! candidates are backed and finalized, enables `CandidateReceiptV3` (bit 4) via a sudo
//! extrinsic. After the next session change (when the config becomes active), verifies that the
//! relay chain continues to accept V2 candidates from the collator.
//!
//! The validator set contains both standard and experimental-collator-protocol validators.
//! Para 2901 uses elastic scaling with 3 cores to verify throughput under dynamic enablement.

use crate::utils::{
	assert_candidates_version, assert_validator_backed_candidates, enable_node_features,
};
use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_finality_lag, assign_cores};
use polkadot_primitives::{CandidateDescriptorVersion, Id as ParaId};
use serde_json::json;
use std::collections::HashMap;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

/// Test: V3 descriptor enabled dynamically while the network is running.
#[tokio::test(flavor = "multi_thread")]
async fn v3_dynamic_enablement_test() -> Result<(), anyhow::Error> {
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
				.with_default_args(vec![("-lparachain=debug,runtime=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								// 2 extra cores beyond each auto-registered para core.
								// Para 2901 uses elastic scaling and will be assigned
								// 0 and 1 in addition.
								"num_cores": 2,
								"max_validators_per_core": 2,
								"group_rotation_frequency": 4
							}
						}
					}
				}))
				// Standard collator protocol validators (groups 0, 1).
				.with_validator(|node| node.with_name("validator-0"));

			let r = (1..4).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			});

			// Experimental collator protocol validators (groups 2, 3).
			(4..8).fold(r, |acc, i| {
				acc.with_validator(|node| {
					node.with_name(&format!("validator-{i}")).with_args(vec![
						("-lparachain=debug,runtime=debug,parachain::collator-protocol=trace")
							.into(),
						("--experimental-collator-protocol").into(),
					])
				})
			})
		})
		.with_parachain(|p| {
			p.with_id(2900)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug,aura=debug").into()])
				.with_collator(|n| n.with_name("collator-2900"))
		})
		.with_parachain(|p| {
			p.with_id(2901)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("elastic-scaling")
				.with_default_args(vec![
					("--authoring=slot-based").into(),
					("-lparachain=debug,aura=debug").into(),
				])
				.with_collator(|n| n.with_name("collator-2901"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node = network.get_node("collator-2900")?;
	let para_node_slot = network.get_node("collator-2901")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// Assign 2 extra cores to para 2901 for elastic scaling (3 cores total).
	assign_cores(&relay_client, 2901, vec![0, 1]).await?;

	let para_2900 = ParaId::from(2900);
	let para_2901 = ParaId::from(2901);

	// Para 2900: 1 core, basic lookahead → ~1 backed candidate per 2 relay blocks.
	// Para 2901: 3 cores, elastic scaling → ~3x throughput.
	log::info!("checking V2 candidates with V3 disabled");
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V2,
		HashMap::from([(para_2900, 15..21), (para_2901, 45..61)]),
		20,
	)
	.await?;

	log::info!("Enabling V3");
	enable_node_features(&relay_client, &[4]).await?;

	log::info!("checking V2 candidates after V3 enabled");
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V2,
		HashMap::from([(para_2900, 15..21), (para_2901, 45..61)]),
		20,
	)
	.await?;

	assert_validator_backed_candidates(relay_node, 30).await?;
	for i in 4..=7 {
		let node = network.get_node(format!("validator-{i}"))?;
		assert_validator_backed_candidates(node, 30).await?;
	}

	assert_finality_lag(&para_node.wait_client().await?, 6).await?;
	assert_finality_lag(&para_node_slot.wait_client().await?, 15).await?;

	log::info!("V3 dynamic enablement test finished successfully");

	Ok(())
}
