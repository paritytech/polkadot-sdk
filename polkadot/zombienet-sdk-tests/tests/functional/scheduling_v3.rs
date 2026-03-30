// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Test that V3 candidate descriptors with scheduling_parent work correctly.

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{assert_finality_lag, assign_cores};
use polkadot_primitives::{CandidateDescriptorVersion, Id as ParaId};
use serde_json::json;
use std::collections::HashMap;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

use crate::utils::{assert_candidates_version, assert_validator_backed_candidates};

#[tokio::test(flavor = "multi_thread")]
async fn scheduling_v3_collator_with_v3_validators() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	// V2 (bit 4) and V3 (bit 3) enabled
	let node_features_with_v3 = json!({"bits": 8, "data": [0b00011000]});

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug,runtime=debug,parachain::candidate-backing=trace,parachain::provisioner=trace,parachain::prospective-parachains=trace,runtime::parachains::scheduler=trace,parachain::collator-protocol=trace,basic-authorship=debug,parachain::statement-distribution=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								"max_validators_per_core": 3,
							},
							"node_features": node_features_with_v3,
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));

			let r = (1..3).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			});

			// Experimental collator protocol validators.
			(3..6).fold(r, |acc, i| {
				acc.with_validator(|node| {
					node.with_name(&format!("validator-{i}")).with_args(vec![
						("-lparachain=debug,runtime=debug,parachain::candidate-backing=trace,parachain::provisioner=trace,parachain::prospective-parachains=trace,runtime::parachains::scheduler=trace,parachain::collator-protocol=trace,basic-authorship=debug,parachain::statement-distribution=debug").into(),
						("--experimental-collator-protocol").into(),
					])
				})
			})
		})
		.with_parachain(|p| {
			p.with_id(2700)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("async-backing-v3")
				.with_default_args(vec![
					("-lparachain=debug,aura=debug,cumulus-collator=debug,parachain::collator-protocol=trace,parachain::collator-protocol::stats=trace,basic-authorship=debug,aura::cumulus=trace").into(),
					"--authoring=slot-based".into(),
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

	// With async backing, expect ~1 candidate per 2 relay blocks → ~10 in 20 blocks.
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V3,
		HashMap::from([(ParaId::from(2700), 15..21)]),
		20,
	)
	.await?;

	assert_validator_backed_candidates(relay_node, 30).await?;
	for i in 3..=5 {
		let node = network.get_node(format!("validator-{i}"))?;
		assert_validator_backed_candidates(node, 30).await?;
	}

	assert_finality_lag(&para_node.wait_client().await?, 5).await?;

	log::info!("V3 scheduling test finished successfully");
	Ok(())
}

/// Test that V3 candidates work correctly with elastic scaling (multiple cores).
///
/// This test assigns 3 cores to a single parachain and verifies that V3 candidates are
/// being backed at elastic scaling throughput.
#[tokio::test(flavor = "multi_thread")]
async fn scheduling_v3_es_collator_with_v3_validators() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	// V2 (bit 4) and V3 (bit 3) enabled
	let node_features_with_v3 = json!({"bits": 8, "data": [0b00011000]});

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug,runtime=debug,parachain::candidate-backing=trace,parachain::provisioner=trace,parachain::prospective-parachains=trace,runtime::parachains::scheduler=trace,parachain::collator-protocol=trace,basic-authorship=debug,parachain::statement-distribution=debug").into()])
				.with_genesis_overrides(json!({
					"configuration": {
						"config": {
							"scheduler_params": {
								// 2 extra cores to assign, plus 1 auto-assigned by zombienet
								"num_cores": 2,
								"max_validators_per_core": 2,
							},
							"node_features": node_features_with_v3,
						}
					}
				}))
				.with_validator(|node| node.with_name("validator-0"));

			let r = (1..6).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			});

			// Experimental collator protocol validators.
			(6..10).fold(r, |acc, i| {
				acc.with_validator(|node| {
					node.with_name(&format!("validator-{i}")).with_args(vec![
						("-lparachain=debug,runtime=debug,parachain::candidate-backing=trace,parachain::provisioner=trace,parachain::prospective-parachains=trace,runtime::parachains::scheduler=trace,parachain::collator-protocol=trace,basic-authorship=debug,parachain::statement-distribution=debug").into(),
						("--experimental-collator-protocol").into(),
					])
				})
			})
		})
		.with_parachain(|p| {
			p.with_id(2900)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_chain("elastic-scaling-v3")
				.with_default_args(vec![
					("-lparachain=debug,aura=debug,cumulus-collator=debug,parachain::collator-protocol=trace,parachain::collator-protocol::stats=trace,basic-authorship=debug,aura::cumulus=trace").into(),
					"--authoring=slot-based".into(),
				])
				.with_collator(|n| n.with_name("collator-2900"))
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

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	// Assign 2 additional cores to the parachain (zombienet already assigns 1)
	assign_cores(&relay_client, 2800, vec![0, 1]).await?;

	// With 3 cores, expect ~3 candidates per 2 relay blocks → ~30 in 20 blocks.
	assert_candidates_version(
		&relay_client,
		CandidateDescriptorVersion::V3,
		HashMap::from([(ParaId::from(2800), 24..31)]),
		20,
	)
	.await?;

	assert_validator_backed_candidates(relay_node, 24).await?;
	for i in 6..=9 {
		let node = network.get_node(format!("validator-{i}"))?;
		assert_validator_backed_candidates(node, 24).await?;
	}

	assert_finality_lag(&para_node.wait_client().await?, 15).await?;

	log::info!("V3 elastic scaling test finished successfully");
	Ok(())
}
